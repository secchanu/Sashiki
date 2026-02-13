//! Terminal panel rendering

use crate::app::SashikiApp;
use crate::session::{LayoutMode, SessionStatus};
use crate::theme::*;
use crate::ui::{render_locked_badge, render_main_badge};
use crate::app::ResizeDrag;
use gpui::{
    AnyElement, Context, DefiniteLength, IntoElement, ParentElement, Styled, div, prelude::*, rgb,
};

/// Properties for rendering a terminal header
struct TerminalHeaderProps {
    name: String,
    branch: Option<String>,
    color: u32,
    status: SessionStatus,
    is_main: bool,
    is_locked: bool,
    path_display: String,
    show_verify_button: bool,
}

impl SashikiApp {
    pub fn render_terminal_area(&self, cx: &Context<Self>) -> AnyElement {
        match self.session_manager.layout_mode() {
            LayoutMode::Single => self.render_single_mode(cx),
            LayoutMode::Parallel => self.render_parallel_mode(cx),
        }
    }

    fn render_single_mode(&self, cx: &Context<Self>) -> AnyElement {
        if self.session_manager.is_empty() {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(TEXT_MUTED))
                .child("No sessions available")
                .into_any_element();
        }

        let active_index = self.session_manager.active_index();

        if self.show_verify_terminal {
            let ratio = self.terminal_split_ratio;
            div()
                .flex_1()
                .flex()
                .flex_row()
                .overflow_hidden()
                .child(
                    div()
                        .w(DefiniteLength::Fraction(ratio))
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .child(self.render_terminal_panel(active_index, true, cx)),
                )
                .child(self.render_resize_handle_v(
                    ResizeDrag::TerminalSplit {
                        start_x: 0.0,
                        initial_ratio: self.terminal_split_ratio,
                    },
                    cx,
                ))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .child(self.render_verify_terminal_panel(active_index, cx)),
                )
                .into_any_element()
        } else {
            self.render_terminal_panel(active_index, true, cx)
        }
    }

    fn render_parallel_mode(&self, cx: &Context<Self>) -> AnyElement {
        let parallel_sessions = self.session_manager.parallel_sessions();
        if parallel_sessions.is_empty() {
            return self.render_single_mode(cx);
        }

        let active_index = self.session_manager.active_index();
        let count = parallel_sessions.len();

        let (rows, cols) = match count {
            1 => (1, 1),
            2 => (1, 2),
            3 | 4 => (2, 2),
            5 | 6 => (2, 3),
            _ => (3, 3),
        };

        let mut row_elements: Vec<AnyElement> = Vec::new();

        for row in 0..rows {
            let mut col_elements: Vec<AnyElement> = Vec::new();

            for col in 0..cols {
                let grid_index = row * cols + col;
                if grid_index < count {
                    let (session_index, _) = parallel_sessions[grid_index];
                    let is_focused = session_index == active_index;
                    col_elements.push(self.render_terminal_panel(session_index, is_focused, cx));
                } else {
                    col_elements.push(div().flex_1().into_any_element());
                }
            }

            row_elements.push(
                div()
                    .flex_1()
                    .flex()
                    .flex_row()
                    .children(col_elements)
                    .into_any_element(),
            );
        }

        div()
            .flex_1()
            .flex()
            .flex_col()
            .children(row_elements)
            .into_any_element()
    }

    pub fn render_terminal_panel(
        &self,
        session_index: usize,
        is_focused: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let sessions = self.session_manager.sessions();
        let session = &sessions[session_index];
        let color = session.color().primary;
        let name = session.name().to_string();
        let branch = session.branch().map(|s| s.to_string());
        let is_main = session.is_main();
        let is_locked = session.is_locked();
        let status = session.status();
        let path_display = session.worktree_path().to_string_lossy().to_string();
        let show_verify_button =
            is_focused && self.session_manager.layout_mode() == LayoutMode::Single;

        let terminal_content: AnyElement = if let Some(terminal) = session.active_terminal() {
            div()
                .flex_1()
                .w_full()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(terminal.clone())
                .into_any_element()
        } else {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(BG_BASE))
                .text_color(rgb(TEXT_MUTED))
                .child("Click to start terminal")
                .into_any_element()
        };

        div()
            .flex_1()
            .flex()
            .flex_col()
            .overflow_hidden()
            .border_2()
            .border_color(if is_focused {
                rgb(color)
            } else {
                rgb(BG_SURFACE0)
            })
            .rounded_md()
            .m_1()
            .child(self.render_terminal_header(
                TerminalHeaderProps {
                    name,
                    branch,
                    color,
                    status,
                    is_main,
                    is_locked,
                    path_display,
                    show_verify_button,
                },
                cx,
            ))
            .child(terminal_content)
            .into_any_element()
    }

    fn render_verify_terminal_panel(
        &self,
        session_index: usize,
        _cx: &Context<Self>,
    ) -> AnyElement {
        let sessions = self.session_manager.sessions();
        let session = &sessions[session_index];
        let color = session.color().primary;

        let terminal_content: AnyElement =
            if let Some(terminal) = session.get_terminal(1) {
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .child(terminal.clone())
                    .into_any_element()
            } else {
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(BG_BASE))
                    .text_color(rgb(TEXT_MUTED))
                    .child("Verify terminal not started")
                    .into_any_element()
            };

        div()
            .flex_1()
            .flex()
            .flex_col()
            .overflow_hidden()
            .border_2()
            .border_color(rgb(BG_SURFACE0))
            .rounded_md()
            .m_1()
            .child(
                div()
                    .h_8()
                    .px_3()
                    .flex()
                    .items_center()
                    .bg(rgb(BG_MANTLE))
                    .border_b_2()
                    .border_color(rgb(color))
                    .child(
                        div()
                            .text_color(rgb(color))
                            .text_sm()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("Verify"),
                    ),
            )
            .child(terminal_content)
            .into_any_element()
    }

    fn render_terminal_header(
        &self,
        props: TerminalHeaderProps,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let TerminalHeaderProps {
            name,
            branch,
            color,
            status,
            is_main,
            is_locked,
            path_display,
            show_verify_button,
        } = props;

        let verify_active = self.show_verify_terminal;

        div()
            .h_8()
            .px_3()
            .flex()
            .items_center()
            .justify_between()
            .bg(rgb(BG_MANTLE))
            .border_b_2()
            .border_color(rgb(color))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_color(match status {
                                SessionStatus::Focused => rgb(GREEN),
                                SessionStatus::Running => rgb(YELLOW),
                                SessionStatus::Stopped => rgb(TEXT_MUTED),
                            })
                            .text_sm()
                            .child(status.symbol()),
                    )
                    .child(div().w_2().h_2().rounded_full().bg(rgb(color)))
                    .child(
                        div()
                            .text_color(rgb(color))
                            .text_sm()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(name),
                    )
                    .when(is_main, |el| el.child(render_main_badge()))
                    .when(is_locked, |el| el.child(render_locked_badge())),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(show_verify_button, |el| {
                        el.child(
                            div()
                                .id("toggle-verify-btn")
                                .px_2()
                                .py_1()
                                .cursor_pointer()
                                .rounded_sm()
                                .bg(if verify_active {
                                    rgb(MAUVE)
                                } else {
                                    rgb(BG_SURFACE0)
                                })
                                .text_color(if verify_active {
                                    rgb(BG_BASE)
                                } else {
                                    rgb(TEXT_MUTED)
                                })
                                .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                .text_xs()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.show_verify_terminal = !this.show_verify_terminal;
                                    if this.show_verify_terminal {
                                        this.session_manager
                                            .ensure_active_session_terminal_count(2, cx);
                                    }
                                    cx.notify();
                                }))
                                .child("Verify"),
                        )
                    })
                    .when_some(branch, |el, branch_name| {
                        el.child(
                            div()
                                .text_color(rgb(TEXT_MUTED))
                                .text_xs()
                                .child(format!("⎇ {}", branch_name)),
                        )
                    })
                    .child(
                        div()
                            .text_color(rgb(BG_SURFACE1))
                            .text_xs()
                            .max_w_48()
                            .truncate()
                            .child(path_display),
                    ),
            )
    }
}
