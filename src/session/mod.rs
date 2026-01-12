//! Session management module
//!
//! Each Worktree has its own session with dedicated terminal.
//! This is the core abstraction for "parallel AI agent execution".

use crate::git::WorktreeInfo;
use crate::terminal::{Terminal, TerminalError};
use std::path::PathBuf;

/// Status of a worktree session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionStatus {
    /// No activity
    #[default]
    Idle,
    /// Terminal has active process
    Running,
    /// Last command completed successfully (future: status bar display)
    #[allow(dead_code)]
    Completed,
    /// Last command failed (future: status bar display)
    #[allow(dead_code)]
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
    /// Future: UI for pinning sessions to prevent auto-cleanup
    #[allow(dead_code)]
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

    /// Stop the terminal
    pub fn stop_terminal(&mut self) {
        self.terminal.stop();
        self.status = SessionStatus::Idle;
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
            if self.sessions.is_empty() {
                self.active_index = 0;
            } else if index < self.active_index {
                // Removed session was before active, shift back
                self.active_index -= 1;
            } else if self.active_index >= self.sessions.len() {
                // Active session was removed (was last), select previous
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

    /// Get all sessions mutably
    pub fn sessions_mut(&mut self) -> &mut [WorktreeSession] {
        &mut self.sessions
    }

    /// Number of sessions
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Start terminal for active session only
    pub fn start_active_terminal(&mut self) -> Result<(), TerminalError> {
        if let Some(session) = self.active_mut() {
            session.start_terminal()
        } else {
            Err(TerminalError::NotRunning)
        }
    }

    /// Ensure active session's terminal is running (start if not already running)
    /// Returns Ok(true) if terminal was started, Ok(false) if already running, Err on failure
    pub fn ensure_active_terminal_running(&mut self) -> Result<bool, crate::terminal::TerminalError> {
        if let Some(session) = self.active_mut() {
            if !session.terminal.is_running() {
                session.start_terminal()?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Find session by worktree path
    #[allow(dead_code)]
    pub fn find_by_path(&self, path: &PathBuf) -> Option<usize> {
        self.sessions.iter().position(|s| &s.worktree.path == path)
    }

    /// Stop terminal for a specific session
    pub fn stop_session_terminal(&mut self, index: usize) {
        if let Some(session) = self.sessions.get_mut(index) {
            session.stop_terminal();
        }
    }

    /// Cycle to next session
    pub fn next_session(&mut self) {
        if !self.sessions.is_empty() {
            self.active_index = (self.active_index + 1) % self.sessions.len();
        }
    }

    /// Cycle to previous session (future: Shift+Tab keybinding)
    #[allow(dead_code)]
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

    #[test]
    fn test_remove_session_index_adjustment() {
        let worktrees = vec![
            create_test_worktree("a"),
            create_test_worktree("b"),
            create_test_worktree("c"),
        ];

        // Test: Remove session before active
        let mut manager = SessionManager::from_worktrees(worktrees.clone());
        manager.set_active(2); // Active is "c"
        manager.remove_session(0); // Remove "a"
        assert_eq!(manager.active_index(), 1); // "c" is now at index 1
        assert_eq!(manager.active().unwrap().display_name(), "c");

        // Test: Remove active session
        let mut manager = SessionManager::from_worktrees(worktrees.clone());
        manager.set_active(1); // Active is "b"
        manager.remove_session(1); // Remove "b"
        assert_eq!(manager.active_index(), 1); // Now "c" at index 1
        assert_eq!(manager.active().unwrap().display_name(), "c");

        // Test: Remove last session when it's active
        let mut manager = SessionManager::from_worktrees(worktrees.clone());
        manager.set_active(2); // Active is "c"
        manager.remove_session(2); // Remove "c"
        assert_eq!(manager.active_index(), 1); // Now "b" at index 1
        assert_eq!(manager.active().unwrap().display_name(), "b");

        // Test: Remove session after active
        let mut manager = SessionManager::from_worktrees(worktrees);
        manager.set_active(0); // Active is "a"
        manager.remove_session(2); // Remove "c"
        assert_eq!(manager.active_index(), 0); // "a" unchanged
        assert_eq!(manager.active().unwrap().display_name(), "a");
    }
}
