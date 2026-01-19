//! Sashiki - Git worktree manager with integrated terminals
//!
//! Each git worktree gets its own terminal session, making it easy to work on
//! multiple branches simultaneously.

mod git;
mod session;
mod terminal;
mod theme;
mod ui;

use git::{ChangedFile, ChangeType, GitRepo};
use gpui::{
    actions, div, px, prelude::*, rgb, rgba, App, Application, Context, Entity, FocusHandle,
    Focusable, IntoElement, KeyBinding, KeyDownEvent, ParentElement, Render, Styled, Window,
    WindowOptions,
};
use session::{SessionManager, SessionStatus, ViewMode};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use terminal::TerminalView;
use theme::*;
use ui::{ChangeInfo, FileListMode, FileTreeNode, FileViewer, read_dir_shallow};

actions!(
    sashiki,
    [
        ToggleParallelMode,
        NextSession,
        PrevSession,
        ToggleSidebar,
        ToggleFileList,
        RefreshAll,
        CreateWorktree,
        CloseFileViewer,
    ]
);

struct SashikiApp {
    session_manager: SessionManager,
    changed_files: Vec<ChangedFile>,
    file_list_mode: FileListMode,
    expanded_dirs: HashSet<PathBuf>,
    file_tree: Option<FileTreeNode>,
    file_viewer: Entity<FileViewer>,
    git_repo: Option<GitRepo>,
    /// Cached repo for active worktree (avoids repeated Repository::discover() calls)
    cached_worktree_repo: Option<GitRepo>,
    cached_worktree_path: Option<PathBuf>,
    show_sidebar: bool,
    show_file_list: bool,
    show_file_viewer: bool,
    show_create_dialog: bool,
    create_branch_input: String,
    show_delete_confirm: bool,
    delete_target_index: Option<usize>,
    focus_handle: FocusHandle,
    create_dialog_focus: FocusHandle,
    fallback_terminal: Option<Entity<TerminalView>>,
    error_message: Option<String>,
}

impl SashikiApp {
    fn new(cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let create_dialog_focus = cx.focus_handle();
        let file_viewer = cx.new(FileViewer::new);

        let git_repo = GitRepo::open(".").ok();

        let mut session_manager = SessionManager::new();
        let fallback_terminal;

        if let Some(ref repo) = git_repo {
            if let Ok(worktrees) = repo.list_worktrees() {
                if !worktrees.is_empty() {
                    session_manager.init_from_worktrees(worktrees);
                    session_manager.start_session(0, cx);
                    session_manager.switch_to(0);
                    fallback_terminal = None;
                } else {
                    fallback_terminal = Some(cx.new(TerminalView::new));
                }
            } else {
                fallback_terminal = Some(cx.new(TerminalView::new));
            }
        } else {
            fallback_terminal = Some(cx.new(TerminalView::new));
        }

        let mut app = Self {
            session_manager,
            changed_files: Vec::new(),
            file_list_mode: FileListMode::default(),
            expanded_dirs: HashSet::new(),
            file_tree: None,
            file_viewer,
            git_repo,
            cached_worktree_repo: None,
            cached_worktree_path: None,
            show_sidebar: true,
            show_file_list: true,
            show_file_viewer: false,
            show_create_dialog: false,
            create_branch_input: String::new(),
            show_delete_confirm: false,
            delete_target_index: None,
            focus_handle,
            create_dialog_focus,
            fallback_terminal,
            error_message: None,
        };

        app.refresh_changed_files();
        app.build_file_tree();
        app
    }

    fn validate_branch_name(name: &str) -> Result<(), &'static str> {
        if name.is_empty() {
            return Err("Branch name cannot be empty");
        }
        if name.starts_with('/') || name.ends_with('/') {
            return Err("Branch name cannot start or end with /");
        }
        if name.starts_with('.') || name.ends_with('.') {
            return Err("Branch name cannot start or end with .");
        }
        if name.contains("..") {
            return Err("Branch name cannot contain ..");
        }
        if name.contains("//") {
            return Err("Branch name cannot contain //");
        }
        if name.ends_with(".lock") {
            return Err("Branch name cannot end with .lock");
        }
        Ok(())
    }

    /// Refresh file list and rebuild tree for the active session
    fn refresh_file_list(&mut self) {
        self.invalidate_worktree_repo_cache();
        self.refresh_changed_files();
        if self.file_list_mode == FileListMode::Changes {
            self.build_file_tree();
        }
    }

    fn refresh_changed_files(&mut self) {
        // Get the active session's worktree path for accurate file status
        let worktree_path = self.session_manager.active_session()
            .map(|s| s.worktree.path.clone());

        if let Some(path) = worktree_path {
            // Open repository from the worktree path to get correct status
            if let Ok(repo) = GitRepo::open(&path)
                && let Ok(files) = repo.get_changed_files() {
                    self.changed_files = files;
                    return;
                }
        }

        // Fallback to main repo if no active session or worktree-specific open fails
        if let Some(ref repo) = self.git_repo
            && let Ok(files) = repo.get_changed_files() {
                self.changed_files = files;
            }
    }

    /// Get or create cached GitRepo for the active worktree.
    /// Returns None if no active session or if opening fails.
    fn get_worktree_repo(&mut self) -> Option<&GitRepo> {
        let worktree_path = self.session_manager.active_session()
            .map(|s| s.worktree.path.clone())?;

        // Check if cache is valid
        if self.cached_worktree_path.as_ref() == Some(&worktree_path) {
            return self.cached_worktree_repo.as_ref();
        }

        // Cache miss - open repository and cache it
        if let Ok(repo) = GitRepo::open(&worktree_path) {
            self.cached_worktree_repo = Some(repo);
            self.cached_worktree_path = Some(worktree_path);
            self.cached_worktree_repo.as_ref()
        } else {
            self.cached_worktree_repo = None;
            self.cached_worktree_path = None;
            None
        }
    }

    /// Invalidate worktree repo cache (call when switching sessions)
    fn invalidate_worktree_repo_cache(&mut self) {
        self.cached_worktree_repo = None;
        self.cached_worktree_path = None;
    }

    /// Build file tree for Changes mode
    fn build_file_tree(&mut self) {
        let files = self.changed_files.iter().map(|f| {
            let info = ChangeInfo {
                change_type: f.change_type,
                staged: f.staged,
            };
            (f.path.clone(), Some(info))
        });
        self.file_tree = Some(FileTreeNode::from_files(files));
    }

    fn toggle_dir_expanded(&mut self, path: &Path) {
        if self.expanded_dirs.contains(path) {
            self.expanded_dirs.remove(path);
        } else {
            self.expanded_dirs.insert(path.to_path_buf());
        }
    }

    fn refresh_worktrees(&mut self, cx: &mut Context<Self>) {
        if let Some(ref repo) = self.git_repo
            && let Ok(worktrees) = repo.list_worktrees() {
                let existing_paths: Vec<_> = self
                    .session_manager
                    .sessions()
                    .iter()
                    .map(|s| s.worktree.path.clone())
                    .collect();

                for worktree in worktrees {
                    if !existing_paths.contains(&worktree.path) {
                        self.session_manager.add_session(worktree);
                    }
                }
            }
        cx.notify();
    }

    fn active_terminal(&self) -> Option<Entity<TerminalView>> {
        self.session_manager
            .active_terminal()
            .or_else(|| self.fallback_terminal.clone())
    }

    fn on_toggle_parallel(
        &mut self,
        _: &ToggleParallelMode,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session_manager.toggle_view_mode();
        cx.notify();
    }

    /// Start terminal for active session, focus it, and refresh file list
    fn activate_and_focus_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session_manager.start_active_session(cx);
        if let Some(terminal) = self.active_terminal() {
            let focus = terminal.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
        self.refresh_file_list();
        cx.notify();
    }

    fn on_next_session(&mut self, _: &NextSession, window: &mut Window, cx: &mut Context<Self>) {
        self.session_manager.next_session();
        self.activate_and_focus_session(window, cx);
    }

    fn on_prev_session(&mut self, _: &PrevSession, window: &mut Window, cx: &mut Context<Self>) {
        self.session_manager.prev_session();
        self.activate_and_focus_session(window, cx);
    }

    fn on_toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        self.show_sidebar = !self.show_sidebar;
        cx.notify();
    }

    fn on_toggle_file_list(&mut self, _: &ToggleFileList, _: &mut Window, cx: &mut Context<Self>) {
        self.show_file_list = !self.show_file_list;
        cx.notify();
    }

    fn on_refresh_all(&mut self, _: &RefreshAll, _: &mut Window, cx: &mut Context<Self>) {
        self.refresh_worktrees(cx);
        self.refresh_file_list();
        cx.notify();
    }

    fn on_session_selected(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.session_manager.switch_to(index);
        self.activate_and_focus_session(window, cx);
    }

    fn on_toggle_parallel_visibility(&mut self, index: usize, cx: &mut Context<Self>) {
        self.session_manager.toggle_parallel_visibility(index);
        cx.notify();
    }

    fn show_create_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_create_dialog = true;
        self.create_branch_input.clear();
        window.focus(&self.create_dialog_focus, cx);
        cx.notify();
    }

    fn hide_create_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_create_dialog = false;
        self.create_branch_input.clear();
        // Return focus to terminal
        if let Some(terminal) = self.active_terminal() {
            let focus = terminal.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    fn submit_create_worktree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let branch = self.create_branch_input.trim().to_string();

        if let Err(msg) = Self::validate_branch_name(&branch) {
            self.error_message = Some(msg.to_string());
            cx.notify();
            return;
        }

        let result = (|| -> Result<(), String> {
            let repo = self.git_repo.as_ref().ok_or("Git repository not available")?;
            let path = repo
                .generate_worktree_path(&branch)
                .ok_or("Failed to generate worktree path")?;

            // Check if worktree path already exists
            if path.exists() {
                return Err(format!(
                    "Worktree directory already exists: {}\nPlease remove it manually or choose a different branch name.",
                    path.display()
                ));
            }

            // Ensure parent directory exists (git2 creates the worktree directory itself)
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create parent directory: {}", e))?;
            }

            // Worktree name uses safe format (/ -> -)
            let worktree_name = branch.replace('/', "-");

            let worktree = repo
                .create_worktree(&worktree_name, &branch, &path)
                .map_err(|e| format!("Failed to create worktree: {}", e))?;

            self.session_manager.add_session(worktree);
            let new_index = self.session_manager.len() - 1;
            self.session_manager.switch_to(new_index);
            self.session_manager.start_active_session(cx);
            Ok(())
        })();

        if let Err(msg) = result {
            self.error_message = Some(msg);
        } else {
            self.refresh_file_list();
        }

        self.hide_create_dialog(window, cx);
    }

    fn show_delete_confirm(&mut self, index: usize, cx: &mut Context<Self>) {
        let sessions = self.session_manager.sessions();
        if index < sessions.len() && !sessions[index].is_main() {
            self.delete_target_index = Some(index);
            self.show_delete_confirm = true;
            cx.notify();
        }
    }

    fn hide_delete_confirm(&mut self, cx: &mut Context<Self>) {
        self.show_delete_confirm = false;
        self.delete_target_index = None;
        cx.notify();
    }

    fn confirm_delete_worktree(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.delete_target_index else {
            self.hide_delete_confirm(cx);
            return;
        };

        let sessions = self.session_manager.sessions();
        if index >= sessions.len() || sessions[index].is_main() {
            self.hide_delete_confirm(cx);
            return;
        }

        let name = sessions[index].name().to_string();
        let worktree_path = sessions[index].worktree.path.clone();

        self.prepare_session_for_deletion(index, cx);
        self.cleanup_resources_for_deletion(index, cx);

        if let Some(ref repo) = self.git_repo {
            // Ignore errors - git worktree may already be removed or invalid
            let _ = repo.remove_worktree(&name);
        }

        if let Err(e) = self.remove_worktree_directory(&worktree_path) {
            self.error_message = Some(e);
            self.hide_delete_confirm(cx);
            return;
        }

        self.session_manager.remove_session(index);
        self.refresh_file_list();
        self.hide_delete_confirm(cx);
    }

    fn prepare_session_for_deletion(&mut self, index: usize, cx: &mut Context<Self>) {
        let is_active = self.session_manager.active_index() == index;
        if !is_active {
            return;
        }

        let new_index = self
            .session_manager
            .sessions()
            .iter()
            .position(|s| s.is_main())
            .or_else(|| (0..self.session_manager.len()).find(|&i| i != index));

        if let Some(new_idx) = new_index {
            self.session_manager.switch_to(new_idx);
            self.session_manager.start_active_session(cx);
        }
    }

    fn cleanup_resources_for_deletion(&mut self, index: usize, cx: &mut Context<Self>) {
        // Send exit command to shell to release directory lock
        if let Some(terminal) = self.session_manager.get_terminal(index) {
            terminal.update(cx, |view, _cx| view.shutdown());
        }

        self.invalidate_worktree_repo_cache();

        self.file_viewer.update(cx, |viewer, _cx| viewer.close());
        self.show_file_viewer = false;

        self.session_manager.stop_session(index);
    }

    fn remove_worktree_directory(&self, path: &Path) -> Result<(), String> {
        const MAX_RETRIES: u32 = 10;
        const RETRY_DELAY_MS: u64 = 100;

        if !path.exists() {
            return Ok(());
        }

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS));
            }

            match std::fs::remove_dir_all(path) {
                Ok(_) => return Ok(()),
                Err(e) if attempt == MAX_RETRIES - 1 => {
                    return Err(format!(
                        "Failed to remove worktree directory '{}': {}",
                        path.display(),
                        e
                    ));
                }
                Err(_) => continue,
            }
        }

        Err(format!(
            "Failed to remove worktree directory '{}': Directory still in use",
            path.display()
        ))
    }

    fn on_file_selected(&mut self, path: PathBuf, change_type: Option<ChangeType>, cx: &mut Context<Self>) {
        let full_path = if let Some(session) = self.session_manager.active_session() {
            session.worktree.path.join(&path)
        } else {
            path.clone()
        };

        // Use cached worktree repo (avoids expensive Repository::discover() on every file select)
        let diff = self.get_worktree_repo().and_then(|repo| {
            match change_type {
                Some(ChangeType::Added) => repo.generate_added_diff(&full_path).ok(),
                Some(ChangeType::Deleted) => repo.generate_deleted_diff(&full_path).ok(),
                _ => repo.get_file_diff(&full_path).ok(),
            }
        });

        self.file_viewer.update(cx, |viewer, _cx| {
            match change_type {
                Some(ChangeType::Deleted) => {
                    // For deleted files, show diff without reading file
                    if let Some(diff_content) = diff {
                        viewer.open_deleted_file_with_diff(full_path.clone(), diff_content);
                    }
                }
                _ => {
                    // Ignore file read errors - viewer will show empty/error state
                    if let Some(diff_content) = diff {
                        let _ = viewer.open_file_with_diff(full_path.clone(), diff_content);
                    } else {
                        let _ = viewer.open_file(full_path.clone());
                    }
                }
            }
        });

        self.show_file_viewer = true;
        cx.notify();
    }

    fn on_close_file_viewer(
        &mut self,
        _: &CloseFileViewer,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_file_viewer = false;
        self.file_viewer.update(cx, |viewer, _cx| {
            viewer.close();
        });
        cx.notify();
    }

    fn render_terminal_panel(
        &self,
        session_index: usize,
        is_focused: bool,
        _cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let sessions = self.session_manager.sessions();
        let session = &sessions[session_index];
        let color = session.color.primary;
        let name = session.name().to_string();
        let branch = session.branch().map(|s| s.to_string());
        let is_main = session.is_main();
        let status = session.status;
        let path_display = session.worktree.path.to_string_lossy().to_string();

        let terminal_content: gpui::AnyElement = if let Some(ref terminal) = session.terminal {
            div()
                .flex_1()
                .overflow_hidden()
                .child(terminal.clone())
                .into_any_element()
        } else {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(BG_BASE))
                .text_color(rgb(TEXT_MUTED))
                .child("Click to start terminal")
                .into_any_element()
        };

        div()
            .flex_1()
            .flex()
            .flex_col()
            .overflow_hidden()
            .border_2()
            .border_color(if is_focused {
                rgb(color)
            } else {
                rgb(BG_SURFACE0)
            })
            .rounded_md()
            .m_1()
            .child(
                div()
                    .h_8()
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(rgb(BG_MANTLE))
                    .border_b_2()
                    .border_color(rgb(color))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(match status {
                                        SessionStatus::Active => rgb(GREEN),
                                        SessionStatus::Running => rgb(YELLOW),
                                        SessionStatus::Stopped => rgb(TEXT_MUTED),
                                    })
                                    .text_sm()
                                    .child(status.symbol()),
                            )
                            .child(div().w_2().h_2().rounded_full().bg(rgb(color)))
                            .child(
                                div()
                                    .text_color(rgb(color))
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child(name),
                            )
                            .when(is_main, |el| {
                                el.child(
                                    div()
                                        .px_1()
                                        .bg(rgb(GREEN))
                                        .text_color(rgb(BG_BASE))
                                        .text_xs()
                                        .rounded_sm()
                                        .child("main"),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .when(branch.is_some(), |el| {
                                let branch_name = branch.unwrap();
                                el.child(
                                    div()
                                        .text_color(rgb(TEXT_MUTED))
                                        .text_xs()
                                        .child(format!("⎇ {}", branch_name)),
                                )
                            })
                            .child(
                                div()
                                    .text_color(rgb(BG_SURFACE1))
                                    .text_xs()
                                    .max_w_48()
                                    .truncate()
                                    .child(path_display),
                            ),
                    ),
            )
            .child(terminal_content)
            .into_any_element()
    }

    fn render_single_mode(&self, cx: &Context<Self>) -> gpui::AnyElement {
        if self.session_manager.is_empty() {
            if let Some(ref terminal) = self.fallback_terminal {
                return div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(
                        div()
                            .h_8()
                            .px_3()
                            .flex()
                            .items_center()
                            .bg(rgb(BG_MANTLE))
                            .border_b_2()
                            .border_color(rgb(BLUE))
                            .child(
                                div()
                                    .text_color(rgb(BLUE))
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("Terminal (No Git Repository)"),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .child(terminal.clone()),
                    )
                    .into_any_element();
            }
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(TEXT_MUTED))
                .child("No sessions")
                .into_any_element();
        }

        let active_index = self.session_manager.active_index();
        self.render_terminal_panel(active_index, true, cx)
    }

    fn render_parallel_mode(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let parallel_sessions = self.session_manager.parallel_sessions();
        if parallel_sessions.is_empty() {
            return self.render_single_mode(cx);
        }

        let active_index = self.session_manager.active_index();
        let count = parallel_sessions.len();

        let (rows, cols) = match count {
            1 => (1, 1),
            2 => (1, 2),
            3 | 4 => (2, 2),
            5 | 6 => (2, 3),
            _ => (3, 3),
        };

        let mut row_elements: Vec<gpui::AnyElement> = Vec::new();

        for row in 0..rows {
            let mut col_elements: Vec<gpui::AnyElement> = Vec::new();

            for col in 0..cols {
                let grid_index = row * cols + col;
                if grid_index < count {
                    let (session_index, _) = parallel_sessions[grid_index];
                    let is_focused = session_index == active_index;
                    col_elements.push(self.render_terminal_panel(session_index, is_focused, cx));
                } else {
                    col_elements.push(div().flex_1().into_any_element());
                }
            }

            row_elements.push(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .children(col_elements)
                    .into_any_element(),
            );
        }

        div()
            .flex_1()
            .flex()
            .flex_col()
            .children(row_elements)
            .into_any_element()
    }

    fn render_terminal_area(&self, cx: &Context<Self>) -> gpui::AnyElement {
        match self.session_manager.view_mode() {
            ViewMode::Single => self.render_single_mode(cx),
            ViewMode::Parallel => self.render_parallel_mode(cx),
        }
    }

    fn render_sidebar(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let sessions = self.session_manager.sessions();
        let active_index = self.session_manager.active_index();
        let view_mode = self.session_manager.view_mode();

        div()
            .w_56()
            .h_full()
            .bg(rgb(BG_MANTLE))
            .border_r_1()
            .border_color(rgb(BG_SURFACE0))
            .flex()
            .flex_col()
            .child(
                div()
                    .h_8()
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(rgb(BG_BASE))
                    .border_b_1()
                    .border_color(rgb(BG_SURFACE0))
                    .child(
                        div()
                            .text_color(rgb(BLUE))
                            .text_sm()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("Sessions"),
                    )
                    .child(
                        div().text_color(rgb(TEXT_MUTED)).text_xs().child(format!(
                            "{}/{}",
                            self.session_manager.running_count(),
                            sessions.len()
                        )),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .children(sessions.iter().enumerate().map(|(i, session)| {
                        let is_selected = i == active_index;
                        let name = session.name().to_string();
                        let branch = session.branch().map(|s| s.to_string());
                        let is_main = session.is_main();
                        let color = session.color.primary;
                        let status = session.status;
                        let visible_in_parallel = session.visible_in_parallel;

                        div()
                            .id(format!("session-{}", i))
                            .px_3()
                            .py_2()
                            .cursor_pointer()
                            .when(is_selected, |el| el.bg(rgb(BG_SURFACE0)))
                            .hover(|el| el.bg(rgb(BG_SURFACE1)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.on_session_selected(i, window, cx);
                            }))
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_color(match status {
                                        SessionStatus::Active => rgb(GREEN),
                                        SessionStatus::Running => rgb(YELLOW),
                                        SessionStatus::Stopped => rgb(TEXT_MUTED),
                                    })
                                    .text_sm()
                                    .child(status.symbol()),
                            )
                            .child(div().w_2().h_2().rounded_full().bg(rgb(color)))
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .overflow_hidden()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_color(rgb(TEXT))
                                                    .text_sm()
                                                    .truncate()
                                                    .child(name),
                                            )
                                            .when(is_main, |el| {
                                                el.child(
                                                    div()
                                                        .px_1()
                                                        .bg(rgb(GREEN))
                                                        .text_color(rgb(BG_BASE))
                                                        .text_xs()
                                                        .rounded_sm()
                                                        .child("main"),
                                                )
                                            }),
                                    )
                                    .when(branch.is_some(), |el| {
                                        let b = branch.clone().unwrap();
                                        el.child(
                                            div()
                                                .text_color(rgb(TEXT_MUTED))
                                                .text_xs()
                                                .truncate()
                                                .child(format!("⎇ {}", b)),
                                        )
                                    }),
                            )
                            .when(view_mode == ViewMode::Parallel, |el| {
                                el.child(
                                    div()
                                        .id(format!("parallel-toggle-{}", i))
                                        .px_1()
                                        .cursor_pointer()
                                        .text_xs()
                                        .text_color(if visible_in_parallel {
                                            rgb(BLUE)
                                        } else {
                                            rgb(BG_SURFACE1)
                                        })
                                        .on_click(cx.listener(
                                            move |this, _event: &gpui::ClickEvent, _, cx| {
                                                this.on_toggle_parallel_visibility(i, cx);
                                            },
                                        ))
                                        .child(if visible_in_parallel { "◉" } else { "○" }),
                                )
                            })
                            .when(!is_main, |el| {
                                el.child(
                                    div()
                                        .id(format!("delete-{}", i))
                                        .px_1()
                                        .cursor_pointer()
                                        .text_xs()
                                        .text_color(rgb(TEXT_MUTED))
                                        .hover(|el| el.text_color(rgb(RED)))
                                        .on_click(cx.listener(
                                            move |this, _event: &gpui::ClickEvent, _, cx| {
                                                this.show_delete_confirm(i, cx);
                                            },
                                        ))
                                        .child("×"),
                                )
                            })
                    })),
            )
            .when(sessions.is_empty(), |this: gpui::Div| {
                this.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(TEXT_MUTED))
                        .text_sm()
                        .child("No worktrees"),
                )
            })
            .child(
                div()
                    .border_t_1()
                    .border_color(rgb(BG_SURFACE0))
                    .px_3()
                    .py_2()
                    .child(
                        div()
                            .id("create-worktree-btn")
                            .w_full()
                            .px_3()
                            .py_2()
                            .cursor_pointer()
                            .rounded_sm()
                            .bg(rgb(BG_SURFACE0))
                            .hover(|el| el.bg(rgb(BG_SURFACE1)))
                            .text_center()
                            .text_xs()
                            .text_color(rgb(GREEN))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_create_dialog(window, cx);
                            }))
                            .child("+ Create Worktree"),
                    ),
            )
            .into_any_element()
    }

    fn render_create_dialog(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let input_value = self.create_branch_input.clone();

        // Container with both backdrop and dialog as siblings (not parent-child)
        // This prevents click events from bubbling from dialog to backdrop
        div()
            .id("create-dialog-container")
            .track_focus(&self.create_dialog_focus)
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke.key;
                if key == "escape" {
                    this.hide_create_dialog(window, cx);
                } else if key == "enter" {
                    this.submit_create_worktree(window, cx);
                } else if key == "backspace" {
                    this.create_branch_input.pop();
                    cx.notify();
                } else if key.len() == 1 {
                    let c = key.chars().next().unwrap();
                    if c.is_alphanumeric() || matches!(c, '-' | '_' | '/' | '.') {
                        this.create_branch_input.push(c);
                        cx.notify();
                    }
                }
            }))
            // Backdrop (sibling, behind dialog)
            .child(
                div()
                    .id("create-dialog-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.hide_create_dialog(window, cx);
                        }),
                    ),
            )
            // Dialog container (sibling, in front of backdrop)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("create-dialog")
                            .occlude()
                            .w_80()
                            .bg(rgb(BG_BASE))
                            .border_1()
                            .border_color(rgb(BG_SURFACE1))
                            .rounded_md()
                            .shadow_lg()
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .text_color(rgb(TEXT))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("Create Worktree"),
                            )
                            .child(
                                div()
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .child(
                                        div()
                                            .text_color(rgb(TEXT_SECONDARY))
                                            .text_sm()
                                            .child("Enter branch name:"),
                                    )
                                    .child(
                                        div()
                                            .id("branch-input")
                                            .w_full()
                                            .px_3()
                                            .py_2()
                                            .bg(rgb(BG_SURFACE0))
                                            .border_1()
                                            .border_color(rgb(BLUE))
                                            .rounded_sm()
                                            .cursor_text()
                                            .text_color(if input_value.is_empty() {
                                                rgb(TEXT_MUTED)
                                            } else {
                                                rgb(TEXT)
                                            })
                                            .text_sm()
                                            .child(if input_value.is_empty() {
                                                "feature/my-branch".to_string()
                                            } else {
                                                format!("{}_", input_value)
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_color(rgb(TEXT_MUTED))
                                            .text_xs()
                                            .child("If the branch doesn't exist, it will be created from HEAD."),
                                    ),
                            )
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_t_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .flex()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id("cancel-create")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.hide_create_dialog(window, cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("submit-create")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(GREEN))
                                            .hover(|el| el.bg(rgb(TEAL)))
                                            .text_xs()
                                            .text_color(rgb(BG_BASE))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.submit_create_worktree(window, cx);
                                            }))
                                            .child("Create"),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_delete_confirm(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let target_name = self
            .delete_target_index
            .and_then(|i| self.session_manager.sessions().get(i))
            .map(|s| s.name().to_string())
            .unwrap_or_default();

        div()
            .id("delete-confirm-container")
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let key = &event.keystroke.key;
                if key == "escape" {
                    this.hide_delete_confirm(cx);
                } else if key == "enter" {
                    this.confirm_delete_worktree(cx);
                }
            }))
            // Backdrop (sibling)
            .child(
                div()
                    .id("delete-confirm-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.hide_delete_confirm(cx);
                        }),
                    ),
            )
            // Dialog container (sibling)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("delete-confirm-dialog")
                            .occlude()
                            .w_96()
                            .bg(rgb(BG_BASE))
                            .border_1()
                            .border_color(rgb(RED))
                            .rounded_md()
                            .shadow_lg()
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .text_color(rgb(RED))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("Delete Worktree"),
                            )
                            .child(
                                div()
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .child(
                                        div().text_color(rgb(TEXT)).text_sm().child(format!(
                                            "Are you sure you want to delete \"{}\"?",
                                            target_name
                                        )),
                                    )
                                    .child(
                                        div()
                                            .text_color(rgb(YELLOW))
                                            .text_xs()
                                            .child("This will remove the worktree directory and its contents."),
                                    ),
                            )
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_t_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .flex()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id("cancel-delete")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.hide_delete_confirm(cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("confirm-delete")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(RED))
                                            .hover(|el| el.bg(rgb(MAROON)))
                                            .text_xs()
                                            .text_color(rgb(BG_BASE))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.confirm_delete_worktree(cx);
                                            }))
                                            .child("Delete"),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn dismiss_error(&mut self, cx: &mut Context<Self>) {
        self.error_message = None;
        cx.notify();
    }

    fn render_error_dialog(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let message = self.error_message.clone().unwrap_or_default();

        div()
            .id("error-dialog-container")
            .absolute()
            .inset_0()
            // Backdrop (sibling)
            .child(
                div()
                    .id("error-dialog-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.dismiss_error(cx);
                        }),
                    ),
            )
            // Dialog container (sibling)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("error-dialog")
                            .occlude()
                            .w_96()
                            .bg(rgb(BG_BASE))
                            .border_1()
                            .border_color(rgb(RED))
                            .rounded_md()
                            .shadow_lg()
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .text_color(rgb(RED))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("Error"),
                            )
                            .child(
                                div()
                                    .p_4()
                                    .text_color(rgb(TEXT))
                                    .text_sm()
                                    .child(message),
                            )
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_t_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .flex()
                                    .justify_end()
                                    .child(
                                        div()
                                            .id("dismiss-error")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.dismiss_error(cx);
                                            }))
                                            .child("OK"),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_file_list(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let mode = self.file_list_mode;

        div()
            .w_64()
            .h_full()
            .bg(rgb(BG_MANTLE))
            .border_l_1()
            .border_color(rgb(BG_SURFACE0))
            .flex()
            .flex_col()
            // Header with mode tabs
            .child(
                div()
                    .h_8()
                    .px_2()
                    .flex()
                    .items_center()
                    .bg(rgb(BG_BASE))
                    .border_b_1()
                    .border_color(rgb(BG_SURFACE0))
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            // Changes tab
                            .child(
                                div()
                                    .id("files-changes-tab")
                                    .px_2()
                                    .py_1()
                                    .cursor_pointer()
                                    .rounded_sm()
                                    .when(mode == FileListMode::Changes, |el| el.bg(rgb(BG_SURFACE1)))
                                    .hover(|el| el.bg(rgb(BG_SURFACE1)))
                                    .text_xs()
                                    .text_color(rgb(YELLOW))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.file_list_mode = FileListMode::Changes;
                                        this.build_file_tree();
                                        cx.notify();
                                    }))
                                    .child("Changes"),
                            )
                            // All Files tab
                            .child(
                                div()
                                    .id("files-all-tab")
                                    .px_2()
                                    .py_1()
                                    .cursor_pointer()
                                    .rounded_sm()
                                    .when(mode == FileListMode::AllFiles, |el| el.bg(rgb(BG_SURFACE1)))
                                    .hover(|el| el.bg(rgb(BG_SURFACE1)))
                                    .text_xs()
                                    .text_color(rgb(BLUE))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.file_list_mode = FileListMode::AllFiles;
                                        // Clear expanded dirs when switching to All Files
                                        this.expanded_dirs.clear();
                                        cx.notify();
                                    }))
                                    .child("All"),
                            ),
                    ),
            )
            // File tree content
            .child(match mode {
                FileListMode::Changes => self.render_changes_tree(cx),
                FileListMode::AllFiles => self.render_all_files_tree(cx),
            })
            .into_any_element()
    }

    /// Render Changes tab as a tree (using pre-built file_tree)
    fn render_changes_tree(&self, cx: &Context<Self>) -> gpui::AnyElement {
        if let Some(ref tree) = self.file_tree {
            div()
                .flex_1()
                .overflow_hidden()
                .children(
                    tree.children
                        .iter()
                        .map(|node| self.render_tree_node(node, 0, cx)),
                )
                .into_any_element()
        } else {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(TEXT_MUTED))
                .text_sm()
                .child("No files")
                .into_any_element()
        }
    }

    fn render_tree_node(
        &self,
        node: &FileTreeNode,
        depth: usize,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let indent = depth * 16;
        let is_expanded = self.expanded_dirs.contains(&node.path);
        let node_path = node.path.clone();
        let node_name = node.name.clone();

        let mut result = div().flex().flex_col();

        if node.is_dir {
            let click_path = node_path.clone();
            let node_element = div()
                .id(format!("tree-dir-{}", node.path.to_string_lossy()))
                .pl(px(indent as f32))
                .pr_3()
                .py_1()
                .cursor_pointer()
                .hover(|el| el.bg(rgb(BG_SURFACE0)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_dir_expanded(&click_path);
                    cx.notify();
                }))
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w_4()
                        .text_center()
                        .text_color(rgb(BLUE))
                        .text_xs()
                        .child(if is_expanded { "▼" } else { "▶" }),
                )
                .child(
                    div()
                        .w_4()
                        .text_center()
                        .text_color(rgb(YELLOW))
                        .text_sm()
                        .child(if is_expanded { "📂" } else { "📁" }),
                )
                .child(div().text_color(rgb(TEXT)).text_sm().child(node_name));

            result = result.child(node_element);

            if is_expanded {
                for child in &node.children {
                    result = result.child(self.render_tree_node(child, depth + 1, cx));
                }
            }
        } else {
            let click_path = node_path.clone();
            let change_info = node.change_info;
            let (color, symbol) = if let Some(info) = change_info {
                match info.change_type {
                    ChangeType::Added => (GREEN, "+"),
                    ChangeType::Modified => (YELLOW, "~"),
                    ChangeType::Deleted => (RED, "-"),
                    ChangeType::Renamed => (BLUE, "→"),
                    ChangeType::Unknown => (TEXT_MUTED, "?"),
                }
            } else {
                (TEXT_MUTED, "")
            };

            let node_element = div()
                .id(format!("tree-file-{}", node.path.to_string_lossy()))
                .pl(px(indent as f32))
                .pr_3()
                .py_1()
                .cursor_pointer()
                .hover(|el| el.bg(rgb(BG_SURFACE0)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.on_file_selected(click_path.clone(), change_info.map(|i| i.change_type), cx);
                }))
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w_4()
                        .text_center()
                        .text_color(rgb(color))
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(symbol),
                )
                .child(
                    div()
                        .w_4()
                        .text_center()
                        .text_color(rgb(TEXT_MUTED))
                        .text_sm()
                        .child("📄"),
                )
                .child(div().text_color(rgb(TEXT)).text_sm().child(node_name));

            result = result.child(node_element);
        }

        result.into_any_element()
    }

    /// Render All Files tab as a lazy-loaded tree
    fn render_all_files_tree(&self, cx: &Context<Self>) -> gpui::AnyElement {
        let base_path = if let Some(session) = self.session_manager.active_session() {
            session.worktree.path.clone()
        } else {
            PathBuf::from(".")
        };

        // Read root directory contents
        let entries = read_dir_shallow(&base_path).unwrap_or_default();

        if entries.is_empty() {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(TEXT_MUTED))
                .text_sm()
                .child("No files")
                .into_any_element();
        }

        div()
            .flex_1()
            .overflow_hidden()
            .children(entries.iter().map(|(path, is_dir)| {
                self.render_lazy_tree_node(path, *is_dir, 0, &base_path, cx)
            }))
            .into_any_element()
    }

    /// Render a single node in the lazy-loaded tree
    fn render_lazy_tree_node(
        &self,
        path: &Path,
        is_dir: bool,
        depth: usize,
        base_path: &Path,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let indent = depth * 16;
        let is_expanded = self.expanded_dirs.contains(path);
        let node_path = path.to_path_buf();
        let node_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let mut result = div().flex().flex_col();

        if is_dir {
            let click_path = node_path.clone();
            let node_element = div()
                .id(format!("lazy-dir-{}", path.to_string_lossy()))
                .pl(px(indent as f32))
                .pr_3()
                .py_1()
                .cursor_pointer()
                .hover(|el| el.bg(rgb(BG_SURFACE0)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.toggle_dir_expanded(&click_path);
                    cx.notify();
                }))
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w_4()
                        .text_center()
                        .text_color(rgb(BLUE))
                        .text_xs()
                        .child(if is_expanded { "▼" } else { "▶" }),
                )
                .child(
                    div()
                        .w_4()
                        .text_center()
                        .text_color(rgb(YELLOW))
                        .text_sm()
                        .child(if is_expanded { "📂" } else { "📁" }),
                )
                .child(div().text_color(rgb(TEXT)).text_sm().child(node_name));

            result = result.child(node_element);

            // Lazy load: only read children when expanded
            if is_expanded
                && let Ok(children) = read_dir_shallow(&node_path) {
                    for (child_path, child_is_dir) in children {
                        result = result.child(self.render_lazy_tree_node(
                            &child_path,
                            child_is_dir,
                            depth + 1,
                            base_path,
                            cx,
                        ));
                    }
                }
        } else {
            // File node - make path relative for on_file_selected
            let relative_path = path.strip_prefix(base_path).unwrap_or(path).to_path_buf();
            let click_path = relative_path.clone();

            let node_element = div()
                .id(format!("lazy-file-{}", path.to_string_lossy()))
                .pl(px(indent as f32))
                .pr_3()
                .py_1()
                .cursor_pointer()
                .hover(|el| el.bg(rgb(BG_SURFACE0)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.on_file_selected(click_path.clone(), None, cx);
                }))
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w_4()
                        .text_center()
                        .text_color(rgb(TEXT_MUTED))
                        .text_xs()
                        .child(""),
                )
                .child(
                    div()
                        .w_4()
                        .text_center()
                        .text_color(rgb(TEXT_MUTED))
                        .text_sm()
                        .child("📄"),
                )
                .child(div().text_color(rgb(TEXT)).text_sm().child(node_name));

            result = result.child(node_element);
        }

        result.into_any_element()
    }
}

impl Focusable for SashikiApp {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SashikiApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view_mode = self.session_manager.view_mode();
        let session_count = self.session_manager.len();
        let running_count = self.session_manager.running_count();

        div()
            .size_full()
            .bg(rgb(BG_BASE))
            .flex()
            .flex_col()
            .on_action(cx.listener(Self::on_toggle_parallel))
            .on_action(cx.listener(Self::on_next_session))
            .on_action(cx.listener(Self::on_prev_session))
            .on_action(cx.listener(Self::on_toggle_sidebar))
            .on_action(cx.listener(Self::on_toggle_file_list))
            .on_action(cx.listener(Self::on_refresh_all))
            .on_action(cx.listener(Self::on_close_file_viewer))
            .child(
                div()
                    .h_8()
                    .px_4()
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(rgb(BG_SURFACE0))
                    .text_color(rgb(TEXT))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_4()
                            .child("Sashiki")
                            .child(
                                div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
                                    "{}/{} running | Ctrl+P Parallel | Ctrl+Tab Next | Ctrl+R Refresh",
                                    running_count, session_count
                                )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id("toggle-parallel")
                                    .px_2()
                                    .py_1()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .bg(if view_mode == ViewMode::Parallel {
                                        rgb(BLUE)
                                    } else {
                                        rgb(BG_SURFACE0)
                                    })
                                    .text_color(if view_mode == ViewMode::Parallel {
                                        rgb(BG_BASE)
                                    } else {
                                        rgb(TEXT)
                                    })
                                    .hover(|this| this.bg(rgb(BG_SURFACE2)))
                                    .text_xs()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.session_manager.toggle_view_mode();
                                        cx.notify();
                                    }))
                                    .child(if view_mode == ViewMode::Parallel {
                                        "Parallel"
                                    } else {
                                        "Single"
                                    }),
                            )
                            .child(
                                div()
                                    .id("toggle-sidebar")
                                    .px_2()
                                    .py_1()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .bg(if self.show_sidebar {
                                        rgb(BG_SURFACE1)
                                    } else {
                                        rgb(BG_SURFACE0)
                                    })
                                    .hover(|this| this.bg(rgb(BG_SURFACE2)))
                                    .text_xs()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.show_sidebar = !this.show_sidebar;
                                        cx.notify();
                                    }))
                                    .child("Sessions"),
                            )
                            .child(
                                div()
                                    .id("toggle-files")
                                    .px_2()
                                    .py_1()
                                    .rounded_sm()
                                    .cursor_pointer()
                                    .bg(if self.show_file_list {
                                        rgb(BG_SURFACE1)
                                    } else {
                                        rgb(BG_SURFACE0)
                                    })
                                    .hover(|this| this.bg(rgb(BG_SURFACE2)))
                                    .text_xs()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.show_file_list = !this.show_file_list;
                                        cx.notify();
                                    }))
                                    .child("Files"),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .overflow_hidden()
                    .when(self.show_sidebar, |this| {
                        this.child(self.render_sidebar(cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .when(self.show_file_viewer, |this| {
                                this.child(
                                    div()
                                        .h_96()
                                        .min_h_48()
                                        .border_b_1()
                                        .border_color(rgb(BG_SURFACE0))
                                        .child(self.file_viewer.clone()),
                                )
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .child(self.render_terminal_area(cx)),
                            ),
                    )
                    .when(self.show_file_list, |this| {
                        this.child(self.render_file_list(cx))
                    }),
            )
            .when(self.show_create_dialog, |this| {
                this.child(self.render_create_dialog(cx))
            })
            .when(self.show_delete_confirm, |this| {
                this.child(self.render_delete_confirm(cx))
            })
            .when(self.error_message.is_some(), |this| {
                this.child(self.render_error_dialog(cx))
            })
    }
}

fn main() {
    Application::new().run(|app: &mut App| {
        TerminalView::bind_keys(app);
        FileViewer::bind_keys(app);

        app.bind_keys([
            KeyBinding::new("ctrl-p", ToggleParallelMode, None),
            KeyBinding::new("ctrl-tab", NextSession, None),
            KeyBinding::new("ctrl-shift-tab", PrevSession, None),
            KeyBinding::new("ctrl-b", ToggleSidebar, None),
            KeyBinding::new("ctrl-e", ToggleFileList, None),
            KeyBinding::new("ctrl-r", RefreshAll, None),
            KeyBinding::new("escape", CloseFileViewer, None),
        ]);

        let window = app
            .open_window(
                WindowOptions {
                    ..Default::default()
                },
                |_window, cx| cx.new(SashikiApp::new),
            )
            .unwrap();

        // Focus the active terminal on startup (ignore if window was closed)
        let _ = window.update(app, |view, window, cx| {
            if let Some(terminal) = view.active_terminal() {
                let focus = terminal.read(cx).focus_handle(cx);
                window.focus(&focus, cx);
            }
        });
    });
}
