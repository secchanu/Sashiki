//! Dialog operation methods

use super::SashikiApp;
use crate::dialog::ActiveDialog;
use crate::git::{GitRepo, validate_branch_name};
use crate::template::{self, TemplateConfig};
use crate::ui::StageSelectionEvent;
use gpui::{Context, Focusable, PathPromptOptions, Window};
use std::path::{Path, PathBuf};

impl SashikiApp {
    pub fn open_create_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::CreateWorktree;
        self.create_input.update(cx, |input, _| input.clear());
        cx.notify();
        // Focus on the next frame so track_focus has registered in the tree.
        cx.on_next_frame(window, |this, window, cx| {
            window.focus(&this.create_dialog_focus, cx);
            cx.notify();
        });
    }

    pub fn close_create_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::None;
        self.create_input.update(cx, |input, _| input.clear());
        if let Some(terminal) = self.active_terminal() {
            let focus = terminal.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    pub fn open_commit_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_changed_files_sync();
        let has_any_changes = self
            .changed_files
            .iter()
            .any(|f| f.has_staged_changes() || f.has_unstaged_changes());
        if !has_any_changes {
            // Nothing to commit — redirect to amend if HEAD exists (P2-Issue 3)
            let has_head = if let Some(repo) = self.worktree_repo() {
                repo.get_last_commit_message().is_ok()
            } else {
                false
            };
            if has_head {
                self.open_amend_dialog(window, cx);
            } else {
                self.active_dialog = ActiveDialog::Error {
                    message: "Nothing to commit".to_string(),
                };
                cx.notify();
            }
            return;
        }

        self.commit_amend_mode = false;
        self.active_dialog = ActiveDialog::Commit;
        self.commit_input.update(cx, |input, _| input.clear());
        cx.notify();
        // Focus on the next frame so track_focus has registered in the tree.
        cx.on_next_frame(window, |this, window, cx| {
            window.focus(&this.commit_dialog_focus, cx);
            cx.notify();
        });
    }

    /// Open the commit dialog pre-configured for amending the last commit.
    pub fn open_amend_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let last_msg = if let Some(repo) = self.worktree_repo() {
            repo.get_last_commit_message().unwrap_or_default()
        } else {
            self.active_dialog = ActiveDialog::Error {
                message: "Git repository not available for active worktree".to_string(),
            };
            cx.notify();
            return;
        };

        self.commit_amend_mode = true;
        self.active_dialog = ActiveDialog::Commit;
        self.commit_input
            .update(cx, |input, _| input.set_text(last_msg));
        cx.notify();
        // Focus on the next frame so track_focus has registered in the tree.
        cx.on_next_frame(window, |this, window, cx| {
            window.focus(&this.commit_dialog_focus, cx);
            cx.notify();
        });
    }

    pub fn close_commit_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::None;
        self.commit_amend_mode = false;
        self.commit_input.update(cx, |input, _| input.clear());
        if let Some(terminal) = self.active_terminal() {
            let focus = terminal.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    pub fn submit_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let message = self.commit_input.read(cx).text().trim().to_string();
        if message.is_empty() {
            self.active_dialog = ActiveDialog::Error {
                message: "Commit message cannot be empty".to_string(),
            };
            cx.notify();
            return;
        }

        let amend = self.commit_amend_mode;
        // Read has_staged before calling worktree_repo() to avoid borrow conflict
        let has_staged = self.changed_files.iter().any(|f| f.has_staged_changes());

        // Amend: ask for confirmation before overwriting the last commit.
        if amend {
            self.active_dialog = ActiveDialog::AmendCommitConfirm;
            cx.notify();
            return;
        }

        // Smart commit: ask for confirmation before auto-staging (P1-Issue 4)
        if !has_staged {
            self.active_dialog = ActiveDialog::SmartCommitConfirm;
            cx.notify();
            return;
        }

        let result = if let Some(repo) = self.worktree_repo() {
            repo.commit(&message)
        } else {
            Err(crate::git::GitError::Command(
                "Git repository not available for active worktree".to_string(),
            ))
        };

        match result {
            Ok(()) => {
                let do_push = self.commit_and_push;
                let do_sync = self.commit_and_sync;
                self.commit_amend_mode = false;
                self.commit_and_push = false;
                self.commit_and_sync = false;
                self.close_commit_dialog(window, cx);
                self.refresh_file_list_async(cx);

                if do_sync || do_push {
                    if let Some(path) = self
                        .session_manager
                        .active_session()
                        .map(|s| s.worktree_path().to_path_buf())
                    {
                        if do_sync {
                            self.git_sync_async(path, cx);
                        } else {
                            self.git_push_async(path, cx);
                        }
                    }
                }
            }
            Err(e) => {
                self.commit_and_push = false;
                self.commit_and_sync = false;
                self.active_dialog = ActiveDialog::Error {
                    message: format!("Failed to commit: {}", e),
                };
                cx.notify();
            }
        }
    }

    pub fn git_push_async(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        self.git_sync_in_progress = true;
        cx.notify();

        cx.spawn(async move |entity, cx| {
            let result = GitRepo::open(&path).and_then(|repo| repo.push());
            let _ = entity.update(cx, move |app, cx| {
                app.git_sync_in_progress = false;
                if let Err(e) = result {
                    app.active_dialog = ActiveDialog::Error {
                        message: format!("Push failed: {}", e),
                    };
                }
                app.refresh_git_sync_state();
                cx.notify();
            });
        })
        .detach();
    }

    pub fn git_pull_async(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        self.git_sync_in_progress = true;
        cx.notify();

        cx.spawn(async move |entity, cx| {
            let result = GitRepo::open(&path).and_then(|repo| repo.pull());
            let _ = entity.update(cx, move |app, cx| {
                app.git_sync_in_progress = false;
                if let Err(e) = result {
                    app.active_dialog = ActiveDialog::Error {
                        message: format!("Pull failed: {}", e),
                    };
                }
                app.refresh_file_list_async(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Sync: pull (rebase) then push, like VSCode's Sync Changes.
    pub fn git_sync_async(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        self.git_sync_in_progress = true;
        cx.notify();

        cx.spawn(async move |entity, cx| {
            let result = GitRepo::open(&path).and_then(|repo| {
                repo.pull()?;
                repo.push()
            });
            let _ = entity.update(cx, move |app, cx| {
                app.git_sync_in_progress = false;
                if let Err(e) = result {
                    app.active_dialog = ActiveDialog::Error {
                        message: format!("Sync failed: {}", e),
                    };
                }
                app.refresh_file_list_async(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn open_stash_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entries_result = if let Some(repo) = self.worktree_repo() {
            repo.list_stashes()
        } else {
            Err(crate::git::GitError::Command(
                "Git repository not available for active worktree".to_string(),
            ))
        };

        match entries_result {
            Ok(entries) => {
                // repo の借用を先にまとめて解放してから self のフィールドを更新する
                let entry_files: Vec<(String, Vec<(String, String)>)> =
                    if let Some(repo) = self.worktree_repo() {
                        entries
                            .iter()
                            .filter_map(|e| {
                                repo.stash_show_files(&e.reference)
                                    .ok()
                                    .map(|files| (e.reference.clone(), files))
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };

                self.stash_entry_files.clear();
                for (reference, files) in entry_files {
                    self.stash_entry_files.insert(reference, files);
                }
                self.stash_entries = entries;
                self.stash_expanded_entries.clear();
                self.stash_input.update(cx, |input, _| input.clear());
                self.active_dialog = ActiveDialog::Stash;
                cx.notify();
                // Focus on the next frame so track_focus has registered in the tree.
                cx.on_next_frame(window, |this, window, cx| {
                    window.focus(&this.stash_dialog_focus, cx);
                    cx.notify();
                });
            }
            Err(e) => {
                self.active_dialog = ActiveDialog::Error {
                    message: format!("Failed to load stashes: {}", e),
                };
                cx.notify();
            }
        }
    }

    pub fn close_stash_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::None;
        self.stash_input.update(cx, |input, _| input.clear());
        if let Some(terminal) = self.active_terminal() {
            let focus = terminal.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    pub fn create_stash(&mut self, cx: &mut Context<Self>) {
        let message = self.stash_input.read(cx).text().trim().to_string();
        let mode = self.stash_mode;
        let result = if let Some(repo) = self.worktree_repo() {
            if message.is_empty() {
                repo.create_stash(None, mode)
            } else {
                repo.create_stash(Some(&message), mode)
            }
        } else {
            Err(crate::git::GitError::Command(
                "Git repository not available for active worktree".to_string(),
            ))
        };

        match result {
            Ok(()) => {
                self.stash_input.update(cx, |input, _| input.clear());
                self.refresh_file_list_async(cx);
                self.refresh_stash_entries(cx);
            }
            Err(e) => {
                self.active_dialog = ActiveDialog::Error {
                    message: format!("Failed to create stash: {}", e),
                };
                cx.notify();
            }
        }
    }

    pub fn apply_stash_entry(&mut self, reference: String, cx: &mut Context<Self>) {
        let result = if let Some(repo) = self.worktree_repo() {
            repo.apply_stash(&reference)
        } else {
            Err(crate::git::GitError::Command(
                "Git repository not available for active worktree".to_string(),
            ))
        };

        match result {
            Ok(()) => {
                self.refresh_file_list_async(cx);
                self.refresh_stash_entries(cx);
            }
            Err(e) => {
                self.active_dialog = ActiveDialog::Error {
                    message: format!("Failed to apply stash {}: {}", reference, e),
                };
                cx.notify();
            }
        }
    }

    pub fn pop_stash_entry(&mut self, reference: String, cx: &mut Context<Self>) {
        let result = if let Some(repo) = self.worktree_repo() {
            repo.pop_stash(&reference)
        } else {
            Err(crate::git::GitError::Command(
                "Git repository not available for active worktree".to_string(),
            ))
        };

        match result {
            Ok(()) => {
                self.refresh_file_list_async(cx);
                self.refresh_stash_entries(cx);
            }
            Err(e) => {
                self.active_dialog = ActiveDialog::Error {
                    message: format!("Failed to pop stash {}: {}", reference, e),
                };
                cx.notify();
            }
        }
    }

    pub fn drop_stash_entry(&mut self, reference: String, cx: &mut Context<Self>) {
        let result = if let Some(repo) = self.worktree_repo() {
            repo.drop_stash(&reference)
        } else {
            Err(crate::git::GitError::Command(
                "Git repository not available for active worktree".to_string(),
            ))
        };

        match result {
            Ok(()) => self.refresh_stash_entries(cx),
            Err(e) => {
                self.active_dialog = ActiveDialog::Error {
                    message: format!("Failed to drop stash {}: {}", reference, e),
                };
                cx.notify();
            }
        }
    }

    fn refresh_stash_entries(&mut self, cx: &mut Context<Self>) {
        if let Some(repo) = self.worktree_repo() {
            match repo.list_stashes() {
                Ok(entries) => self.stash_entries = entries,
                Err(e) => {
                    self.active_dialog = ActiveDialog::Error {
                        message: format!("Failed to refresh stash list: {}", e),
                    };
                    cx.notify();
                    return;
                }
            }
        }
        cx.notify();
    }

    pub fn submit_create_worktree(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let branch = self.create_input.read(cx).text().trim().to_string();

        if let Err(msg) = validate_branch_name(&branch) {
            self.active_dialog = ActiveDialog::Error {
                message: msg.to_string(),
            };
            cx.notify();
            return;
        }

        let repo = match self.session_manager.active_git_repo().cloned() {
            Some(r) => r,
            None => {
                self.active_dialog = ActiveDialog::Error {
                    message: "Git repository not available".to_string(),
                };
                cx.notify();
                return;
            }
        };

        let worktree_path = match repo.generate_worktree_path(&branch) {
            Some(p) => p,
            None => {
                self.active_dialog = ActiveDialog::Error {
                    message: "Failed to generate worktree path".to_string(),
                };
                cx.notify();
                return;
            }
        };

        if worktree_path.exists() {
            self.active_dialog = ActiveDialog::Error {
                message: format!(
                    "Worktree directory already exists: {}\nPlease remove it manually or choose a different branch name.",
                    worktree_path.display()
                ),
            };
            cx.notify();
            return;
        }

        // Load template config
        let template = TemplateConfig::load(&repo);
        let steps = template.creation_steps();

        // Switch to Creating dialog with progress
        self.active_dialog = ActiveDialog::Creating {
            branch: branch.clone(),
            steps: steps.clone(),
            current_step: 0,
        };
        cx.notify();

        // Gather data needed for async pipeline
        let main_workdir = repo.workdir().to_path_buf();
        let git_dir = repo.git_dir().to_path_buf();
        let worktree_name = branch.replace('/', "-");

        // Close create dialog state (branch input is no longer needed)
        self.create_input.update(cx, |input, _| input.clear());

        // Spawn async creation pipeline
        cx.spawn(async move |entity, cx| {
            let result = Self::run_creation_pipeline(
                &entity,
                cx,
                main_workdir,
                git_dir,
                branch,
                worktree_name,
                worktree_path,
                template,
            )
            .await;

            if let Err(msg) = result {
                let _ = entity.update(cx, |app, cx| {
                    app.active_dialog = ActiveDialog::Error { message: msg };
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Async creation pipeline: pre-create -> worktree -> file copy -> file sync -> post-create
    async fn run_creation_pipeline(
        entity: &gpui::WeakEntity<Self>,
        cx: &mut gpui::AsyncApp,
        main_workdir: PathBuf,
        git_dir: PathBuf,
        branch: String,
        worktree_name: String,
        worktree_path: PathBuf,
        template: TemplateConfig,
    ) -> Result<(), String> {
        let mut step_index: usize = 0;

        // --- Phase 1: Pre-create commands ---
        for cmd in &template.pre_create_commands {
            let cmd = cmd.clone();
            let workdir = main_workdir.clone();

            let result = smol::unblock(move || template::run_shell_command(&cmd, &workdir)).await;

            if let Err(e) = result {
                return Err(format!("Pre-create command failed: {}", e));
            }

            step_index += 1;
            let step = step_index;
            let _ = entity.update(cx, |app, cx| {
                if let ActiveDialog::Creating {
                    ref mut current_step,
                    ..
                } = app.active_dialog
                {
                    *current_step = step;
                }
                cx.notify();
            });
        }

        // --- Phase 2: Create worktree ---
        {
            let mw = main_workdir.clone();
            let gd = git_dir.clone();
            let wn = worktree_name.clone();
            let br = branch.clone();
            let wp = worktree_path.clone();

            let worktree = smol::unblock(move || {
                if let Some(parent) = wp.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| format!("Failed to create parent directory: {}", e))?;
                }
                let repo = GitRepo::from_parts(mw, gd);
                repo.create_worktree(&wn, &br, &wp)
                    .map_err(|e| format!("Failed to create worktree: {}", e))
            })
            .await?;

            step_index += 1;
            let step = step_index;
            let wt = worktree.clone();
            let _ = entity.update(cx, |app, cx| {
                if let ActiveDialog::Creating {
                    ref mut current_step,
                    ..
                } = app.active_dialog
                {
                    *current_step = step;
                }
                // Add the session now so it appears in sidebar
                app.session_manager.add_session(wt);
                cx.notify();
            });
        }

        // --- Phase 3: Copy files ---
        if !template.file_copies.is_empty() {
            let src = main_workdir.clone();
            let dst = worktree_path.clone();
            let tmpl = template.clone();

            let copy_results = smol::unblock(move || tmpl.copy_files(&src, &dst)).await;

            // Check for errors
            let errors: Vec<_> = copy_results.iter().filter(|r| !r.success).collect();

            if !errors.is_empty() {
                let msg = errors
                    .iter()
                    .map(|r| {
                        format!(
                            "{}: {}",
                            r.path,
                            r.error.as_deref().unwrap_or("unknown error")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                eprintln!("Warning: some file copies failed:\n{}", msg);
                // Continue despite copy errors (non-fatal)
            }

            step_index += 1;
            let step = step_index;
            let _ = entity.update(cx, |app, cx| {
                if let ActiveDialog::Creating {
                    ref mut current_step,
                    ..
                } = app.active_dialog
                {
                    *current_step = step;
                }
                cx.notify();
            });
        }

        // --- Phase 4: Sync files ---
        if !template.file_syncs.is_empty() {
            let src = main_workdir.clone();
            let dst = worktree_path.clone();
            let tmpl = template.clone();

            let sync_results = smol::unblock(move || tmpl.sync_files(&src, &dst)).await;

            // Check for hard errors
            let errors: Vec<_> = sync_results.iter().filter(|r| !r.success).collect();
            if !errors.is_empty() {
                let msg = errors
                    .iter()
                    .map(|r| {
                        format!(
                            "{}: {}",
                            r.path,
                            r.error.as_deref().unwrap_or("unknown error")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                eprintln!("Warning: some file syncs failed:\n{}", msg);
                // Continue despite sync errors (non-fatal)
            }

            // Report fallback usage for visibility
            let fallback_results: Vec<_> =
                sync_results.iter().filter(|r| r.copied_instead).collect();
            if !fallback_results.is_empty() {
                let msg = fallback_results
                    .iter()
                    .map(|r| {
                        format!(
                            "{}: {}",
                            r.path,
                            r.error
                                .as_deref()
                                .unwrap_or("hard-link failed, copied instead")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                eprintln!(
                    "Warning: {} file(s) could not be hard-linked and were copied instead:\n{}",
                    fallback_results.len(),
                    msg
                );
            }

            step_index += 1;
            let step = step_index;
            let _ = entity.update(cx, |app, cx| {
                if let ActiveDialog::Creating {
                    ref mut current_step,
                    ..
                } = app.active_dialog
                {
                    *current_step = step;
                }
                cx.notify();
            });
        }

        // --- Phase 5: Post-create commands ---
        let effective_workdir = template.resolve_working_directory(&worktree_path);

        for cmd in &template.post_create_commands {
            let cmd = cmd.clone();
            let workdir = effective_workdir.clone();

            let result = smol::unblock(move || template::run_shell_command(&cmd, &workdir)).await;

            if let Err(e) = result {
                return Err(format!("Post-create command failed: {}", e));
            }

            step_index += 1;
            let step = step_index;
            let _ = entity.update(cx, |app, cx| {
                if let ActiveDialog::Creating {
                    ref mut current_step,
                    ..
                } = app.active_dialog
                {
                    *current_step = step;
                }
                cx.notify();
            });
        }

        // --- Finish: switch to new session and start terminal ---
        let ew = effective_workdir.clone();
        let _ = entity.update(cx, |app, cx| {
            app.finish_create_worktree(ew, cx);
        });

        Ok(())
    }

    /// Called when async creation pipeline completes successfully
    fn finish_create_worktree(&mut self, effective_workdir: PathBuf, cx: &mut Context<Self>) {
        let new_index = self.session_manager.len() - 1;
        self.session_manager.switch_to(new_index);
        self.session_manager
            .ensure_active_session_terminal_in(effective_workdir, cx);

        self.refresh_file_list();
        self.active_dialog = ActiveDialog::None;
        cx.notify();
    }

    // === Delete worktree ===

    pub fn open_undo_commit_confirm(&mut self, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::UndoCommitConfirm;
        cx.notify();
    }

    pub fn close_undo_commit_confirm(&mut self, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::None;
        cx.notify();
    }

    /// Undo the last commit (git reset --soft HEAD~1).
    /// The undone commit's changes are left staged.
    pub fn confirm_undo_last_commit(&mut self, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::None;
        let result = if let Some(repo) = self.worktree_repo() {
            repo.undo_last_commit()
        } else {
            Err(crate::git::GitError::Command(
                "Git repository not available for active worktree".to_string(),
            ))
        };

        match result {
            Ok(()) => self.refresh_file_list_async(cx),
            Err(e) => {
                self.active_dialog = ActiveDialog::Error {
                    message: format!("Failed to undo last commit: {}", e),
                };
                cx.notify();
            }
        }
    }

    pub fn open_discard_all_confirm(&mut self, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::DiscardAllConfirm;
        cx.notify();
    }

    pub fn confirm_discard_all(&mut self, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::None;
        let result = if let Some(repo) = self.worktree_repo() {
            repo.discard_all_changes()
        } else {
            Err(crate::git::GitError::Command(
                "Git repository not available for active worktree".to_string(),
            ))
        };

        match result {
            Ok(()) => self.refresh_file_list_async(cx),
            Err(e) => {
                self.active_dialog = ActiveDialog::Error {
                    message: format!("Failed to discard all changes: {}", e),
                };
                cx.notify();
            }
        }
    }

    pub fn close_discard_all_confirm(&mut self, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::None;
        cx.notify();
    }

    /// User confirmed Smart Commit: stage all changes then commit.
    pub fn confirm_smart_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let stage_result = if let Some(repo) = self.worktree_repo() {
            repo.stage_all()
        } else {
            Err(crate::git::GitError::Command(
                "Git repository not available for active worktree".to_string(),
            ))
        };
        if let Err(e) = stage_result {
            self.active_dialog = ActiveDialog::Error {
                message: format!("Failed to stage changes: {}", e),
            };
            cx.notify();
            return;
        }

        let message = self.commit_input.read(cx).text().trim().to_string();
        let result = if let Some(repo) = self.worktree_repo() {
            repo.commit(&message)
        } else {
            Err(crate::git::GitError::Command(
                "Git repository not available for active worktree".to_string(),
            ))
        };
        match result {
            Ok(()) => {
                let do_push = self.commit_and_push;
                let do_sync = self.commit_and_sync;
                self.commit_amend_mode = false;
                self.commit_and_push = false;
                self.commit_and_sync = false;
                self.close_commit_dialog(window, cx);
                self.refresh_file_list_async(cx);

                if do_sync || do_push {
                    if let Some(path) = self
                        .session_manager
                        .active_session()
                        .map(|s| s.worktree_path().to_path_buf())
                    {
                        if do_sync {
                            self.git_sync_async(path, cx);
                        } else {
                            self.git_push_async(path, cx);
                        }
                    }
                }
            }
            Err(e) => {
                self.commit_and_push = false;
                self.commit_and_sync = false;
                self.active_dialog = ActiveDialog::Error {
                    message: format!("Failed to commit: {}", e),
                };
                cx.notify();
            }
        }
    }

    pub fn cancel_smart_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Return user to the commit dialog they were in
        self.active_dialog = ActiveDialog::Commit;
        cx.notify();
        cx.on_next_frame(window, |this, window, cx| {
            window.focus(&this.commit_dialog_focus, cx);
            cx.notify();
        });
    }

    // === Amend Commit Confirm ===

    /// Execute the amend after user confirmed via AmendCommitConfirm dialog.
    pub fn confirm_amend_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let message = self.commit_input.read(cx).text().trim().to_string();
        let result = if let Some(repo) = self.worktree_repo() {
            repo.amend_commit(Some(&message))
        } else {
            Err(crate::git::GitError::Command(
                "Git repository not available for active worktree".to_string(),
            ))
        };
        match result {
            Ok(()) => {
                let do_push = self.commit_and_push;
                let do_sync = self.commit_and_sync;
                self.commit_amend_mode = false;
                self.commit_and_push = false;
                self.commit_and_sync = false;
                self.close_commit_dialog(window, cx);
                self.refresh_file_list_async(cx);

                if do_sync || do_push {
                    if let Some(path) = self
                        .session_manager
                        .active_session()
                        .map(|s| s.worktree_path().to_path_buf())
                    {
                        if do_sync {
                            self.git_sync_async(path, cx);
                        } else {
                            self.git_push_async(path, cx);
                        }
                    }
                }
            }
            Err(e) => {
                self.commit_and_push = false;
                self.commit_and_sync = false;
                self.active_dialog = ActiveDialog::Error {
                    message: format!("Failed to amend commit: {}", e),
                };
                cx.notify();
            }
        }
    }

    pub fn cancel_amend_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::Commit;
        cx.notify();
        cx.on_next_frame(window, |this, window, cx| {
            window.focus(&this.commit_dialog_focus, cx);
            cx.notify();
        });
    }

    // === Discard Hunk Confirm ===

    pub fn open_discard_hunk_confirm(
        &mut self,
        event: StageSelectionEvent,
        cx: &mut Context<Self>,
    ) {
        self.pending_discard_hunk = Some(event);
        self.active_dialog = ActiveDialog::DiscardHunkConfirm;
        cx.notify();
    }

    pub fn confirm_discard_hunk(&mut self, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::None;
        let event = match self.pending_discard_hunk.take() {
            Some(e) => e,
            None => return,
        };
        // Use execute_stage_selection to bypass the confirmation gate.
        self.execute_stage_selection(event, cx);
    }

    pub fn cancel_discard_hunk(&mut self, cx: &mut Context<Self>) {
        self.pending_discard_hunk = None;
        self.active_dialog = ActiveDialog::None;
        cx.notify();
    }

    // === Stash Apply Confirm ===

    pub fn open_stash_apply_confirm(&mut self, reference: String, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::StashApplyConfirm { reference };
        cx.notify();
    }

    pub fn confirm_apply_stash(&mut self, cx: &mut Context<Self>) {
        let reference = match &self.active_dialog {
            ActiveDialog::StashApplyConfirm { reference } => reference.clone(),
            _ => return,
        };
        self.active_dialog = ActiveDialog::Stash;
        self.apply_stash_entry(reference, cx);
    }

    pub fn cancel_stash_apply(&mut self, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::Stash;
        cx.notify();
    }

    // === Stash Pop Confirm ===

    pub fn open_stash_pop_confirm(&mut self, reference: String, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::StashPopConfirm { reference };
        cx.notify();
    }

    pub fn confirm_pop_stash(&mut self, cx: &mut Context<Self>) {
        let reference = match &self.active_dialog {
            ActiveDialog::StashPopConfirm { reference } => reference.clone(),
            _ => return,
        };
        self.active_dialog = ActiveDialog::Stash;
        self.pop_stash_entry(reference, cx);
    }

    pub fn cancel_stash_pop(&mut self, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::Stash;
        cx.notify();
    }

    // === Stash Drop Confirm ===

    pub fn open_stash_drop_confirm(&mut self, reference: String, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::StashDropConfirm { reference };
        cx.notify();
    }

    pub fn confirm_drop_stash(&mut self, cx: &mut Context<Self>) {
        let reference = match &self.active_dialog {
            ActiveDialog::StashDropConfirm { reference } => reference.clone(),
            _ => return,
        };
        self.active_dialog = ActiveDialog::Stash;
        self.drop_stash_entry(reference, cx);
    }

    pub fn cancel_stash_drop(&mut self, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::Stash;
        cx.notify();
    }

    pub fn open_discard_confirm(
        &mut self,
        path: std::path::PathBuf,
        change_type: crate::git::ChangeType,
        cx: &mut Context<Self>,
    ) {
        self.active_dialog = ActiveDialog::DiscardFileConfirm { path, change_type };
        cx.notify();
    }

    pub fn confirm_discard_file(&mut self, cx: &mut Context<Self>) {
        let ActiveDialog::DiscardFileConfirm {
            ref path,
            change_type,
        } = self.active_dialog
        else {
            return;
        };
        let path = path.clone();
        self.active_dialog = ActiveDialog::None;
        self.discard_file(path, change_type, cx);
    }

    pub fn close_discard_confirm(&mut self, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::None;
        cx.notify();
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

        let repo = self.session_manager.active_git_repo().cloned();

        self.active_dialog = ActiveDialog::Deleting;
        cx.spawn(async move |entity, cx| {
            // ターミナルプロセスが完全に終了してからworktreeを削除するため、
            // git worktree removeもリトライ付きの非同期処理で実行する。
            // 同期実行するとWindowsでファイルハンドルが残りPermission deniedになる。
            // Non-fatal: git worktree prune will clean up orphaned entries.
            if let Some(r) = repo {
                if let Err(e) = Self::remove_worktree_async(r, worktree_name).await {
                    eprintln!("Warning: {}", e);
                }
            }
            // git worktree remove が失敗した場合もディレクトリを確実に削除する
            let result = Self::remove_worktree_directory_async(&worktree_path).await;
            let _ = entity.update(cx, |app, cx| {
                app.finish_delete_worktree(index, result, cx);
            });
        })
        .detach();
    }

    /// Async version of git worktree remove with retries.
    ///
    /// ターミナルプロセスの終了を待ちながらリトライすることで、
    /// Windowsのファイルロックによる Permission denied を回避する。
    async fn remove_worktree_async(repo: GitRepo, name: String) -> Result<(), String> {
        const MAX_RETRIES: u32 = 30;
        const RETRY_DELAY_MS: u64 = 200;

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                smol::Timer::after(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
            }

            let repo_clone = repo.clone();
            let name_clone = name.clone();
            let result = smol::unblock(move || repo_clone.remove_worktree(&name_clone)).await;

            match result {
                Ok(_) => return Ok(()),
                Err(e) if attempt == MAX_RETRIES - 1 => {
                    return Err(format!("git worktree remove failed: {}", e));
                }
                Err(_) => continue,
            }
        }

        Err(format!(
            "git worktree remove failed: still in use after retries"
        ))
    }

    /// Async version of directory removal with retries.
    pub(crate) async fn remove_worktree_directory_async(path: &Path) -> Result<(), String> {
        const MAX_RETRIES: u32 = 30;
        const RETRY_DELAY_MS: u64 = 200;

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
        // /exitせずに削除した場合も含め、セッションの全ターミナルをシャットダウンする。
        // active terminalだけでなくverify terminalなど全て対象にすることで
        // ファイルハンドルを確実に解放してディレクトリ削除を可能にする。
        for terminal in self.session_manager.get_session_all_terminals(index) {
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

    // === Template settings ===

    pub fn open_template_settings(
        &mut self,
        group_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let template = self
            .session_manager
            .groups()
            .get(group_index)
            .map(|g| TemplateConfig::load(&g.git_repo))
            .unwrap_or_default();
        self.settings_group_index = group_index;
        let texts = [
            template.pre_create_commands.join("\n"),
            template.file_copies.join("\n"),
            template.file_syncs.join("\n"),
            template.post_create_commands.join("\n"),
            template.working_directory.clone().unwrap_or_default(),
        ];
        for (i, text) in texts.into_iter().enumerate() {
            self.settings_inputs[i].update(cx, |input, _| input.set_text(text));
        }
        self.template_edit = Some(template);
        self.settings_active_section = 0;
        self.active_dialog = ActiveDialog::TemplateSettings;
        cx.notify();
        // Focus on the next frame so track_focus has registered the handle
        // in the dispatch tree during the render pass
        cx.on_next_frame(window, |this, window, cx| {
            window.focus(&this.settings_dialog_focus, cx);
            cx.notify();
        });
    }

    pub fn close_template_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.template_edit = None;
        for input in &self.settings_inputs {
            input.update(cx, |i, _| i.clear());
        }
        self.active_dialog = ActiveDialog::None;
        if let Some(terminal) = self.active_terminal() {
            let focus = terminal.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    pub fn save_template_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let parse_lines = |s: &str| -> Vec<String> {
            s.lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        };

        let texts: Vec<String> = self
            .settings_inputs
            .iter()
            .map(|e| e.read(cx).text().to_string())
            .collect();

        if let Some(ref mut template) = self.template_edit {
            template.pre_create_commands = parse_lines(&texts[0]);
            template.file_copies = parse_lines(&texts[1]);
            template.file_syncs = parse_lines(&texts[2]);
            template.post_create_commands = parse_lines(&texts[3]);
            let workdir = texts[4].trim().to_string();
            template.working_directory = if workdir.is_empty() {
                None
            } else {
                Some(workdir)
            };

            if let Some(group) = self.session_manager.groups().get(self.settings_group_index) {
                if let Err(e) = template.save(&group.git_repo) {
                    self.active_dialog = ActiveDialog::Error {
                        message: format!("Failed to save settings: {}", e),
                    };
                    self.template_edit = None;
                    cx.notify();
                    return;
                }
            }
        }

        self.apply_template_working_directory_defaults();

        self.template_edit = None;
        for input in &self.settings_inputs {
            input.update(cx, |i, _| i.clear());
        }
        self.active_dialog = ActiveDialog::None;
        if let Some(terminal) = self.active_terminal() {
            let focus = terminal.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    // === Close group confirm ===

    pub fn open_close_group_dialog(&mut self, group_index: usize, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::CloseGroupConfirm { group_index };
        cx.notify();
    }

    pub fn close_close_group_dialog(&mut self, cx: &mut Context<Self>) {
        self.active_dialog = ActiveDialog::None;
        cx.notify();
    }

    pub fn confirm_close_group(&mut self, cx: &mut Context<Self>) {
        let ActiveDialog::CloseGroupConfirm { group_index } = self.active_dialog else {
            self.close_close_group_dialog(cx);
            return;
        };
        self.active_dialog = ActiveDialog::None;
        self.close_group(group_index, cx);
    }

    // === Open folder ===

    pub fn on_open_folder(
        &mut self,
        _: &super::OpenFolder,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_menu = None;

        let paths_receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });

        cx.spawn(async move |entity, cx| {
            if let Ok(Ok(Some(paths))) = paths_receiver.await {
                if let Some(path) = paths.into_iter().next() {
                    let _ = entity.update(cx, |app, cx| {
                        app.open_project(path, cx);
                    });
                }
            }
        })
        .detach();
    }
}
