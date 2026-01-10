//! Session management module
//!
//! Each Worktree has its own session with dedicated terminal.
//! This is the core abstraction for "parallel AI agent execution".

use crate::git::WorktreeInfo;
use crate::terminal::{Terminal, TerminalError};
use std::path::PathBuf;

/// Status of a worktree session
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    /// No activity
    Idle,
    /// Terminal has active process
    Running,
    /// Last command completed successfully
    Completed,
    /// Last command failed
    Error,
}

impl SessionStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            SessionStatus::Idle => "○",
            SessionStatus::Running => "▶",
            SessionStatus::Completed => "✓",
            SessionStatus::Error => "✗",
        }
    }
}

impl Default for SessionStatus {
    fn default() -> Self {
        SessionStatus::Idle
    }
}

/// A session combines a worktree with its dedicated terminal
pub struct WorktreeSession {
    /// Worktree information
    pub worktree: WorktreeInfo,
    /// Dedicated terminal for this worktree
    pub terminal: Terminal,
    /// Current status
    pub status: SessionStatus,
    /// Label for display (user can rename)
    pub label: Option<String>,
    /// Whether this session is pinned (won't be auto-closed)
    pub pinned: bool,
}

impl WorktreeSession {
    /// Create a new session for a worktree
    pub fn new(worktree: WorktreeInfo) -> Self {
        let terminal = Terminal::new(&worktree.path, None);
        Self {
            worktree,
            terminal,
            status: SessionStatus::Idle,
            label: None,
            pinned: false,
        }
    }

    /// Get display name for this session
    pub fn display_name(&self) -> &str {
        if let Some(ref label) = self.label {
            label
        } else if let Some(ref branch) = self.worktree.branch {
            branch
        } else {
            self.worktree.display_name()
        }
    }

    /// Start the terminal for this session
    pub fn start_terminal(&mut self) -> Result<(), TerminalError> {
        self.terminal.start()?;
        self.status = SessionStatus::Running;
        Ok(())
    }

    /// Send text to terminal (for click-to-insert feature)
    pub fn insert_to_terminal(&mut self, text: &str) -> Result<(), TerminalError> {
        self.terminal.write_str(text)
    }

    /// Send text and execute (append newline)
    pub fn execute_in_terminal(&mut self, command: &str) -> Result<(), TerminalError> {
        self.terminal.write_str(command)?;
        self.terminal.write_str("\r")
    }

    /// Check if terminal is running
    pub fn is_terminal_running(&self) -> bool {
        self.terminal.is_running()
    }

    /// Update status based on terminal state
    pub fn update_status(&mut self) {
        if self.terminal.is_running() {
            // Mark as running if terminal is active
            if self.status == SessionStatus::Idle {
                self.status = SessionStatus::Running;
            }
        }
    }
}

/// Manager for multiple sessions
pub struct SessionManager {
    sessions: Vec<WorktreeSession>,
    active_index: usize,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            active_index: 0,
        }
    }

    /// Create sessions from worktree list
    pub fn from_worktrees(worktrees: Vec<WorktreeInfo>) -> Self {
        let sessions = worktrees.into_iter().map(WorktreeSession::new).collect();
        Self {
            sessions,
            active_index: 0,
        }
    }

    /// Add a new session
    pub fn add_session(&mut self, worktree: WorktreeInfo) -> usize {
        let session = WorktreeSession::new(worktree);
        self.sessions.push(session);
        self.sessions.len() - 1
    }

    /// Remove a session by index
    pub fn remove_session(&mut self, index: usize) -> Option<WorktreeSession> {
        if index < self.sessions.len() {
            let session = self.sessions.remove(index);
            // Adjust active index if needed
            if self.active_index >= self.sessions.len() && !self.sessions.is_empty() {
                self.active_index = self.sessions.len() - 1;
            }
            Some(session)
        } else {
            None
        }
    }

    /// Get active session
    pub fn active(&self) -> Option<&WorktreeSession> {
        self.sessions.get(self.active_index)
    }

    /// Get active session mutably
    pub fn active_mut(&mut self) -> Option<&mut WorktreeSession> {
        self.sessions.get_mut(self.active_index)
    }

    /// Set active session by index
    pub fn set_active(&mut self, index: usize) -> bool {
        if index < self.sessions.len() {
            self.active_index = index;
            true
        } else {
            false
        }
    }

    /// Get active index
    pub fn active_index(&self) -> usize {
        self.active_index
    }

    /// Get all sessions
    pub fn sessions(&self) -> &[WorktreeSession] {
        &self.sessions
    }

    /// Number of sessions
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Start terminal for active session only
    pub fn start_active_terminal(&mut self) -> Result<(), TerminalError> {
        if let Some(session) = self.active_mut() {
            session.start_terminal()
        } else {
            Err(TerminalError::NotRunning)
        }
    }

    /// Update all session statuses
    pub fn update_all_statuses(&mut self) {
        for session in &mut self.sessions {
            session.update_status();
        }
    }

    /// Insert text to active terminal
    pub fn insert_to_active(&mut self, text: &str) -> Result<(), TerminalError> {
        if let Some(session) = self.active_mut() {
            session.insert_to_terminal(text)
        } else {
            Err(TerminalError::NotRunning)
        }
    }

    /// Find session by worktree path
    pub fn find_by_path(&self, path: &PathBuf) -> Option<usize> {
        self.sessions.iter().position(|s| &s.worktree.path == path)
    }

    /// Cycle to next session
    pub fn next_session(&mut self) {
        if !self.sessions.is_empty() {
            self.active_index = (self.active_index + 1) % self.sessions.len();
        }
    }

    /// Cycle to previous session
    pub fn prev_session(&mut self) {
        if !self.sessions.is_empty() {
            self.active_index = if self.active_index == 0 {
                self.sessions.len() - 1
            } else {
                self.active_index - 1
            };
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_worktree(name: &str) -> WorktreeInfo {
        WorktreeInfo {
            name: name.to_string(),
            path: PathBuf::from(format!("/tmp/{}", name)),
            branch: Some(name.to_string()),
            is_main: name == "main",
        }
    }

    #[test]
    fn test_session_status_symbols() {
        assert_eq!(SessionStatus::Idle.symbol(), "○");
        assert_eq!(SessionStatus::Running.symbol(), "▶");
        assert_eq!(SessionStatus::Completed.symbol(), "✓");
        assert_eq!(SessionStatus::Error.symbol(), "✗");
    }

    #[test]
    fn test_session_creation() {
        let wt = create_test_worktree("main");
        let session = WorktreeSession::new(wt);

        assert_eq!(session.status, SessionStatus::Idle);
        assert_eq!(session.display_name(), "main");
        assert!(!session.pinned);
    }

    #[test]
    fn test_session_display_name_priority() {
        let wt = create_test_worktree("feature");
        let mut session = WorktreeSession::new(wt);

        // Branch name by default
        assert_eq!(session.display_name(), "feature");

        // Custom label takes priority
        session.label = Some("My Feature".to_string());
        assert_eq!(session.display_name(), "My Feature");
    }

    #[test]
    fn test_session_manager_creation() {
        let worktrees = vec![
            create_test_worktree("main"),
            create_test_worktree("feature-a"),
            create_test_worktree("feature-b"),
        ];

        let manager = SessionManager::from_worktrees(worktrees);

        assert_eq!(manager.len(), 3);
        assert_eq!(manager.active_index(), 0);
    }

    #[test]
    fn test_session_manager_navigation() {
        let worktrees = vec![
            create_test_worktree("main"),
            create_test_worktree("feature-a"),
            create_test_worktree("feature-b"),
        ];

        let mut manager = SessionManager::from_worktrees(worktrees);

        assert_eq!(manager.active_index(), 0);

        manager.next_session();
        assert_eq!(manager.active_index(), 1);

        manager.next_session();
        assert_eq!(manager.active_index(), 2);

        manager.next_session(); // Wrap around
        assert_eq!(manager.active_index(), 0);

        manager.prev_session(); // Wrap around backwards
        assert_eq!(manager.active_index(), 2);
    }

    #[test]
    fn test_session_manager_set_active() {
        let worktrees = vec![
            create_test_worktree("main"),
            create_test_worktree("feature"),
        ];

        let mut manager = SessionManager::from_worktrees(worktrees);

        assert!(manager.set_active(1));
        assert_eq!(manager.active_index(), 1);

        assert!(!manager.set_active(10)); // Out of bounds
        assert_eq!(manager.active_index(), 1); // Unchanged
    }

    #[test]
    fn test_session_manager_add_remove() {
        let mut manager = SessionManager::new();

        let idx = manager.add_session(create_test_worktree("main"));
        assert_eq!(idx, 0);
        assert_eq!(manager.len(), 1);

        manager.add_session(create_test_worktree("feature"));
        assert_eq!(manager.len(), 2);

        manager.set_active(1);
        let removed = manager.remove_session(1);
        assert!(removed.is_some());
        assert_eq!(manager.len(), 1);
        assert_eq!(manager.active_index(), 0); // Adjusted
    }

    #[test]
    fn test_session_manager_find_by_path() {
        let worktrees = vec![
            create_test_worktree("main"),
            create_test_worktree("feature"),
        ];

        let manager = SessionManager::from_worktrees(worktrees);

        assert_eq!(manager.find_by_path(&PathBuf::from("/tmp/main")), Some(0));
        assert_eq!(manager.find_by_path(&PathBuf::from("/tmp/feature")), Some(1));
        assert_eq!(manager.find_by_path(&PathBuf::from("/tmp/unknown")), None);
    }
}
