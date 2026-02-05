//! Sidebar rendering for session list

use crate::app::SashikiApp;
use crate::session::{LayoutMode, SessionStatus};
use crate::theme::*;
use crate::ui::{render_locked_badge, render_main_badge};
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, prelude::*, rgb};

impl SashikiApp {
    pub fn render_sidebar(&self, cx: &Context<Self>) -> AnyElement {
        let sessions = self.session_manager.sessions();
        let active_index = self.session_manager.active_index();
        let layout_mode = self.session_manager.layout_mode();

        div()
            .w_56()
            .h_full()
            .bg(rgb(BG_MANTLE))
            .border_r_1()
            .border_color(rgb(BG_SURFACE0))
            .flex()
            .flex_col()
            .child(self.render_sidebar_header(layout_mode, cx))
            .child(self.render_session_list(sessions, active_index, layout_mode, cx))
            .when(sessions.is_empty(), |this: gpui::Div| {
                this.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(TEXT_MUTED))
                        .text_sm()
                        .child("No worktrees"),
                )
            })
            .child(self.render_create_button(cx))
            .into_any_element()
    }

    fn render_sidebar_header(
        &self,
        layout_mode: LayoutMode,
        _cx: &Context<Self>,
    ) -> impl IntoElement {
        div()
            .h_8()
            .px_3()
            .flex()
            .items_center()
            .justify_between()
            .bg(rgb(BG_BASE))
            .border_b_1()
            .border_color(rgb(BG_SURFACE0))
            .child(
                div()
                    .text_color(rgb(BLUE))
                    .text_sm()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(if layout_mode == LayoutMode::Parallel {
                        "Select Sessions"
                    } else {
                        "Sessions"
                    }),
            )
            .child(div().text_color(rgb(TEXT_MUTED)).text_xs().child(
                if layout_mode == LayoutMode::Parallel {
                    format!(
                        "{} selected",
                        self.session_manager.parallel_sessions().len()
                    )
                } else {
                    format!(
                        "{}/{}",
                        self.session_manager.running_session_count(),
                        self.session_manager.sessions().len()
                    )
                },
            ))
    }

    fn render_session_list(
        &self,
        sessions: &[crate::session::Session],
        active_index: usize,
        layout_mode: LayoutMode,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex_1()
            .overflow_hidden()
            .children(sessions.iter().enumerate().map(|(i, session)| {
                self.render_session_item(i, session, active_index, layout_mode, cx)
            }))
    }

    fn render_session_item(
        &self,
        i: usize,
        session: &crate::session::Session,
        active_index: usize,
        layout_mode: LayoutMode,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let name = session.name().to_string();
        let branch = session.branch().map(|s| s.to_string());
        let is_main = session.is_main();
        let is_locked = session.is_locked();
        let color = session.color().primary;
        let status = session.status();
        let visible_in_parallel = session.is_visible_in_parallel();

        let is_selected = match layout_mode {
            LayoutMode::Single => i == active_index,
            LayoutMode::Parallel => visible_in_parallel,
        };

        div()
            .id(format!("session-{}", i))
            .px_3()
            .py_2()
            .cursor_pointer()
            .when(is_selected, |el| el.bg(rgb(BG_SURFACE0)))
            .hover(|el| el.bg(rgb(BG_SURFACE1)))
            .on_click(cx.listener(move |this, _, window, cx| {
                match this.session_manager.layout_mode() {
                    LayoutMode::Single => {
                        this.on_session_selected(i, window, cx);
                    }
                    LayoutMode::Parallel => {
                        this.on_toggle_parallel_visibility(i, cx);
                    }
                }
            }))
            .flex()
            .items_center()
            .gap_2()
            .when(layout_mode == LayoutMode::Parallel, |el| {
                el.child(
                    div()
                        .w_4()
                        .text_center()
                        .text_xs()
                        .text_color(if visible_in_parallel {
                            rgb(BLUE)
                        } else {
                            rgb(TEXT_MUTED)
                        })
                        .child(if visible_in_parallel { "☑" } else { "☐" }),
                )
            })
            .when(layout_mode == LayoutMode::Single, |el| {
                el.child(
                    div()
                        .text_color(match status {
                            SessionStatus::Focused => rgb(GREEN),
                            SessionStatus::Running => rgb(YELLOW),
                            SessionStatus::Stopped => rgb(TEXT_MUTED),
                        })
                        .text_sm()
                        .child(status.symbol()),
                )
            })
            .child(div().w_2().h_2().rounded_full().bg(rgb(color)))
            .child(self.render_session_name_section(name, branch, is_main, is_locked))
            .when(layout_mode == LayoutMode::Single && !is_main, |el| {
                el.child(
                    div()
                        .id(format!("delete-{}", i))
                        .px_1()
                        .cursor_pointer()
                        .text_xs()
                        .text_color(rgb(TEXT_MUTED))
                        .hover(|el| el.text_color(rgb(RED)))
                        .on_click(cx.listener(move |this, _event: &gpui::ClickEvent, _, cx| {
                            this.open_delete_dialog(i, cx);
                        }))
                        .child("×"),
                )
            })
    }

    fn render_session_name_section(
        &self,
        name: String,
        branch: Option<String>,
        is_main: bool,
        is_locked: bool,
    ) -> impl IntoElement {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap_1()
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_color(rgb(TEXT)).text_sm().truncate().child(name))
                    .when(is_main, |el| el.child(render_main_badge()))
                    .when(is_locked, |el| el.child(render_locked_badge())),
            )
            .when_some(branch, |el, b| {
                el.child(
                    div()
                        .text_color(rgb(TEXT_MUTED))
                        .text_xs()
                        .truncate()
                        .child(format!("⎇ {}", b)),
                )
            })
    }

    fn render_create_button(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .border_t_1()
            .border_color(rgb(BG_SURFACE0))
            .px_3()
            .py_2()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .id("create-worktree-btn")
                    .w_full()
                    .px_3()
                    .py_2()
                    .cursor_pointer()
                    .rounded_sm()
                    .bg(rgb(BG_SURFACE0))
                    .hover(|el| el.bg(rgb(BG_SURFACE1)))
                    .text_center()
                    .text_xs()
                    .text_color(rgb(GREEN))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_create_dialog(window, cx);
                    }))
                    .child("+ Create Worktree"),
            )
            .child(
                div()
                    .id("template-settings-btn")
                    .w_full()
                    .px_3()
                    .py_1()
                    .cursor_pointer()
                    .rounded_sm()
                    .hover(|el| el.bg(rgb(BG_SURFACE1)))
                    .text_center()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open_template_settings(cx);
                    }))
                    .child("Template Settings"),
            )
    }
}
