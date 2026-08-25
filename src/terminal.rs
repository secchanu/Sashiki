//! Terminal emulator built on libghostty-vt.
//!
//! ## Module structure
//! - `vt`: terminal state thread owning the libghostty-vt handles
//! - `pty`: pseudo terminal process management
//! - `frame`: owned viewport snapshot shared with the renderer
//! - `input`: translation from GPUI key events to libghostty-vt key events
//! - `view`: main TerminalView struct, mouse/IME handling, Render
//! - `keybindings`: action definitions, key bindings, action handlers
//! - `element`: TerminalElement for custom GPUI rendering

mod element;
mod frame;
mod input;
mod input_probe;
mod keybindings;
mod pty;
mod view;
mod vt;

pub use view::TerminalView;

use frame::Frame;
use std::sync::Mutex;
use vt::{MouseInput, VtCommand};

#[derive(Clone, Debug)]
pub enum TerminalEvent {
    /// A new frame is available.
    Wakeup,
    Bell,
    Exit,
    Title(String),
    /// The terminal application asked for text to be put on the clipboard.
    ClipboardWrite(String),
    /// Result of a selection copy request.
    Copy(String),
}

/// Handle to a running terminal. All operations are messages to the VT thread,
/// because libghostty-vt state cannot be touched from the UI thread.
pub struct Terminal {
    handle: vt::VtHandle,
    /// Last requested size, used to skip redundant resize messages.
    size: Mutex<(u16, u16)>,
    killer: Mutex<Box<dyn portable_pty::ChildKiller + Send + Sync>>,
}

impl Terminal {
    pub fn new(
        working_directory: Option<std::path::PathBuf>,
    ) -> anyhow::Result<(Self, smol::channel::Receiver<TerminalEvent>)> {
        // Buffer size 100 allows a burst of terminal events without blocking
        // the VT thread.
        let (event_tx, event_rx) = smol::channel::bounded(100);
        let handle = vt::spawn(working_directory, event_tx)?;
        let killer = Mutex::new(handle.killer.clone_killer());

        Ok((
            Self {
                handle,
                size: Mutex::new((0, 0)),
                killer,
            },
            event_rx,
        ))
    }

    fn send(&self, command: VtCommand) {
        // A closed channel means the terminal already exited.
        let _ = self.handle.commands.try_send(command);
    }

    /// Take the most recent frame, if one was published since the last call.
    pub(super) fn take_frame(&self) -> Option<Frame> {
        self.handle.frame.lock().ok()?.take()
    }

    pub fn write(&self, data: &[u8]) {
        self.send(VtCommand::Write(data.to_vec()));
    }

    pub(super) fn key(&self, input: vt::KeyInput) {
        self.send(VtCommand::Key(input));
    }

    pub(super) fn paste(&self, text: String) {
        self.send(VtCommand::Paste(text));
    }

    pub(super) fn mouse_down(&self, input: MouseInput, click_count: u8) {
        self.send(VtCommand::MouseDown { input, click_count });
    }

    pub(super) fn mouse_drag(&self, input: MouseInput) {
        self.send(VtCommand::MouseDrag(input));
    }

    pub(super) fn mouse_up(&self, input: MouseInput) {
        self.send(VtCommand::MouseUp(input));
    }

    pub(super) fn mouse_move(&self, input: MouseInput) {
        self.send(VtCommand::MouseMove(input));
    }

    pub(super) fn scroll(&self, input: MouseInput, lines: i32) {
        self.send(VtCommand::Scroll { input, lines });
    }

    pub(super) fn scroll_lines(&self, lines: i32) {
        self.send(VtCommand::ScrollLines(lines));
    }

    pub(super) fn clear_selection(&self) {
        self.send(VtCommand::ClearSelection);
    }

    pub(super) fn copy_selection(&self) {
        self.send(VtCommand::CopySelection);
    }

    pub(super) fn set_focused(&self, focused: bool) {
        self.send(VtCommand::Focus(focused));
    }

    /// Resize the terminal grid. Redundant sizes are dropped so that layout
    /// passes do not flood the VT thread.
    pub fn resize(&self, cols: u16, rows: u16, cell_width: u16, cell_height: u16, padding: u16) {
        {
            let Ok(mut size) = self.size.lock() else {
                return;
            };
            if *size == (cols, rows) {
                return;
            }
            *size = (cols, rows);
        }

        self.send(VtCommand::Resize {
            cols,
            rows,
            cell_width,
            cell_height,
            padding,
        });
    }

    /// Terminate the shell and release the pty.
    ///
    /// The child is killed on the calling thread so that its handles are gone
    /// by the time the caller removes the working directory, which Windows
    /// requires before a directory can be deleted.
    pub fn shutdown(&self) {
        if let Ok(mut killer) = self.killer.lock() {
            let _ = killer.kill();
        }
        self.send(VtCommand::Shutdown);
    }
}
