//! SashikiApp core module

mod actions;
mod dialogs;
mod file_ops;
mod persistence;

use self::persistence::{PersistedAppState, PersistedGroupState, PersistedUiState};
use crate::dialog::ActiveDialog;
use crate::git::GitRepo;
use crate::language::LanguageRegistry;
use crate::lsp::LspManager;
use crate::session::{SessionGroup, SessionGroupManager};
use crate::template::TemplateConfig;
use crate::terminal::TerminalView;
use crate::ui::{
    ChangeSection, FileListMode, FileView, FileViewCloseEvent, GotoDefinitionEvent,
    StageSelectionEvent, TextInput,
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
    pub(crate) session_manager: SessionGroupManager,
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
    /// Cached repo for active worktree (avoids repeated Repository::discover() calls)
    pub(crate) cached_worktree: Option<(GitRepo, PathBuf)>,
    pub(crate) show_sidebar: bool,
    pub(crate) show_file_list: bool,
    pub(crate) show_file_view: bool,
    pub(crate) active_dialog: ActiveDialog,
    pub(crate) create_input: Entity<TextInput>,
    pub(crate) commit_input: Entity<TextInput>,
    /// true when the commit dialog is being used to amend the last commit
    pub(crate) commit_amend_mode: bool,
    pub(crate) stash_input: Entity<TextInput>,
    pub(crate) stash_entries: Vec<crate::git::StashEntry>,
    pub(crate) focus_handle: FocusHandle,
    pub(crate) create_dialog_focus: FocusHandle,
    pub(crate) commit_dialog_focus: FocusHandle,
    pub(crate) stash_dialog_focus: FocusHandle,
    /// Template config being edited in the settings dialog
    pub(crate) template_edit: Option<TemplateConfig>,
    /// Input fields for template settings dialog (one per section, newline-delimited)
    pub(crate) settings_inputs: [Entity<TextInput>; 5],
    /// Which section is active in settings (0=pre, 1=copy, 2=sync, 3=post, 4=workdir)
    pub(crate) settings_active_section: usize,
    /// Which group's template settings are being edited
    pub(crate) settings_group_index: usize,
    pub(crate) settings_dialog_focus: FocusHandle,
    /// Which menu dropdown is currently open (None = all closed)
    pub(crate) open_menu: Option<MenuId>,
    /// Focus handle for the menu overlay (keyboard navigation)
    pub(crate) menu_focus: FocusHandle,
    /// Currently highlighted menu item index for keyboard navigation
    pub(crate) menu_focused_item: usize,
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
        let menu_focus = cx.focus_handle();
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

        // Subscribe to close event from FileView to restore full-screen terminal
        cx.subscribe(&file_view, |this, _, _: &FileViewCloseEvent, cx| {
            this.show_file_view = false;
            cx.notify();
        })
        .detach();

        let persisted_state = persistence::load_app_state();
        let (mut session_manager, active_dialog) =
            Self::initialize_session_groups(persisted_state.as_ref());

        if !session_manager.is_empty() {
            let active_session = session_manager.active_index();
            session_manager.ensure_session_terminal(active_session, cx);
            session_manager.switch_to(active_session);
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
            cached_worktree: None,
            show_sidebar: true,
            show_file_list: true,
            show_file_view: false,
            active_dialog,
            create_input: cx.new(|_| TextInput::new()),
            commit_input: cx.new(|_| TextInput::new()),
            commit_amend_mode: false,
            stash_input: cx.new(|_| TextInput::new()),
            stash_entries: Vec::new(),
            focus_handle,
            create_dialog_focus,
            commit_dialog_focus,
            stash_dialog_focus,
            template_edit: None,
            settings_inputs: std::array::from_fn(|_| cx.new(|_| TextInput::new())),
            settings_active_section: 0,
            settings_group_index: 0,
            settings_dialog_focus: cx.focus_handle(),
            open_menu: None,
            menu_focus,
            menu_focused_item: 0,
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

        if let Some(state) = persisted_state.as_ref() {
            app.restore_persisted_state(state, cx);
        }

        app.refresh_changed_files_sync();
        app
    }

    fn initialize_session_groups(
        persisted_state: Option<&PersistedAppState>,
    ) -> (SessionGroupManager, ActiveDialog) {
        let mut session_manager = SessionGroupManager::new();
        let mut loaded_from_persisted = false;

        if let Some(state) = persisted_state {
            let mut seen = HashSet::new();
            for group_state in &state.groups {
                if group_state.project_path.as_os_str().is_empty() {
                    continue;
                }
                if !seen.insert(group_state.project_path.clone()) {
                    continue;
                }
                if let Ok(group) = Self::create_group_from_path(&group_state.project_path) {
                    let idx = session_manager.add_group(group);
                    session_manager.switch_group(idx);
                    loaded_from_persisted = true;
                }
            }
        }

        if loaded_from_persisted {
            return (session_manager, ActiveDialog::None);
        }

        match Self::create_group_from_path(std::path::Path::new(".")) {
            Ok(group) => {
                let idx = session_manager.add_group(group);
                session_manager.switch_group(idx);
                (session_manager, ActiveDialog::None)
            }
            Err(message) => (session_manager, ActiveDialog::Error { message }),
        }
    }

    fn create_group_from_path(path: &std::path::Path) -> Result<SessionGroup, String> {
        let is_current_dir = path == std::path::Path::new(".");

        let repo = GitRepo::open(path).map_err(|e| {
            if is_current_dir {
                "Git repository not found in current directory".to_string()
            } else {
                format!("Failed to open repository {}: {}", path.display(), e)
            }
        })?;

        let worktrees = repo.list_worktrees().map_err(|e| {
            if is_current_dir {
                "Failed to list worktrees".to_string()
            } else {
                format!(
                    "Failed to list worktrees in {}: {}",
                    repo.workdir().display(),
                    e
                )
            }
        })?;

        if worktrees.is_empty() {
            return Err("No worktrees found in repository".to_string());
        }

        let project_name = repo
            .workdir()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "project".to_string());
        let project_path = repo.workdir().to_path_buf();
        let template = TemplateConfig::load(&repo);
        let mut group = SessionGroup::new(project_name, project_path, repo);
        group.session_manager.init_from_worktrees(worktrees);
        group
            .session_manager
            .apply_terminal_default_directory_to_all(template.working_directory.as_deref());
        group.set_expanded(true);

        Ok(group)
    }

    fn restore_persisted_state(&mut self, state: &PersistedAppState, cx: &mut Context<Self>) {
        self.show_sidebar = state.ui.show_sidebar;
        self.show_file_list = state.ui.show_file_list;
        self.sidebar_width = state.ui.sidebar_width.clamp(120.0, 500.0);
        self.file_view_height = state.ui.file_view_height.clamp(100.0, 800.0);
        self.terminal_split_ratio = state.ui.terminal_split_ratio.clamp(0.2, 0.8);
        self.file_list_width = state.ui.file_list_width.clamp(120.0, 500.0);
        self.file_list_mode = state.ui.file_list_mode.into();
        self.changes_view_is_tree = state.ui.changes_view_is_tree;
        self.staged_section_collapsed = state.ui.staged_section_collapsed;
        self.unstaged_section_collapsed = state.ui.unstaged_section_collapsed;

        for group_state in &state.groups {
            let Some(group_index) = self
                .session_manager
                .find_group_by_path(&group_state.project_path)
            else {
                continue;
            };

            self.session_manager.switch_group(group_index);
            if let Some(group) = self.session_manager.active_group_mut() {
                group.set_expanded(group_state.expanded);
                group
                    .session_manager
                    .set_layout_mode(group_state.layout_mode.into());

                let visible_paths: HashSet<PathBuf> =
                    group_state.parallel_visible_paths.iter().cloned().collect();
                group
                    .session_manager
                    .set_parallel_visibility_by_paths(&visible_paths);

                let active_session_index = group_state
                    .active_session_path
                    .as_ref()
                    .and_then(|path| group.session_manager.find_session_by_path(path))
                    .unwrap_or(0);
                group.session_manager.switch_to(active_session_index);
            }
        }

        if let Some(active_group_path) = state.active_group_path.as_ref()
            && let Some(index) = self.session_manager.find_group_by_path(active_group_path)
        {
            self.session_manager.switch_group(index);
        }

        if !self.session_manager.is_empty() {
            let active_session = self.session_manager.active_index();
            self.session_manager
                .ensure_session_terminal(active_session, cx);
            self.session_manager.switch_to(active_session);
            if self.session_manager.active_show_sub_terminal() {
                self.session_manager
                    .ensure_active_session_terminal_count(2, cx);
            }
            self.active_dialog = ActiveDialog::None;
        }
    }

    fn build_persisted_state(&self) -> PersistedAppState {
        let groups = self
            .session_manager
            .groups()
            .iter()
            .map(|group| PersistedGroupState {
                project_path: group.project_path().to_path_buf(),
                expanded: group.is_expanded(),
                active_session_path: group
                    .session_manager
                    .active_session()
                    .map(|session| session.worktree_path().to_path_buf()),
                layout_mode: group.session_manager.layout_mode().into(),
                parallel_visible_paths: group
                    .session_manager
                    .parallel_sessions()
                    .into_iter()
                    .map(|(_, session)| session.worktree_path().to_path_buf())
                    .collect(),
            })
            .collect();

        PersistedAppState {
            version: 1,
            active_group_path: self
                .session_manager
                .active_group()
                .map(|group| group.project_path().to_path_buf()),
            ui: PersistedUiState {
                show_sidebar: self.show_sidebar,
                show_file_list: self.show_file_list,
                sidebar_width: self.sidebar_width,
                file_view_height: self.file_view_height,
                terminal_split_ratio: self.terminal_split_ratio,
                file_list_width: self.file_list_width,
                file_list_mode: self.file_list_mode.into(),
                changes_view_is_tree: self.changes_view_is_tree,
                staged_section_collapsed: self.staged_section_collapsed,
                unstaged_section_collapsed: self.unstaged_section_collapsed,
            },
            groups,
        }
    }

    fn save_state_snapshot(&self) {
        let state = self.build_persisted_state();
        if let Err(e) = persistence::save_app_state(&state) {
            eprintln!("Failed to save app state: {}", e);
        }
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

        let project_path = repo.workdir().to_path_buf();

        // 3. 既に同じプロジェクトが開いていれば切り替えのみ行う
        if let Some(idx) = self.session_manager.find_group_by_path(&project_path) {
            self.session_manager.switch_group(idx);
            self.cached_worktree = None;
            self.refresh_changed_files_sync();
            cx.notify();
            return;
        }

        // 4. 新規グループとして追加する
        let project_name = project_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "project".to_string());
        let template = TemplateConfig::load(&repo);
        let mut group = SessionGroup::new(project_name, project_path, repo);
        group.session_manager.init_from_worktrees(worktrees);
        group
            .session_manager
            .apply_terminal_default_directory_to_all(template.working_directory.as_deref());
        group.set_expanded(true);
        let idx = self.session_manager.add_group(group);
        self.session_manager.switch_group(idx);

        // 5. UI 状態リセット（グループ切り替え時の残留データを消去）
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

        // 6. 最初のセッションのターミナルを起動
        self.session_manager.ensure_session_terminal(0, cx);
        self.session_manager.switch_to(0);

        // 7. ファイルリスト更新
        self.refresh_changed_files_sync();

        cx.notify();
    }

    pub(crate) fn apply_template_working_directory_defaults(&mut self) {
        let relative = self
            .session_manager
            .active_git_repo()
            .map(TemplateConfig::load)
            .and_then(|t| t.working_directory);
        self.session_manager
            .apply_terminal_default_directory_to_all(relative.as_deref());
    }

    /// 指定グループを閉じる（全セッションのターミナルをシャットダウンしてから削除）
    pub fn close_group(&mut self, group_index: usize, cx: &mut Context<Self>) {
        let group_count = self.session_manager.group_count();
        if group_index >= group_count {
            return;
        }

        // グループ内の全セッションのターミナルをシャットダウン
        if let Some(group) = self.session_manager.groups().get(group_index) {
            let session_count = group.session_manager.len();
            for i in 0..session_count {
                if let Some(terminal) = group.session_manager.get_session_active_terminal(i) {
                    terminal.update(cx, |view, _cx| view.shutdown());
                }
            }
        }

        self.session_manager.remove_group(group_index);
        self.cached_worktree = None;
        self.changed_files.clear();
        self.refresh_changed_files_sync();
        cx.notify();
    }
}

impl Drop for SashikiApp {
    fn drop(&mut self) {
        self.save_state_snapshot();
    }
}
