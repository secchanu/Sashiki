//! Sidebar rendering for session list

use crate::app::SashikiApp;
use crate::session::{LayoutMode, SessionGroup};
use crate::theme::*;
use crate::ui::{render_locked_badge, render_main_badge};
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, prelude::*, px, rgb};

impl SashikiApp {
    pub fn render_sidebar(&self, cx: &Context<Self>) -> AnyElement {
        let groups = self.session_manager.groups();
        let active_group_index = self.session_manager.active_group_index();
        let layout_mode = self.session_manager.layout_mode();

        div()
            .w(px(self.sidebar_width))
            .h_full()
            .bg(rgb(BG_MANTLE))
            .flex()
            .flex_col()
            .child(self.render_sidebar_header(layout_mode, cx))
            .child(div().flex_1().overflow_hidden().flex().flex_col().children(
                groups.iter().enumerate().map(|(gi, group)| {
                    self.render_group_section(gi, group, active_group_index, layout_mode, cx)
                }),
            ))
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

    fn render_group_section(
        &self,
        group_index: usize,
        group: &SessionGroup,
        active_group_index: usize,
        layout_mode: LayoutMode,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let is_active_group = group_index == active_group_index;
        let is_expanded = group.is_expanded();
        let group_name = group.name().to_string();

        let sessions = if is_active_group {
            self.session_manager.sessions()
        } else {
            group.session_manager.sessions()
        };
        let active_session_index = if is_active_group {
            self.session_manager.active_index()
        } else {
            group.session_manager.active_index()
        };

        let header = {
            let group_name_clone = group_name.clone();
            div()
                .id(format!("group-header-{}", group_index))
                .h_7()
                .px_2()
                .flex()
                .items_center()
                .gap_1()
                .when(is_active_group, |el| el.bg(rgb(BG_SURFACE0)))
                .hover(|el| el.bg(rgb(BG_SURFACE1)))
                .child(
                    // 展開/折りたたみ矢印（グループ切り替えは発生しない独立クリック領域）
                    div()
                        .id(format!("group-expand-{}", group_index))
                        .text_xs()
                        .text_color(rgb(TEXT_MUTED))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _event: &gpui::ClickEvent, _, cx| {
                            this.session_manager.toggle_group_expanded(group_index);
                            cx.notify();
                        }))
                        .child(if is_expanded { "▼" } else { "▶" }),
                )
                .child(
                    // グループ名（クリックでグループ切り替え）
                    div()
                        .id(format!("group-name-{}", group_index))
                        .flex_1()
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(if is_active_group {
                            rgb(TEXT)
                        } else {
                            rgb(TEXT_MUTED)
                        })
                        .truncate()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.session_manager.switch_group(group_index);
                            this.activate_and_focus_session(window, cx);
                        }))
                        .child(group_name_clone),
                )
                // テンプレート設定アイコン
                .child(
                    div()
                        .id(format!("group-template-{}", group_index))
                        .px_1()
                        .cursor_pointer()
                        .text_xs()
                        .text_color(rgb(TEXT_MUTED))
                        .hover(|el| el.text_color(rgb(TEXT)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_template_settings(group_index, window, cx);
                        }))
                        .child("⚙"),
                )
                .child(
                    div()
                        .id(format!("group-close-{}", group_index))
                        .px_1()
                        .cursor_pointer()
                        .text_xs()
                        .text_color(rgb(TEXT_MUTED))
                        .hover(|el| el.text_color(rgb(RED)))
                        .on_click(cx.listener(move |this, _event: &gpui::ClickEvent, _, cx| {
                            this.open_close_group_dialog(group_index, cx);
                        }))
                        .child("×"),
                )
        };

        div()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(rgb(BG_SURFACE0))
            .child(header)
            .when(is_expanded, |el| {
                el.children(sessions.iter().enumerate().map(|(si, session)| {
                    self.render_session_item(
                        group_index,
                        si,
                        session,
                        is_active_group,
                        active_session_index,
                        layout_mode,
                        cx,
                    )
                }))
                .when(sessions.is_empty(), |el| {
                    el.child(
                        div()
                            .px_4()
                            .py_1()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .child("No sessions"),
                    )
                })
                .child(
                    div().px_2().py_1().child(
                        div()
                            .id(format!("group-create-worktree-{}", group_index))
                            .w_full()
                            .px_3()
                            .py_1()
                            .cursor_pointer()
                            .rounded_sm()
                            .bg(rgb(BG_SURFACE0))
                            .hover(|el| el.bg(rgb(BG_SURFACE1)))
                            .text_center()
                            .text_xs()
                            .text_color(rgb(GREEN))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.session_manager.switch_group(group_index);
                                this.open_create_dialog(window, cx);
                            }))
                            .child("+ New Worktree"),
                    ),
                )
            })
    }

    fn render_session_item(
        &self,
        group_index: usize,
        session_index: usize,
        session: &crate::session::Session,
        is_active_group: bool,
        active_session_index: usize,
        layout_mode: LayoutMode,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let name = session.name().to_string();
        let branch = session.branch().map(|s| s.to_string());
        let is_main = session.is_main();
        let is_locked = session.is_locked();
        let color = session.color().primary;
        let visible_in_parallel = session.is_visible_in_parallel();

        let is_selected = if !is_active_group {
            false
        } else {
            match layout_mode {
                LayoutMode::Single => session_index == active_session_index,
                LayoutMode::Parallel => visible_in_parallel,
            }
        };

        div()
            .id(format!("session-{}-{}", group_index, session_index))
            .pl(px(20.0))
            .pr_3()
            .py_2()
            .cursor_pointer()
            .when(is_selected, |el| el.bg(rgb(BG_SURFACE0)))
            .hover(|el| el.bg(rgb(BG_SURFACE1)))
            .on_click(cx.listener(move |this, _, window, cx| {
                // 別グループのセッションをクリックしたらグループを切り替えてからセッションを選択
                if !is_active_group {
                    this.session_manager.switch_group(group_index);
                }
                match this.session_manager.layout_mode() {
                    LayoutMode::Single => {
                        this.on_session_selected(session_index, window, cx);
                    }
                    LayoutMode::Parallel => {
                        this.on_toggle_parallel_visibility(session_index, cx);
                    }
                }
            }))
            .flex()
            .items_center()
            .gap_2()
            .when(
                layout_mode == LayoutMode::Parallel && is_active_group,
                |el| {
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
                },
            )
            .child(div().w_2().h_2().rounded_full().bg(rgb(color)))
            .child(self.render_session_name_section(name, branch, is_main, is_locked))
            .when(
                layout_mode == LayoutMode::Single && is_active_group && !is_main,
                |el| {
                    el.child(
                        div()
                            .id(format!("delete-{}-{}", group_index, session_index))
                            .px_1()
                            .cursor_pointer()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .hover(|el| el.text_color(rgb(RED)))
                            .on_click(cx.listener(move |this, _event: &gpui::ClickEvent, _, cx| {
                                this.open_delete_dialog(session_index, cx);
                            }))
                            .child("×"),
                    )
                },
            )
    }

    fn render_session_name_section(
        &self,
        name: String,
        branch: Option<String>,
        is_main: bool,
        is_locked: bool,
    ) -> impl IntoElement {
        // ブランチ名を主テキストとして使用。ブランチ名がない場合（detached HEAD等）はセッション名で代替
        let display_name = branch.unwrap_or(name);
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
                    .child(
                        div()
                            .text_color(rgb(TEXT))
                            .text_sm()
                            .truncate()
                            .child(display_name),
                    )
                    .when(is_main, |el| el.child(render_main_badge()))
                    .when(is_locked, |el| el.child(render_locked_badge())),
            )
    }

    fn render_create_button(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .border_t_1()
            .border_color(rgb(BG_SURFACE0))
            .px_3()
            .py_2()
            .child(
                div()
                    .id("open-project-btn")
                    .w_full()
                    .px_3()
                    .py_1()
                    .cursor_pointer()
                    .rounded_sm()
                    .hover(|el| el.bg(rgb(BG_SURFACE1)))
                    .text_center()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.on_open_folder(&crate::app::OpenFolder, window, cx);
                    }))
                    .child("+ Open Project"),
            )
    }
}
