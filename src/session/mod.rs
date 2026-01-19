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
        SessionColor { primary: theme::BLUE },
        SessionColor { primary: theme::GREEN },
        SessionColor { primary: theme::YELLOW },
        SessionColor { primary: theme::RED },
        SessionColor { primary: theme::MAUVE },
        SessionColor { primary: theme::TEAL },
        SessionColor { primary: theme::PEACH },
        SessionColor { primary: theme::PINK },
    ];

    pub fn for_index(index: usize) -> Self {
        Self::COLORS[index % Self::COLORS.len()]
    }
}

/// Session status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    /// Terminal is open and active (currently focused)
    Active,
    /// Terminal is open but not focused
    Running,
    /// Terminal is closed/not started
    Stopped,
}

impl SessionStatus {
    pub fn symbol(&self) -> &'static str {
        match self {
            SessionStatus::Active => "●",
            SessionStatus::Running => "○",
            SessionStatus::Stopped => "◌",
        }
    }
}

/// A session represents a worktree with its associated terminal
pub struct Session {
    pub worktree: Worktree,
    pub terminal: Option<Entity<TerminalView>>,
    pub color: SessionColor,
    pub status: SessionStatus,
    /// Whether to show in parallel mode
    pub visible_in_parallel: bool,
}

impl Session {
    /// Create a new session for a worktree (terminal not started yet)
    pub fn new_without_terminal(worktree: Worktree, color_index: usize) -> Self {
        Self {
            worktree,
            terminal: None,
            color: SessionColor::for_index(color_index),
            status: SessionStatus::Stopped,
            visible_in_parallel: true,
        }
    }

    /// Start the terminal for this session
    pub fn start_terminal<V: 'static>(&mut self, cx: &mut Context<V>) {
        if self.terminal.is_none() {
            let path = self.worktree.path.clone();
            let terminal = cx.new(|cx| TerminalView::new_with_directory(path, cx));
            self.terminal = Some(terminal);
            self.status = SessionStatus::Running;
        }
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
}

/// View mode for the application
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum ViewMode {
    /// Single session view - one terminal fullscreen
    #[default]
    Single,
    /// Parallel view - multiple terminals in a grid
    Parallel,
}


/// Manages all sessions (one per worktree)
#[derive(Default)]
pub struct SessionManager {
    sessions: Vec<Session>,
    active_index: usize,
    view_mode: ViewMode,
}

impl SessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Initialize sessions from worktrees (terminals not started yet)
    pub fn init_from_worktrees(&mut self, worktrees: Vec<Worktree>) {
        self.sessions.clear();
        for (i, worktree) in worktrees.into_iter().enumerate() {
            let session = Session::new_without_terminal(worktree, i);
            self.sessions.push(session);
        }
        self.active_index = 0;
    }

    /// Start terminal for a session
    pub fn start_session<V: 'static>(&mut self, index: usize, cx: &mut Context<V>) {
        if let Some(session) = self.sessions.get_mut(index) {
            session.start_terminal(cx);
        }
    }

    /// Start terminal for active session
    pub fn start_active_session<V: 'static>(&mut self, cx: &mut Context<V>) {
        self.start_session(self.active_index, cx);
    }

    /// Stop terminal for a session (releases file handles)
    pub fn stop_session(&mut self, index: usize) {
        if let Some(session) = self.sessions.get_mut(index) {
            session.terminal = None;
            session.status = SessionStatus::Stopped;
        }
    }

    /// Add a new session for a worktree
    pub fn add_session(&mut self, worktree: Worktree) {
        let index = self.sessions.len();
        let session = Session::new_without_terminal(worktree, index);
        self.sessions.push(session);
    }

    /// Remove a session by index
    pub fn remove_session(&mut self, index: usize) {
        if index < self.sessions.len() && self.sessions.len() > 1 {
            self.sessions.remove(index);
            if self.active_index >= self.sessions.len() {
                self.active_index = self.sessions.len().saturating_sub(1);
            }
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

    /// Get active terminal
    pub fn active_terminal(&self) -> Option<Entity<TerminalView>> {
        self.active_session().and_then(|s| s.terminal.clone())
    }

    /// Get terminal for a specific session
    pub fn get_terminal(&self, index: usize) -> Option<Entity<TerminalView>> {
        self.sessions.get(index).and_then(|s| s.terminal.clone())
    }

    /// Switch to session by index and update statuses
    pub fn switch_to(&mut self, index: usize) {
        if index < self.sessions.len() {
            // Update old active session status
            if let Some(old_session) = self.sessions.get_mut(self.active_index)
                && old_session.terminal.is_some() {
                    old_session.status = SessionStatus::Running;
                }
            // Switch and update new active session status
            self.active_index = index;
            if let Some(new_session) = self.sessions.get_mut(self.active_index)
                && new_session.terminal.is_some() {
                    new_session.status = SessionStatus::Active;
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

    /// Get view mode
    pub fn view_mode(&self) -> ViewMode {
        self.view_mode
    }

    /// Toggle between Single and Parallel mode
    pub fn toggle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            ViewMode::Single => ViewMode::Parallel,
            ViewMode::Parallel => ViewMode::Single,
        };
    }

    /// Toggle whether a session is shown in parallel mode
    pub fn toggle_parallel_visibility(&mut self, index: usize) {
        if let Some(session) = self.sessions.get_mut(index) {
            session.visible_in_parallel = !session.visible_in_parallel;
        }
    }

    /// Get sessions that should be shown in parallel mode
    pub fn parallel_sessions(&self) -> Vec<(usize, &Session)> {
        self.sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| s.visible_in_parallel && s.terminal.is_some())
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

    /// Get running session count
    pub fn running_count(&self) -> usize {
        self.sessions.iter().filter(|s| s.terminal.is_some()).count()
    }
}

