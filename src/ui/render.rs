//! Render trait implementation for SashikiApp

use crate::app::{MenuId, SashikiApp};
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
            .on_action(cx.listener(Self::on_open_folder))
            .on_action(cx.listener(Self::on_toggle_verify_terminal))
            .child(self.render_header(layout_mode, session_count, running_session_count, cx))
            .child(self.render_main_content(layout_mode, cx))
            .when(self.open_menu.is_some(), |this| {
                this.child(self.render_menu_overlay(cx))
            })
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
                    ActiveDialog::Creating {
                        branch,
                        steps,
                        current_step,
                    } => Some((branch.as_str(), steps.as_slice(), *current_step)),
                    _ => None,
                },
                |this, (branch, steps, current_step)| {
                    this.child(self.render_creating_dialog(branch, steps, current_step))
                },
            )
            .when(
                matches!(self.active_dialog, ActiveDialog::TemplateSettings),
                |this| this.child(self.render_template_settings_dialog(cx)),
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
            .px_2()
            .flex()
            .items_center()
            .justify_between()
            .bg(rgb(BG_SURFACE0))
            .text_color(rgb(TEXT))
            .child(
                // Left: global menu bar
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(self.render_menu_button("Sashiki", MenuId::App, cx))
                    .child(self.render_menu_button("File", MenuId::File, cx))
                    .child(self.render_menu_button("View", MenuId::View, cx)),
            )
            .child(
                // Center: toolbar (session status)
                div()
                    .flex()
                    .items_center()
                    .gap_2()
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
    }

    // === Menu bar ===

    fn render_menu_button(
        &self,
        label: &str,
        menu_id: MenuId,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let is_open = self.open_menu == Some(menu_id);
        let label_owned = label.to_string();

        div()
            .id(label_owned.clone())
            .px_2()
            .py_1()
            .rounded_sm()
            .cursor_pointer()
            .bg(if is_open {
                rgb(BG_SURFACE2)
            } else {
                rgb(BG_SURFACE0)
            })
            .hover(|this| this.bg(rgb(BG_SURFACE2)))
            .text_xs()
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.open_menu == Some(menu_id) {
                    this.open_menu = None;
                } else {
                    this.open_menu = Some(menu_id);
                }
                cx.notify();
            }))
            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                if this.open_menu.is_some() && this.open_menu != Some(menu_id) {
                    this.open_menu = Some(menu_id);
                    cx.notify();
                }
            }))
            .child(label_owned)
    }

    fn render_menu_dropdown(&self, menu_id: MenuId, cx: &Context<Self>) -> impl IntoElement {
        let mut dropdown = div()
            .id(("menu-dropdown", menu_id as u32))
            .occlude()
            .min_w_48()
            .bg(rgb(BG_BASE))
            .border_1()
            .border_color(rgb(BG_SURFACE1))
            .rounded_sm()
            .shadow_lg()
            .py_1();

        match menu_id {
            MenuId::App => {
                dropdown = dropdown
                    .child(Self::render_menu_item("Template Settings...", None, cx, |this, window, cx| {
                        this.open_menu = None;
                        this.open_template_settings(window, cx);
                    }))
                    .child(Self::render_menu_separator())
                    .child(Self::render_menu_item("Quit", Some("Alt+F4"), cx, |this, _, cx| {
                        this.open_menu = None;
                        cx.quit();
                    }));
            }
            MenuId::File => {
                dropdown = dropdown
                    .child(Self::render_menu_item("Open Folder...", Some("Ctrl+O"), cx, |this, _, cx| {
                        this.open_menu = None;
                        cx.notify();
                        let paths_receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
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
                    }));
            }
            MenuId::View => {
                dropdown = dropdown
                    .child(Self::render_menu_item("Toggle Sidebar", Some("Ctrl+B"), cx, |this, _, cx| {
                        this.open_menu = None;
                        this.show_sidebar = !this.show_sidebar;
                        cx.notify();
                    }))
                    .child(Self::render_menu_item("Toggle File List", Some("Ctrl+E"), cx, |this, _, cx| {
                        this.open_menu = None;
                        this.show_file_list = !this.show_file_list;
                        cx.notify();
                    }))
                    .child(Self::render_menu_item("Toggle Parallel", Some("Ctrl+P"), cx, |this, _, cx| {
                        this.open_menu = None;
                        this.session_manager.toggle_layout_mode();
                        cx.notify();
                    }))
                    .child(Self::render_menu_item("Toggle Verify Terminal", Some("Ctrl+T"), cx, |this, _, cx| {
                        this.open_menu = None;
                        this.show_verify_terminal = !this.show_verify_terminal;
                        if this.show_verify_terminal {
                            this.session_manager.ensure_active_session_terminal_count(2, cx);
                        }
                        cx.notify();
                    }))
                    .child(Self::render_menu_separator())
                    .child(Self::render_menu_item("Refresh All", Some("Ctrl+R"), cx, |this, _, cx| {
                        this.open_menu = None;
                        this.refresh_worktrees(cx);
                        this.refresh_file_list_async(cx);
                        cx.notify();
                    }));
            }
        }

        dropdown
    }

    fn render_menu_item(
        label: &str,
        shortcut: Option<&str>,
        cx: &Context<Self>,
        handler: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        let label_owned = label.to_string();
        let shortcut_owned = shortcut.map(|s| s.to_string());

        div()
            .id(label_owned.clone())
            .w_full()
            .px_3()
            .py_1()
            .flex()
            .items_center()
            .justify_between()
            .cursor_pointer()
            .hover(|this| this.bg(rgb(BG_SURFACE1)))
            .text_xs()
            .on_click(cx.listener(move |this, _, window, cx| {
                handler(this, window, cx);
            }))
            .child(div().text_color(rgb(TEXT)).child(label_owned))
            .when_some(shortcut_owned, |this, sc| {
                this.child(
                    div()
                        .ml_4()
                        .text_color(rgb(TEXT_MUTED))
                        .child(sc),
                )
            })
    }

    fn render_menu_separator() -> impl IntoElement {
        div()
            .my_1()
            .mx_2()
            .h_px()
            .bg(rgb(BG_SURFACE1))
    }

    /// Full-screen overlay with backdrop + positioned dropdown.
    fn render_menu_overlay(&self, cx: &Context<Self>) -> impl IntoElement {
        let menu_id = self.open_menu.unwrap();
        // Approximate horizontal offset for each menu button
        let left_px = match menu_id {
            MenuId::App => gpui::px(8.),
            MenuId::File => gpui::px(68.),
            MenuId::View => gpui::px(108.),
        };

        div()
            .id("menu-overlay")
            .absolute()
            .inset_0()
            .child(
                div()
                    .id("menu-backdrop")
                    .absolute()
                    .inset_0()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.open_menu = None;
                            cx.notify();
                        }),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .top(gpui::px(32.))
                    .left(left_px)
                    .child(self.render_menu_dropdown(menu_id, cx)),
            )
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
                            .flex()
                            .flex_col()
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
