//! SashikiApp core module

mod actions;
mod dialogs;
mod file_ops;

use crate::dialog::ActiveDialog;
use crate::git::GitRepo;
use crate::session::SessionManager;
use crate::template::TemplateConfig;
use crate::terminal::TerminalView;
use crate::ui::{FileListMode, FileTreeNode, FileView};
use gpui::{AppContext, Context, Entity, FocusHandle};
use std::collections::HashSet;
use std::path::PathBuf;

pub use actions::*;

/// Identifies which menu is currently open
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuId {
    App,
    File,
    View,
}

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
    /// Template config being edited in the settings dialog
    pub(crate) template_edit: Option<TemplateConfig>,
    /// Input fields for template settings dialog (one per section, newline-delimited)
    pub(crate) settings_inputs: [String; 4],
    /// Cursor position (char index) per section
    pub(crate) settings_cursors: [usize; 4],
    /// Which section is active in settings (0=pre, 1=copy, 2=post, 3=workdir)
    pub(crate) settings_active_section: usize,
    pub(crate) settings_dialog_focus: FocusHandle,
    /// Which menu dropdown is currently open (None = all closed)
    pub(crate) open_menu: Option<MenuId>,
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
                    let template = TemplateConfig::load(repo);
                    session_manager.apply_terminal_default_directory_to_all(
                        template.working_directory.as_deref(),
                    );
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
            template_edit: None,
            settings_inputs: Default::default(),
            settings_cursors: Default::default(),
            settings_active_section: 0,
            settings_dialog_focus: cx.focus_handle(),
            open_menu: None,
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

    /// Open a new project (Git repository) at the given path.
    /// Shuts down all current terminals, resets state, and initializes from the new repo.
    pub fn open_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        // 1. Shutdown all session terminals
        for i in 0..self.session_manager.len() {
            if let Some(terminal) = self.session_manager.get_session_active_terminal(i) {
                terminal.update(cx, |view, _cx| view.shutdown());
            }
            self.session_manager.clear_session_terminals(i);
        }

        // 2. Close file view
        self.file_view.update(cx, |view, _cx| view.close());
        self.show_file_view = false;

        // 3. Reset cached state
        self.cached_worktree = None;
        self.changed_files.clear();
        self.expanded_dirs.clear();
        self.file_tree = None;

        // 4. Open new repository
        let repo = match GitRepo::open(&path) {
            Ok(r) => r,
            Err(e) => {
                self.active_dialog = crate::dialog::ActiveDialog::Error {
                    message: format!("Failed to open repository: {}", e),
                };
                cx.notify();
                return;
            }
        };

        // 5. List worktrees and initialize sessions
        let worktrees = match repo.list_worktrees() {
            Ok(w) if !w.is_empty() => w,
            Ok(_) => {
                self.active_dialog = crate::dialog::ActiveDialog::Error {
                    message: "No worktrees found in repository".to_string(),
                };
                self.git_repo = Some(repo);
                cx.notify();
                return;
            }
            Err(e) => {
                self.active_dialog = crate::dialog::ActiveDialog::Error {
                    message: format!("Failed to list worktrees: {}", e),
                };
                cx.notify();
                return;
            }
        };

        self.git_repo = Some(repo);
        self.session_manager.init_from_worktrees(worktrees);

        // 6. Apply template defaults
        if let Some(ref repo) = self.git_repo {
            let template = TemplateConfig::load(repo);
            self.session_manager
                .apply_terminal_default_directory_to_all(template.working_directory.as_deref());
        }

        // 7. Start first session terminal
        self.session_manager.ensure_session_terminal(0, cx);
        self.session_manager.switch_to(0);

        // 8. Refresh file list
        self.refresh_changed_files_sync();
        self.build_file_tree();

        cx.notify();
    }

    pub(crate) fn apply_template_working_directory_defaults(&mut self) {
        let relative = self
            .git_repo
            .as_ref()
            .map(TemplateConfig::load)
            .and_then(|t| t.working_directory);
        self.session_manager
            .apply_terminal_default_directory_to_all(relative.as_deref());
    }
}
