//! Terminal view for GPUI rendering
//!
//! This module provides the main TerminalView struct and its implementation.

use super::Terminal;
use super::element::{
    DEFAULT_CELL_HEIGHT, DEFAULT_CELL_WIDTH, MULTI_CLICK_THRESHOLD_MS, SCROLL_LINES_WHEEL,
    TERMINAL_PADDING, TerminalElement, TerminalLayout, rgb_to_hsla,
};
use super::frame::{CellWidth, Frame};
use super::input;
use super::input_probe;
use super::vt::MouseInput;
use crate::terminal::TerminalEvent;
use crate::theme::*;
use gpui::prelude::FluentBuilder;
use gpui::{
    App, AsyncApp, Bounds, ClipboardItem, Context, EntityInputHandler, FocusHandle, Focusable,
    InteractiveElement, IntoElement, Modifiers, MouseButton, MouseMoveEvent, ParentElement, Pixels,
    Point, Render, ScrollWheelEvent, Size, Styled, Subscription, UTF16Selection, WeakEntity,
    Window, div, px, rgb,
};
use libghostty_vt::mouse;
use regex::Regex;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

static URL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https?://[^\s\x00-\x1f\x7f<>"'\)\]]+"#).unwrap());

/// A URL detected in the terminal output.
/// Cell-to-URL mapping is stored separately in `TerminalView::url_cells`.
#[derive(Clone, Debug)]
pub(super) struct DetectedUrl {
    pub url: String,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PendingCtrlCState {
    pub armed_at: Instant,
    pub rapid_tap_detected: bool,
    pub ctrl_c_released: bool,
}

#[derive(Clone, Debug)]
struct PendingAccessibilityCommit {
    text: String,
    armed_at: Instant,
}

pub struct TerminalView {
    pub(super) terminal: Option<Arc<Terminal>>,
    pub(super) focus_handle: FocusHandle,
    pub(super) preedit_text: String,
    /// Error message if terminal creation failed
    error_message: Option<String>,
    /// Window title set by terminal application (via OSC 2)
    pub title: Option<String>,
    /// Whether bell was recently triggered (for UI visual feedback)
    pub bell_active: bool,
    /// Most recent viewport snapshot published by the terminal thread
    frame: Option<Frame>,
    /// Whether mouse is currently dragging for selection
    is_dragging: bool,
    /// Last click time for double/triple click detection
    last_click_time: Option<Instant>,
    /// Click count for multi-click detection
    click_count: u8,
    /// Cell dimensions for mouse position to cell conversion
    pub(super) cell_width: f32,
    pub(super) cell_height: f32,
    /// Terminal content origin for mouse coordinate conversion
    pub(super) content_origin: (f32, f32),
    /// URLs detected in the current terminal content
    pub(super) detected_urls: Vec<DetectedUrl>,
    /// (screen_line, col) → detected_urls index for O(1) URL lookup during layout build.
    /// Wrapped in Arc so build_layout can share without deep-copying the map every frame.
    pub(super) url_cells: Arc<HashMap<(usize, usize), usize>>,
    /// Index of the URL currently hovered with Ctrl held
    pub(super) hovered_url_index: Option<usize>,
    /// Whether Ctrl key is currently held (for lazy URL detection)
    pub(super) ctrl_held: bool,
    /// Pending Ctrl+C intent resolution.
    /// Ctrl+C is held until we can disambiguate:
    /// - normal manual Ctrl+C -> flush as SIGINT
    /// - automation Ctrl+C->paste chain -> canceled by paste
    pub(super) pending_ctrl_c: Option<PendingCtrlCState>,
    /// Caret bounds for platform text-input integrations (IME/voice tools).
    pub(super) input_cursor_bounds: Option<Bounds<Pixels>>,
    /// Lightweight text shadow exposed through InputHandler APIs.
    /// This is not terminal history; it only reflects recent committed input.
    input_shadow_text: String,
    /// Recently committed UTF-16 range in `input_shadow_text`.
    /// Some automation tools query selected range to verify paste success.
    recent_committed_range: Option<(Range<usize>, Instant)>,
    /// Pending accessibility text commit to confirm against terminal output.
    pending_accessibility_commit: Option<PendingAccessibilityCommit>,
    /// Focus-in subscription (kept alive for focus reporting)
    focus_in_subscription: Option<Subscription>,
    /// Focus-out subscription (kept alive for focus reporting)
    focus_out_subscription: Option<Subscription>,
}

impl TerminalView {
    pub(super) fn input_trace_enabled() -> bool {
        static ENABLED: LazyLock<bool> = LazyLock::new(|| {
            std::env::var("SASHIKI_TERMINAL_INPUT_TRACE")
                .map(|v| v != "0")
                .unwrap_or(false)
        });
        *ENABLED
    }

    fn input_trace_verbose_enabled() -> bool {
        static ENABLED: LazyLock<bool> = LazyLock::new(|| {
            std::env::var("SASHIKI_TERMINAL_INPUT_TRACE_VERBOSE")
                .map(|v| v != "0")
                .unwrap_or(false)
        });
        *ENABLED
    }

    fn should_emit_input_trace(message: &str) -> bool {
        if Self::input_trace_verbose_enabled() {
            return true;
        }
        message.starts_with("action ctrl-c")
            || message.starts_with("action ctrl-v")
            || message.starts_with("paste_text called")
            || message.starts_with("drop accessibility_commit timeout")
    }

    pub(super) fn trace_input_event(message: impl AsRef<str>) {
        let message = message.as_ref();
        if Self::input_trace_enabled() && Self::should_emit_input_trace(message) {
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            eprintln!("[terminal-input {ms}] {}", message);
        }
    }

    /// Create a new terminal with a specific working directory
    pub fn new_with_directory(
        working_directory: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_internal(Some(working_directory), cx)
    }

    fn new_internal(working_directory: Option<std::path::PathBuf>, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        match Terminal::new(working_directory) {
            Ok((terminal, event_rx)) => {
                let terminal = Arc::new(terminal);

                // Event-based refresh: batch process all pending events before updating
                // This prevents catching intermediate states during rapid event sequences
                cx.spawn(
                    async move |this: WeakEntity<TerminalView>, cx: &mut AsyncApp| {
                        while let Ok(event) = event_rx.recv().await {
                            // Collect all pending events first
                            let mut events = vec![event];
                            while let Ok(e) = event_rx.try_recv() {
                                events.push(e);
                            }

                            let should_break = cx.update(|cx| {
                                if let Some(this) = this.upgrade() {
                                    this.update(cx, move |view, cx: &mut Context<TerminalView>| {
                                        let mut need_notify = false;

                                        for event in events {
                                            match event {
                                                TerminalEvent::Wakeup => {
                                                    view.take_frame();
                                                    need_notify = true;
                                                }
                                                TerminalEvent::Bell => {
                                                    view.bell_active = true;
                                                    need_notify = true;
                                                }
                                                TerminalEvent::Exit => {
                                                    // Terminal exited - handled elsewhere.
                                                    view.take_frame();
                                                    need_notify = true;
                                                }
                                                TerminalEvent::Title(title) => {
                                                    view.title = Some(title);
                                                    need_notify = true;
                                                }
                                                TerminalEvent::ClipboardWrite(text)
                                                | TerminalEvent::Copy(text) => {
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(text),
                                                    );
                                                }
                                            }
                                        }

                                        if need_notify {
                                            cx.notify();
                                        }
                                    });
                                    false
                                } else {
                                    true
                                }
                            });
                            if should_break {
                                break;
                            }
                        }
                    },
                )
                .detach();

                Self::with_terminal(Some(terminal), focus_handle, None)
            }
            Err(e) => Self::with_terminal(
                None,
                focus_handle,
                Some(format!("Failed to create terminal: {}", e)),
            ),
        }
    }

    fn with_terminal(
        terminal: Option<Arc<Terminal>>,
        focus_handle: FocusHandle,
        error_message: Option<String>,
    ) -> Self {
        Self {
            terminal,
            focus_handle,
            preedit_text: String::new(),
            error_message,
            title: None,
            bell_active: false,
            frame: None,
            is_dragging: false,
            last_click_time: None,
            click_count: 0,
            cell_width: DEFAULT_CELL_WIDTH,
            cell_height: DEFAULT_CELL_HEIGHT,
            content_origin: (0.0, 0.0),
            detected_urls: Vec::new(),
            url_cells: Arc::new(HashMap::new()),
            hovered_url_index: None,
            ctrl_held: false,
            pending_ctrl_c: None,
            input_cursor_bounds: None,
            input_shadow_text: String::new(),
            recent_committed_range: None,
            pending_accessibility_commit: None,
            focus_in_subscription: None,
            focus_out_subscription: None,
        }
    }

    /// Shutdown the terminal by terminating the shell
    pub fn shutdown(&self) {
        if let Some(ref terminal) = self.terminal {
            terminal.shutdown();
        }
    }

    /// Write text to the terminal (for pasting from file view)
    pub fn write_text(&self, text: &str) {
        self.write_to_terminal(text.as_bytes());
    }

    /// Write bytes to the terminal (used by action handlers)
    pub(super) fn write_to_terminal(&self, data: &[u8]) {
        if let Some(ref terminal) = self.terminal {
            terminal.write(data);
        }
    }

    /// Send a translated key event to the terminal.
    pub(super) fn send_key(&self, keystroke: &gpui::Keystroke) -> bool {
        let Some(ref terminal) = self.terminal else {
            return false;
        };
        let Some(input) = input::key_input(keystroke, !self.preedit_text.is_empty()) else {
            return false;
        };
        terminal.key(input);
        true
    }

    /// Paste text into the terminal. Bracketed paste and control byte
    /// stripping are handled by libghostty-vt.
    pub(super) fn paste_text(&mut self, text: &str) {
        Self::trace_input_event(format!("paste_text called len={}", text.len()));
        self.cancel_pending_ctrl_c();

        if let Some(ref terminal) = self.terminal {
            terminal.paste(text.to_string());
        }
        self.update_input_shadow_text(text);
        self.arm_accessibility_commit(text);
    }

    pub(super) fn is_alt_screen(&self) -> bool {
        self.frame
            .as_ref()
            .map(|frame| frame.alt_screen)
            .unwrap_or(false)
    }

    pub(super) fn has_selection(&self) -> bool {
        self.frame
            .as_ref()
            .map(|frame| frame.has_selection)
            .unwrap_or(false)
    }

    fn mouse_tracking(&self) -> bool {
        self.frame
            .as_ref()
            .map(|frame| frame.mouse_tracking)
            .unwrap_or(false)
    }

    /// Copy the current selection to the clipboard. The text arrives
    /// asynchronously as `TerminalEvent::Copy`.
    pub(super) fn copy_selection(&self) {
        if let Some(ref terminal) = self.terminal {
            terminal.copy_selection();
        }
    }

    pub(super) fn clear_selection(&self) {
        if let Some(ref terminal) = self.terminal {
            terminal.clear_selection();
        }
    }

    /// Number of lines to scroll per page (Shift+PageUp/Down).
    /// Uses current screen height minus 1 (standard terminal behavior),
    /// falling back to 10 lines if no frame has arrived yet.
    pub(super) fn page_scroll_lines(&self) -> i32 {
        self.frame
            .as_ref()
            .map(|frame| (frame.line_count() as i32).saturating_sub(1).max(1))
            .unwrap_or(10)
    }

    pub(super) fn scroll_lines(&self, lines: i32) {
        if let Some(ref terminal) = self.terminal {
            terminal.scroll_lines(lines);
        }
    }

    /// Adopt the newest frame published by the terminal thread.
    fn take_frame(&mut self) {
        let Some(ref terminal) = self.terminal else {
            return;
        };
        let Some(frame) = terminal.take_frame() else {
            return;
        };
        self.frame = Some(frame);

        // Only run URL detection when Ctrl is held; URLs are only needed for Ctrl+click.
        // During AI streaming this avoids scanning all terminal rows on every wakeup.
        if self.ctrl_held {
            self.detect_urls_from_frame();
        }
        self.resolve_pending_accessibility_commit();
    }

    fn arm_accessibility_commit(&mut self, text: &str) {
        let sanitized: String = text
            .chars()
            .filter(|c| *c != '\0' && *c != '\x1b')
            .collect::<String>()
            .trim()
            .to_string();
        if sanitized.is_empty() {
            return;
        }

        Self::trace_input_event(format!(
            "arm accessibility_commit len={} text_preview={:?}",
            sanitized.chars().count(),
            sanitized.chars().take(16).collect::<String>()
        ));

        self.pending_accessibility_commit = Some(PendingAccessibilityCommit {
            text: sanitized.clone(),
            armed_at: Instant::now(),
        });
        // Keep compatibility with clients that expect immediate accessibility updates,
        // then send another notification once terminal output confirms the commit.
        input_probe::notify_accessibility_text_committed(&sanitized);
        Self::trace_input_event("accessibility_commit immediate notify");
    }

    fn resolve_pending_accessibility_commit(&mut self) {
        const ACCESSIBILITY_COMMIT_TIMEOUT: Duration = Duration::from_secs(3);

        let Some(pending) = self.pending_accessibility_commit.clone() else {
            return;
        };

        if self.visible_text_contains(&pending.text) {
            Self::trace_input_event(format!(
                "confirm accessibility_commit age_ms={} len={}",
                pending.armed_at.elapsed().as_millis(),
                pending.text.chars().count()
            ));
            input_probe::notify_accessibility_text_committed(&pending.text);
            self.pending_accessibility_commit = None;
            return;
        }

        if pending.armed_at.elapsed() >= ACCESSIBILITY_COMMIT_TIMEOUT {
            Self::trace_input_event(format!(
                "drop accessibility_commit timeout age_ms={} len={}",
                pending.armed_at.elapsed().as_millis(),
                pending.text.chars().count()
            ));
            self.pending_accessibility_commit = None;
        }
    }

    fn visible_text_contains(&self, needle: &str) -> bool {
        let Some(frame) = self.frame.as_ref() else {
            return false;
        };
        if needle.is_empty() {
            return false;
        }

        frame.rows.iter().any(|row| row.text().contains(needle))
    }

    fn update_input_shadow_text(&mut self, text: &str) {
        let sanitized: String = text.chars().filter(|c| *c != '\0').collect();
        self.input_shadow_text = sanitized;
        let utf16_len = self.input_shadow_text.encode_utf16().count();
        self.recent_committed_range = if utf16_len > 0 {
            Some((0..utf16_len, Instant::now()))
        } else {
            None
        };
        Self::trace_input_event(format!("input_shadow_text updated utf16_len={utf16_len}"));
    }

    fn slice_utf16(text: &str, range: Range<usize>) -> String {
        if text.is_empty() || range.start >= range.end {
            return String::new();
        }

        let mut utf16_offset = 0usize;
        let mut byte_start: Option<usize> = None;
        let mut byte_end: Option<usize> = None;

        for (idx, ch) in text.char_indices() {
            let width = ch.len_utf16();
            let next = utf16_offset + width;

            if byte_start.is_none() && range.start <= utf16_offset {
                byte_start = Some(idx);
            }
            if byte_end.is_none() && range.end <= utf16_offset {
                byte_end = Some(idx);
                break;
            }
            if byte_start.is_none() && range.start < next {
                byte_start = Some(idx);
            }
            if byte_end.is_none() && range.end <= next {
                byte_end = Some(idx + ch.len_utf8());
                break;
            }

            utf16_offset = next;
        }

        let start = byte_start.unwrap_or(text.len());
        let end = byte_end.unwrap_or(text.len()).max(start);
        text[start..end].to_string()
    }

    /// Scan the current frame for URLs and record their viewport positions.
    pub(super) fn detect_urls_from_frame(&mut self) {
        self.detected_urls.clear();

        let Some(frame) = self.frame.as_ref() else {
            self.url_cells = Arc::new(HashMap::new());
            return;
        };

        let mut url_cells = HashMap::new();
        let mut detected = Vec::new();

        for (line_idx, row) in frame.rows.iter().enumerate() {
            // Column indices must line up with the rendered grid, so spacer
            // cells are kept as placeholders rather than dropped.
            let line_text: String = row
                .cells
                .iter()
                .map(|cell| match cell.width {
                    CellWidth::Spacer => ' ',
                    _ => cell.ch,
                })
                .collect();

            for mat in URL_REGEX.find_iter(&line_text) {
                // Strip trailing punctuation that is commonly not part of URLs
                // (e.g. "Visit https://example.com." should not include the period)
                let url_str = mat
                    .as_str()
                    .trim_end_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?'));
                if url_str.len() <= "https://".len() {
                    continue;
                }

                // Convert byte offsets to column indices. Each char maps 1:1 to
                // a column because the line is built cell by cell.
                let start_col = line_text[..mat.start()].chars().count();
                let end_col = start_col + url_str.chars().count() - 1;

                let url_idx = detected.len();
                detected.push(DetectedUrl {
                    url: url_str.to_string(),
                });

                // Build O(1) cell lookup: store every (line, col) that belongs to this URL.
                // URLs are always single-line (detected per-line), so start.0 == end.0.
                for col in start_col..=end_col {
                    url_cells.insert((line_idx, col), url_idx);
                }
            }
        }

        self.detected_urls = detected;
        self.url_cells = Arc::new(url_cells);
    }

    // ========================================================================
    // Mouse handling
    // ========================================================================

    /// Convert a window position to a position relative to the terminal content,
    /// which is what the mouse encoder expects.
    fn position_to_element(&self, x: f32, y: f32) -> (f32, f32) {
        (
            (x - self.content_origin.0).max(0.0),
            (y - self.content_origin.1).max(0.0),
        )
    }

    /// Convert mouse position (window coordinates) to cell coordinates
    fn position_to_cell(&self, x: f32, y: f32) -> (usize, usize) {
        let (x, y) = self.position_to_element(x, y);
        let x = (x - TERMINAL_PADDING).max(0.0);
        let y = (y - TERMINAL_PADDING).max(0.0);
        let col = (x / self.cell_width.max(1.0)) as usize;
        let line = (y / self.cell_height.max(1.0)) as usize;
        (line, col)
    }

    fn mouse_input(
        &self,
        x: f32,
        y: f32,
        button: mouse::Button,
        modifiers: &Modifiers,
    ) -> MouseInput {
        let (x, y) = self.position_to_element(x, y);
        MouseInput {
            x,
            y,
            button,
            mods: input::mods_from_gpui(modifiers),
        }
    }

    /// Handle Ctrl+click to open a URL under the cursor.
    /// Returns true if a URL was opened (so the caller can skip selection logic).
    fn try_open_url_at(&self, screen_line: usize, col: usize) -> bool {
        if let Some(&url_idx) = self.url_cells.get(&(screen_line, col)) {
            if let Some(url) = self.detected_urls.get(url_idx) {
                let _ = open::that(&url.url);
                return true;
            }
        }
        false
    }

    /// Update hovered URL index based on current mouse position and Ctrl state.
    fn update_hovered_url(&mut self, screen_line: usize, col: usize, ctrl: bool) {
        if ctrl {
            // If URL detection hasn't run yet (e.g. Ctrl was held before the window
            // received a modifiers-changed event), scan now as a fallback.
            if !self.ctrl_held {
                self.ctrl_held = true;
                self.detect_urls_from_frame();
            }
            self.hovered_url_index = self.url_cells.get(&(screen_line, col)).copied();
        } else {
            self.hovered_url_index = None;
        }
    }

    /// Handle mouse down event for selection
    fn handle_mouse_down(&mut self, x: f32, y: f32, modifiers: &Modifiers, cx: &mut Context<Self>) {
        let (screen_line, col) = self.position_to_cell(x, y);

        // Ctrl+click opens the URL under the cursor
        if modifiers.control && self.try_open_url_at(screen_line, col) {
            return;
        }

        let now = Instant::now();
        let is_multi_click = self
            .last_click_time
            .map(|t| now.duration_since(t).as_millis() < MULTI_CLICK_THRESHOLD_MS)
            .unwrap_or(false);

        if is_multi_click {
            self.click_count = (self.click_count % 3) + 1;
        } else {
            self.click_count = 1;
        }
        self.last_click_time = Some(now);
        self.is_dragging = true;

        if let Some(ref terminal) = self.terminal {
            let input = self.mouse_input(x, y, mouse::Button::Left, modifiers);
            terminal.mouse_down(input, self.click_count);
        }

        cx.notify();
    }

    /// Handle mouse drag event for selection
    fn handle_mouse_drag(&mut self, x: f32, y: f32, modifiers: &Modifiers) {
        if !self.is_dragging {
            return;
        }
        if let Some(ref terminal) = self.terminal {
            let input = self.mouse_input(x, y, mouse::Button::Left, modifiers);
            terminal.mouse_drag(input);
        }
    }

    /// Handle mouse up event
    fn handle_mouse_up(&mut self, x: f32, y: f32, modifiers: &Modifiers) {
        self.is_dragging = false;
        if let Some(ref terminal) = self.terminal {
            let input = self.mouse_input(x, y, mouse::Button::Left, modifiers);
            terminal.mouse_up(input);
        }
    }

    /// Handle scroll wheel event
    fn handle_scroll(&mut self, x: f32, y: f32, delta_y: f32, modifiers: &Modifiers) {
        // GPUI scroll: positive delta_y = wheel up = scroll back in history
        let lines = if delta_y > 0.0 {
            SCROLL_LINES_WHEEL
        } else {
            -SCROLL_LINES_WHEEL
        };
        if let Some(ref terminal) = self.terminal {
            let input = self.mouse_input(x, y, mouse::Button::Left, modifiers);
            terminal.scroll(input, lines);
        }
    }

    // ========================================================================
    // Layout building
    // ========================================================================

    /// Build terminal layout for the paint phase.
    /// Shares row data from the frame via Arc::clone (O(1) instead of O(rows×cols)).
    pub(super) fn build_layout(
        &self,
        cell_width: Pixels,
        line_height: Pixels,
    ) -> Option<TerminalLayout> {
        let frame = self.frame.as_ref()?;
        Some(TerminalLayout {
            rows: Arc::clone(&frame.rows),
            cell_width,
            line_height,
            cursor: frame.cursor,
            foreground: rgb_to_hsla(frame.foreground),
            background: rgb_to_hsla(frame.background),
            cursor_color: rgb_to_hsla(frame.cursor_color),
            url_cells: Arc::clone(&self.url_cells),
            hovered_url_index: self.hovered_url_index,
            preedit_text: self.preedit_text.clone(),
        })
    }

    /// Compute caret bounds used by IME/text-input integrations.
    ///
    /// This uses cursor coordinates from the current frame, independent of cursor
    /// visibility/blink state, so platform integrations always have a stable caret anchor.
    pub(super) fn compute_input_cursor_bounds(
        &self,
        origin: Point<Pixels>,
        cell_width: Pixels,
        line_height: Pixels,
    ) -> Option<Bounds<Pixels>> {
        let frame = self.frame.as_ref()?;
        let cursor = frame.cursor?;
        let row = frame.rows.get(cursor.y as usize)?;
        let cell = row.cells.get(cursor.x as usize)?;

        let width = if cell.width == CellWidth::Wide {
            cell_width * 2.0
        } else {
            cell_width
        };

        Some(Bounds::new(
            Point::new(
                origin.x + cell_width * cursor.x as usize,
                origin.y + line_height * cursor.y as usize,
            ),
            Size {
                width,
                height: line_height,
            },
        ))
    }
}

// ============================================================================
// Trait implementations
// ============================================================================

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// IME input handler for terminal
impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let in_alt_screen = self.is_alt_screen();
        Self::trace_input_event(format!(
            "text_for_range req={:?} in_alt_screen={}",
            range_utf16, in_alt_screen
        ));
        if in_alt_screen {
            return None;
        }
        let total = self.input_shadow_text.encode_utf16().count();
        let start = range_utf16.start.min(total);
        let end = range_utf16.end.min(total);
        *actual_range = Some(start..end);
        let text = Self::slice_utf16(&self.input_shadow_text, start..end);
        Self::trace_input_event(format!(
            "text_for_range resp_len={} total_utf16={}",
            text.encode_utf16().count(),
            total
        ));
        Some(text)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        const RECENT_COMMIT_SELECTION_TTL: Duration = Duration::from_secs(4);
        let in_alt_screen = self.is_alt_screen();
        Self::trace_input_event(format!("selected_text_range in_alt_screen={in_alt_screen}"));
        if in_alt_screen {
            None
        } else {
            if let Some((range, at)) = self.recent_committed_range.as_ref()
                && at.elapsed() <= RECENT_COMMIT_SELECTION_TTL
                && !range.is_empty()
            {
                Self::trace_input_event(format!(
                    "commit_probe selected_range={}..{} age_ms={}",
                    range.start,
                    range.end,
                    at.elapsed().as_millis()
                ));
                return Some(UTF16Selection {
                    range: range.clone(),
                    reversed: false,
                });
            }
            let len = self.input_shadow_text.encode_utf16().count();
            Some(UTF16Selection {
                range: len..len,
                reversed: false,
            })
        }
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        if self.preedit_text.is_empty() {
            None
        } else {
            Some(0..self.preedit_text.encode_utf16().count())
        }
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.preedit_text.clear();
    }

    fn replace_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Clear preedit and send committed text to terminal
        self.preedit_text.clear();
        if !text.is_empty() {
            Self::trace_input_event(format!("replace_text_in_range len={}", text.len()));
            self.cancel_pending_ctrl_c();
            self.write_to_terminal(text.as_bytes());
            self.update_input_shadow_text(text);
            self.arm_accessibility_commit(text);
            window.invalidate_character_coordinates();
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Update preedit text (IME composing state)
        self.preedit_text = new_text.to_string();
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let mut caret = self.input_cursor_bounds?;
        if range_utf16.start > 0 {
            caret.origin.x += px(self.cell_width * range_utf16.start as f32);
        }
        let x: f32 = caret.origin.x.into();
        let y: f32 = caret.origin.y.into();
        let w: f32 = caret.size.width.into();
        let h: f32 = caret.size.height.into();
        Self::trace_input_event(format!(
            "bounds_for_range start={} -> ({:.1},{:.1},{:.1},{:.1})",
            range_utf16.start, x, y, w, h
        ));
        Some(caret)
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }

    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        Self::trace_input_event("accepts_text_input=true");
        true
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_in_subscription.is_none() {
            let focus_handle = self.focus_handle.clone();
            self.focus_in_subscription =
                Some(cx.on_focus_in(&focus_handle, window, |this, _window, _cx| {
                    if let Some(ref terminal) = this.terminal {
                        terminal.set_focused(true);
                    }
                }));
        }
        if self.focus_out_subscription.is_none() {
            let focus_handle = self.focus_handle.clone();
            self.focus_out_subscription =
                Some(
                    cx.on_focus_out(&focus_handle, window, |this, _event, _window, _cx| {
                        if let Some(ref terminal) = this.terminal {
                            terminal.set_focused(false);
                        }
                        // Reset modifier tracking so ctrl_held doesn't get stuck when
                        // the window loses focus (no ModifiersChangedEvent is sent).
                        if this.ctrl_held {
                            this.ctrl_held = false;
                            this.detected_urls.clear();
                            this.url_cells = Arc::new(HashMap::new());
                            this.hovered_url_index = None;
                        }
                    }),
                );
        }

        // Show error message if terminal creation failed
        if let Some(ref error) = self.error_message {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(BG_BASE))
                .child(div().text_color(rgb(RED)).child(error.clone()))
                .into_any_element();
        }

        // Outer div handles focus, key context, and events
        // Uses flex_col layout so children can use flex_1 to fill
        div()
            .id("terminal-view")
            .key_context("Terminal")
            .track_focus(&self.focus_handle)
            .flex_1()
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .when_else(
                self.hovered_url_index.is_some(),
                |d: gpui::Stateful<gpui::Div>| d.cursor_pointer(),
                |d: gpui::Stateful<gpui::Div>| d.cursor_text(),
            )
            .on_key_down(cx.listener(Self::on_terminal_key_down))
            .on_key_up(cx.listener(Self::on_terminal_key_up))
            .on_modifiers_changed(cx.listener(Self::on_terminal_modifiers_changed))
            // Actions the terminal handles itself instead of forwarding
            .on_action(cx.listener(Self::on_ctrl_c))
            .on_action(cx.listener(Self::on_ctrl_v))
            .on_action(cx.listener(Self::on_ctrl_shift_c))
            .on_action(cx.listener(Self::on_ctrl_shift_v))
            .on_action(cx.listener(Self::on_shift_insert))
            .on_action(cx.listener(Self::on_shift_page_up))
            .on_action(cx.listener(Self::on_shift_page_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    this.handle_mouse_down(x, y, &event.modifiers, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                let x: f32 = event.position.x.into();
                let y: f32 = event.position.y.into();

                if this.is_dragging {
                    this.handle_mouse_drag(x, y, &event.modifiers);
                } else if this.mouse_tracking() {
                    if let Some(ref terminal) = this.terminal {
                        let input = this.mouse_input(x, y, mouse::Button::Left, &event.modifiers);
                        terminal.mouse_move(input);
                    }
                }

                // URL hover check only when Ctrl is held
                if event.modifiers.control {
                    let (screen_line, col) = this.position_to_cell(x, y);
                    let prev = this.hovered_url_index;
                    this.update_hovered_url(screen_line, col, true);
                    if this.hovered_url_index != prev {
                        cx.notify();
                    }
                } else if this.hovered_url_index.is_some() {
                    this.hovered_url_index = None;
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseUpEvent, _window, _cx| {
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    this.handle_mouse_up(x, y, &event.modifiers);
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _window, _cx| {
                let delta = event.delta.pixel_delta(Pixels::from(16.0));
                let y: f32 = delta.y.into();
                let position_x: f32 = event.position.x.into();
                let position_y: f32 = event.position.y.into();
                this.handle_scroll(position_x, position_y, y, &event.modifiers);
            }))
            .child(
                // Wrapper div as flex container for proper layout propagation
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .flex_col()
                    .bg(rgb(BG_BASE))
                    .child(TerminalElement::new(cx.entity())),
            )
            .into_any_element()
    }
}
