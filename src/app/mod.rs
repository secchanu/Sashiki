//! SashikiApp core module

mod actions;
mod dialogs;
mod file_ops;

use crate::dialog::ActiveDialog;
use crate::git::GitRepo;
use crate::session::SessionManager;
use crate::terminal::TerminalView;
use crate::ui::{FileListMode, FileTreeNode, FileView};
use gpui::{AppContext, Context, Entity, FocusHandle};
use std::collections::HashSet;
use std::path::PathBuf;

pub use actions::*;

/// Main application state
pub struct SashikiApp {
    pub(crate) session_manager: SessionManager,
    pub(crate) changed_files: Vec<crate::git::ChangedFile>,
    pub(crate) file_list_mode: FileListMode,
    pub(crate) expanded_dirs: HashSet<PathBuf>,
    pub(crate) file_tree: Option<FileTreeNode>,
    pub(crate) file_view: Entity<FileView>,
    pub(crate) git_repo: Option<GitRepo>,
    /// Cached repo for active worktree (avoids repeated Repository::discover() calls)
    pub(crate) cached_worktree: Option<(GitRepo, PathBuf)>,
    pub(crate) show_sidebar: bool,
    pub(crate) show_file_list: bool,
    pub(crate) show_file_view: bool,
    pub(crate) active_dialog: ActiveDialog,
    pub(crate) create_branch_input: String,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) create_dialog_focus: FocusHandle,
}

impl SashikiApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let create_dialog_focus = cx.focus_handle();
        let file_view = cx.new(FileView::new);

        // Subscribe to SendToTerminalEvent from FileView
        cx.subscribe(
            &file_view,
            |this, _, event: &crate::ui::SendToTerminalEvent, cx| {
                this.send_to_terminal(&event.0, cx);
            },
        )
        .detach();

        let git_repo = GitRepo::open(".").ok();
        let mut session_manager = SessionManager::new();
        let mut active_dialog = ActiveDialog::None;

        if let Some(repo) = &git_repo {
            if let Ok(worktrees) = repo.list_worktrees() {
                if !worktrees.is_empty() {
                    session_manager.init_from_worktrees(worktrees);
                    session_manager.ensure_session_terminal(0, cx);
                    session_manager.switch_to(0);
                } else {
                    active_dialog = ActiveDialog::Error {
                        message: "No worktrees found in repository".to_string(),
                    };
                }
            } else {
                active_dialog = ActiveDialog::Error {
                    message: "Failed to list worktrees".to_string(),
                };
            }
        } else {
            active_dialog = ActiveDialog::Error {
                message: "Git repository not found in current directory".to_string(),
            };
        }

        let mut app = Self {
            session_manager,
            changed_files: Vec::new(),
            file_list_mode: FileListMode::default(),
            expanded_dirs: HashSet::new(),
            file_tree: None,
            file_view,
            git_repo,
            cached_worktree: None,
            show_sidebar: true,
            show_file_list: true,
            show_file_view: false,
            active_dialog,
            create_branch_input: String::new(),
            focus_handle,
            create_dialog_focus,
        };

        app.refresh_changed_files_sync();
        app.build_file_tree();
        app
    }

    pub fn active_terminal(&self) -> Option<Entity<TerminalView>> {
        self.session_manager.active_terminal()
    }

    /// Send text to the active terminal
    pub fn send_to_terminal(&self, text: &str, cx: &mut Context<Self>) {
        if let Some(terminal) = self.active_terminal() {
            terminal.update(cx, |view, _cx| {
                view.write_text(text);
            });
        }
    }
}
