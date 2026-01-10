//! Main application state and logic
//!
//! Redesigned around the session model where each worktree has its own terminal.

use crate::action::{Action, ActionQueue, PathFormat};
use crate::buffer::TextBuffer;
use crate::config::Config;
use crate::diff::{calculate_diff, to_side_by_side, DiffResult, SideBySideLine};
use crate::git::{GitManager, WorktreeInfo};
use crate::session::{SessionManager, SessionStatus};
use crate::ui::{DiffView, FileTree, MarkdownEditor, Sidebar, SplitDirection, TerminalView, TextView, Theme};
use std::path::PathBuf;

/// Main application state
pub struct App {
    // Configuration
    pub config: Config,
    pub theme: Theme,

    // Git state
    pub git_manager: Option<GitManager>,

    // Session management (worktree + terminal pairs)
    pub sessions: SessionManager,

    // View state
    pub current_view: ViewState,
    pub diff_result: Option<DiffResult>,
    pub side_by_side: Vec<SideBySideLine>,

    // Action queue for deferred actions
    pub action_queue: ActionQueue,
    pub path_format: PathFormat,

    // UI components
    pub sidebar: Sidebar,
    pub text_view: TextView,
    pub diff_view: DiffView,
    pub terminal_view: TerminalView,
    pub markdown_editor: MarkdownEditor,
    pub file_tree: FileTree,

    // Layout state
    pub split_direction: SplitDirection,
    pub split_ratio: f32,
    pub terminal_visible: bool,

    // Dialog state
    pub show_open_dialog: bool,
    pub dialog_path: String,

    // Worktree dialog state
    pub show_worktree_create_dialog: bool,
    pub worktree_branch_name: String,
    pub show_worktree_delete_dialog: bool,
    pub worktree_to_delete: Option<String>,
    pub worktree_error: Option<String>,
}

/// What is currently displayed in the main view
#[derive(Debug, Clone)]
pub enum ViewState {
    /// Empty welcome screen
    Empty,
    /// Viewing a single file
    File {
        path: PathBuf,
        buffer: TextBuffer,
    },
    /// Viewing diff between HEAD and working tree
    Diff {
        path: PathBuf,
    },
    /// Viewing changed files list
    ChangedFiles,
    /// Editing a file
    Edit {
        path: PathBuf,
    },
}

impl Default for ViewState {
    fn default() -> Self {
        ViewState::Empty
    }
}

impl App {
    pub fn new(config: Config) -> Self {
        let theme = match config.theme {
            crate::config::Theme::Dark => Theme::dark(),
            crate::config::Theme::Light => Theme::light(),
        };

        Self {
            sidebar: Sidebar::new(config.layout.sidebar_width),
            text_view: TextView::new(),
            diff_view: DiffView::new(),
            terminal_view: TerminalView::new(config.layout.terminal_height),
            markdown_editor: MarkdownEditor::new(),
            file_tree: FileTree::new(),
            split_direction: if config.layout.split_horizontal {
                SplitDirection::Horizontal
            } else {
                SplitDirection::Vertical
            },
            split_ratio: config.layout.split_ratio,
            terminal_visible: true,
            sessions: SessionManager::new(),
            config,
            theme,
            git_manager: None,
            current_view: ViewState::Empty,
            diff_result: None,
            side_by_side: Vec::new(),
            action_queue: ActionQueue::new(),
            path_format: PathFormat::Relative,
            show_open_dialog: false,
            dialog_path: String::new(),
            show_worktree_create_dialog: false,
            worktree_branch_name: String::new(),
            show_worktree_delete_dialog: false,
            worktree_to_delete: None,
            worktree_error: None,
        }
    }

    /// Open a git repository and create sessions for all worktrees
    pub fn open_repository(&mut self, path: impl Into<PathBuf>) -> Result<(), String> {
        let path = path.into();
        let manager = GitManager::open(&path).map_err(|e| e.to_string())?;

        let worktrees = manager.list_worktrees().unwrap_or_default();
        self.sessions = SessionManager::from_worktrees(worktrees);
        self.git_manager = Some(manager);

        // Start terminal for active session
        let _ = self.sessions.start_active_terminal();

        // Show changed files by default
        self.current_view = ViewState::ChangedFiles;

        Ok(())
    }

    /// Get current worktree (from active session)
    pub fn current_worktree(&self) -> Option<&WorktreeInfo> {
        self.sessions.active().map(|s| &s.worktree)
    }

    /// Select session by index
    pub fn select_session(&mut self, index: usize) {
        if self.sessions.set_active(index) {
            // Start terminal if not running
            if let Some(session) = self.sessions.active_mut() {
                if !session.is_terminal_running() {
                    let _ = session.start_terminal();
                }
            }
        }
    }

    /// Open a file for viewing
    pub fn open_file(&mut self, path: impl Into<PathBuf>) -> Result<(), String> {
        let path = path.into();
        let full_path = if path.is_absolute() {
            path.clone()
        } else if let Some(wt) = self.current_worktree() {
            wt.path.join(&path)
        } else {
            path.clone()
        };

        let buffer = TextBuffer::from_file(&full_path).map_err(|e| e.to_string())?;

        self.current_view = ViewState::File { path, buffer };
        Ok(())
    }

    /// Show diff for a file
    pub fn show_diff(&mut self, file_path: impl Into<PathBuf>) -> Result<(), String> {
        let file_path = file_path.into();
        let worktree = self
            .current_worktree()
            .ok_or("No worktree selected")?;

        let git_manager = self
            .git_manager
            .as_ref()
            .ok_or("No repository open")?;

        // Get HEAD version
        let old_content = git_manager
            .get_file_content_at_head(&worktree.path, &file_path)
            .unwrap_or_default();

        // Get working tree version
        let full_path = worktree.path.join(&file_path);
        let new_content = std::fs::read_to_string(&full_path).unwrap_or_default();

        // Calculate diff
        let diff_result = calculate_diff(&old_content, &new_content);
        self.side_by_side = to_side_by_side(&diff_result);
        self.diff_result = Some(diff_result);

        self.current_view = ViewState::Diff { path: file_path };

        Ok(())
    }

    /// Process an action
    pub fn process_action(&mut self, action: Action) {
        match action {
            Action::InsertToTerminal(text) => {
                let _ = self.sessions.insert_to_active(&text);
            }
            Action::ExecuteInTerminal(command) => {
                if let Some(session) = self.sessions.active_mut() {
                    let _ = session.execute_in_terminal(&command);
                }
            }
            Action::ShowDiff(path) => {
                let _ = self.show_diff(path);
            }
            Action::CopyToClipboard(_text) => {
                // TODO: Implement clipboard
            }
            Action::None => {}
        }
    }

    /// Process all pending actions
    pub fn process_pending_actions(&mut self) {
        let actions = self.action_queue.take();
        for action in actions {
            self.process_action(action);
        }
    }

    /// Create action for clicking on a file path
    pub fn create_path_action(&self, path: &PathBuf, line: Option<usize>) -> Action {
        let base = self.current_worktree().map(|w| &w.path);
        let formatted = self.path_format.format_path(path, base, line);
        Action::InsertToTerminal(formatted)
    }

    /// Toggle split direction
    pub fn toggle_split_direction(&mut self) {
        self.split_direction.toggle();
    }

    /// Toggle terminal visibility
    pub fn toggle_terminal(&mut self) {
        self.terminal_visible = !self.terminal_visible;
    }

    /// Open editor for a file
    pub fn edit_file(&mut self, path: impl Into<PathBuf>) -> Result<(), String> {
        let path = path.into();
        let full_path = if path.is_absolute() {
            path.clone()
        } else if let Some(wt) = self.current_worktree() {
            wt.path.join(&path)
        } else {
            path.clone()
        };

        // Open in the editor component
        self.markdown_editor.open_any(&full_path)?;

        // Switch view state to show editor in main content area
        self.current_view = ViewState::Edit { path: full_path };

        Ok(())
    }

    /// Get changed files for current worktree
    pub fn get_changed_files(&self) -> Vec<crate::git::FileStatus> {
        if let (Some(manager), Some(worktree)) = (&self.git_manager, self.current_worktree()) {
            manager.get_changed_files(&worktree.path).unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// Get session statuses for status bar
    pub fn get_session_statuses(&self) -> Vec<(String, SessionStatus)> {
        self.sessions
            .sessions()
            .iter()
            .map(|s| (s.display_name().to_string(), s.status))
            .collect()
    }

    /// Handle keyboard shortcuts
    pub fn handle_keyboard(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            // Ctrl+1-9: Select session
            for n in 1..=9 {
                let key = match n {
                    1 => egui::Key::Num1,
                    2 => egui::Key::Num2,
                    3 => egui::Key::Num3,
                    4 => egui::Key::Num4,
                    5 => egui::Key::Num5,
                    6 => egui::Key::Num6,
                    7 => egui::Key::Num7,
                    8 => egui::Key::Num8,
                    9 => egui::Key::Num9,
                    _ => continue,
                };

                if i.modifiers.ctrl && i.key_pressed(key) {
                    self.select_session(n - 1);
                }
            }

            // Ctrl+Tab: Next session
            if i.modifiers.ctrl && i.key_pressed(egui::Key::Tab) {
                self.sessions.next_session();
                self.select_session(self.sessions.active_index());
            }

            // Ctrl+Shift+Tab: Previous session
            if i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::Tab) {
                self.sessions.prev_session();
                self.select_session(self.sessions.active_index());
            }

            // Ctrl+\: Toggle split direction
            if i.modifiers.ctrl && i.key_pressed(egui::Key::Backslash) {
                self.toggle_split_direction();
            }

            // Ctrl+`: Toggle terminal
            if i.modifiers.ctrl && i.key_pressed(egui::Key::Backtick) {
                self.toggle_terminal();
            }

            // Ctrl+O: Open repository
            if i.modifiers.ctrl && i.key_pressed(egui::Key::O) {
                self.show_open_dialog = true;
            }

            // Ctrl+Q: Quit
            if i.modifiers.ctrl && i.key_pressed(egui::Key::Q) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }

            // Escape: Back to changed files view
            if i.key_pressed(egui::Key::Escape) {
                self.current_view = ViewState::ChangedFiles;
            }
        });
    }

    /// Update all session statuses
    pub fn update_sessions(&mut self) {
        self.sessions.update_all_statuses();
    }

    /// Create a new worktree and add a session for it
    pub fn create_worktree(&mut self, branch_name: &str) -> Result<(), String> {
        let git_manager = self
            .git_manager
            .as_ref()
            .ok_or("No repository open")?;

        // Create the worktree
        let worktree_info = git_manager
            .create_worktree(branch_name)
            .map_err(|e| e.to_string())?;

        // Add a new session for the worktree
        self.sessions.add_session(worktree_info);

        // Select the new session (it's at the end)
        let new_index = self.sessions.sessions().len() - 1;
        self.select_session(new_index);

        Ok(())
    }

    /// Remove a worktree and its session
    pub fn remove_worktree(&mut self, worktree_name: &str) -> Result<(), String> {
        let git_manager = self
            .git_manager
            .as_ref()
            .ok_or("No repository open")?;

        // Find the session index
        let session_index = self
            .sessions
            .sessions()
            .iter()
            .position(|s| s.worktree.name == worktree_name)
            .ok_or(format!("Session for worktree '{}' not found", worktree_name))?;

        // Check if it's the main worktree
        if self.sessions.sessions()[session_index].worktree.is_main {
            return Err("Cannot remove the main worktree".to_string());
        }

        // Stop the terminal first to release the directory lock
        self.sessions.stop_session_terminal(session_index);

        // Small delay to ensure processes have released file handles
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Remove the worktree from git
        git_manager
            .remove_worktree(worktree_name)
            .map_err(|e| e.to_string())?;

        // Remove the session
        self.sessions.remove_session(session_index);

        Ok(())
    }

    /// Get list of removable worktrees (non-main worktrees)
    pub fn get_removable_worktrees(&self) -> Vec<String> {
        self.sessions
            .sessions()
            .iter()
            .filter(|s| !s.worktree.is_main)
            .map(|s| s.worktree.name.clone())
            .collect()
    }

    /// Refresh worktrees from git
    pub fn refresh_worktrees(&mut self) -> Result<(), String> {
        let git_manager = self
            .git_manager
            .as_ref()
            .ok_or("No repository open")?;

        let worktrees = git_manager.list_worktrees().map_err(|e| e.to_string())?;

        // Remember current active session's path
        let current_path = self.current_worktree().map(|w| w.path.clone());

        // Recreate sessions
        self.sessions = SessionManager::from_worktrees(worktrees);

        // Try to restore the active session
        if let Some(path) = current_path {
            if let Some(idx) = self.sessions.find_by_path(&path) {
                self.sessions.set_active(idx);
            }
        }

        // Start terminal for active session
        let _ = self.sessions.start_active_terminal();

        Ok(())
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_creation() {
        let app = App::default();
        assert!(app.git_manager.is_none());
        assert!(app.sessions.is_empty());
        assert!(matches!(app.current_view, ViewState::Empty));
    }

    #[test]
    fn test_view_state_default() {
        let state = ViewState::default();
        assert!(matches!(state, ViewState::Empty));
    }

    #[test]
    fn test_toggle_split_direction() {
        let mut app = App::default();
        assert_eq!(app.split_direction, SplitDirection::Horizontal);

        app.toggle_split_direction();
        assert_eq!(app.split_direction, SplitDirection::Vertical);

        app.toggle_split_direction();
        assert_eq!(app.split_direction, SplitDirection::Horizontal);
    }

    #[test]
    fn test_toggle_terminal() {
        let mut app = App::default();
        assert!(app.terminal_visible);

        app.toggle_terminal();
        assert!(!app.terminal_visible);

        app.toggle_terminal();
        assert!(app.terminal_visible);
    }

    #[test]
    fn test_create_path_action() {
        let app = App::default();
        let path = PathBuf::from("/home/user/project/src/main.rs");
        let action = app.create_path_action(&path, Some(42));

        if let Action::InsertToTerminal(s) = action {
            // Without worktree, should use absolute path with line
            assert!(s.contains("main.rs"));
        } else {
            panic!("Expected InsertToTerminal action");
        }
    }

    #[test]
    fn test_get_session_statuses_empty() {
        let app = App::default();
        let statuses = app.get_session_statuses();
        assert!(statuses.is_empty());
    }

    #[test]
    fn test_process_action_switch_session() {
        let mut app = App::default();
        // Add some mock sessions
        app.sessions = SessionManager::from_worktrees(vec![
            WorktreeInfo {
                name: "main".to_string(),
                path: PathBuf::from("/tmp/main"),
                branch: Some("main".to_string()),
                is_main: true,
            },
            WorktreeInfo {
                name: "feature".to_string(),
                path: PathBuf::from("/tmp/feature"),
                branch: Some("feature".to_string()),
                is_main: false,
            },
        ]);

        // Use set_active directly to avoid terminal startup
        app.sessions.set_active(1);
        assert_eq!(app.sessions.active_index(), 1);
    }
}
