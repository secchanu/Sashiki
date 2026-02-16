//! File operation methods

use super::SashikiApp;
use crate::git::{ChangeType, GitRepo};
use crate::lsp::{LspRequestError, WorkspaceId};
use crate::ui::{ChangeInfo, FileListMode, FileTreeNode, GotoDefinitionEvent};
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

    /// Refresh file list and rebuild tree for the active session (sync)
    pub fn refresh_file_list(&mut self) {
        self.invalidate_worktree_repo_cache();
        self.refresh_changed_files_sync();
        if self.file_list_mode == FileListMode::Changes {
            self.build_file_tree();
        }
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
                if app.file_list_mode == FileListMode::Changes {
                    app.build_file_tree();
                }
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

    /// Build file tree for Changes mode
    pub fn build_file_tree(&mut self) {
        let files = self.changed_files.iter().map(|f| {
            let info = ChangeInfo {
                change_type: f.change_type,
                staged: f.staged,
            };
            (f.path.clone(), Some(info))
        });
        self.file_tree = Some(FileTreeNode::from_files(files));
    }

    pub fn toggle_dir_expanded(&mut self, path: &Path) {
        if self.expanded_dirs.contains(path) {
            self.expanded_dirs.remove(path);
        } else {
            self.expanded_dirs.insert(path.to_path_buf());
        }
    }

    pub fn on_file_selected(
        &mut self,
        path: PathBuf,
        change_type: Option<ChangeType>,
        target_line: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let full_path = if let Some(session) = self.session_manager.active_session() {
            session.worktree_path().join(&path)
        } else {
            path.clone()
        };

        let diff = self.worktree_repo().and_then(|repo| match change_type {
            Some(ChangeType::Added) => repo.generate_added_diff(&full_path).ok(),
            Some(ChangeType::Deleted) => repo.generate_deleted_diff(&full_path).ok(),
            _ => repo.get_file_diff(&full_path).ok(),
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

            app.on_file_selected(relative_path, None, Some(target_line), cx);
        });
    }
}
