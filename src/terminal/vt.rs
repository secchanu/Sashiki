//! Terminal state thread.
//!
//! libghostty-vt handles are `!Send` and `!Sync`, so every terminal operation
//! runs on one dedicated thread that owns the terminal, the render state and
//! the input encoders. The UI thread talks to it through [`VtCommand`] and
//! receives [`TerminalEvent`]s plus [`Frame`] snapshots.

use super::TerminalEvent;
use super::frame::{CellAttrs, CellWidth, Frame, FrameCell, FrameCursor, FrameRow, Rgb, Underline};
use super::pty;
use crate::theme;
use libghostty_vt::render::{CellIterator, Dirty, RenderState, RowIterator};
use libghostty_vt::screen::{CellWide, Screen};
use libghostty_vt::selection::{FormatOptions, SelectLineOptions, SelectWordOptions, Selection};
use libghostty_vt::style::{Palette, RgbColor};
use libghostty_vt::terminal::{
    Mode, Options as TerminalOptions, Point, PointCoordinate, ScrollViewport,
};
use libghostty_vt::{Terminal, key, mouse, paste};
use portable_pty::PtySize;
use std::cell::RefCell;
use std::io::{Read, Write};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Scrollback limit in bytes, matching Ghostty's own default. Pages are
/// allocated on demand, so this bounds growth rather than reserving memory.
const MAX_SCROLLBACK_BYTES: usize = 10_000_000;
/// Initial grid size, replaced by the first layout pass.
const INITIAL_COLS: u16 = 80;
const INITIAL_ROWS: u16 = 24;
/// Bytes of pty output processed before a frame is published, so that a
/// long-running command cannot starve the display.
const MAX_BATCH_BYTES: usize = 512 * 1024;
/// Shortest interval between published frames while output keeps arriving.
const FRAME_INTERVAL: Duration = Duration::from_millis(8);
/// Read buffer size for pty output.
const READ_BUFFER_SIZE: usize = 64 * 1024;

/// A key press already translated from the UI toolkit's representation.
#[derive(Clone, Debug)]
pub struct KeyInput {
    pub key: key::Key,
    pub mods: key::Mods,
    /// Text produced by the key, when the platform reports one.
    pub text: Option<String>,
    /// Codepoint the key would produce without modifiers, used for the
    /// control-code and Kitty protocol encodings.
    pub unshifted: Option<char>,
    /// Set while an IME preedit is active so the key is not encoded twice.
    pub composing: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct MouseInput {
    pub x: f32,
    pub y: f32,
    pub button: mouse::Button,
    pub mods: key::Mods,
}

#[derive(Clone, Debug)]
pub enum VtCommand {
    /// Bytes to send to the pty verbatim.
    Write(Vec<u8>),
    Key(KeyInput),
    Paste(String),
    MouseDown {
        input: MouseInput,
        click_count: u8,
    },
    MouseDrag(MouseInput),
    MouseUp(MouseInput),
    MouseMove(MouseInput),
    /// Wheel scroll. Positive `lines` scrolls towards the scrollback.
    Scroll {
        input: MouseInput,
        lines: i32,
    },
    /// Keyboard scrollback paging. Positive `lines` scrolls towards the scrollback.
    ScrollLines(i32),
    Resize {
        cols: u16,
        rows: u16,
        cell_width: u16,
        cell_height: u16,
        padding: u16,
    },
    ClearSelection,
    CopySelection,
    Focus(bool),
    Shutdown,
    /// Output read from the pty.
    Output(Vec<u8>),
    /// The pty reached end of file.
    Closed,
}

/// Latest published frame. The UI takes it when it processes a wakeup.
pub type FrameSlot = Arc<Mutex<Option<Frame>>>;

/// Effects raised from terminal callbacks during `vt_write`.
///
/// Callbacks run while the terminal is mutably borrowed, so they cannot touch
/// the thread state directly and record their effects here instead.
#[derive(Default)]
struct Effects {
    pty_writes: Vec<u8>,
    bell: bool,
    title: Option<String>,
    clipboard: Option<String>,
}

pub struct VtHandle {
    pub commands: smol::channel::Sender<VtCommand>,
    pub frame: FrameSlot,
    pub killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
}

/// Start the pty and its terminal thread.
pub fn spawn(
    working_directory: Option<std::path::PathBuf>,
    events: smol::channel::Sender<TerminalEvent>,
) -> anyhow::Result<VtHandle> {
    let mut pty = pty::spawn(
        working_directory,
        PtySize {
            rows: INITIAL_ROWS,
            cols: INITIAL_COLS,
            pixel_width: 0,
            pixel_height: 0,
        },
    )?;

    let (command_tx, command_rx) = smol::channel::unbounded::<VtCommand>();
    let frame: FrameSlot = Arc::new(Mutex::new(None));
    let killer = pty.killer.clone_killer();

    let reader = pty
        .reader
        .take()
        .ok_or_else(|| anyhow::anyhow!("pty reader unavailable"))?;
    spawn_reader(reader, command_tx.clone())?;

    let thread_frame = Arc::clone(&frame);
    std::thread::Builder::new()
        .name("sashiki-vt".into())
        .spawn(move || {
            run(pty, command_rx, events, thread_frame);
        })?;

    Ok(VtHandle {
        commands: command_tx,
        frame,
        killer,
    })
}

fn spawn_reader(
    mut reader: Box<dyn Read + Send>,
    commands: smol::channel::Sender<VtCommand>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("sashiki-pty-read".into())
        .spawn(move || {
            let mut buffer = vec![0u8; READ_BUFFER_SIZE];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if commands
                            .send_blocking(VtCommand::Output(buffer[..n].to_vec()))
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
            let _ = commands.send_blocking(VtCommand::Closed);
        })?;
    Ok(())
}

fn run(
    pty: pty::Pty,
    commands: smol::channel::Receiver<VtCommand>,
    events: smol::channel::Sender<TerminalEvent>,
    frame: FrameSlot,
) {
    let mut state = match VtState::new(pty, events.clone(), frame) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("terminal: failed to initialize: {error}");
            let _ = events.try_send(TerminalEvent::Exit);
            return;
        }
    };

    while let Ok(command) = commands.recv_blocking() {
        let mut batch_bytes = command_size(&command);
        if !state.handle(command) {
            break;
        }
        while batch_bytes < MAX_BATCH_BYTES {
            match commands.try_recv() {
                Ok(next) => {
                    batch_bytes += command_size(&next);
                    if !state.handle(next) {
                        return;
                    }
                }
                Err(_) => break,
            }
        }

        state.flush();
        if commands.is_empty() || state.last_frame_age() >= FRAME_INTERVAL {
            state.publish_frame();
        }
    }
}

fn command_size(command: &VtCommand) -> usize {
    match command {
        VtCommand::Output(data) => data.len(),
        _ => 0,
    }
}

struct VtState {
    terminal: Terminal<'static, 'static>,
    render: RenderState<'static>,
    rows: RowIterator<'static>,
    cells: CellIterator<'static>,
    key_encoder: key::Encoder<'static>,
    key_event: key::Event<'static>,
    mouse_encoder: mouse::Encoder<'static>,
    mouse_event: mouse::Event<'static>,
    effects: Rc<RefCell<Effects>>,
    events: smol::channel::Sender<TerminalEvent>,
    frame: FrameSlot,
    master: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
    /// Anchor of an in-progress mouse selection, tracked so it survives
    /// scrollback movement during the drag.
    selection_anchor: Option<libghostty_vt::screen::TrackedGridRef>,
    has_selection: bool,
    cols: u16,
    grid_rows: u16,
    cell_width: f32,
    cell_height: f32,
    padding: f32,
    encode_buffer: Vec<u8>,
    last_frame: Instant,
    /// Forces the next frame even when the terminal reports no dirty rows.
    force_frame: bool,
    closed: bool,
}

impl VtState {
    fn new(
        pty: pty::Pty,
        events: smol::channel::Sender<TerminalEvent>,
        frame: FrameSlot,
    ) -> anyhow::Result<Self> {
        let mut terminal = Terminal::new(TerminalOptions {
            cols: INITIAL_COLS,
            rows: INITIAL_ROWS,
            max_scrollback: MAX_SCROLLBACK_BYTES,
        })?;

        apply_theme(&mut terminal)?;

        let effects = Rc::new(RefCell::new(Effects::default()));

        let sink = Rc::clone(&effects);
        terminal.on_pty_write(move |_terminal, data| {
            sink.borrow_mut().pty_writes.extend_from_slice(data);
        })?;

        let sink = Rc::clone(&effects);
        terminal.on_bell(move |_terminal| {
            sink.borrow_mut().bell = true;
        })?;

        let sink = Rc::clone(&effects);
        terminal.on_title_changed(move |terminal| {
            if let Ok(title) = terminal.title() {
                sink.borrow_mut().title = Some(title.to_string());
            }
        })?;

        let sink = Rc::clone(&effects);
        terminal.on_clipboard_write(move |_terminal, write| {
            let mut text = None;
            for content in write.contents() {
                let is_text = content.mime.is_empty() || content.mime.starts_with("text/");
                if text.is_none() || is_text {
                    text = Some(content.data.to_string());
                }
                if is_text {
                    break;
                }
            }
            if let Some(text) = text {
                sink.borrow_mut().clipboard = Some(text);
            }
            Ok(())
        })?;

        Ok(Self {
            terminal,
            render: RenderState::new()?,
            rows: RowIterator::new()?,
            cells: CellIterator::new()?,
            key_encoder: key::Encoder::new()?,
            key_event: key::Event::new()?,
            mouse_encoder: mouse::Encoder::new()?,
            mouse_event: mouse::Event::new()?,
            effects,
            events,
            frame,
            master: pty.master,
            writer: pty.writer,
            killer: pty.killer,
            selection_anchor: None,
            has_selection: false,
            cols: INITIAL_COLS,
            grid_rows: INITIAL_ROWS,
            cell_width: 1.0,
            cell_height: 1.0,
            padding: 0.0,
            encode_buffer: Vec::with_capacity(128),
            last_frame: Instant::now(),
            force_frame: true,
            closed: false,
        })
    }

    /// Handle one command. Returns false when the thread should stop.
    fn handle(&mut self, command: VtCommand) -> bool {
        match command {
            VtCommand::Output(data) => {
                self.terminal.vt_write(&data);
            }
            VtCommand::Closed => {
                self.closed = true;
                let _ = self.events.try_send(TerminalEvent::Exit);
                return false;
            }
            VtCommand::Shutdown => {
                self.shutdown();
                return false;
            }
            VtCommand::Write(data) => {
                self.scroll_to_bottom();
                self.write_pty(&data);
            }
            VtCommand::Key(input) => self.handle_key(&input),
            VtCommand::Paste(text) => self.handle_paste(&text),
            VtCommand::MouseDown { input, click_count } => {
                self.handle_mouse_down(input, click_count)
            }
            VtCommand::MouseDrag(input) => self.handle_mouse_drag(input),
            VtCommand::MouseUp(input) => self.handle_mouse_up(input),
            VtCommand::MouseMove(input) => self.handle_mouse_move(input),
            VtCommand::Scroll { input, lines } => self.handle_scroll(input, lines),
            VtCommand::ScrollLines(lines) => self.scroll_lines(lines),
            VtCommand::Resize {
                cols,
                rows,
                cell_width,
                cell_height,
                padding,
            } => self.handle_resize(cols, rows, cell_width, cell_height, padding),
            VtCommand::ClearSelection => self.clear_selection(),
            VtCommand::CopySelection => self.copy_selection(),
            VtCommand::Focus(focused) => self.handle_focus(focused),
        }
        true
    }

    /// Drain effects raised by terminal callbacks.
    fn flush(&mut self) {
        let (writes, bell, title, clipboard) = {
            let mut effects = self.effects.borrow_mut();
            (
                std::mem::take(&mut effects.pty_writes),
                std::mem::take(&mut effects.bell),
                effects.title.take(),
                effects.clipboard.take(),
            )
        };

        if !writes.is_empty() {
            self.write_pty(&writes);
        }
        if bell {
            let _ = self.events.try_send(TerminalEvent::Bell);
        }
        if let Some(title) = title {
            let _ = self.events.try_send(TerminalEvent::Title(title));
        }
        if let Some(text) = clipboard {
            let _ = self.events.try_send(TerminalEvent::ClipboardWrite(text));
        }
    }

    fn write_pty(&mut self, data: &[u8]) {
        if self.closed {
            return;
        }
        if self.writer.write_all(data).is_err() || self.writer.flush().is_err() {
            self.closed = true;
        }
    }

    fn shutdown(&mut self) {
        let _ = self.killer.kill();
        self.closed = true;
    }

    fn last_frame_age(&self) -> Duration {
        self.last_frame.elapsed()
    }

    // --------------------------------------------------------------------
    // Input
    // --------------------------------------------------------------------

    fn handle_key(&mut self, input: &KeyInput) {
        self.key_encoder.set_options_from_terminal(&self.terminal);
        // macOS only: Option acts as a modifier rather than a compose key,
        // which is what shell word motions such as Alt+B expect. Reading the
        // terminal state resets the option, so it is set again on every key.
        self.key_encoder
            .set_macos_option_as_alt(key::OptionAsAlt::True);

        self.key_event
            .set_action(key::Action::Press)
            .set_key(input.key)
            .set_mods(input.mods)
            .set_composing(input.composing);
        self.key_event.set_utf8(input.text.clone());
        if let Some(codepoint) = input.unshifted {
            self.key_event.set_unshifted_codepoint(codepoint);
        } else {
            self.key_event.set_unshifted_codepoint('\0');
        }

        self.encode_buffer.clear();
        if self
            .key_encoder
            .encode_to_vec(&self.key_event, &mut self.encode_buffer)
            .is_err()
            || self.encode_buffer.is_empty()
        {
            return;
        }

        self.scroll_to_bottom();
        let encoded = std::mem::take(&mut self.encode_buffer);
        self.write_pty(&encoded);
        self.encode_buffer = encoded;
    }

    fn handle_paste(&mut self, text: &str) {
        let bracketed = self.terminal.mode(Mode::BRACKETED_PASTE).unwrap_or(false);
        let mut data = text.as_bytes().to_vec();
        // Bracketed paste adds a prefix and a suffix; the encoder reports the
        // exact requirement if the guess is too small.
        let mut buffer = vec![0u8; data.len() + 16];
        loop {
            match paste::encode(&mut data, bracketed, &mut buffer) {
                Ok(len) => {
                    self.scroll_to_bottom();
                    let encoded = buffer[..len].to_vec();
                    self.write_pty(&encoded);
                    return;
                }
                Err(libghostty_vt::Error::OutOfSpace { required }) if required > buffer.len() => {
                    buffer.resize(required, 0);
                }
                Err(_) => return,
            }
        }
    }

    fn handle_focus(&mut self, focused: bool) {
        if self.terminal.mode(Mode::FOCUS_EVENT).unwrap_or(false) {
            let sequence: &[u8] = if focused { b"\x1b[I" } else { b"\x1b[O" };
            self.write_pty(sequence);
        }
    }

    // --------------------------------------------------------------------
    // Mouse
    // --------------------------------------------------------------------

    fn mouse_tracking(&self) -> bool {
        self.terminal.is_mouse_tracking().unwrap_or(false)
    }

    fn encode_mouse(
        &mut self,
        action: mouse::Action,
        button: Option<mouse::Button>,
        input: MouseInput,
    ) -> bool {
        self.mouse_encoder.set_options_from_terminal(&self.terminal);
        self.mouse_encoder.set_size(mouse::EncoderSize {
            screen_width: (self.cols as f32 * self.cell_width + self.padding * 2.0) as u32,
            screen_height: (self.grid_rows as f32 * self.cell_height + self.padding * 2.0) as u32,
            cell_width: self.cell_width.max(1.0) as u32,
            cell_height: self.cell_height.max(1.0) as u32,
            padding_top: self.padding as u32,
            padding_bottom: self.padding as u32,
            padding_left: self.padding as u32,
            padding_right: self.padding as u32,
        });
        // Motion reports carry the drag button, which the protocol encodes
        // only while a button is down. Deduplicating by cell keeps motion
        // reports to one per cell crossed.
        self.mouse_encoder
            .set_any_button_pressed(button.is_some() && action != mouse::Action::Release)
            .set_track_last_cell(true);

        self.mouse_event
            .set_action(action)
            .set_button(button)
            .set_mods(input.mods)
            .set_position(mouse::Position {
                x: input.x,
                y: input.y,
            });

        self.encode_buffer.clear();
        if self
            .mouse_encoder
            .encode_to_vec(&self.mouse_event, &mut self.encode_buffer)
            .is_err()
            || self.encode_buffer.is_empty()
        {
            return false;
        }

        let encoded = std::mem::take(&mut self.encode_buffer);
        self.write_pty(&encoded);
        self.encode_buffer = encoded;
        true
    }

    /// Convert a position in element pixels to a viewport cell.
    fn position_to_cell(&self, x: f32, y: f32) -> (u16, u16) {
        let x = ((x - self.padding) / self.cell_width.max(1.0)).floor();
        let y = ((y - self.padding) / self.cell_height.max(1.0)).floor();
        let col = x.max(0.0) as u16;
        let row = y.max(0.0) as u16;
        (
            col.min(self.cols.saturating_sub(1)),
            row.min(self.grid_rows.saturating_sub(1)),
        )
    }

    fn handle_mouse_down(&mut self, input: MouseInput, click_count: u8) {
        if self.mouse_tracking() {
            self.encode_mouse(mouse::Action::Press, Some(input.button), input);
            return;
        }

        let (col, row) = self.position_to_cell(input.x, input.y);
        let point = Point::Viewport(PointCoordinate {
            x: col,
            y: row as u32,
        });

        match click_count {
            2 => self.select_word(point),
            3 => self.select_line(point),
            _ => {
                self.selection_anchor = self.terminal.track_grid_ref(point).ok();
                self.set_selection(None);
            }
        }
        self.force_frame = true;
    }

    fn handle_mouse_drag(&mut self, input: MouseInput) {
        if self.mouse_tracking() {
            self.encode_mouse(mouse::Action::Motion, Some(input.button), input);
            return;
        }

        let Some(anchor) = self.selection_anchor.as_ref() else {
            return;
        };
        let Ok(Some(start)) = anchor.snapshot(&self.terminal) else {
            return;
        };
        let (col, row) = self.position_to_cell(input.x, input.y);
        let Ok(end) = self.terminal.grid_ref(Point::Viewport(PointCoordinate {
            x: col,
            y: row as u32,
        })) else {
            return;
        };

        let selection = Selection::new(start, end, false);
        let _ = self.terminal.set_selection(Some(&selection));
        self.has_selection = true;
        self.force_frame = true;
    }

    fn handle_mouse_up(&mut self, input: MouseInput) {
        if self.mouse_tracking() {
            self.encode_mouse(mouse::Action::Release, Some(input.button), input);
            return;
        }
        self.selection_anchor = None;
    }

    fn handle_mouse_move(&mut self, input: MouseInput) {
        if self.mouse_tracking() {
            self.encode_mouse(mouse::Action::Motion, None, input);
        }
    }

    fn handle_scroll(&mut self, input: MouseInput, lines: i32) {
        if self.mouse_tracking() {
            let button = if lines > 0 {
                mouse::Button::Four
            } else {
                mouse::Button::Five
            };
            self.encode_mouse(
                mouse::Action::Press,
                Some(button),
                MouseInput { button, ..input },
            );
            return;
        }

        let alt_screen = matches!(self.terminal.active_screen(), Ok(Screen::Alternate));
        if alt_screen && self.terminal.mode(Mode::ALT_SCROLL).unwrap_or(false) {
            // Applications on the alternate screen have no scrollback, so the
            // wheel is reported as arrow keys instead.
            let key = if lines > 0 {
                key::Key::ArrowUp
            } else {
                key::Key::ArrowDown
            };
            for _ in 0..lines.abs() {
                self.handle_key(&KeyInput {
                    key,
                    mods: key::Mods::empty(),
                    text: None,
                    unshifted: None,
                    composing: false,
                });
            }
            return;
        }

        self.scroll_lines(lines);
    }

    fn scroll_lines(&mut self, lines: i32) {
        if lines == 0 {
            return;
        }
        // Positive input scrolls towards the scrollback, which libghostty
        // expresses as a negative delta.
        self.terminal
            .scroll_viewport(ScrollViewport::Delta(-(lines as isize)));
        self.force_frame = true;
    }

    fn scroll_to_bottom(&mut self) {
        self.terminal.scroll_viewport(ScrollViewport::Bottom);
        self.force_frame = true;
    }

    // --------------------------------------------------------------------
    // Selection
    // --------------------------------------------------------------------

    fn set_selection(&mut self, selection: Option<&Selection<'_>>) {
        let _ = self.terminal.set_selection(selection);
        self.has_selection = selection.is_some();
    }

    fn select_word(&mut self, point: Point) {
        let Ok(grid_ref) = self.terminal.grid_ref(point) else {
            return;
        };
        let Ok(Some(selection)) = self.terminal.select_word(SelectWordOptions::new(grid_ref))
        else {
            return;
        };
        let _ = self.terminal.set_selection(Some(&selection));
        self.has_selection = true;
    }

    fn select_line(&mut self, point: Point) {
        let Ok(grid_ref) = self.terminal.grid_ref(point) else {
            return;
        };
        let Ok(Some(selection)) = self.terminal.select_line(SelectLineOptions::new(grid_ref))
        else {
            return;
        };
        let _ = self.terminal.set_selection(Some(&selection));
        self.has_selection = true;
    }

    fn clear_selection(&mut self) {
        self.selection_anchor = None;
        self.set_selection(None);
        self.force_frame = true;
    }

    fn copy_selection(&mut self) {
        if !self.has_selection {
            return;
        }
        // Ghostty's own copy behavior: plain text, soft wraps joined and
        // trailing whitespace removed.
        let mut buffer = vec![0u8; 4096];
        loop {
            let options = FormatOptions::new().with_unwrap(true).with_trim(true);
            match self.terminal.format_selection_buf(options, &mut buffer) {
                Ok(Some(len)) => {
                    if let Ok(text) = String::from_utf8(buffer[..len].to_vec()) {
                        let _ = self.events.try_send(TerminalEvent::Copy(text));
                    }
                    return;
                }
                Ok(None) => return,
                Err(libghostty_vt::Error::OutOfSpace { required }) if required > buffer.len() => {
                    buffer.resize(required, 0);
                }
                Err(_) => return,
            }
        }
    }

    // --------------------------------------------------------------------
    // Resize
    // --------------------------------------------------------------------

    fn handle_resize(
        &mut self,
        cols: u16,
        rows: u16,
        cell_width: u16,
        cell_height: u16,
        padding: u16,
    ) {
        self.cols = cols;
        self.grid_rows = rows;
        self.cell_width = cell_width.max(1) as f32;
        self.cell_height = cell_height.max(1) as f32;
        self.padding = padding as f32;

        let _ = self.terminal.resize(
            cols,
            rows,
            u32::from(cell_width.max(1)),
            u32::from(cell_height.max(1)),
        );
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: cols.saturating_mul(cell_width.max(1)),
            pixel_height: rows.saturating_mul(cell_height.max(1)),
        });
        self.force_frame = true;
    }

    // --------------------------------------------------------------------
    // Frame
    // --------------------------------------------------------------------

    fn publish_frame(&mut self) {
        let Some(frame) = self.build_frame() else {
            return;
        };
        self.last_frame = Instant::now();
        if let Ok(mut slot) = self.frame.lock() {
            *slot = Some(frame);
        }
        let _ = self.events.try_send(TerminalEvent::Wakeup);
    }

    fn build_frame(&mut self) -> Option<Frame> {
        let alt_screen = matches!(self.terminal.active_screen(), Ok(Screen::Alternate));
        let mouse_tracking = self.mouse_tracking();
        let has_selection = self.has_selection;
        let force = std::mem::take(&mut self.force_frame);

        let snapshot = self.render.update(&self.terminal).ok()?;
        if !force && matches!(snapshot.dirty(), Ok(Dirty::Clean)) {
            return None;
        }

        let colors = snapshot.colors().ok()?;
        let cursor = if snapshot.cursor_visible().unwrap_or(false) {
            snapshot
                .cursor_viewport()
                .ok()
                .flatten()
                .map(|position| FrameCursor {
                    x: position.x,
                    y: position.y,
                    style: snapshot
                        .cursor_visual_style()
                        .map(Into::into)
                        .unwrap_or_default(),
                })
        } else {
            None
        };
        let cols = snapshot.cols().unwrap_or(self.cols);
        let row_count = snapshot.rows().unwrap_or(self.grid_rows) as usize;

        let mut rows = Vec::with_capacity(row_count);
        {
            let mut row_iteration = self.rows.update(&snapshot).ok()?;
            while let Some(row) = row_iteration.next() {
                rows.push(build_row(row, &mut self.cells, cols));
            }
        }
        let _ = snapshot.set_dirty(Dirty::Clean);

        Some(Frame {
            rows: Arc::new(rows),
            cursor,
            foreground: colors.foreground.into(),
            background: colors.background.into(),
            cursor_color: colors
                .cursor
                .unwrap_or_else(|| rgb_from_u32(theme::ansi::CURSOR))
                .into(),
            alt_screen,
            mouse_tracking,
            has_selection,
        })
    }
}

fn build_row(
    row: &libghostty_vt::render::RowIteration<'static, '_>,
    cells: &mut CellIterator<'static>,
    cols: u16,
) -> FrameRow {
    let selection = row
        .selection()
        .ok()
        .flatten()
        .map(|range| (range.start_x, range.end_x));
    let has_clusters = row
        .raw_row()
        .and_then(|raw| raw.has_grapheme_cluster())
        .unwrap_or(false);

    let mut frame_row = FrameRow {
        cells: Vec::with_capacity(cols as usize),
        clusters: Vec::new(),
        selection,
    };

    let Ok(mut cell_iteration) = cells.update(row) else {
        frame_row.cells.resize(cols as usize, FrameCell::default());
        return frame_row;
    };

    let mut column: u16 = 0;
    while let Some(cell) = cell_iteration.next() {
        let Ok(raw) = cell.raw_cell() else {
            frame_row.cells.push(FrameCell::default());
            column += 1;
            continue;
        };

        let has_text = raw.has_text().unwrap_or(false);
        let styled = cell.has_styling().unwrap_or(false);
        let (fg, bg) = if styled || !has_text {
            (
                cell.fg_color().ok().flatten().map(Into::into),
                cell.bg_color().ok().flatten().map(Into::into),
            )
        } else {
            (None, None)
        };

        let attrs = if styled {
            cell.style().map(cell_attrs).unwrap_or_default()
        } else {
            CellAttrs::default()
        };

        let width = match raw.wide() {
            Ok(CellWide::Wide) => CellWidth::Wide,
            Ok(CellWide::SpacerTail) | Ok(CellWide::SpacerHead) => CellWidth::Spacer,
            _ => CellWidth::Narrow,
        };

        let ch = if has_text {
            raw.codepoint()
                .ok()
                .and_then(char::from_u32)
                .filter(|c| *c != '\0')
                .unwrap_or(' ')
        } else {
            ' '
        };

        if has_clusters && cell.graphemes_len().unwrap_or(0) > 1 {
            let mut text = String::new();
            if cell.graphemes_utf8(&mut text).is_ok() && !text.is_empty() {
                frame_row.clusters.push((column, text.into_boxed_str()));
            }
        }

        frame_row.cells.push(FrameCell {
            ch,
            fg,
            bg,
            attrs,
            width,
        });
        column += 1;
    }

    if frame_row.cells.len() < cols as usize {
        frame_row.cells.resize(cols as usize, FrameCell::default());
    }
    frame_row
}

fn cell_attrs(style: libghostty_vt::style::Style) -> CellAttrs {
    CellAttrs {
        bold: style.bold,
        italic: style.italic,
        faint: style.faint,
        inverse: style.inverse,
        invisible: style.invisible,
        strikethrough: style.strikethrough,
        underline: Underline::from(style.underline),
    }
}

fn rgb_from_u32(value: u32) -> RgbColor {
    RgbColor::from(Rgb::from_u32(value))
}

/// Install the app palette as the terminal's defaults so palette lookups
/// resolve against the theme, while still allowing OSC overrides.
fn apply_theme(terminal: &mut Terminal<'static, 'static>) -> anyhow::Result<()> {
    let background = rgb_from_u32(theme::ansi::BACKGROUND);
    let foreground = rgb_from_u32(theme::ansi::FOREGROUND);

    let named = [
        theme::ansi::BLACK,
        theme::ansi::RED,
        theme::ansi::GREEN,
        theme::ansi::YELLOW,
        theme::ansi::BLUE,
        theme::ansi::MAGENTA,
        theme::ansi::CYAN,
        theme::ansi::WHITE,
        theme::ansi::BRIGHT_BLACK,
        theme::ansi::BRIGHT_RED,
        theme::ansi::BRIGHT_GREEN,
        theme::ansi::BRIGHT_YELLOW,
        theme::ansi::BRIGHT_BLUE,
        theme::ansi::BRIGHT_MAGENTA,
        theme::ansi::BRIGHT_CYAN,
        theme::ansi::BRIGHT_WHITE,
    ];

    // The 216-color cube and the grayscale ramp are derived from the 16 named
    // colors so that the whole palette stays consistent with the theme.
    let mut base = Palette::default();
    for (index, color) in named.iter().enumerate() {
        base.set(
            libghostty_vt::style::PaletteIndex(index as u8),
            rgb_from_u32(*color),
        );
    }
    let palette = Palette::generate(Some(&base), None, background, foreground, true);

    terminal.set_default_color_palette(Some(palette))?;
    terminal.set_default_fg_color(Some(foreground))?;
    terminal.set_default_bg_color(Some(background))?;
    terminal.set_default_cursor_color(Some(rgb_from_u32(theme::ansi::CURSOR)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mislinked libghostty-vt takes the first non-ASCII byte down with it,
    /// which a suite that only ever writes ASCII would not notice.
    #[test]
    fn vt_write_accepts_multibyte_text() {
        let mut terminal = Terminal::new(TerminalOptions {
            cols: INITIAL_COLS,
            rows: INITIAL_ROWS,
            max_scrollback: MAX_SCROLLBACK_BYTES,
        })
        .unwrap();
        apply_theme(&mut terminal).unwrap();

        terminal.vt_write("新機能と improvements と émoji 🎌\r\n".as_bytes());

        let mut render = RenderState::new().unwrap();
        let mut rows = RowIterator::new().unwrap();
        let mut cells = CellIterator::new().unwrap();
        let snapshot = render.update(&terminal).unwrap();

        let mut first = None;
        {
            let mut row_iteration = rows.update(&snapshot).unwrap();
            while let Some(row) = row_iteration.next() {
                let text = build_row(row, &mut cells, INITIAL_COLS).text();
                if first.is_none() {
                    first = Some(text);
                }
            }
        }

        assert!(
            first.as_deref().unwrap_or_default().contains("新機能"),
            "expected the written text back, got {first:?}"
        );
    }
}
