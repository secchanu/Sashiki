//! File operation methods

use super::SashikiApp;
use crate::git::{ChangeType, GitRepo};
use crate::ui::{ChangeInfo, FileListMode, FileTreeNode};
use gpui::Context;
use std::path::{Path, PathBuf};

impl SashikiApp {
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

        let worktree_path = self
            .session_manager
            .active_session()
            .map(|s| s.worktree_path().to_path_buf());

        let file_list_mode = self.file_list_mode;

        cx.spawn(async move |entity, cx| {
            let files = if let Some(path) = worktree_path {
                GitRepo::open(&path)
                    .ok()
                    .and_then(|repo| repo.get_changed_files().ok())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            // Ignore error: only fails if entity was dropped (app closed)
            let _ = entity.update(cx, |app, cx| {
                app.changed_files = files;
                if file_list_mode == FileListMode::Changes {
                    app.build_file_tree();
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Synchronous version of refresh_changed_files (for initial load)
    pub fn refresh_changed_files_sync(&mut self) {
        let worktree_path = self
            .session_manager
            .active_session()
            .map(|s| s.worktree_path().to_path_buf());

        if let Some(path) = worktree_path
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
        }
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

        self.file_view.update(cx, |view, _cx| match change_type {
            Some(ChangeType::Deleted) => {
                if let Some(diff_content) = diff {
                    view.open_deleted_file_with_diff(full_path.clone(), diff_content);
                }
            }
            _ => {
                if let Some(diff_content) = diff {
                    let _ = view.open_file_with_diff(full_path.clone(), diff_content);
                } else {
                    let _ = view.open_file(full_path.clone());
                }
            }
        });

        self.show_file_view = true;
        cx.notify();
    }
}
