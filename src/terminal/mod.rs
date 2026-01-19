//! Terminal emulator using alacritty_terminal
//!
//! This module provides terminal functionality integrated with GPUI.

mod view;
pub use view::TerminalView;

use alacritty_terminal::event::{Event as AlacEvent, EventListener, Notify, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Notifier};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config as TermConfig, Term};
use alacritty_terminal::tty;
use std::sync::Arc;

pub struct Terminal {
    term: Arc<FairMutex<Term<TerminalEventListener>>>,
    pty_tx: Notifier,
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
    pub fn new(working_directory: Option<std::path::PathBuf>) -> anyhow::Result<(Self, smol::channel::Receiver<TerminalEvent>)> {
        let (event_tx, event_rx) = smol::channel::bounded(100);
        let listener = TerminalEventListener { sender: event_tx };

        let config = TermConfig::default();
        // Standard terminal size (80 columns x 24 lines)
        let term_size = TermSize::new(80, 24);
        let term = Term::new(config, &term_size, listener.clone());
        let term = Arc::new(FairMutex::new(term));

        let pty_config = tty::Options {
            shell: None,
            working_directory,
            env: std::collections::HashMap::new(),
            ..Default::default()
        };

        // Cell dimensions are approximate; actual rendering uses font metrics
        let window_size = WindowSize {
            num_lines: 24,
            num_cols: 80,
            cell_width: 10,
            cell_height: 20,
        };

        // window_id parameter (0) is unused on Windows
        let pty = tty::new(&pty_config, window_size, 0)?;

        let event_loop = EventLoop::new(
            term.clone(),
            listener,
            pty,
            pty_config.drain_on_exit,
            false,
        )?;

        let pty_tx = Notifier(event_loop.channel());
        // Thread handle intentionally dropped - PTY thread runs until Terminal is dropped
        // and channel closes, at which point it exits naturally
        let _pty_thread = event_loop.spawn();

        Ok((
            Self {
                term,
                pty_tx,
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

    pub fn with_term<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Term<TerminalEventListener>) -> R,
    {
        let term = self.term.lock();
        f(&term)
    }
}
