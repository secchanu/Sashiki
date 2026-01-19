//! Dialog operation methods

use super::SashikiApp;
use crate::dialog::ActiveDialog;
use crate::git::validate_branch_name;
use gpui::{Context, Focusable, Window};
use std::path::Path;

impl SashikiApp {
    pub fn open_create_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::CreateWorktree;
        self.create_branch_input.clear();
        window.focus(&self.create_dialog_focus, cx);
        cx.notify();
    }

    pub fn close_create_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::None;
        self.create_branch_input.clear();
        if let Some(terminal) = self.active_terminal() {
            let focus = terminal.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    pub fn submit_create_worktree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let branch = self.create_branch_input.trim().to_string();

        if let Err(msg) = validate_branch_name(&branch) {
            self.active_dialog = ActiveDialog::Error {
                message: msg.to_string(),
            };
            cx.notify();
            return;
        }

        let result = (|| -> Result<(), String> {
            let repo = self
                .git_repo
                .as_ref()
                .ok_or("Git repository not available")?;
            let path = repo
                .generate_worktree_path(&branch)
                .ok_or("Failed to generate worktree path")?;

            if path.exists() {
                return Err(format!(
                    "Worktree directory already exists: {}\nPlease remove it manually or choose a different branch name.",
                    path.display()
                ));
            }

            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create parent directory: {}", e))?;
            }

            let worktree_name = branch.replace('/', "-");

            let worktree = repo
                .create_worktree(&worktree_name, &branch, &path)
                .map_err(|e| format!("Failed to create worktree: {}", e))?;

            self.session_manager.add_session(worktree);
            let new_index = self.session_manager.len() - 1;
            self.session_manager.switch_to(new_index);
            self.session_manager.ensure_active_session_terminal(cx);
            Ok(())
        })();

        if let Err(msg) = result {
            self.active_dialog = ActiveDialog::Error { message: msg };
        } else {
            self.refresh_file_list();
            self.close_create_dialog(window, cx);
        }
    }

    pub fn open_delete_dialog(&mut self, index: usize, cx: &mut Context<Self>) {
        let sessions = self.session_manager.sessions();
        if index < sessions.len() && !sessions[index].is_main() {
            self.active_dialog = ActiveDialog::DeleteConfirm {
                target_index: index,
            };
            cx.notify();
        }
    }

    pub fn close_delete_dialog(&mut self, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::None;
        cx.notify();
    }

    pub fn confirm_delete_worktree(&mut self, cx: &mut Context<Self>) {
        let ActiveDialog::DeleteConfirm {
            target_index: index,
        } = self.active_dialog
        else {
            self.close_delete_dialog(cx);
            return;
        };

        let (worktree_name, worktree_path, is_main) = {
            let sessions = self.session_manager.sessions();
            if index >= sessions.len() {
                self.close_delete_dialog(cx);
                return;
            }
            let session = &sessions[index];
            (
                session.name().to_string(),
                session.worktree_path().to_path_buf(),
                session.is_main(),
            )
        };

        if is_main {
            self.close_delete_dialog(cx);
            return;
        }

        self.prepare_session_for_deletion(index, cx);
        self.cleanup_resources_for_deletion(index, cx);

        if let Some(ref repo) = self.git_repo {
            // Non-fatal: git worktree prune will clean up orphaned entries.
            // Not shown to user as it would be confusing - the directory is removed anyway.
            if let Err(e) = repo.remove_worktree(&worktree_name) {
                eprintln!("Warning: git worktree remove failed: {}", e);
            }
        }

        self.active_dialog = ActiveDialog::Deleting;
        cx.spawn(async move |entity, cx| {
            let result = Self::remove_worktree_directory_async(&worktree_path).await;
            // Ignore error: only fails if entity was dropped (app closed)
            let _ = entity.update(cx, |app, cx| {
                app.finish_delete_worktree(index, result, cx);
            });
        })
        .detach();
    }

    /// Async version of directory removal with retries.
    ///
    /// Retries are needed on Windows because file handles may be held briefly
    /// after terminal shutdown, causing "directory not empty" errors.
    pub(crate) async fn remove_worktree_directory_async(path: &Path) -> Result<(), String> {
        const MAX_RETRIES: u32 = 10;
        const RETRY_DELAY_MS: u64 = 100;

        let path = path.to_path_buf();

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                smol::Timer::after(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
            }

            let path_clone = path.clone();
            let result = smol::unblock(move || {
                if !path_clone.exists() {
                    return Ok(());
                }
                std::fs::remove_dir_all(&path_clone)
            })
            .await;

            match result {
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

    /// Called when async directory deletion completes
    pub fn finish_delete_worktree(
        &mut self,
        index: usize,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        if let Err(e) = result {
            self.active_dialog = ActiveDialog::Error { message: e };
            cx.notify();
            return;
        }

        self.session_manager.remove_session(index);
        self.refresh_file_list();
        self.active_dialog = ActiveDialog::None;
        cx.notify();
    }

    pub fn prepare_session_for_deletion(&mut self, index: usize, cx: &mut Context<Self>) {
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
            self.session_manager.ensure_active_session_terminal(cx);
        }
    }

    pub fn cleanup_resources_for_deletion(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(terminal) = self.session_manager.get_session_active_terminal(index) {
            terminal.update(cx, |view, _cx| view.shutdown());
        }

        self.invalidate_worktree_repo_cache();

        self.file_view.update(cx, |view, _cx| view.close());
        self.show_file_view = false;

        self.session_manager.clear_session_terminals(index);
    }

    pub fn close_error_dialog(&mut self, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::None;
        cx.notify();
    }
}
