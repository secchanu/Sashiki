//! Render trait implementation for SashikiApp

use crate::app::*;
use crate::dialog::ActiveDialog;
use crate::session::LayoutMode;
use crate::theme::*;
use gpui::{
    App, Context, FocusHandle, Focusable, IntoElement, Render, Styled, Window, div, prelude::*, rgb,
};

impl Focusable for SashikiApp {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SashikiApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout_mode = self.session_manager.layout_mode();
        let session_count = self.session_manager.len();
        let running_session_count = self.session_manager.running_session_count();

        div()
            .size_full()
            .bg(rgb(BG_BASE))
            .flex()
            .flex_col()
            .on_action(cx.listener(Self::on_toggle_parallel))
            .on_action(cx.listener(Self::on_next_session))
            .on_action(cx.listener(Self::on_prev_session))
            .on_action(cx.listener(Self::on_toggle_sidebar))
            .on_action(cx.listener(Self::on_toggle_file_list))
            .on_action(cx.listener(Self::on_refresh_all))
            .on_action(cx.listener(Self::on_close_file_view))
            .child(self.render_header(layout_mode, session_count, running_session_count, cx))
            .child(self.render_main_content(layout_mode, cx))
            .when(
                matches!(self.active_dialog, ActiveDialog::CreateWorktree),
                |this| this.child(self.render_create_dialog(cx)),
            )
            .when_some(
                match &self.active_dialog {
                    ActiveDialog::DeleteConfirm { target_index } => Some(*target_index),
                    _ => None,
                },
                |this, idx| this.child(self.render_delete_dialog(idx, cx)),
            )
            .when(
                matches!(self.active_dialog, ActiveDialog::Deleting),
                |this| this.child(self.render_deleting_dialog()),
            )
            .when_some(
                match &self.active_dialog {
                    ActiveDialog::Error { message } => Some(message.as_str()),
                    _ => None,
                },
                |this, msg| this.child(self.render_error_dialog(msg, cx)),
            )
    }
}

impl SashikiApp {
    fn render_header(
        &self,
        layout_mode: LayoutMode,
        session_count: usize,
        running_session_count: usize,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        div()
            .h_8()
            .px_4()
            .flex()
            .items_center()
            .justify_between()
            .bg(rgb(BG_SURFACE0))
            .text_color(rgb(TEXT))
            .child(
                div()
                    .id("toggle-sidebar")
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .bg(if self.show_sidebar {
                        rgb(BG_SURFACE1)
                    } else {
                        rgb(BG_SURFACE0)
                    })
                    .hover(|this| this.bg(rgb(BG_SURFACE2)))
                    .text_xs()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_sidebar = !this.show_sidebar;
                        cx.notify();
                    }))
                    .child("Sessions"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child("Sashiki")
                    .child(
                        div()
                            .id("toggle-parallel")
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .bg(if layout_mode == LayoutMode::Parallel {
                                rgb(BLUE)
                            } else {
                                rgb(BG_SURFACE0)
                            })
                            .text_color(if layout_mode == LayoutMode::Parallel {
                                rgb(BG_BASE)
                            } else {
                                rgb(TEXT)
                            })
                            .hover(|this| this.bg(rgb(BG_SURFACE2)))
                            .text_xs()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.session_manager.toggle_layout_mode();
                                cx.notify();
                            }))
                            .child(if layout_mode == LayoutMode::Parallel {
                                "Parallel"
                            } else {
                                "Single"
                            }),
                    )
                    .child(div().text_xs().text_color(rgb(TEXT_MUTED)).child(format!(
                        "{}/{} running",
                        running_session_count, session_count
                    ))),
            )
            .child(div().min_w_16().flex().justify_end().when(
                layout_mode == LayoutMode::Single,
                |el| {
                    el.child(
                        div()
                            .id("toggle-files")
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .bg(if self.show_file_list {
                                rgb(BG_SURFACE1)
                            } else {
                                rgb(BG_SURFACE0)
                            })
                            .hover(|this| this.bg(rgb(BG_SURFACE2)))
                            .text_xs()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_file_list = !this.show_file_list;
                                cx.notify();
                            }))
                            .child("Files"),
                    )
                },
            ))
    }

    fn render_main_content(&self, layout_mode: LayoutMode, cx: &Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .flex()
            .flex_row()
            .overflow_hidden()
            .when(self.show_sidebar, |this| {
                this.child(self.render_sidebar(cx))
            })
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .when(
                        self.show_file_view && layout_mode == LayoutMode::Single,
                        |this| {
                            this.child(
                                div()
                                    .h_96()
                                    .min_h_48()
                                    .border_b_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .child(self.file_view.clone()),
                            )
                        },
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .child(self.render_terminal_area(cx)),
                    ),
            )
            .when(
                self.show_file_list && layout_mode == LayoutMode::Single,
                |this| this.child(self.render_file_list(cx)),
            )
    }
}
