//! Session management - each worktree has its own session with terminal

use crate::git::Worktree;
use crate::terminal::TerminalView;
use crate::theme;
use gpui::{AppContext, Context, Entity};

/// Color for visual identification of sessions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionColor {
    pub primary: u32,
}

impl SessionColor {
    /// Predefined colors for sessions
    pub const COLORS: [SessionColor; 8] = [
        SessionColor {
            primary: theme::BLUE,
        },
        SessionColor {
            primary: theme::GREEN,
        },
        SessionColor {
            primary: theme::YELLOW,
        },
        SessionColor {
            primary: theme::RED,
        },
        SessionColor {
            primary: theme::MAUVE,
        },
        SessionColor {
            primary: theme::TEAL,
        },
        SessionColor {
            primary: theme::PEACH,
        },
        SessionColor {
            primary: theme::PINK,
        },
    ];

    pub fn for_index(index: usize) -> Self {
        Self::COLORS[index % Self::COLORS.len()]
    }
}

/// Session status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    /// Terminal is open and currently focused
    Focused,
    /// Terminal is open but not focused
    Running,
    /// Terminal is closed/not started
    Stopped,
}

impl SessionStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            SessionStatus::Focused => "●",
            SessionStatus::Running => "○",
            SessionStatus::Stopped => "◌",
        }
    }
}

/// A session represents a worktree with its associated terminals.
/// Each session can have multiple terminals (1:N relationship).
/// Session : Worktree = 1:1 (immutable after creation)
/// Session : Terminal = 1:N
pub struct Session {
    worktree: Worktree,
    terminals: Vec<Entity<TerminalView>>,
    active_terminal_index: usize,
    color: SessionColor,
    status: SessionStatus,
    /// Whether to show in parallel mode
    visible_in_parallel: bool,
}

impl Session {
    /// Create a new session for a worktree (no terminals yet)
    pub fn new(worktree: Worktree, color_index: usize) -> Self {
        Self {
            worktree,
            terminals: Vec::new(),
            active_terminal_index: 0,
            color: SessionColor::for_index(color_index),
            status: SessionStatus::Stopped,
            visible_in_parallel: false,
        }
    }

    /// Add a new terminal to this session and make it active
    pub fn add_terminal<V: 'static>(&mut self, cx: &mut Context<V>) {
        let path = self.worktree.path.clone();
        self.add_terminal_in_directory(path, cx);
    }

    /// Add a new terminal with a custom working directory
    pub fn add_terminal_in_directory<V: 'static>(
        &mut self,
        path: std::path::PathBuf,
        cx: &mut Context<V>,
    ) {
        let terminal = cx.new(|cx| TerminalView::new_with_directory(path, cx));
        self.terminals.push(terminal);
        self.active_terminal_index = self.terminals.len() - 1;
        self.status = SessionStatus::Running;
    }

    /// Start a terminal if none exists (convenience method for initial terminal)
    pub fn ensure_terminal<V: 'static>(&mut self, cx: &mut Context<V>) {
        if self.terminals.is_empty() {
            self.add_terminal(cx);
        }
    }

    /// Remove a terminal by index
    #[allow(dead_code)]
    pub fn remove_terminal(&mut self, index: usize) {
        if index >= self.terminals.len() {
            return;
        }

        self.terminals.remove(index);

        if self.terminals.is_empty() {
            self.active_terminal_index = 0;
            self.status = SessionStatus::Stopped;
        } else if index < self.active_terminal_index {
            // Removed terminal was before active - shift index down
            self.active_terminal_index -= 1;
        } else if index == self.active_terminal_index {
            // Removed the active terminal - clamp to valid range
            if self.active_terminal_index >= self.terminals.len() {
                self.active_terminal_index = self.terminals.len() - 1;
            }
            // If index is still valid, keep it (now points to next terminal)
        }
        // If index > active_terminal_index, no adjustment needed
    }

    /// Remove all terminals
    pub fn clear_terminals(&mut self) {
        self.terminals.clear();
        self.active_terminal_index = 0;
        self.status = SessionStatus::Stopped;
    }

    /// Get the active terminal
    pub fn active_terminal(&self) -> Option<&Entity<TerminalView>> {
        self.terminals.get(self.active_terminal_index)
    }

    /// Switch to a specific terminal by index
    #[allow(dead_code)]
    pub fn switch_terminal(&mut self, index: usize) {
        if index < self.terminals.len() {
            self.active_terminal_index = index;
        }
    }

    /// Switch to next terminal within this session
    #[allow(dead_code)]
    pub fn next_terminal(&mut self) {
        if !self.terminals.is_empty() {
            self.active_terminal_index = (self.active_terminal_index + 1) % self.terminals.len();
        }
    }

    /// Switch to previous terminal within this session
    #[allow(dead_code)]
    pub fn prev_terminal(&mut self) {
        if !self.terminals.is_empty() {
            self.active_terminal_index = if self.active_terminal_index == 0 {
                self.terminals.len() - 1
            } else {
                self.active_terminal_index - 1
            };
        }
    }

    /// Check if this session has any terminals
    pub fn has_terminals(&self) -> bool {
        !self.terminals.is_empty()
    }

    /// Get the number of terminals in this session
    #[allow(dead_code)]
    pub fn terminal_count(&self) -> usize {
        self.terminals.len()
    }

    /// Get reference to the worktree (read-only)
    #[allow(dead_code)]
    pub fn worktree(&self) -> &Worktree {
        &self.worktree
    }

    /// Get the worktree path
    pub fn worktree_path(&self) -> &std::path::Path {
        &self.worktree.path
    }

    /// Update worktree information (branch, locked status)
    /// Note: path and is_main cannot be changed as they are immutable identifiers
    pub fn update_worktree_info(&mut self, updated: &Worktree) {
        debug_assert_eq!(self.worktree.path, updated.path, "Worktree path mismatch");
        self.worktree.branch = updated.branch.clone();
        self.worktree.locked = updated.locked;
    }

    /// Get display name (worktree name)
    pub fn name(&self) -> &str {
        &self.worktree.name
    }

    /// Get branch name if available
    pub fn branch(&self) -> Option<&str> {
        self.worktree.branch.as_deref()
    }

    /// Check if this is the main worktree
    pub fn is_main(&self) -> bool {
        self.worktree.is_main
    }

    /// Check if this worktree is locked
    pub fn is_locked(&self) -> bool {
        self.worktree.locked
    }

    /// Get session color
    pub fn color(&self) -> SessionColor {
        self.color
    }

    /// Get session status
    pub fn status(&self) -> SessionStatus {
        self.status
    }

    /// Check if visible in parallel mode
    pub fn is_visible_in_parallel(&self) -> bool {
        self.visible_in_parallel
    }

    /// Set session status (crate-internal)
    pub(crate) fn set_status(&mut self, status: SessionStatus) {
        self.status = status;
    }

    /// Toggle parallel visibility
    pub(crate) fn toggle_visibility(&mut self) {
        self.visible_in_parallel = !self.visible_in_parallel;
    }

    /// Set visible in parallel mode
    pub(crate) fn set_visible_in_parallel(&mut self, visible: bool) {
        self.visible_in_parallel = visible;
    }
}

/// Layout mode for terminal operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutMode {
    /// Focus on a single session - interact with only the active session's terminal
    #[default]
    Single,
    /// Operate across sessions - view and interact with multiple session terminals simultaneously
    Parallel,
}

/// Manages all sessions (one per worktree)
#[derive(Default)]
pub struct SessionManager {
    sessions: Vec<Session>,
    active_index: usize,
    layout_mode: LayoutMode,
}

impl SessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Initialize sessions from worktrees (no terminals yet)
    pub fn init_from_worktrees(&mut self, worktrees: Vec<Worktree>) {
        self.sessions.clear();
        for (i, worktree) in worktrees.into_iter().enumerate() {
            let session = Session::new(worktree, i);
            self.sessions.push(session);
        }
        self.active_index = 0;
    }

    /// Ensure the session has at least one terminal (starts one if none exist)
    pub fn ensure_session_terminal<V: 'static>(&mut self, index: usize, cx: &mut Context<V>) {
        if let Some(session) = self.sessions.get_mut(index) {
            session.ensure_terminal(cx);
        }
    }

    /// Ensure the active session has at least one terminal
    pub fn ensure_active_session_terminal<V: 'static>(&mut self, cx: &mut Context<V>) {
        self.ensure_session_terminal(self.active_index, cx);
    }

    /// Ensure the active session has a terminal, using a custom working directory
    pub fn ensure_active_session_terminal_in<V: 'static>(
        &mut self,
        directory: std::path::PathBuf,
        cx: &mut Context<V>,
    ) {
        if let Some(session) = self.sessions.get_mut(self.active_index) {
            if session.terminals.is_empty() {
                session.add_terminal_in_directory(directory, cx);
            }
        }
    }

    /// Add a new terminal to a session
    #[allow(dead_code)]
    pub fn add_terminal_to_session<V: 'static>(&mut self, index: usize, cx: &mut Context<V>) {
        if let Some(session) = self.sessions.get_mut(index) {
            session.add_terminal(cx);
        }
    }

    /// Add a new terminal to the active session
    #[allow(dead_code)]
    pub fn add_terminal_to_active_session<V: 'static>(&mut self, cx: &mut Context<V>) {
        self.add_terminal_to_session(self.active_index, cx);
    }

    /// Clear all terminals for a session (releases file handles)
    pub fn clear_session_terminals(&mut self, index: usize) {
        if let Some(session) = self.sessions.get_mut(index) {
            session.clear_terminals();
        }
    }

    /// Add a new session for a worktree.
    /// Returns true if added, false if a session for this worktree already exists.
    pub fn add_session(&mut self, worktree: Worktree) -> bool {
        // Check for duplicate by path
        if self.find_session_by_path(&worktree.path).is_some() {
            return false;
        }
        let index = self.sessions.len();
        let session = Session::new(worktree, index);
        self.sessions.push(session);
        true
    }

    /// Find a session by worktree path
    pub fn find_session_by_path(&self, path: &std::path::Path) -> Option<usize> {
        self.sessions.iter().position(|s| s.worktree_path() == path)
    }

    /// Update worktree information for a session identified by path
    #[allow(dead_code)]
    pub fn update_session_worktree(&mut self, updated: &Worktree) -> bool {
        if let Some(index) = self.find_session_by_path(&updated.path)
            && let Some(session) = self.sessions.get_mut(index)
        {
            session.update_worktree_info(updated);
            return true;
        }
        false
    }

    /// Synchronize sessions with current worktrees.
    /// - Adds new worktrees as sessions
    /// - Removes sessions for deleted worktrees
    /// - Updates worktree info (branch, locked) for existing sessions
    ///
    /// Returns (added_count, removed_count, updated_count)
    pub fn sync_with_worktrees(&mut self, worktrees: Vec<Worktree>) -> (usize, usize, usize) {
        use std::collections::HashSet;

        let mut added = 0;
        let mut removed = 0;
        let mut updated = 0;

        // Create a set of current worktree paths
        let current_paths: HashSet<_> = worktrees.iter().map(|w| w.path.clone()).collect();

        // Remove sessions for deleted worktrees (iterate in reverse to preserve indices)
        let mut i = self.sessions.len();
        while i > 0 {
            i -= 1;
            let session_path = self.sessions[i].worktree_path().to_path_buf();
            if !current_paths.contains(&session_path) {
                self.sessions.remove(i);
                removed += 1;
                // Adjust active_index after removal
                if i < self.active_index {
                    self.active_index -= 1;
                } else if i == self.active_index && self.active_index >= self.sessions.len() {
                    self.active_index = self.sessions.len().saturating_sub(1);
                }
            }
        }

        // Add new worktrees and update existing ones
        for worktree in worktrees {
            if let Some(index) = self.find_session_by_path(&worktree.path) {
                // Update existing session's worktree info
                if let Some(session) = self.sessions.get_mut(index) {
                    session.update_worktree_info(&worktree);
                    updated += 1;
                }
            } else {
                // Add new session
                let index = self.sessions.len();
                let session = Session::new(worktree, index);
                self.sessions.push(session);
                added += 1;
            }
        }

        (added, removed, updated)
    }

    /// Remove a session by index
    pub fn remove_session(&mut self, index: usize) {
        if index < self.sessions.len() && self.sessions.len() > 1 {
            self.sessions.remove(index);
            // Adjust active_index after removal
            if index < self.active_index {
                // Removed session was before active - shift index down
                self.active_index -= 1;
            } else if index == self.active_index {
                // Removed active session - clamp to valid range
                if self.active_index >= self.sessions.len() {
                    self.active_index = self.sessions.len() - 1;
                }
            }
            // If index > active_index, no adjustment needed
        }
    }

    /// Get all sessions
    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }

    /// Get active session
    pub fn active_session(&self) -> Option<&Session> {
        self.sessions.get(self.active_index)
    }

    /// Get active terminal (the active terminal of the active session)
    pub fn active_terminal(&self) -> Option<Entity<TerminalView>> {
        self.active_session()
            .and_then(|s| s.active_terminal())
            .cloned()
    }

    /// Get the active terminal for a specific session
    pub fn get_session_active_terminal(&self, index: usize) -> Option<Entity<TerminalView>> {
        self.sessions
            .get(index)
            .and_then(|s| s.active_terminal())
            .cloned()
    }

    /// Switch to session by index and update statuses
    /// Also marks the session as visible in parallel mode
    pub fn switch_to(&mut self, index: usize) {
        if index < self.sessions.len() {
            // Update old active session status
            if let Some(old_session) = self.sessions.get_mut(self.active_index)
                && old_session.has_terminals()
            {
                old_session.set_status(SessionStatus::Running);
            }
            // Switch and update new active session status
            self.active_index = index;
            if let Some(new_session) = self.sessions.get_mut(self.active_index) {
                if new_session.has_terminals() {
                    new_session.set_status(SessionStatus::Focused);
                }
                // Active session should always be visible in parallel mode
                new_session.set_visible_in_parallel(true);
            }
        }
    }

    /// Switch to next session
    pub fn next_session(&mut self) {
        if !self.sessions.is_empty() {
            let next = (self.active_index + 1) % self.sessions.len();
            self.switch_to(next);
        }
    }

    /// Switch to previous session
    pub fn prev_session(&mut self) {
        if !self.sessions.is_empty() {
            let prev = if self.active_index == 0 {
                self.sessions.len() - 1
            } else {
                self.active_index - 1
            };
            self.switch_to(prev);
        }
    }

    /// Get active session index
    pub fn active_index(&self) -> usize {
        self.active_index
    }

    /// Get layout mode
    pub fn layout_mode(&self) -> LayoutMode {
        self.layout_mode
    }

    /// Toggle between Single and Parallel mode
    pub fn toggle_layout_mode(&mut self) {
        self.layout_mode = match self.layout_mode {
            LayoutMode::Single => LayoutMode::Parallel,
            LayoutMode::Parallel => LayoutMode::Single,
        };
    }

    /// Toggle whether a session is shown in parallel mode
    pub fn toggle_parallel_visibility(&mut self, index: usize) {
        if let Some(session) = self.sessions.get_mut(index) {
            session.toggle_visibility();
        }
    }

    /// Get sessions that should be shown in parallel mode
    /// Note: Caller should ensure terminals exist for these sessions before rendering
    pub fn parallel_sessions(&self) -> Vec<(usize, &Session)> {
        self.sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| s.is_visible_in_parallel())
            .collect()
    }

    /// Check if there are any sessions
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// Get session count
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Get count of sessions with at least one terminal
    pub fn running_session_count(&self) -> usize {
        self.sessions.iter().filter(|s| s.has_terminals()).count()
    }

    /// Get total terminal count across all sessions
    #[allow(dead_code)]
    pub fn total_terminal_count(&self) -> usize {
        self.sessions.iter().map(|s| s.terminal_count()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_worktree(name: &str, is_main: bool) -> Worktree {
        Worktree {
            name: name.to_string(),
            path: PathBuf::from(format!("/worktrees/{}", name)),
            branch: Some(format!("feature/{}", name)),
            is_main,
            locked: false,
        }
    }

    // ===== SessionColor tests =====

    #[test]
    fn test_session_color_for_index_cycles() {
        let color0 = SessionColor::for_index(0);
        let color8 = SessionColor::for_index(8);
        assert_eq!(color0, color8);
    }

    #[test]
    fn test_session_color_all_unique() {
        let colors: Vec<_> = (0..8).map(SessionColor::for_index).collect();
        for i in 0..8 {
            for j in (i + 1)..8 {
                assert_ne!(
                    colors[i], colors[j],
                    "Colors at {} and {} should differ",
                    i, j
                );
            }
        }
    }

    // ===== SessionStatus tests =====

    #[test]
    fn test_session_status_symbols() {
        assert_eq!(SessionStatus::Focused.symbol(), "●");
        assert_eq!(SessionStatus::Running.symbol(), "○");
        assert_eq!(SessionStatus::Stopped.symbol(), "◌");
    }

    // ===== Session tests (without terminal operations) =====

    #[test]
    fn test_session_new() {
        let worktree = make_worktree("test", false);
        let session = Session::new(worktree, 0);

        assert_eq!(session.name(), "test");
        assert_eq!(session.branch(), Some("feature/test"));
        assert!(!session.is_main());
        assert!(!session.is_locked());
        assert_eq!(session.status(), SessionStatus::Stopped);
        assert!(!session.is_visible_in_parallel());
        assert!(!session.has_terminals());
        assert_eq!(session.terminal_count(), 0);
    }

    #[test]
    fn test_session_main_worktree() {
        let worktree = make_worktree("main", true);
        let session = Session::new(worktree, 0);

        assert!(session.is_main());
    }

    #[test]
    fn test_session_update_worktree_info() {
        let worktree = make_worktree("test", false);
        let mut session = Session::new(worktree, 0);

        let updated = Worktree {
            name: "test".to_string(),
            path: PathBuf::from("/worktrees/test"),
            branch: Some("main".to_string()),
            is_main: false,
            locked: true,
        };

        session.update_worktree_info(&updated);

        assert_eq!(session.branch(), Some("main"));
        assert!(session.is_locked());
    }

    // ===== LayoutMode tests =====

    #[test]
    fn test_layout_mode_default() {
        let mode = LayoutMode::default();
        assert_eq!(mode, LayoutMode::Single);
    }

    // ===== SessionManager tests =====

    #[test]
    fn test_session_manager_new() {
        let manager = SessionManager::new();
        assert!(manager.is_empty());
        assert_eq!(manager.len(), 0);
        assert_eq!(manager.active_index(), 0);
        assert_eq!(manager.layout_mode(), LayoutMode::Single);
    }

    #[test]
    fn test_session_manager_init_from_worktrees() {
        let mut manager = SessionManager::new();
        let worktrees = vec![
            make_worktree("main", true),
            make_worktree("feature1", false),
            make_worktree("feature2", false),
        ];

        manager.init_from_worktrees(worktrees);

        assert_eq!(manager.len(), 3);
        assert_eq!(manager.active_index(), 0);
        assert_eq!(manager.sessions()[0].name(), "main");
        assert_eq!(manager.sessions()[1].name(), "feature1");
        assert_eq!(manager.sessions()[2].name(), "feature2");
    }

    #[test]
    fn test_session_manager_add_session() {
        let mut manager = SessionManager::new();

        let result1 = manager.add_session(make_worktree("test1", false));
        assert!(result1);
        assert_eq!(manager.len(), 1);

        let result2 = manager.add_session(make_worktree("test2", false));
        assert!(result2);
        assert_eq!(manager.len(), 2);
    }

    #[test]
    fn test_session_manager_add_session_duplicate() {
        let mut manager = SessionManager::new();

        manager.add_session(make_worktree("test", false));
        let result = manager.add_session(make_worktree("test", false));

        assert!(!result);
        assert_eq!(manager.len(), 1);
    }

    #[test]
    fn test_session_manager_remove_session() {
        let mut manager = SessionManager::new();
        manager.init_from_worktrees(vec![
            make_worktree("main", true),
            make_worktree("feature1", false),
            make_worktree("feature2", false),
        ]);

        manager.remove_session(1);

        assert_eq!(manager.len(), 2);
        assert_eq!(manager.sessions()[0].name(), "main");
        assert_eq!(manager.sessions()[1].name(), "feature2");
    }

    #[test]
    fn test_session_manager_remove_session_adjusts_active_index() {
        let mut manager = SessionManager::new();
        manager.init_from_worktrees(vec![
            make_worktree("main", true),
            make_worktree("feature1", false),
            make_worktree("feature2", false),
        ]);
        manager.switch_to(2);
        assert_eq!(manager.active_index(), 2);

        manager.remove_session(1);

        assert_eq!(manager.active_index(), 1);
    }

    #[test]
    fn test_session_manager_remove_active_session() {
        let mut manager = SessionManager::new();
        manager.init_from_worktrees(vec![
            make_worktree("main", true),
            make_worktree("feature1", false),
            make_worktree("feature2", false),
        ]);
        manager.switch_to(1);

        manager.remove_session(1);

        assert_eq!(manager.active_index(), 1);
        assert_eq!(
            manager.sessions()[manager.active_index()].name(),
            "feature2"
        );
    }

    #[test]
    fn test_session_manager_switch_to() {
        let mut manager = SessionManager::new();
        manager.init_from_worktrees(vec![
            make_worktree("main", true),
            make_worktree("feature1", false),
        ]);

        manager.switch_to(1);

        assert_eq!(manager.active_index(), 1);
        assert!(manager.sessions()[1].is_visible_in_parallel());
    }

    #[test]
    fn test_session_manager_next_session() {
        let mut manager = SessionManager::new();
        manager.init_from_worktrees(vec![
            make_worktree("s0", false),
            make_worktree("s1", false),
            make_worktree("s2", false),
        ]);

        manager.next_session();
        assert_eq!(manager.active_index(), 1);

        manager.next_session();
        assert_eq!(manager.active_index(), 2);

        manager.next_session();
        assert_eq!(manager.active_index(), 0);
    }

    #[test]
    fn test_session_manager_prev_session() {
        let mut manager = SessionManager::new();
        manager.init_from_worktrees(vec![
            make_worktree("s0", false),
            make_worktree("s1", false),
            make_worktree("s2", false),
        ]);

        manager.prev_session();
        assert_eq!(manager.active_index(), 2);

        manager.prev_session();
        assert_eq!(manager.active_index(), 1);

        manager.prev_session();
        assert_eq!(manager.active_index(), 0);
    }

    #[test]
    fn test_session_manager_toggle_layout_mode() {
        let mut manager = SessionManager::new();

        assert_eq!(manager.layout_mode(), LayoutMode::Single);

        manager.toggle_layout_mode();
        assert_eq!(manager.layout_mode(), LayoutMode::Parallel);

        manager.toggle_layout_mode();
        assert_eq!(manager.layout_mode(), LayoutMode::Single);
    }

    #[test]
    fn test_session_manager_toggle_parallel_visibility() {
        let mut manager = SessionManager::new();
        manager.init_from_worktrees(vec![make_worktree("s0", false), make_worktree("s1", false)]);

        assert!(!manager.sessions()[0].is_visible_in_parallel());

        manager.toggle_parallel_visibility(0);
        assert!(manager.sessions()[0].is_visible_in_parallel());

        manager.toggle_parallel_visibility(0);
        assert!(!manager.sessions()[0].is_visible_in_parallel());
    }

    #[test]
    fn test_session_manager_parallel_sessions() {
        let mut manager = SessionManager::new();
        manager.init_from_worktrees(vec![
            make_worktree("s0", false),
            make_worktree("s1", false),
            make_worktree("s2", false),
        ]);

        manager.toggle_parallel_visibility(0);
        manager.toggle_parallel_visibility(2);

        let parallel = manager.parallel_sessions();
        assert_eq!(parallel.len(), 2);
        assert_eq!(parallel[0].0, 0);
        assert_eq!(parallel[1].0, 2);
    }

    #[test]
    fn test_session_manager_sync_with_worktrees_add() {
        let mut manager = SessionManager::new();
        manager.init_from_worktrees(vec![make_worktree("main", true)]);

        let worktrees = vec![
            make_worktree("main", true),
            make_worktree("new_feature", false),
        ];

        let (added, removed, _updated) = manager.sync_with_worktrees(worktrees);

        assert_eq!(added, 1);
        assert_eq!(removed, 0);
        assert_eq!(manager.len(), 2);
    }

    #[test]
    fn test_session_manager_sync_with_worktrees_remove() {
        let mut manager = SessionManager::new();
        manager.init_from_worktrees(vec![
            make_worktree("main", true),
            make_worktree("to_remove", false),
        ]);

        let worktrees = vec![make_worktree("main", true)];

        let (added, removed, _updated) = manager.sync_with_worktrees(worktrees);

        assert_eq!(added, 0);
        assert_eq!(removed, 1);
        assert_eq!(manager.len(), 1);
    }

    #[test]
    fn test_session_manager_sync_with_worktrees_update() {
        let mut manager = SessionManager::new();
        manager.init_from_worktrees(vec![make_worktree("main", true)]);

        let mut updated_worktree = make_worktree("main", true);
        updated_worktree.locked = true;

        let worktrees = vec![updated_worktree];
        let (_added, _removed, updated) = manager.sync_with_worktrees(worktrees);

        assert_eq!(updated, 1);
        assert!(manager.sessions()[0].is_locked());
    }

    #[test]
    fn test_session_manager_find_session_by_path() {
        let mut manager = SessionManager::new();
        manager.init_from_worktrees(vec![make_worktree("s0", false), make_worktree("s1", false)]);

        let found = manager.find_session_by_path(&PathBuf::from("/worktrees/s1"));
        assert_eq!(found, Some(1));

        let not_found = manager.find_session_by_path(&PathBuf::from("/worktrees/nonexistent"));
        assert_eq!(not_found, None);
    }

    #[test]
    fn test_session_manager_running_session_count() {
        let manager = SessionManager::new();
        assert_eq!(manager.running_session_count(), 0);
    }
}
