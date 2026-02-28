//! SashikiApp core module

mod actions;
mod dialogs;
mod file_ops;

use crate::dialog::ActiveDialog;
use crate::git::GitRepo;
use crate::language::LanguageRegistry;
use crate::lsp::LspManager;
use crate::session::SessionManager;
use crate::template::TemplateConfig;
use crate::terminal::TerminalView;
use crate::ui::{
    ChangeSection, FileListMode, FileView, GotoDefinitionEvent, StageSelectionEvent,
};
use async_lock::Mutex as AsyncMutex;
use gpui::{AppContext, Context, Entity, FocusHandle};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

pub use actions::*;

/// Identifies which menu is currently open
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuId {
    App,
    File,
    View,
}

/// Tracks which panel boundary is being dragged for resize
#[derive(Debug, Clone, Copy)]
pub(crate) enum ResizeDrag {
    Sidebar { start_x: f32, initial_width: f32 },
    FileViewTerminal { start_y: f32, initial_height: f32 },
    TerminalSplit { start_x: f32, initial_ratio: f32 },
    FileList { start_x: f32, initial_width: f32 },
}

/// Main application state
pub struct SashikiApp {
    pub(crate) session_manager: SessionManager,
    pub(crate) changed_files: Vec<crate::git::ChangedFile>,
    pub(crate) file_list_mode: FileListMode,
    /// 展開ディレクトリ (Files タブ用)
    pub(crate) expanded_dirs: HashSet<PathBuf>,
    /// Changes タブのツリービュー用: セクション別展開状態
    pub(crate) staged_expanded_dirs: HashSet<PathBuf>,
    pub(crate) unstaged_expanded_dirs: HashSet<PathBuf>,
    pub(crate) selected_file_path: Option<PathBuf>,
    pub(crate) selected_file_section: Option<ChangeSection>,
    pub(crate) hovered_file_path: Option<PathBuf>,
    pub(crate) hovered_file_section: Option<ChangeSection>,
    pub(crate) staged_section_collapsed: bool,
    pub(crate) unstaged_section_collapsed: bool,
    pub(crate) file_view: Entity<FileView>,
    pub(crate) language_registry: LanguageRegistry,
    pub(crate) git_repo: Option<GitRepo>,
    /// Cached repo for active worktree (avoids repeated Repository::discover() calls)
    pub(crate) cached_worktree: Option<(GitRepo, PathBuf)>,
    pub(crate) show_sidebar: bool,
    pub(crate) show_file_list: bool,
    pub(crate) show_file_view: bool,
    pub(crate) active_dialog: ActiveDialog,
    pub(crate) create_branch_input: String,
    pub(crate) create_branch_cursor: usize,
    pub(crate) create_branch_selection_anchor: Option<usize>,
    pub(crate) commit_message_input: String,
    pub(crate) commit_message_cursor: usize,
    pub(crate) commit_message_selection_anchor: Option<usize>,
    /// true when the commit dialog is being used to amend the last commit
    pub(crate) commit_amend_mode: bool,
    pub(crate) stash_message_input: String,
    pub(crate) stash_message_cursor: usize,
    pub(crate) stash_message_selection_anchor: Option<usize>,
    pub(crate) stash_entries: Vec<crate::git::StashEntry>,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) create_dialog_focus: FocusHandle,
    pub(crate) commit_dialog_focus: FocusHandle,
    pub(crate) stash_dialog_focus: FocusHandle,
    /// Template config being edited in the settings dialog
    pub(crate) template_edit: Option<TemplateConfig>,
    /// Input fields for template settings dialog (one per section, newline-delimited)
    pub(crate) settings_inputs: [String; 5],
    /// Cursor position (char index) per section
    pub(crate) settings_cursors: [usize; 5],
    /// Selection anchor (char index) per section
    pub(crate) settings_selection_anchors: [Option<usize>; 5],
    /// Which section is active in settings (0=pre, 1=copy, 2=sync, 3=post, 4=workdir)
    pub(crate) settings_active_section: usize,
    pub(crate) settings_dialog_focus: FocusHandle,
    /// Which menu dropdown is currently open (None = all closed)
    pub(crate) open_menu: Option<MenuId>,
    /// Whether the verify terminal (2nd terminal) is shown in single mode
    pub(crate) show_verify_terminal: bool,
    pub(crate) sidebar_width: f32,
    pub(crate) file_view_height: f32,
    pub(crate) terminal_split_ratio: f32,
    pub(crate) file_list_width: f32,
    pub(crate) resize_drag: Option<ResizeDrag>,
    pub(crate) lsp_manager: Arc<AsyncMutex<LspManager>>,
    /// Whether the source control changes list uses tree view (true) or flat list (false)
    pub(crate) changes_view_is_tree: bool,
    /// Pending hunk/range discard event, held while DiscardHunkConfirm dialog is shown.
    pub(crate) pending_discard_hunk: Option<StageSelectionEvent>,
    /// Whether the commit split-button dropdown is open.
    pub(crate) commit_dropdown_open: bool,
    /// Scope of changes to include when pushing a new stash.
    pub(crate) stash_mode: crate::git::StashMode,
    /// Files changed in each stash entry, keyed by stash reference. Loaded on dialog open.
    pub(crate) stash_entry_files: std::collections::HashMap<String, Vec<(String, String)>>,
    /// Stash entries currently expanded to show their file list.
    pub(crate) stash_expanded_entries: std::collections::HashSet<String>,
}

impl SashikiApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let create_dialog_focus = cx.focus_handle();
        let commit_dialog_focus = cx.focus_handle();
        let stash_dialog_focus = cx.focus_handle();
        let file_view = cx.new(FileView::new);

        // Subscribe to SendToTerminalEvent from FileView
        cx.subscribe(
            &file_view,
            |this, _, event: &crate::ui::SendToTerminalEvent, cx| {
                this.send_to_terminal(&event.0, cx);
            },
        )
        .detach();

        // Subscribe to GotoDefinitionEvent from FileView
        cx.subscribe(&file_view, |this, _, event: &GotoDefinitionEvent, cx| {
            this.handle_goto_definition(event.clone(), cx);
        })
        .detach();

        // Subscribe to staging requests from FileView (hunk/range staging)
        cx.subscribe(&file_view, |this, _, event: &StageSelectionEvent, cx| {
            this.handle_stage_selection(event.clone(), cx);
        })
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
            staged_expanded_dirs: HashSet::new(),
            unstaged_expanded_dirs: HashSet::new(),
            selected_file_path: None,
            selected_file_section: None,
            hovered_file_path: None,
            hovered_file_section: None,
            staged_section_collapsed: false,
            unstaged_section_collapsed: false,
            file_view,
            language_registry: LanguageRegistry::new(),
            git_repo,
            cached_worktree: None,
            show_sidebar: true,
            show_file_list: true,
            show_file_view: false,
            active_dialog,
            create_branch_input: String::new(),
            create_branch_cursor: 0,
            create_branch_selection_anchor: None,
            commit_message_input: String::new(),
            commit_message_cursor: 0,
            commit_message_selection_anchor: None,
            commit_amend_mode: false,
            stash_message_input: String::new(),
            stash_message_cursor: 0,
            stash_message_selection_anchor: None,
            stash_entries: Vec::new(),
            focus_handle,
            create_dialog_focus,
            commit_dialog_focus,
            stash_dialog_focus,
            template_edit: None,
            settings_inputs: Default::default(),
            settings_cursors: Default::default(),
            settings_selection_anchors: Default::default(),
            settings_active_section: 0,
            settings_dialog_focus: cx.focus_handle(),
            open_menu: None,
            show_verify_terminal: false,
            sidebar_width: 224.0,
            file_view_height: 384.0,
            terminal_split_ratio: 0.5,
            file_list_width: 308.0,
            resize_drag: None,
            lsp_manager: Arc::new(AsyncMutex::new(LspManager::new())),
            changes_view_is_tree: true,
            pending_discard_hunk: None,
            commit_dropdown_open: false,
            stash_mode: crate::git::StashMode::default(),
            stash_entry_files: std::collections::HashMap::new(),
            stash_expanded_entries: std::collections::HashSet::new(),
        };

        app.refresh_changed_files_sync();
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
        // 1. Validate target repository first so current state remains intact on failure.
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

        let worktrees = match repo.list_worktrees() {
            Ok(w) if !w.is_empty() => w,
            Ok(_) => {
                self.active_dialog = crate::dialog::ActiveDialog::Error {
                    message: "No worktrees found in repository".to_string(),
                };
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

        // 2. Shutdown all LSP servers (drain under lock, then shut down without it)
        let lsp = Arc::clone(&self.lsp_manager);
        cx.spawn(async move |_, _| {
            let old_servers = {
                let mut manager = lsp.lock().await;
                manager.take_servers()
            };
            for (_id, mut client) in old_servers {
                let _ = client.shutdown().await;
            }
        })
        .detach();

        // 3. Shutdown all current session terminals.
        for i in 0..self.session_manager.len() {
            if let Some(terminal) = self.session_manager.get_session_active_terminal(i) {
                terminal.update(cx, |view, _cx| view.shutdown());
            }
            self.session_manager.clear_session_terminals(i);
        }

        // 4. Close file view and reset project-scoped state.
        self.file_view.update(cx, |view, _cx| view.close());
        self.show_file_view = false;
        self.show_verify_terminal = false;
        self.cached_worktree = None;
        self.changed_files.clear();
        self.expanded_dirs.clear();
        self.staged_expanded_dirs.clear();
        self.unstaged_expanded_dirs.clear();
        self.selected_file_path = None;
        self.selected_file_section = None;
        self.hovered_file_path = None;
        self.hovered_file_section = None;
        self.staged_section_collapsed = false;
        self.unstaged_section_collapsed = false;
        self.active_dialog = ActiveDialog::None;
        self.open_menu = None;
        self.create_branch_input.clear();
        self.create_branch_cursor = 0;
        self.create_branch_selection_anchor = None;
        self.commit_message_input.clear();
        self.commit_message_cursor = 0;
        self.commit_message_selection_anchor = None;
        self.stash_message_input.clear();
        self.stash_message_cursor = 0;
        self.stash_message_selection_anchor = None;
        self.stash_entries.clear();
        self.stash_entry_files.clear();
        self.stash_expanded_entries.clear();
        self.stash_mode = crate::git::StashMode::default();
        self.settings_inputs = Default::default();
        self.settings_cursors = Default::default();
        self.settings_selection_anchors = Default::default();

        // 5. Initialize repository and sessions for the selected project.
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
