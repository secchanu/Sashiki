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

#[derive(Debug, Clone, Copy)]
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
    /// Reader thread handle for graceful shutdown
    reader_handle: Option<thread::JoinHandle<()>>,
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
            size,
            running: false,
            state: Arc::new(Mutex::new(TerminalState::new(
                size.rows as usize,
                size.cols as usize,
            ))),
            parser: Arc::new(Mutex::new(Parser::new())),
            reader_handle: None,
        }
    }

    pub fn start(&mut self) -> Result<(), TerminalError> {
        let pty_system = native_pty_system();

        let pair = pty_system
            .openpty(self.size.into())
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

        // Spawn reader thread with proper error handling
        let handle = thread::Builder::new()
            .name("terminal-reader".to_string())
            .spawn(move || {
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
                        Err(e) => {
                            tracing::debug!("Terminal reader error: {}", e);
                            break;
                        }
                    }
                }
            })
            .map_err(|e| TerminalError::SpawnError(format!("Failed to spawn reader thread: {}", e)))?;

        self.pty_pair = Some(pair);
        self.writer = Some(writer);
        self.output_receiver = Some(rx);
        self.reader_handle = Some(handle);
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
            // Collect all pending data first to minimize lock hold time
            let mut all_data = Vec::new();
            while let Ok(data) = receiver.try_recv() {
                all_data.extend(data);
            }

            if all_data.is_empty() {
                return;
            }

            // Acquire locks once for all data processing
            // Lock order: state -> parser (always in this order to prevent deadlocks)
            let Ok(mut state) = self.state.lock() else {
                tracing::warn!(
                    "Failed to acquire terminal state lock, {} bytes dropped",
                    all_data.len()
                );
                return;
            };
            let Ok(mut parser) = self.parser.lock() else {
                tracing::warn!(
                    "Failed to acquire terminal parser lock, {} bytes dropped",
                    all_data.len()
                );
                return;
            };

            for byte in all_data {
                parser.advance(&mut *state, byte);
            }
            state.scroll_to_bottom();
        }
    }

    pub fn resize(&mut self, size: TerminalSize) -> Result<(), TerminalError> {
        if size.rows == self.size.rows && size.cols == self.size.cols {
            return Ok(());
        }

        self.size = size;

        // Resize PTY
        if let Some(ref pair) = self.pty_pair {
            pair.master
                .resize(size.into())
                .map_err(|e| TerminalError::IoError(std::io::Error::other(e.to_string())))?;
        }

        // Resize terminal state
        if let Ok(mut state) = self.state.lock() {
            state.resize(size.rows as usize, size.cols as usize);
        }

        Ok(())
    }

    /// Check if terminal is running
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Stop the terminal and release resources
    pub fn stop(&mut self) {
        // Drop writer first to close the PTY input (signals EOF to child)
        self.writer = None;
        // Drop receiver to signal reader thread to stop
        self.output_receiver = None;
        // Drop PTY pair to release the terminal
        self.pty_pair = None;
        self.running = false;

        // Wait for reader thread to finish gracefully
        if let Some(handle) = self.reader_handle.take() {
            if let Err(e) = handle.join() {
                tracing::warn!("Reader thread panicked: {:?}", e);
            }
        }
    }
}

impl Terminal {
    /// Get visible text from terminal state
    pub fn get_visible_text(&self) -> String {
        if let Ok(state) = self.state.lock() {
            state
                .visible_rows()
                .map(|row| {
                    row.iter()
                        .filter(|cell| !cell.is_continuation)
                        .map(|cell| cell.c)
                        .collect::<String>()
                        .trim_end()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            String::new()
        }
    }

    /// Get cursor position (row, col) from terminal state
    pub fn get_cursor_position(&self) -> (usize, usize) {
        if let Ok(state) = self.state.lock() {
            (state.cursor.row, state.cursor.col)
        } else {
            (0, 0)
        }
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
}
