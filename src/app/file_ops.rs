//! File operation methods

use super::SashikiApp;
use crate::git::{ChangeType, GitRepo};
use crate::lsp::{LspRequestError, WorkspaceId};
use crate::ui::{
    ChangeSection, GotoDefinitionEvent, SelectionAction, StageSelectionEvent,
    StageSelectionKind,
};
use gpui::Context;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use url::Url;

impl SashikiApp {
    fn active_worktree_path(&self) -> Option<PathBuf> {
        self.session_manager
            .active_session()
            .map(|s| s.worktree_path().to_path_buf())
    }

    /// Refresh file list for the active session (sync)
    pub fn refresh_file_list(&mut self) {
        self.invalidate_worktree_repo_cache();
        self.refresh_changed_files_sync();
    }

    /// Async version of refresh_file_list - spawns background task
    pub fn refresh_file_list_async(&mut self, cx: &mut Context<Self>) {
        self.invalidate_worktree_repo_cache();

        let requested_worktree_path = self.active_worktree_path();

        cx.spawn(async move |entity, cx| {
            let files = if let Some(path) = requested_worktree_path.as_ref() {
                GitRepo::open(&path)
                    .ok()
                    .and_then(|repo| repo.get_changed_files().ok())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            // Ignore error: only fails if entity was dropped (app closed)
            let _ = entity.update(cx, move |app, cx| {
                // Drop stale async refresh results when active session has already changed.
                if app.active_worktree_path() != requested_worktree_path {
                    return;
                }

                app.changed_files = files;
                cx.notify();
            });
        })
        .detach();
    }

    /// Synchronous version of refresh_changed_files (for initial load)
    pub fn refresh_changed_files_sync(&mut self) {
        if let Some(path) = self.active_worktree_path()
            && let Ok(repo) = GitRepo::open(&path)
            && let Ok(files) = repo.get_changed_files()
        {
            self.changed_files = files;
            return;
        }

        if let Some(ref repo) = self.git_repo
            && let Ok(files) = repo.get_changed_files()
        {
            self.changed_files = files;
            return;
        }

        // Clear stale entries if both active worktree and fallback repo refresh fail.
        self.changed_files.clear();
    }

    /// Returns a cached GitRepo for the active worktree, creating it if needed.
    pub fn worktree_repo(&mut self) -> Option<&GitRepo> {
        let worktree_path = self
            .session_manager
            .active_session()
            .map(|s| s.worktree_path().to_path_buf())?;

        if let Some((_, cached_path)) = &self.cached_worktree
            && cached_path == &worktree_path
        {
            return self.cached_worktree.as_ref().map(|(repo, _)| repo);
        }

        if let Ok(repo) = GitRepo::open(&worktree_path) {
            self.cached_worktree = Some((repo, worktree_path));
            self.cached_worktree.as_ref().map(|(repo, _)| repo)
        } else {
            self.cached_worktree = None;
            None
        }
    }

    /// Invalidate worktree repo cache (call when switching sessions)
    pub fn invalidate_worktree_repo_cache(&mut self) {
        self.cached_worktree = None;
    }

    pub fn toggle_dir_expanded(&mut self, path: &Path) {
        if self.expanded_dirs.contains(path) {
            self.expanded_dirs.remove(path);
        } else {
            self.expanded_dirs.insert(path.to_path_buf());
        }
    }

    /// Changes タブのツリービュー用: セクション別にディレクトリの展開/折りたたみを切り替える
    pub fn toggle_dir_expanded_section(&mut self, path: &Path, section: ChangeSection) {
        let dirs = match section {
            ChangeSection::Staged => &mut self.staged_expanded_dirs,
            ChangeSection::Unstaged => &mut self.unstaged_expanded_dirs,
        };
        if dirs.contains(path) {
            dirs.remove(path);
        } else {
            dirs.insert(path.to_path_buf());
        }
    }

    pub fn on_file_selected(
        &mut self,
        path: PathBuf,
        section: Option<ChangeSection>,
        change_type: Option<ChangeType>,
        target_line: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        // When no section is specified (e.g. Files tab), derive the section from the file's
        // git status so that stage/unstage buttons are always functional.
        let (section, change_type) = if section.is_none() {
            if let Some(file) = self.changed_files.iter().find(|f| f.path == path) {
                if file.has_unstaged_changes() {
                    (Some(ChangeSection::Unstaged), file.unstaged_change)
                } else if file.has_staged_changes() {
                    (Some(ChangeSection::Staged), file.staged_change)
                } else {
                    (None, change_type)
                }
            } else {
                (None, change_type)
            }
        } else {
            (section, change_type)
        };

        self.selected_file_path = Some(path.clone());
        self.selected_file_section = section;

        let full_path = if let Some(session) = self.session_manager.active_session() {
            session.worktree_path().join(&path)
        } else {
            path.clone()
        };

        let diff = self.worktree_repo().and_then(|repo| match section {
            Some(ChangeSection::Staged) => repo.get_file_diff_staged(&full_path).ok(),
            Some(ChangeSection::Unstaged) => match change_type {
                Some(ChangeType::Added) => repo.generate_added_diff(&full_path).ok(),
                _ => repo.get_file_diff_unstaged(&full_path).ok(),
            },
            None => match change_type {
                Some(ChangeType::Added) => repo.generate_added_diff(&full_path).ok(),
                Some(ChangeType::Deleted) => repo.generate_deleted_diff(&full_path).ok(),
                _ => repo.get_file_diff(&full_path).ok(),
            },
        });

        let is_binary = std::fs::File::open(&full_path)
            .ok()
            .and_then(|mut file| {
                let mut buffer = [0u8; 4096];
                let size = file.read(&mut buffer).ok()?;
                (size > 0).then(|| {
                    content_inspector::inspect(&buffer[..size])
                        == content_inspector::ContentType::BINARY
                })
            })
            .unwrap_or(false);

        if is_binary {
            self.file_view
                .update(cx, |view, _cx| view.open_binary(full_path));
            self.show_file_view = true;
            cx.notify();
            return;
        }

        let has_diff = diff.is_some();
        let (new_content, old_content) = match section {
            Some(ChangeSection::Staged) => {
                let new_content = if !matches!(change_type, Some(ChangeType::Deleted)) {
                    self.worktree_repo()
                        .and_then(|repo| repo.get_file_content_from_index(&full_path).ok())
                } else {
                    None
                };
                let old_content = if has_diff && !matches!(change_type, Some(ChangeType::Added)) {
                    self.worktree_repo()
                        .and_then(|repo| repo.get_file_content_from_head(&full_path).ok())
                } else {
                    None
                };
                (new_content, old_content)
            }
            Some(ChangeSection::Unstaged) => {
                let new_content = if !matches!(change_type, Some(ChangeType::Deleted)) {
                    std::fs::read_to_string(&full_path).ok()
                } else {
                    None
                };
                let old_content = if has_diff && !matches!(change_type, Some(ChangeType::Added)) {
                    self.worktree_repo()
                        .and_then(|repo| repo.get_file_content_from_index(&full_path).ok())
                } else {
                    None
                };
                (new_content, old_content)
            }
            None => {
                let new_content = if !matches!(change_type, Some(ChangeType::Deleted)) {
                    std::fs::read_to_string(&full_path).ok()
                } else {
                    None
                };
                let old_content = if has_diff && !matches!(change_type, Some(ChangeType::Added)) {
                    self.worktree_repo()
                        .and_then(|repo| repo.get_file_content_from_head(&full_path).ok())
                } else {
                    None
                };
                (new_content, old_content)
            }
        };

        let mut highlight_content: Option<Arc<crate::highlight::HighlightedDoc>> = None;
        let mut highlight_old: Option<Arc<crate::highlight::HighlightedDoc>> = None;
        let mut highlight_new: Option<Arc<crate::highlight::HighlightedDoc>> = None;

        if let Some(config) = self.language_registry.detect(&full_path) {
            let capture_names = crate::language::capture_names_for(config);

            if !matches!(change_type, Some(ChangeType::Deleted)) {
                let hl = new_content.as_deref().and_then(|content| {
                    crate::highlight::adapter::highlight_source(content, config, &capture_names)
                        .map(Arc::new)
                });
                highlight_content = hl.clone();
                highlight_new = hl;
            }

            if has_diff && !matches!(change_type, Some(ChangeType::Added)) {
                highlight_old = old_content.as_deref().and_then(|content| {
                    crate::highlight::adapter::highlight_source(content, config, &capture_names)
                        .map(Arc::new)
                });
            } else {
                highlight_old = highlight_content.clone();
            }
        }

        self.file_view.update(cx, move |view, _cx| {
            let saved_mode = target_line.map(|_| view.mode());
            view.set_change_section(section);
            view.set_change_type(change_type);

            match change_type {
                Some(ChangeType::Deleted) => {
                    if let Some(diff_content) = diff {
                        view.open_deleted_file_with_diff(full_path.clone(), diff_content);
                    } else {
                        view.open_with_content(full_path.clone(), String::new());
                    }
                }
                _ => {
                    if let Some(diff_content) = diff {
                        if let Some(content) = new_content.clone() {
                            view.open_with_diff_and_content(
                                full_path.clone(),
                                content,
                                diff_content,
                            );
                        } else if view
                            .open_file_with_diff(full_path.clone(), diff_content)
                            .is_err()
                        {
                            view.open_with_content(full_path.clone(), String::new());
                        }
                    } else if let Some(content) = new_content.clone() {
                        view.open_with_content(full_path.clone(), content);
                    } else if view.open_file(full_path.clone()).is_err() {
                        view.open_with_content(full_path.clone(), String::new());
                    }
                }
            }

            if let Some(mode) = saved_mode {
                view.restore_mode(mode);
            }

            view.set_highlights(highlight_content, highlight_old, highlight_new);
            if let Some(line) = target_line {
                view.set_target_line(line);
            }
        });

        self.show_file_view = true;
        cx.notify();
    }

    /// Stage all changes in the active worktree with a single git command.
    pub fn stage_all_files(&mut self, cx: &mut Context<Self>) {
        let result = if let Some(repo) = self.worktree_repo() {
            repo.stage_all()
        } else {
            Err(crate::git::GitError::Command(
                "Git repository not available for active worktree".to_string(),
            ))
        };
        match result {
            Ok(()) => self.refresh_file_list_async(cx),
            Err(e) => {
                self.active_dialog = crate::dialog::ActiveDialog::Error {
                    message: format!("Failed to stage all files: {}", e),
                };
                cx.notify();
            }
        }
    }

    /// Unstage all staged changes in the active worktree with a single git command.
    pub fn unstage_all_files(&mut self, cx: &mut Context<Self>) {
        let result = if let Some(repo) = self.worktree_repo() {
            repo.unstage_all()
        } else {
            Err(crate::git::GitError::Command(
                "Git repository not available for active worktree".to_string(),
            ))
        };
        match result {
            Ok(()) => self.refresh_file_list_async(cx),
            Err(e) => {
                self.active_dialog = crate::dialog::ActiveDialog::Error {
                    message: format!("Failed to unstage all files: {}", e),
                };
                cx.notify();
            }
        }
    }

    /// Discard all unstaged changes for a file.
    /// Tracked files (Modified/Deleted): `git restore -- <path>`
    /// Untracked files (Added/??): `git clean -f -- <path>`
    pub fn discard_file(&mut self, path: PathBuf, change_type: ChangeType, cx: &mut Context<Self>) {
        let full_path = if let Some(session) = self.session_manager.active_session() {
            session.worktree_path().join(&path)
        } else {
            path.clone()
        };

        let result = if let Some(repo) = self.worktree_repo() {
            if change_type == ChangeType::Added {
                repo.clean_file(&full_path)
            } else {
                repo.restore_file(&full_path)
            }
        } else {
            Err(crate::git::GitError::Command(
                "Git repository not available for active worktree".to_string(),
            ))
        };

        match result {
            Ok(()) => {
                self.refresh_file_list_async(cx);
            }
            Err(e) => {
                self.active_dialog = crate::dialog::ActiveDialog::Error {
                    message: format!(
                        "Failed to discard changes for `{}`: {}",
                        path.to_string_lossy(),
                        e
                    ),
                };
                cx.notify();
            }
        }
    }

    pub fn toggle_file_staging(
        &mut self,
        path: PathBuf,
        currently_staged: bool,
        cx: &mut Context<Self>,
    ) {
        let full_path = if let Some(session) = self.session_manager.active_session() {
            session.worktree_path().join(&path)
        } else {
            path.clone()
        };

        let result = {
            if let Some(repo) = self.worktree_repo() {
                if currently_staged {
                    repo.unstage_file(&full_path)
                } else {
                    repo.stage_file(&full_path)
                }
            } else {
                Err(crate::git::GitError::Command(
                    "Git repository not available for active worktree".to_string(),
                ))
            }
        };

        match result {
            Ok(()) => {
                self.refresh_file_list_async(cx);
            }
            Err(e) => {
                self.active_dialog = crate::dialog::ActiveDialog::Error {
                    message: format!(
                        "Failed to {} file `{}`: {}",
                        if currently_staged { "unstage" } else { "stage" },
                        path.to_string_lossy(),
                        e
                    ),
                };
                cx.notify();
            }
        }
    }

    /// Stage or unstage all files under a directory path.
    /// `git add -- <dir>` / `git restore --staged -- <dir>` を使用してディレクトリ単位で操作する。
    pub fn toggle_dir_staging(
        &mut self,
        dir_path: PathBuf,
        currently_staged: bool,
        cx: &mut Context<Self>,
    ) {
        let full_path = if let Some(session) = self.session_manager.active_session() {
            session.worktree_path().join(&dir_path)
        } else {
            dir_path.clone()
        };

        let result = {
            if let Some(repo) = self.worktree_repo() {
                if currently_staged {
                    repo.unstage_file(&full_path)
                } else {
                    repo.stage_file(&full_path)
                }
            } else {
                Err(crate::git::GitError::Command(
                    "Git repository not available for active worktree".to_string(),
                ))
            }
        };

        match result {
            Ok(()) => {
                self.refresh_file_list_async(cx);
            }
            Err(e) => {
                self.active_dialog = crate::dialog::ActiveDialog::Error {
                    message: format!(
                        "Failed to {} directory `{}`: {}",
                        if currently_staged { "unstage" } else { "stage" },
                        dir_path.to_string_lossy(),
                        e
                    ),
                };
                cx.notify();
            }
        }
    }

    pub fn handle_stage_selection(&mut self, event: StageSelectionEvent, cx: &mut Context<Self>) {
        // Discard is irreversible — show confirmation dialog before executing.
        if matches!(event.action, SelectionAction::Discard) {
            self.open_discard_hunk_confirm(event, cx);
            return;
        }
        self.execute_stage_selection(event, cx);
    }

    /// Execute a stage/unstage/discard selection without showing a confirmation dialog.
    /// Called from `confirm_discard_hunk` after the user has already confirmed.
    pub(crate) fn execute_stage_selection(
        &mut self,
        event: StageSelectionEvent,
        cx: &mut Context<Self>,
    ) {
        let result = {
            if let Some(repo) = self.worktree_repo() {
                match (event.action, event.kind) {
                    (SelectionAction::Stage, StageSelectionKind::HunkAtLine(line)) => {
                        repo.stage_hunk_at_line(&event.file_path, line)
                    }
                    (SelectionAction::Stage, StageSelectionKind::LineRange { start, end }) => {
                        repo.stage_line_range(&event.file_path, start, end)
                    }
                    (SelectionAction::Unstage, StageSelectionKind::HunkAtLine(line)) => {
                        repo.unstage_hunk_at_line(&event.file_path, line)
                    }
                    (SelectionAction::Unstage, StageSelectionKind::LineRange { start, end }) => {
                        repo.unstage_line_range(&event.file_path, start, end)
                    }
                    (SelectionAction::Discard, StageSelectionKind::HunkAtLine(line)) => {
                        repo.discard_hunk_at_line(&event.file_path, line)
                    }
                    (SelectionAction::Discard, StageSelectionKind::LineRange { start, end }) => {
                        repo.discard_line_range(&event.file_path, start, end)
                    }
                }
            } else {
                Err(crate::git::GitError::Command(
                    "Git repository not available for active worktree".to_string(),
                ))
            }
        };

        match result {
            Ok(()) => {
                if let Some(session) = self.session_manager.active_session()
                    && let Ok(relative_path) = event.file_path.strip_prefix(session.worktree_path())
                {
                    let relative_path = relative_path.to_path_buf();
                    let change_type = self
                        .changed_files
                        .iter()
                        .find(|f| f.path == relative_path)
                        .and_then(|f| match event.section {
                            ChangeSection::Staged => f.staged_change,
                            ChangeSection::Unstaged => f.unstaged_change,
                        });
                    self.on_file_selected(
                        relative_path,
                        Some(event.section),
                        change_type,
                        None,
                        cx,
                    );
                }
                self.refresh_file_list_async(cx);
            }
            Err(e) => {
                self.active_dialog = crate::dialog::ActiveDialog::Error {
                    message: format!("Failed to stage selection: {}", e),
                };
                cx.notify();
            }
        }
    }

    /// Handle GotoDefinitionEvent: start LSP if needed, send definition request, navigate to result
    pub fn handle_goto_definition(&mut self, event: GotoDefinitionEvent, cx: &mut Context<Self>) {
        let file_path = event.file_path.clone();
        let line = event.line;
        let character = event.character;

        let lang_config = self.language_registry.detect(&file_path);
        let lsp_spec = lang_config.and_then(|config| config.lsp.as_ref());

        let Some(lsp_spec) = lsp_spec else {
            return;
        };

        let Some(worktree_root) = self.active_worktree_path() else {
            return;
        };

        let workspace_id = WorkspaceId {
            root: worktree_root,
            server_id: lsp_spec.server_id.to_string(),
        };
        let command = lsp_spec.command.to_string();
        let args: Vec<String> = lsp_spec.args.iter().map(|a| a.to_string()).collect();
        let config = lang_config.unwrap();
        let language_id = lsp_spec.language_id.unwrap_or(config.id).to_string();

        let lsp = Arc::clone(&self.lsp_manager);

        let file_content = std::fs::read_to_string(&file_path).unwrap_or_default();

        let file_uri = match Url::from_file_path(&file_path) {
            Ok(uri) => uri,
            Err(_) => return,
        };

        cx.spawn(async move |entity, cx| {
            let mut manager = lsp.lock().await;
            let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

            if let Err(e) = manager
                .ensure_server(&workspace_id, &command, &args_ref)
                .await
            {
                eprintln!("LSP: {e:#}");
                return;
            }

            if let Err(e) = manager
                .sync_document(&workspace_id, file_uri.clone(), &language_id, &file_content)
                .await
            {
                eprintln!("LSP ensure_did_open failed: {e:#}");
                return;
            }

            // ContentModified (-32801) はサーバーがまだインデックス中の場合に返る。
            // リトライで解消するため最大5回まで再試行する。
            const MAX_RETRIES: u32 = 5;
            const RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(500);

            for attempt in 0..=MAX_RETRIES {
                let result = manager
                    .definition(&workspace_id, file_uri.clone(), line, character)
                    .await;

                match result {
                    Ok(Some(response)) => {
                        drop(manager);
                        Self::navigate_to_definition(entity, cx, response);
                        return;
                    }
                    Ok(None) => return,
                    Err(e) => {
                        let is_retryable = e.chain().any(|cause| {
                            cause
                                .downcast_ref::<LspRequestError>()
                                .is_some_and(|le| le.is_content_modified())
                        });
                        if is_retryable && attempt < MAX_RETRIES {
                            drop(manager);
                            smol::Timer::after(RETRY_DELAY).await;
                            manager = lsp.lock().await;
                            continue;
                        }
                        eprintln!("LSP definition request failed: {e:#}");
                        return;
                    }
                }
            }
        })
        .detach();
    }

    fn navigate_to_definition(
        entity: gpui::WeakEntity<Self>,
        cx: &mut gpui::AsyncApp,
        response: lsp_types::GotoDefinitionResponse,
    ) {
        use lsp_types::{GotoDefinitionResponse, Location};

        let location: Option<Location> = match response {
            GotoDefinitionResponse::Scalar(loc) => Some(loc),
            GotoDefinitionResponse::Array(locs) => locs.into_iter().next(),
            GotoDefinitionResponse::Link(links) => links.into_iter().next().map(|link| Location {
                uri: link.target_uri,
                range: link.target_selection_range,
            }),
        };

        let Some(location) = location else {
            return;
        };

        let Ok(url) = Url::parse(location.uri.as_str()) else {
            return;
        };
        let Ok(target_path) = url.to_file_path() else {
            return;
        };

        let target_line = location.range.start.line as usize + 1;

        let _ = entity.update(cx, |app, cx| {
            let relative_path: PathBuf = if let Some(session) = app.session_manager.active_session()
            {
                target_path
                    .strip_prefix(session.worktree_path())
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|_| target_path.clone())
            } else {
                target_path.clone()
            };

            app.on_file_selected(relative_path, None, None, Some(target_line), cx);
        });
    }
}
