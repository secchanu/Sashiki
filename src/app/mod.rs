//! Main application state and logic for Sashiki
//!
//! Implements the Elm architecture pattern:
//! - State: Application data
//! - Message: User interactions and events
//! - Update: State transitions
//! - View: UI rendering

mod keyboard;
mod message;
mod views;

pub use message::Message;
pub use views::{FileListMode, FileSource, ViewState};

use crate::config::Config;
use crate::diff::{calculate_diff, DiffResult};
use crate::git::{FileStatus, GitManager};
use crate::session::SessionManager;
use crate::theme::Palette;

use iced::event::{self, Event};
use iced::keyboard::{key::Named, Key, Modifiers};
use iced::widget::{button, column, container, row, text_editor};
use iced::{Element, Length, Size, Subscription, Task, Theme};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// Layout constants
const DEFAULT_SIDEBAR_WIDTH: f32 = 200.0;
const DEFAULT_TERMINAL_HEIGHT: f32 = 200.0;
const DEFAULT_WINDOW_WIDTH: f32 = 1280.0;
const DEFAULT_WINDOW_HEIGHT: f32 = 800.0;
// Approximate character dimensions for 14px monospace font
const TERMINAL_CHAR_WIDTH: f32 = 8.0;
const TERMINAL_LINE_HEIGHT: f32 = 16.0;
const TERMINAL_PADDING: f32 = 32.0;
const TERMINAL_HEADER_HEIGHT: f32 = 16.0;

// File collection limits
const MAX_FILES_TO_COLLECT: usize = 5000;
const MAX_DIRECTORY_DEPTH: usize = 20;

// Cache invalidation interval
const FILE_CACHE_TTL_MS: u64 = 2000;

/// Application state
pub struct Sashiki {
    pub palette: Palette,
    is_dark_theme: bool,
    git_manager: Option<GitManager>,
    sessions: SessionManager,
    current_view: ViewState,
    diff_result: Option<DiffResult>,
    terminal_visible: bool,
    terminal_height: f32,
    sidebar_width: f32,
    show_open_dialog: bool,
    dialog_path: String,
    show_worktree_dialog: bool,
    worktree_branch: String,
    error_message: Option<String>,
    terminal_focused: bool,
    pending_delete_worktree: Option<String>,
    window_size: Size,
    file_source: FileSource,
    file_list_mode: FileListMode,
    expanded_dirs: HashSet<PathBuf>,
    editor_content: text_editor::Content,
    editor_modified: bool,
    // File list cache
    cached_git_files: Vec<FileStatus>,
    cached_all_files: Vec<PathBuf>,
    cache_updated_at: Option<Instant>,
    cache_worktree_path: Option<PathBuf>,
    // IME preedit text (composition in progress)
    preedit_text: String,
}

impl Sashiki {
    /// Create new application instance
    pub fn new() -> (Self, Task<Message>) {
        let config = Config::load_or_default();
        let is_dark_theme = config.theme == crate::config::Theme::Dark;
        let palette = if is_dark_theme {
            Palette::dark()
        } else {
            Palette::light()
        };

        let mut app = Self {
            palette,
            is_dark_theme,
            git_manager: None,
            sessions: SessionManager::new(),
            current_view: ViewState::Welcome,
            diff_result: None,
            terminal_visible: true,
            terminal_height: DEFAULT_TERMINAL_HEIGHT,
            sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            show_open_dialog: false,
            dialog_path: String::new(),
            show_worktree_dialog: false,
            worktree_branch: String::new(),
            error_message: None,
            terminal_focused: false,
            pending_delete_worktree: None,
            window_size: Size::new(DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT),
            file_source: FileSource::default(),
            file_list_mode: FileListMode::default(),
            expanded_dirs: HashSet::new(),
            editor_content: text_editor::Content::new(),
            editor_modified: false,
            cached_git_files: Vec::new(),
            cached_all_files: Vec::new(),
            cache_updated_at: None,
            cache_worktree_path: None,
            preedit_text: String::new(),
        };

        if let Ok(cwd) = std::env::current_dir() {
            if let Err(e) = app.open_repository(&cwd) {
                tracing::debug!("No repository in current directory: {}", e);
            }
        }

        (app, Task::none())
    }

    /// Update application state based on message
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SelectSession(idx) => {
                self.sessions.set_active(idx);
                if let Err(e) = self.sessions.ensure_active_terminal_running() {
                    tracing::warn!("Failed to start terminal: {}", e);
                    self.error_message = Some(format!("Failed to start terminal: {}", e));
                }
                self.resize_terminal();
                self.invalidate_cache();
                self.refresh_view();
            }
            Message::ToggleTerminal => {
                self.terminal_visible = !self.terminal_visible;
                if self.terminal_visible {
                    self.terminal_focused = true;
                }
            }
            Message::TerminalFocus(focused) => {
                self.terminal_focused = focused;
                if !focused {
                    self.preedit_text.clear();
                }
            }
            Message::TerminalTick => {
                for session in self.sessions.sessions_mut() {
                    session.terminal.process_output();
                }
                // Update file cache periodically
                self.update_cache();
            }
            Message::OpenRepositoryDialog => {
                self.show_open_dialog = true;
                self.dialog_path.clear();
            }
            Message::CloseDialog => {
                self.show_open_dialog = false;
                self.show_worktree_dialog = false;
                self.error_message = None;
            }
            Message::DialogPathChanged(path) => {
                self.dialog_path = path;
            }
            Message::OpenRepository => {
                let path = self.dialog_path.clone();
                if !path.is_empty() {
                    match self.open_repository(&path) {
                        Ok(()) => {
                            self.show_open_dialog = false;
                            self.error_message = None;
                        }
                        Err(e) => {
                            self.error_message = Some(e);
                        }
                    }
                }
            }
            Message::ShowWorktreeDialog => {
                self.show_worktree_dialog = true;
                self.worktree_branch.clear();
            }
            Message::CloseWorktreeDialog => {
                self.show_worktree_dialog = false;
                self.error_message = None;
            }
            Message::WorktreeBranchChanged(branch) => {
                self.worktree_branch = branch;
            }
            Message::CreateWorktree => {
                let branch = self.worktree_branch.clone();
                if !branch.is_empty() {
                    match self.create_worktree(&branch) {
                        Ok(()) => {
                            self.show_worktree_dialog = false;
                            self.error_message = None;
                        }
                        Err(e) => {
                            self.error_message = Some(e);
                        }
                    }
                }
            }
            Message::ShowDeleteConfirm(name) => {
                self.pending_delete_worktree = Some(name);
                self.error_message = None;
            }
            Message::ConfirmDelete => {
                if let Some(name) = self.pending_delete_worktree.take() {
                    match self.remove_worktree(&name) {
                        Ok(()) => {
                            self.pending_delete_worktree = None;
                            self.error_message = None;
                        }
                        Err(e) => {
                            self.error_message = Some(e);
                        }
                    }
                }
            }
            Message::CancelDelete => {
                self.pending_delete_worktree = None;
                self.error_message = None;
            }
            Message::ShowDiff(path) => {
                self.show_diff(&path);
            }
            Message::OpenFile(path) => {
                self.open_file(&path);
            }
            Message::EditFile(path) => {
                self.edit_file(&path);
            }
            Message::EditorAction(action) => {
                let is_edit = action.is_edit();
                self.editor_content.perform(action);
                if is_edit {
                    self.editor_modified = true;
                }
            }
            Message::SaveFile => {
                if let ViewState::Editor { ref path } = self.current_view {
                    // Re-validate path (defense in depth)
                    if !Self::is_safe_path(path) {
                        self.error_message = Some("Invalid file path".to_string());
                        return Task::none();
                    }

                    let Some(session) = self.sessions.active() else {
                        self.error_message = Some("No active session".to_string());
                        return Task::none();
                    };

                    let content = self.editor_content.text();
                    let full_path = session.worktree.path.join(path);

                    if let Err(e) = std::fs::write(&full_path, content) {
                        self.error_message = Some(format!("Save failed: {}", e));
                    } else {
                        self.editor_modified = false;
                        self.error_message = None;
                    }
                }
            }
            Message::CancelEdit => {
                if let ViewState::Editor { path } = self.current_view.clone() {
                    self.open_file(&path);
                }
            }
            Message::InsertPath(path) => {
                if let Some(session) = self.sessions.active_mut() {
                    let path_str = path.display().to_string();
                    if let Err(e) = session.terminal.write_str(&path_str) {
                        tracing::warn!("Failed to insert path to terminal: {}", e);
                    }
                }
            }
            Message::ToggleFileSource => {
                self.file_source.toggle();
            }
            Message::ToggleFileListMode => {
                self.file_list_mode.toggle();
            }
            Message::ToggleDir(path) => {
                if self.expanded_dirs.contains(&path) {
                    self.expanded_dirs.remove(&path);
                } else {
                    self.expanded_dirs.insert(path);
                }
            }
            Message::KeyPressed(key, modifiers, text) => {
                if let Some(task) = self.handle_keyboard(key, modifiers, text) {
                    return task;
                }
            }
            Message::ImeCommit(text) => {
                // Clear preedit text when committed
                self.preedit_text.clear();
                // Send IME-committed text to terminal when focused
                if self.terminal_focused && self.terminal_visible {
                    if let Some(session) = self.sessions.active_mut() {
                        if let Err(e) = session.terminal.write_str(&text) {
                            tracing::debug!("Failed to write IME text to terminal: {}", e);
                        }
                    }
                }
            }
            Message::ImePreedit(text) => {
                // Store preedit text for display
                if self.terminal_focused && self.terminal_visible {
                    self.preedit_text = text;
                }
            }
            Message::WindowResized(size) => {
                self.window_size = size;
                self.resize_terminal();
            }
        }
        Task::none()
    }

    /// Render the application view
    pub fn view(&self) -> Element<'_, Message> {
        let content = row![self.view_sidebar(), self.view_main_content(),];

        let mut layout = column![self.view_menu_bar(), content,];

        if self.terminal_visible {
            layout = layout.push(views::view_terminal(
                self.sessions.active(),
                self.terminal_focused,
                self.terminal_height,
                &self.palette,
                &self.preedit_text,
            ));
        }

        layout = layout.push(views::view_status_bar(&self.sessions, &self.palette));

        let base: Element<Message> = container(layout)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_theme| container::Style {
                background: Some(self.palette.bg_primary.into()),
                ..Default::default()
            })
            .into();

        // Overlay dialogs
        if self.show_open_dialog {
            views::view_open_dialog(
                base,
                &self.dialog_path,
                self.error_message.as_ref(),
                &self.palette,
            )
        } else if self.show_worktree_dialog {
            views::view_worktree_dialog(
                base,
                &self.worktree_branch,
                self.error_message.as_ref(),
                &self.palette,
            )
        } else if let Some(ref name) = self.pending_delete_worktree {
            views::view_delete_confirm_dialog(
                base,
                name,
                self.error_message.as_ref(),
                &self.palette,
            )
        } else {
            base
        }
    }

    /// Get the application theme
    pub fn theme(&self) -> Theme {
        if self.is_dark_theme {
            Theme::Dark
        } else {
            Theme::Light
        }
    }

    /// Subscriptions for background events
    pub fn subscription(&self) -> Subscription<Message> {
        let events = event::listen_with(|event, _status, _id| match event {
            Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            }) => Some(Message::KeyPressed(key, modifiers, text.map(|s| s.to_string()))),
            Event::Window(iced::window::Event::Resized(size)) => Some(Message::WindowResized(size)),
            Event::InputMethod(iced::advanced::input_method::Event::Commit(text)) => {
                Some(Message::ImeCommit(text))
            }
            Event::InputMethod(iced::advanced::input_method::Event::Preedit(text, _)) => {
                Some(Message::ImePreedit(text))
            }
            _ => None,
        });

        let tick = iced::time::every(Duration::from_millis(50)).map(|_| Message::TerminalTick);

        Subscription::batch([events, tick])
    }

    // --- View helpers ---

    fn view_menu_bar(&self) -> Element<'_, Message> {
        let file_menu = button("File")
            .on_press(Message::OpenRepositoryDialog)
            .padding([4, 8]);

        let view_menu = button("View")
            .on_press(Message::ToggleTerminal)
            .padding([4, 8]);

        row![file_menu, view_menu].spacing(4).padding(4).into()
    }

    fn view_sidebar(&self) -> Element<'_, Message> {
        views::view_sidebar(views::SidebarState {
            sessions: &self.sessions,
            file_source: self.file_source,
            file_list_mode: self.file_list_mode,
            git_files: self.get_changed_files(),
            all_files: self.get_all_files(),
            expanded_dirs: &self.expanded_dirs,
            palette: &self.palette,
            width: self.sidebar_width,
        })
    }

    fn view_main_content(&self) -> Element<'_, Message> {
        let content: Element<Message> = match &self.current_view {
            ViewState::Welcome => views::view_welcome(&self.palette, self.git_manager.is_some()),
            ViewState::ChangedFiles => {
                let files = self.get_changed_files();
                views::view_changed_files(&files, &self.palette)
            }
            ViewState::File { path, content } => views::view_file(path, content, &self.palette),
            ViewState::Editor { path } => views::view_editor(
                path,
                &self.editor_content,
                self.editor_modified,
                self.error_message.as_ref(),
                &self.palette,
            ),
            ViewState::Diff { path } => {
                views::view_diff(path, self.diff_result.as_ref(), &self.palette)
            }
        };

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(16)
            .into()
    }

    // --- Keyboard handling ---

    fn handle_keyboard(
        &mut self,
        key: Key,
        modifiers: Modifiers,
        text: Option<String>,
    ) -> Option<Task<Message>> {
        if modifiers.control() {
            match &key {
                Key::Character(c) if c.as_str() == "o" => {
                    self.show_open_dialog = true;
                    return None;
                }
                Key::Character(c) if c.as_str() == "q" => {
                    // Graceful shutdown: cleanup and close window
                    for session in self.sessions.sessions_mut() {
                        session.stop_terminal();
                    }
                    return Some(iced::window::latest().and_then(iced::window::close));
                }
                Key::Character(c) if c.as_str() == "`" => {
                    self.terminal_visible = !self.terminal_visible;
                    if self.terminal_visible {
                        self.terminal_focused = true;
                    }
                    return None;
                }
                Key::Named(Named::Tab) => {
                    self.sessions.next_session();
                    if let Err(e) = self.sessions.ensure_active_terminal_running() {
                        tracing::warn!("Failed to start terminal: {}", e);
                        self.error_message = Some(format!("Failed to start terminal: {}", e));
                    }
                    self.resize_terminal();
                    return None;
                }
                _ => {}
            }

            if let Key::Character(c) = &key {
                if let Ok(n) = c.as_str().parse::<usize>() {
                    if (1..=9).contains(&n) {
                        self.sessions.set_active(n - 1);
                        if let Err(e) = self.sessions.ensure_active_terminal_running() {
                            tracing::warn!("Failed to start terminal: {}", e);
                            self.error_message = Some(format!("Failed to start terminal: {}", e));
                        }
                        self.resize_terminal();
                        return None;
                    }
                }
            }
        }

        if matches!(key, Key::Named(Named::Escape)) {
            if self.show_open_dialog || self.show_worktree_dialog {
                self.show_open_dialog = false;
                self.show_worktree_dialog = false;
            } else if self.pending_delete_worktree.is_some() {
                self.pending_delete_worktree = None;
            } else {
                self.terminal_focused = false;
            }
            return None;
        }

        // Only send keyboard input to terminal when no dialog is open
        let dialog_open =
            self.show_open_dialog || self.show_worktree_dialog || self.pending_delete_worktree.is_some();

        if self.terminal_focused && self.terminal_visible && !dialog_open {
            // Prefer text field if available (includes IME-composed characters)
            if let Some(ref input_text) = text {
                if !input_text.is_empty() {
                    if let Some(session) = self.sessions.active_mut() {
                        if let Err(e) = session.terminal.write_str(input_text) {
                            tracing::debug!("Failed to write text to terminal: {}", e);
                        }
                    }
                    return None;
                }
            }

            // Fall back to key-based conversion for special keys
            let bytes = keyboard::key_to_terminal_bytes(&key, &modifiers);
            if !bytes.is_empty() {
                if let Some(session) = self.sessions.active_mut() {
                    if let Err(e) = session.terminal.write(&bytes) {
                        tracing::debug!("Failed to write to terminal: {}", e);
                    }
                }
            }
        }
        None
    }

    // --- Business logic ---

    fn refresh_view(&mut self) {
        if self.git_manager.is_some() {
            self.current_view = ViewState::ChangedFiles;
        }
    }

    fn resize_terminal(&mut self) {
        let terminal_width =
            self.window_size.width - self.sidebar_width - TERMINAL_PADDING;
        let terminal_rows =
            (self.terminal_height - TERMINAL_HEADER_HEIGHT) / TERMINAL_LINE_HEIGHT;

        let cols = (terminal_width / TERMINAL_CHAR_WIDTH).max(40.0) as u16;
        let rows = terminal_rows.max(10.0) as u16;

        let size = crate::terminal::TerminalSize { rows, cols };

        for session in self.sessions.sessions_mut() {
            if let Err(e) = session.terminal.resize(size) {
                tracing::debug!("Failed to resize terminal: {}", e);
            }
        }
    }

    pub fn open_repository(&mut self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        let path = path.as_ref();
        let manager = GitManager::open(path).map_err(|e| e.to_string())?;

        let worktrees = manager.list_worktrees().map_err(|e| e.to_string())?;
        self.sessions = SessionManager::from_worktrees(worktrees);

        if let Err(e) = self.sessions.start_active_terminal() {
            tracing::warn!("Failed to start terminal: {}", e);
            self.error_message = Some(format!("Failed to start terminal: {}", e));
        }
        self.resize_terminal();

        self.git_manager = Some(manager);
        self.current_view = ViewState::ChangedFiles;
        self.invalidate_cache();

        Ok(())
    }

    fn create_worktree(&mut self, branch: &str) -> Result<(), String> {
        let manager = self.git_manager.as_ref().ok_or("No repository open")?;
        let worktree = manager.create_worktree(branch).map_err(|e| e.to_string())?;
        let idx = self.sessions.add_session(worktree);
        self.sessions.set_active(idx);
        if let Err(e) = self.sessions.start_active_terminal() {
            tracing::warn!("Failed to start terminal: {}", e);
            self.error_message = Some(format!("Failed to start terminal: {}", e));
        }
        self.resize_terminal();
        Ok(())
    }

    fn remove_worktree(&mut self, name: &str) -> Result<(), String> {
        // Find session index once, reuse for both stop and remove
        let session_idx = self
            .sessions
            .sessions()
            .iter()
            .position(|s| s.worktree.name == name);

        if let Some(idx) = session_idx {
            self.sessions.stop_session_terminal(idx);
        }

        let manager = self.git_manager.as_ref().ok_or("No repository open")?;
        manager.remove_worktree(name).map_err(|e| e.to_string())?;

        if let Some(idx) = session_idx {
            self.sessions.remove_session(idx);
        }
        Ok(())
    }

    /// Check if cache is valid
    fn is_cache_valid(&self) -> bool {
        if let Some(updated_at) = self.cache_updated_at {
            if updated_at.elapsed().as_millis() < FILE_CACHE_TTL_MS as u128 {
                // Check if worktree changed
                if let Some(session) = self.sessions.active() {
                    return self.cache_worktree_path.as_ref() == Some(&session.worktree.path);
                }
            }
        }
        false
    }

    /// Invalidate cache (call when files change)
    fn invalidate_cache(&mut self) {
        self.cache_updated_at = None;
    }

    /// Update file cache if needed
    fn update_cache(&mut self) {
        if self.is_cache_valid() {
            return;
        }

        // Update git files
        if let Some(ref manager) = self.git_manager {
            if let Some(session) = self.sessions.active() {
                match manager.get_changed_files(&session.worktree.path) {
                    Ok(files) => {
                        self.cached_git_files = files;
                    }
                    Err(e) => {
                        tracing::debug!("Failed to get changed files: {}", e);
                        self.cached_git_files.clear();
                    }
                }

                // Update all files
                let worktree_path = &session.worktree.path;
                let mut files = Vec::new();
                self.collect_files(worktree_path, worktree_path, &mut files, 0);
                files.sort();
                self.cached_all_files = files;

                self.cache_worktree_path = Some(session.worktree.path.clone());
            }
        } else {
            self.cached_git_files.clear();
            self.cached_all_files.clear();
            self.cache_worktree_path = None;
        }

        self.cache_updated_at = Some(Instant::now());
    }

    fn get_changed_files(&self) -> &[FileStatus] {
        &self.cached_git_files
    }

    fn get_all_files(&self) -> &[PathBuf] {
        &self.cached_all_files
    }

    /// Check if a relative path is safe (no directory traversal)
    fn is_safe_path(path: &Path) -> bool {
        // Reject absolute paths
        if path.is_absolute() {
            return false;
        }

        // Reject paths containing parent directory references
        for component in path.components() {
            if matches!(component, std::path::Component::ParentDir) {
                return false;
            }
        }

        true
    }

    fn collect_files(
        &self,
        base: &std::path::Path,
        dir: &std::path::Path,
        files: &mut Vec<PathBuf>,
        depth: usize,
    ) {
        // Stop collecting if we've reached the limits
        if files.len() >= MAX_FILES_TO_COLLECT || depth >= MAX_DIRECTORY_DEPTH {
            return;
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                if files.len() >= MAX_FILES_TO_COLLECT {
                    return;
                }

                let path = entry.path();
                let file_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                if file_name.starts_with('.') || file_name == "node_modules" || file_name == "target"
                {
                    continue;
                }

                if path.is_dir() {
                    self.collect_files(base, &path, files, depth + 1);
                } else if let Ok(rel) = path.strip_prefix(base) {
                    files.push(rel.to_path_buf());
                }
            }
        }
    }

    fn show_diff(&mut self, path: &Path) {
        if let Some(ref manager) = self.git_manager {
            if let Some(session) = self.sessions.active() {
                // Validate path to prevent directory traversal
                if !Self::is_safe_path(path) {
                    self.error_message = Some("Invalid file path".to_string());
                    return;
                }

                // Get old content from HEAD (empty string for new files)
                let old_content = manager
                    .get_file_content_at_head(&session.worktree.path, path)
                    .unwrap_or_else(|e| {
                        tracing::debug!("Could not get file at HEAD (may be new file): {}", e);
                        String::new()
                    });

                // Get new content from filesystem (empty string for deleted files)
                let full_path = session.worktree.path.join(path);
                let new_content = std::fs::read_to_string(&full_path).unwrap_or_else(|e| {
                    tracing::debug!("Could not read file (may be deleted): {}", e);
                    String::new()
                });

                let result = calculate_diff(&old_content, &new_content);
                self.diff_result = Some(result);
                self.current_view = ViewState::Diff { path: path.to_path_buf() };
            }
        }
    }

    fn open_file(&mut self, path: &Path) {
        // Validate path to prevent directory traversal
        if !Self::is_safe_path(path) {
            self.error_message = Some("Invalid file path".to_string());
            return;
        }

        if let Some(session) = self.sessions.active() {
            let full_path = session.worktree.path.join(path);
            match std::fs::read_to_string(&full_path) {
                Ok(content) => {
                    self.current_view = ViewState::File {
                        path: path.to_path_buf(),
                        content,
                    };
                    self.error_message = None;
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to open file: {}", e));
                }
            }
        }
    }

    fn edit_file(&mut self, path: &Path) {
        // Validate path to prevent directory traversal
        if !Self::is_safe_path(path) {
            self.error_message = Some("Invalid file path".to_string());
            return;
        }

        if let Some(session) = self.sessions.active() {
            let full_path = session.worktree.path.join(path);
            match std::fs::read_to_string(&full_path) {
                Ok(content) => {
                    self.editor_content = text_editor::Content::with_text(&content);
                    self.editor_modified = false;
                    self.error_message = None;
                    self.current_view = ViewState::Editor { path: path.to_path_buf() };
                }
                Err(e) => {
                    self.error_message = Some(format!("Failed to open file for editing: {}", e));
                }
            }
        }
    }
}

impl Default for Sashiki {
    fn default() -> Self {
        Self::new().0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_view_state_default() {
        let state = ViewState::default();
        assert!(matches!(state, ViewState::Welcome));
    }

    #[test]
    fn test_file_source_toggle() {
        let mut source = FileSource::Git;
        source.toggle();
        assert_eq!(source, FileSource::All);
        source.toggle();
        assert_eq!(source, FileSource::Git);
    }

    #[test]
    fn test_file_list_mode_toggle() {
        let mut mode = FileListMode::Flat;
        mode.toggle();
        assert_eq!(mode, FileListMode::Tree);
        mode.toggle();
        assert_eq!(mode, FileListMode::Flat);
    }

    #[test]
    fn test_is_safe_path_valid() {
        use std::path::Path;

        // Valid relative paths
        assert!(Sashiki::is_safe_path(Path::new("file.txt")));
        assert!(Sashiki::is_safe_path(Path::new("src/main.rs")));
        assert!(Sashiki::is_safe_path(Path::new("foo/bar/baz.txt")));
    }

    #[test]
    fn test_is_safe_path_absolute_paths() {
        use std::path::Path;

        // Absolute paths should be rejected
        #[cfg(unix)]
        {
            assert!(!Sashiki::is_safe_path(Path::new("/etc/passwd")));
            assert!(!Sashiki::is_safe_path(Path::new("/home/user/file.txt")));
        }
        #[cfg(windows)]
        {
            assert!(!Sashiki::is_safe_path(Path::new("C:\\Windows\\System32")));
            assert!(!Sashiki::is_safe_path(Path::new("\\\\server\\share")));
        }
    }

    #[test]
    fn test_is_safe_path_parent_directory() {
        use std::path::Path;

        // Paths with parent directory references should be rejected
        assert!(!Sashiki::is_safe_path(Path::new("../file.txt")));
        assert!(!Sashiki::is_safe_path(Path::new("foo/../bar.txt")));
        assert!(!Sashiki::is_safe_path(Path::new("foo/bar/../../baz.txt")));
        assert!(!Sashiki::is_safe_path(Path::new("..")));
    }

    #[test]
    fn test_is_safe_path_current_dir() {
        use std::path::Path;

        // Paths with current directory references should be allowed
        assert!(Sashiki::is_safe_path(Path::new("./file.txt")));
        assert!(Sashiki::is_safe_path(Path::new("foo/./bar.txt")));
        assert!(Sashiki::is_safe_path(Path::new("./src/./main.rs")));
    }

    #[test]
    fn test_is_safe_path_edge_cases() {
        use std::path::Path;

        // Empty path should be allowed (handled elsewhere)
        assert!(Sashiki::is_safe_path(Path::new("")));

        // Single dot should be allowed
        assert!(Sashiki::is_safe_path(Path::new(".")));

        // Deeply nested paths should be allowed
        assert!(Sashiki::is_safe_path(Path::new("a/b/c/d/e/f/g.txt")));

        // Paths with special characters in names
        assert!(Sashiki::is_safe_path(Path::new("file with spaces.txt")));
        assert!(Sashiki::is_safe_path(Path::new("file-with-dashes.txt")));
        assert!(Sashiki::is_safe_path(Path::new("file_with_underscores.txt")));
    }
}
