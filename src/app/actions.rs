//! Action definitions and event handlers

use super::SashikiApp;
use gpui::{Context, Focusable, Window, actions};

actions!(
    sashiki,
    [
        ToggleParallelMode,
        NextSession,
        PrevSession,
        ToggleSidebar,
        ToggleFileList,
        RefreshAll,
        CreateWorktree,
        CloseFileView,
    ]
);

impl SashikiApp {
    pub fn on_toggle_parallel(
        &mut self,
        _: &ToggleParallelMode,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session_manager.toggle_layout_mode();
        cx.notify();
    }

    /// Start terminal for active session, focus it, and refresh file list
    pub fn activate_and_focus_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session_manager.ensure_active_session_terminal(cx);
        if let Some(terminal) = self.active_terminal() {
            let focus = terminal.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
        self.refresh_file_list_async(cx);
        cx.notify();
    }

    pub fn on_next_session(
        &mut self,
        _: &NextSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session_manager.next_session();
        self.activate_and_focus_session(window, cx);
    }

    pub fn on_prev_session(
        &mut self,
        _: &PrevSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session_manager.prev_session();
        self.activate_and_focus_session(window, cx);
    }

    pub fn on_toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        self.show_sidebar = !self.show_sidebar;
        cx.notify();
    }

    pub fn on_toggle_file_list(
        &mut self,
        _: &ToggleFileList,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_file_list = !self.show_file_list;
        cx.notify();
    }

    pub fn on_refresh_all(&mut self, _: &RefreshAll, _: &mut Window, cx: &mut Context<Self>) {
        self.refresh_worktrees(cx);
        self.refresh_file_list_async(cx);
        cx.notify();
    }

    pub fn on_session_selected(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session_manager.switch_to(index);
        self.activate_and_focus_session(window, cx);
    }

    pub fn on_toggle_parallel_visibility(&mut self, index: usize, cx: &mut Context<Self>) {
        let was_visible = self
            .session_manager
            .sessions()
            .get(index)
            .map(|s| s.is_visible_in_parallel())
            .unwrap_or(false);

        self.session_manager.toggle_parallel_visibility(index);

        if !was_visible {
            self.session_manager.ensure_session_terminal(index, cx);
        }
        cx.notify();
    }

    pub fn on_close_file_view(
        &mut self,
        _: &CloseFileView,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_file_view = false;
        self.file_view.update(cx, |view, _cx| {
            view.close();
        });
        cx.notify();
    }

    pub fn refresh_worktrees(&mut self, cx: &mut Context<Self>) {
        if let Some(ref repo) = self.git_repo
            && let Ok(worktrees) = repo.list_worktrees()
        {
            self.session_manager.sync_with_worktrees(worktrees);
            self.apply_template_working_directory_defaults();
        }
        cx.notify();
    }
}
