//! Dialog operation methods

use super::SashikiApp;
use crate::dialog::ActiveDialog;
use crate::git::{GitRepo, validate_branch_name};
use crate::template::{self, TemplateConfig};
use gpui::{Context, Focusable, PathPromptOptions, Window};
use std::path::{Path, PathBuf};

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

    pub fn submit_create_worktree(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let branch = self.create_branch_input.trim().to_string();

        if let Err(msg) = validate_branch_name(&branch) {
            self.active_dialog = ActiveDialog::Error {
                message: msg.to_string(),
            };
            cx.notify();
            return;
        }

        let repo = match self.git_repo.as_ref() {
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
        let template = TemplateConfig::load(repo);
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
        self.create_branch_input.clear();

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

    /// Async creation pipeline: pre-create -> worktree -> file copy -> post-create
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

        // --- Phase 4: Post-create commands ---
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
            if let Err(e) = repo.remove_worktree(&worktree_name) {
                eprintln!("Warning: git worktree remove failed: {}", e);
            }
        }

        self.active_dialog = ActiveDialog::Deleting;
        cx.spawn(async move |entity, cx| {
            let result = Self::remove_worktree_directory_async(&worktree_path).await;
            let _ = entity.update(cx, |app, cx| {
                app.finish_delete_worktree(index, result, cx);
            });
        })
        .detach();
    }

    /// Async version of directory removal with retries.
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

    // === Template settings ===

    pub fn open_template_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let template = self
            .git_repo
            .as_ref()
            .map(TemplateConfig::load)
            .unwrap_or_default();
        self.settings_inputs = [
            template.pre_create_commands.join("\n"),
            template.file_copies.join("\n"),
            template.post_create_commands.join("\n"),
            template.working_directory.clone().unwrap_or_default(),
        ];
        self.settings_cursors = [
            self.settings_inputs[0].chars().count(),
            self.settings_inputs[1].chars().count(),
            self.settings_inputs[2].chars().count(),
            self.settings_inputs[3].chars().count(),
        ];
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
        self.settings_inputs = Default::default();
        self.settings_cursors = Default::default();
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

        if let Some(ref mut template) = self.template_edit {
            template.pre_create_commands = parse_lines(&self.settings_inputs[0]);
            template.file_copies = parse_lines(&self.settings_inputs[1]);
            template.post_create_commands = parse_lines(&self.settings_inputs[2]);
            let workdir = self.settings_inputs[3].trim().to_string();
            template.working_directory = if workdir.is_empty() {
                None
            } else {
                Some(workdir)
            };

            if let Some(ref repo) = self.git_repo {
                if let Err(e) = template.save(repo) {
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
        self.settings_inputs = Default::default();
        self.settings_cursors = Default::default();
        self.active_dialog = ActiveDialog::None;
        if let Some(terminal) = self.active_terminal() {
            let focus = terminal.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
        cx.notify();
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
