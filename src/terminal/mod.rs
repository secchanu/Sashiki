//! Terminal emulator using alacritty_terminal
//!
//! This module provides terminal functionality integrated with GPUI.
//!
//! ## Module structure
//! - `view`: Main TerminalView struct, initialization, mouse/IME handling, Render
//! - `keybindings`: Action definitions, key bindings, action handlers
//! - `element`: TerminalElement for custom GPUI rendering

mod element;
mod keybindings;
mod view;

pub use view::TerminalView;

use alacritty_terminal::event::{Event as AlacEvent, EventListener, Notify, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Msg, Notifier};
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config as TermConfig, Term};
use alacritty_terminal::tty;
use std::sync::Arc;

pub struct Terminal {
    term: Arc<FairMutex<Term<TerminalEventListener>>>,
    pty_tx: Notifier,
    /// Current terminal size (cols, lines) for deduplication
    current_size: std::sync::Mutex<(u16, u16)>,
}

#[derive(Clone)]
pub struct TerminalEventListener {
    sender: smol::channel::Sender<TerminalEvent>,
}

impl EventListener for TerminalEventListener {
    fn send_event(&self, event: AlacEvent) {
        let terminal_event = match event {
            AlacEvent::Wakeup => TerminalEvent::Wakeup,
            AlacEvent::Bell => TerminalEvent::Bell,
            AlacEvent::Exit => TerminalEvent::Exit,
            AlacEvent::Title(_) => TerminalEvent::Title,
            _ => return,
        };
        // Ignore send failure - channel full or receiver dropped is non-fatal
        let _ = self.sender.try_send(terminal_event);
    }
}

#[derive(Debug, Clone)]
pub enum TerminalEvent {
    Wakeup,
    Bell,
    Exit,
    Title,
}

impl Terminal {
    pub fn new(
        working_directory: Option<std::path::PathBuf>,
    ) -> anyhow::Result<(Self, smol::channel::Receiver<TerminalEvent>)> {
        // Buffer size 100 allows burst of terminal events without blocking PTY thread
        let (event_tx, event_rx) = smol::channel::bounded(100);
        let listener = TerminalEventListener { sender: event_tx };

        let config = TermConfig::default();
        // 80x24 is the VT100 standard terminal size, used as initial default
        let term_size = TermSize::new(80, 24);
        let term = Term::new(config, &term_size, listener.clone());
        let term = Arc::new(FairMutex::new(term));

        let pty_config = tty::Options {
            shell: None,
            working_directory,
            env: std::collections::HashMap::new(),
            ..Default::default()
        };

        // Initial PTY window size. Cell dimensions (10x20) are placeholder values;
        // actual rendering calculates precise dimensions from font metrics.
        // These values are used by the PTY for initial SIGWINCH reporting.
        let window_size = WindowSize {
            num_lines: 24,
            num_cols: 80,
            cell_width: 10,
            cell_height: 20,
        };

        // window_id parameter (0) is unused on Windows
        let pty = tty::new(&pty_config, window_size, 0)?;

        let event_loop =
            EventLoop::new(term.clone(), listener, pty, pty_config.drain_on_exit, false)?;

        let pty_tx = Notifier(event_loop.channel());
        // Thread handle intentionally dropped - PTY thread runs until Terminal is dropped
        // and channel closes, at which point it exits naturally
        let _pty_thread = event_loop.spawn();

        Ok((
            Self {
                term,
                pty_tx,
                current_size: std::sync::Mutex::new((80, 24)),
            },
            event_rx,
        ))
    }

    pub fn write(&self, input: &[u8]) {
        self.pty_tx.notify(input.to_vec());
    }

    /// Send exit command to the shell to terminate the PTY process
    pub fn shutdown(&self) {
        // Send "exit" command to terminate the shell
        // This works for cmd.exe, powershell, bash, etc.
        self.pty_tx.notify(b"exit\r".to_vec());
    }

    /// Resize the terminal to new dimensions
    pub fn resize(&self, cols: u16, lines: u16, cell_width: u16, cell_height: u16) {
        // Check if size actually changed
        {
            let Ok(mut current) = self.current_size.lock() else {
                eprintln!("Warning: Terminal size mutex poisoned, skipping resize");
                return;
            };
            if current.0 == cols && current.1 == lines {
                return;
            }
            *current = (cols, lines);
        }

        let size = WindowSize {
            num_cols: cols,
            num_lines: lines,
            cell_width,
            cell_height,
        };

        // Resize the terminal grid
        {
            let mut term = self.term.lock();
            term.resize(TermSize::new(cols as usize, lines as usize));
        }

        // Notify PTY of size change
        let _ = self.pty_tx.0.send(Msg::Resize(size));
    }

    /// Scroll the terminal viewport
    pub fn scroll(&self, scroll: Scroll) {
        let mut term = self.term.lock();
        term.scroll_display(scroll);
    }

    pub fn with_term<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Term<TerminalEventListener>) -> R,
    {
        let term = self.term.lock();
        f(&term)
    }
}
