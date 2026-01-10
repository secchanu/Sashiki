//! Terminal emulation module
//!
//! Provides terminal functionality using portable-pty and vte.

mod state;

pub use state::TerminalState;

use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use thiserror::Error;
use vte::Parser;

#[derive(Error, Debug)]
pub enum TerminalError {
    #[error("Failed to create PTY: {0}")]
    PtyCreationError(String),
    #[error("Failed to spawn shell: {0}")]
    SpawnError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Terminal not running")]
    NotRunning,
}

#[derive(Debug, Clone)]
pub struct TerminalSize {
    pub rows: u16,
    pub cols: u16,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self { rows: 24, cols: 80 }
    }
}

impl From<TerminalSize> for PtySize {
    fn from(size: TerminalSize) -> Self {
        PtySize {
            rows: size.rows,
            cols: size.cols,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

pub struct Terminal {
    pty_pair: Option<PtyPair>,
    writer: Option<Box<dyn Write + Send>>,
    output_receiver: Option<Receiver<Vec<u8>>>,
    working_dir: PathBuf,
    shell_path: Option<String>,
    size: TerminalSize,
    running: bool,
    /// Terminal state with grid and cursor
    pub state: Arc<Mutex<TerminalState>>,
    /// VTE parser for processing output
    parser: Arc<Mutex<Parser>>,
}

impl Terminal {
    pub fn new(working_dir: impl AsRef<Path>, shell_path: Option<String>) -> Self {
        let size = TerminalSize::default();
        Self {
            pty_pair: None,
            writer: None,
            output_receiver: None,
            working_dir: working_dir.as_ref().to_path_buf(),
            shell_path,
            size: size.clone(),
            running: false,
            state: Arc::new(Mutex::new(TerminalState::new(
                size.rows as usize,
                size.cols as usize,
            ))),
            parser: Arc::new(Mutex::new(Parser::new())),
        }
    }

    pub fn start(&mut self) -> Result<(), TerminalError> {
        let pty_system = native_pty_system();

        let pair = pty_system
            .openpty(self.size.clone().into())
            .map_err(|e| TerminalError::PtyCreationError(e.to_string()))?;

        let shell = self.get_shell_command();
        let mut cmd = CommandBuilder::new(&shell);
        cmd.cwd(&self.working_dir);

        // Set environment variables
        #[cfg(windows)]
        {
            cmd.env("TERM", "xterm-256color");
        }
        #[cfg(not(windows))]
        {
            cmd.env("TERM", "xterm-256color");
            cmd.env("COLORTERM", "truecolor");
        }

        let _child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| TerminalError::SpawnError(e.to_string()))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| TerminalError::IoError(std::io::Error::other(e.to_string())))?;

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| TerminalError::IoError(std::io::Error::other(e.to_string())))?;

        let (tx, rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = mpsc::channel();

        // Spawn reader thread
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        if tx.send(data).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        self.pty_pair = Some(pair);
        self.writer = Some(writer);
        self.output_receiver = Some(rx);
        self.running = true;

        Ok(())
    }

    fn get_shell_command(&self) -> String {
        if let Some(ref shell) = self.shell_path {
            return shell.clone();
        }

        #[cfg(windows)]
        {
            // Try PowerShell first, then cmd
            if std::path::Path::new("C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe").exists() {
                "powershell.exe".to_string()
            } else {
                std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string())
            }
        }

        #[cfg(not(windows))]
        {
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
        }
    }

    pub fn write(&mut self, data: &[u8]) -> Result<(), TerminalError> {
        if let Some(ref mut writer) = self.writer {
            writer.write_all(data)?;
            writer.flush()?;
            Ok(())
        } else {
            Err(TerminalError::NotRunning)
        }
    }

    pub fn write_str(&mut self, s: &str) -> Result<(), TerminalError> {
        self.write(s.as_bytes())
    }

    /// Process output and update terminal state
    pub fn process_output(&mut self) {
        if let Some(ref receiver) = self.output_receiver {
            while let Ok(data) = receiver.try_recv() {
                // Parse through VTE and update state
                if let (Ok(mut state), Ok(mut parser)) =
                    (self.state.lock(), self.parser.lock())
                {
                    for byte in data {
                        parser.advance(&mut *state, byte);
                    }
                    // Reset scroll to bottom when new output arrives
                    state.scroll_to_bottom();
                }
            }
        }
    }

    pub fn resize(&mut self, size: TerminalSize) -> Result<(), TerminalError> {
        if size.rows == self.size.rows && size.cols == self.size.cols {
            return Ok(());
        }

        self.size = size.clone();

        // Resize PTY
        if let Some(ref pair) = self.pty_pair {
            pair.master
                .resize(size.clone().into())
                .map_err(|e| TerminalError::IoError(std::io::Error::other(e.to_string())))?;
        }

        // Resize terminal state
        if let Ok(mut state) = self.state.lock() {
            state.resize(size.rows as usize, size.cols as usize);
        }

        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.running
    }
}

/// Key input for terminal
#[derive(Debug, Clone)]
pub enum TerminalKey {
    Char(char),
    Enter,
    Tab,
    Backspace,
    Escape,
    Up,
    Down,
    Right,
    Left,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Insert,
    F(u8),
    CtrlC,
    CtrlD,
    CtrlZ,
    CtrlL,
    CtrlA,
    CtrlE,
    CtrlK,
    CtrlU,
    CtrlW,
    CtrlR,
}

impl TerminalKey {
    /// Convert key to bytes to send to PTY
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            TerminalKey::Char(c) => {
                let mut buf = [0u8; 4];
                c.encode_utf8(&mut buf).as_bytes().to_vec()
            }
            TerminalKey::Enter => b"\r".to_vec(),
            TerminalKey::Tab => b"\t".to_vec(),
            TerminalKey::Backspace => b"\x7f".to_vec(),
            TerminalKey::Escape => b"\x1b".to_vec(),
            TerminalKey::Up => b"\x1b[A".to_vec(),
            TerminalKey::Down => b"\x1b[B".to_vec(),
            TerminalKey::Right => b"\x1b[C".to_vec(),
            TerminalKey::Left => b"\x1b[D".to_vec(),
            TerminalKey::Home => b"\x1b[H".to_vec(),
            TerminalKey::End => b"\x1b[F".to_vec(),
            TerminalKey::PageUp => b"\x1b[5~".to_vec(),
            TerminalKey::PageDown => b"\x1b[6~".to_vec(),
            TerminalKey::Delete => b"\x1b[3~".to_vec(),
            TerminalKey::Insert => b"\x1b[2~".to_vec(),
            TerminalKey::F(n) => match n {
                1 => b"\x1bOP".to_vec(),
                2 => b"\x1bOQ".to_vec(),
                3 => b"\x1bOR".to_vec(),
                4 => b"\x1bOS".to_vec(),
                5 => b"\x1b[15~".to_vec(),
                6 => b"\x1b[17~".to_vec(),
                7 => b"\x1b[18~".to_vec(),
                8 => b"\x1b[19~".to_vec(),
                9 => b"\x1b[20~".to_vec(),
                10 => b"\x1b[21~".to_vec(),
                11 => b"\x1b[23~".to_vec(),
                12 => b"\x1b[24~".to_vec(),
                _ => vec![],
            },
            TerminalKey::CtrlC => b"\x03".to_vec(),
            TerminalKey::CtrlD => b"\x04".to_vec(),
            TerminalKey::CtrlZ => b"\x1a".to_vec(),
            TerminalKey::CtrlL => b"\x0c".to_vec(),
            TerminalKey::CtrlA => b"\x01".to_vec(),
            TerminalKey::CtrlE => b"\x05".to_vec(),
            TerminalKey::CtrlK => b"\x0b".to_vec(),
            TerminalKey::CtrlU => b"\x15".to_vec(),
            TerminalKey::CtrlW => b"\x17".to_vec(),
            TerminalKey::CtrlR => b"\x12".to_vec(),
        }
    }
}

impl Terminal {
    /// Send a key to the terminal
    pub fn send_key(&mut self, key: TerminalKey) -> Result<(), TerminalError> {
        self.write(&key.to_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_terminal_size_default() {
        let size = TerminalSize::default();
        assert_eq!(size.rows, 24);
        assert_eq!(size.cols, 80);
    }

    #[test]
    fn test_terminal_creation() {
        let terminal = Terminal::new(env::current_dir().unwrap(), None);
        assert!(!terminal.is_running());
    }

    #[test]
    fn test_get_shell_command() {
        let terminal = Terminal::new(env::current_dir().unwrap(), Some("/bin/bash".to_string()));
        assert_eq!(terminal.get_shell_command(), "/bin/bash");

        let terminal2 = Terminal::new(env::current_dir().unwrap(), None);
        let shell = terminal2.get_shell_command();
        assert!(!shell.is_empty());
    }

    #[test]
    fn test_terminal_key_bytes() {
        assert_eq!(TerminalKey::Enter.to_bytes(), b"\r");
        assert_eq!(TerminalKey::Tab.to_bytes(), b"\t");
        assert_eq!(TerminalKey::Up.to_bytes(), b"\x1b[A");
        assert_eq!(TerminalKey::CtrlC.to_bytes(), b"\x03");
        assert_eq!(TerminalKey::Char('a').to_bytes(), b"a");
    }
}
