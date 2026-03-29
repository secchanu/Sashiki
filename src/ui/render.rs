//! Render trait implementation for SashikiApp

use crate::app::{MenuId, ResizeDrag, SashikiApp};
use crate::dialog::ActiveDialog;
use crate::session::LayoutMode;
use crate::theme::*;
use gpui::{
    App, Context, FocusHandle, Focusable, IntoElement, KeyDownEvent, MouseButton, Render, Styled,
    Window, div, prelude::*, px, rgb,
};

impl Focusable for SashikiApp {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SashikiApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout_mode = self.session_manager.layout_mode();

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
            .on_action(cx.listener(Self::on_toggle_sub_terminal))
            .on_action(cx.listener(Self::on_open_commit_dialog))
            .on_action(cx.listener(Self::on_open_stash_dialog))
            .on_action(cx.listener(Self::on_close_active_group))
            .child(self.render_header(cx))
            .child(self.render_main_content(layout_mode, cx))
            .when(self.open_menu.is_some(), |this| {
                this.child(self.render_menu_overlay(cx))
            })
            .when(self.commit_dropdown_open, |this| {
                this.child(self.render_commit_dropdown_overlay(cx))
            })
            .when(matches!(self.active_dialog, ActiveDialog::Commit), |this| {
                this.child(self.render_commit_dialog(cx))
            })
            .when(matches!(self.active_dialog, ActiveDialog::Stash), |this| {
                this.child(self.render_stash_dialog(cx))
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
            .when_some(
                match &self.active_dialog {
                    ActiveDialog::DiscardFileConfirm { path, change_type } => {
                        Some((path.clone(), *change_type))
                    }
                    _ => None,
                },
                |this, (path, change_type)| {
                    this.child(self.render_discard_confirm_dialog(&path, change_type, cx))
                },
            )
            .when(
                matches!(self.active_dialog, ActiveDialog::DiscardAllConfirm),
                |this| this.child(self.render_discard_all_confirm_dialog(cx)),
            )
            .when(
                matches!(self.active_dialog, ActiveDialog::SmartCommitConfirm),
                |this| this.child(self.render_smart_commit_confirm_dialog(cx)),
            )
            .when(
                matches!(self.active_dialog, ActiveDialog::UndoCommitConfirm),
                |this| this.child(self.render_undo_commit_confirm_dialog(cx)),
            )
            .when(
                matches!(self.active_dialog, ActiveDialog::AmendCommitConfirm),
                |this| this.child(self.render_amend_commit_confirm_dialog(cx)),
            )
            .when(
                matches!(self.active_dialog, ActiveDialog::DiscardHunkConfirm),
                |this| this.child(self.render_discard_hunk_confirm_dialog(cx)),
            )
            .when_some(
                match &self.active_dialog {
                    ActiveDialog::StashApplyConfirm { reference } => Some(reference.as_str()),
                    _ => None,
                },
                |this, r| this.child(self.render_stash_apply_confirm_dialog(r, cx)),
            )
            .when_some(
                match &self.active_dialog {
                    ActiveDialog::StashPopConfirm { reference } => Some(reference.as_str()),
                    _ => None,
                },
                |this, r| this.child(self.render_stash_pop_confirm_dialog(r, cx)),
            )
            .when_some(
                match &self.active_dialog {
                    ActiveDialog::StashDropConfirm { reference } => Some(reference.as_str()),
                    _ => None,
                },
                |this, r| this.child(self.render_stash_drop_confirm_dialog(r, cx)),
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
                    ActiveDialog::CloseGroupConfirm { group_index } => Some(*group_index),
                    _ => None,
                },
                |this, idx| this.child(self.render_close_group_dialog(idx, cx)),
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
    fn render_header(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .h_8()
            .px_2()
            .flex()
            .items_center()
            .bg(rgb(BG_SURFACE0))
            .text_color(rgb(TEXT))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(self.render_menu_button("Sashiki", MenuId::App, cx))
                    .child(self.render_menu_button("File", MenuId::File, cx))
                    .child(self.render_menu_button("View", MenuId::View, cx)),
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
            .min_h(px(24.0))
            .rounded_sm()
            .cursor_pointer()
            .bg(if is_open {
                rgb(BG_SURFACE2)
            } else {
                rgb(BG_SURFACE0)
            })
            .hover(|this| this.bg(rgb(BG_SURFACE2)))
            .text_sm()
            .flex()
            .items_center()
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.open_menu == Some(menu_id) {
                    this.open_menu = None;
                } else {
                    this.open_menu = Some(menu_id);
                    this.menu_focused_item = 0;
                    window.focus(&this.menu_focus, cx);
                }
                cx.notify();
            }))
            .on_mouse_move(cx.listener(move |this, _, window, cx| {
                if this.open_menu.is_some() && this.open_menu != Some(menu_id) {
                    this.open_menu = Some(menu_id);
                    this.menu_focused_item = 0;
                    window.focus(&this.menu_focus, cx);
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
                    .child(self.render_menu_item_indexed(
                        "Template Settings...",
                        None,
                        0,
                        cx,
                        |this, window, cx| {
                            this.open_menu = None;
                            let gi = this.session_manager.active_group_index();
                            this.open_template_settings(gi, window, cx);
                        },
                    ))
                    .child(Self::render_menu_separator())
                    .child(self.render_menu_item_indexed(
                        "Quit",
                        Some("Alt+F4"),
                        1,
                        cx,
                        |this, _, cx| {
                            this.open_menu = None;
                            cx.quit();
                        },
                    ));
            }
            MenuId::File => {
                dropdown = dropdown
                    .child(self.render_menu_item_indexed(
                        "Open Folder...",
                        Some("Ctrl+O"),
                        0,
                        cx,
                        |this, _, cx| {
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
                        },
                    ))
                    .child(self.render_menu_item_indexed(
                        "Commit...",
                        Some("Ctrl+Shift+M"),
                        1,
                        cx,
                        |this, window, cx| {
                            this.open_menu = None;
                            this.open_commit_dialog(window, cx);
                        },
                    ))
                    .child(self.render_menu_item_indexed(
                        "Stash...",
                        Some("Ctrl+Shift+H"),
                        2,
                        cx,
                        |this, window, cx| {
                            this.open_menu = None;
                            this.open_stash_dialog(window, cx);
                        },
                    ));
            }
            MenuId::View => {
                dropdown = dropdown
                    .child(self.render_menu_item_indexed(
                        "Toggle Sidebar",
                        Some("Ctrl+B"),
                        0,
                        cx,
                        |this, _, cx| {
                            this.open_menu = None;
                            this.show_sidebar = !this.show_sidebar;
                            cx.notify();
                        },
                    ))
                    .child(self.render_menu_item_indexed(
                        "Toggle File List",
                        Some("Ctrl+E"),
                        1,
                        cx,
                        |this, _, cx| {
                            this.open_menu = None;
                            this.show_file_list = !this.show_file_list;
                            cx.notify();
                        },
                    ))
                    .child(self.render_menu_item_indexed(
                        "Toggle Parallel",
                        Some("Ctrl+P"),
                        2,
                        cx,
                        |this, _, cx| {
                            this.open_menu = None;
                            this.session_manager.toggle_layout_mode();
                            cx.notify();
                        },
                    ))
                    .child(self.render_menu_item_indexed(
                        "Toggle Sub Terminal",
                        Some("Ctrl+T"),
                        3,
                        cx,
                        |this, _, cx| {
                            this.open_menu = None;
                            let show = !this.session_manager.active_show_sub_terminal();
                            this.session_manager.set_active_show_sub_terminal(show);
                            if show {
                                this.session_manager
                                    .ensure_active_session_terminal_count(2, cx);
                            }
                            cx.notify();
                        },
                    ))
                    .child(Self::render_menu_separator())
                    .child(self.render_menu_item_indexed(
                        "Refresh All",
                        Some("Ctrl+R"),
                        4,
                        cx,
                        |this, _, cx| {
                            this.open_menu = None;
                            this.refresh_worktrees(cx);
                            this.refresh_file_list_async(cx);
                            cx.notify();
                        },
                    ));
            }
        }

        dropdown
    }

    fn render_menu_item_indexed(
        &self,
        label: &str,
        shortcut: Option<&str>,
        item_index: usize,
        cx: &Context<Self>,
        handler: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        let label_owned = label.to_string();
        let shortcut_owned = shortcut.map(|s| s.to_string());
        let is_focused = self.menu_focused_item == item_index;

        div()
            .id(label_owned.clone())
            .w_full()
            .px_3()
            .min_h(px(28.0))
            .flex()
            .items_center()
            .justify_between()
            .cursor_pointer()
            .when(is_focused, |el| el.bg(rgb(BG_SURFACE1)))
            .hover(|this| this.bg(rgb(BG_SURFACE1)))
            .text_sm()
            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                if this.menu_focused_item != item_index {
                    this.menu_focused_item = item_index;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                handler(this, window, cx);
            }))
            .child(div().text_color(rgb(TEXT)).child(label_owned))
            .when_some(shortcut_owned, |this, sc| {
                this.child(div().ml_4().text_color(rgb(TEXT_MUTED)).text_xs().child(sc))
            })
    }

    fn execute_menu_item(
        &mut self,
        menu_id: MenuId,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_menu = None;
        self.menu_focused_item = 0;

        match menu_id {
            MenuId::App => match index {
                0 => {
                    let gi = self.session_manager.active_group_index();
                    self.open_template_settings(gi, window, cx);
                }
                1 => cx.quit(),
                _ => {}
            },
            MenuId::File => match index {
                0 => {
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
                }
                1 => self.open_commit_dialog(window, cx),
                2 => self.open_stash_dialog(window, cx),
                _ => {}
            },
            MenuId::View => match index {
                0 => {
                    self.show_sidebar = !self.show_sidebar;
                    cx.notify();
                }
                1 => {
                    self.show_file_list = !self.show_file_list;
                    cx.notify();
                }
                2 => {
                    self.session_manager.toggle_layout_mode();
                    cx.notify();
                }
                3 => {
                    let show = !self.session_manager.active_show_sub_terminal();
                    self.session_manager.set_active_show_sub_terminal(show);
                    if show {
                        self.session_manager
                            .ensure_active_session_terminal_count(2, cx);
                    }
                    cx.notify();
                }
                4 => {
                    self.refresh_worktrees(cx);
                    self.refresh_file_list_async(cx);
                    cx.notify();
                }
                _ => {}
            },
        }
    }

    fn render_menu_separator() -> impl IntoElement {
        div().my_1().mx_2().h_px().bg(rgb(BG_SURFACE1))
    }

    fn menu_item_count(menu_id: MenuId) -> usize {
        match menu_id {
            MenuId::App => 2,  // Template Settings, Quit (separator not counted)
            MenuId::File => 3, // Open Folder, Commit, Stash
            MenuId::View => 5, // Toggle Sidebar/FileList/Parallel/VerifyTerminal, Refresh All
        }
    }

    /// Full-screen overlay with backdrop + positioned dropdown.
    fn render_menu_overlay(&self, cx: &Context<Self>) -> impl IntoElement {
        let menu_id = self.open_menu.unwrap();
        let left_px = match menu_id {
            MenuId::App => gpui::px(8.),
            MenuId::File => gpui::px(78.),
            MenuId::View => gpui::px(118.),
        };

        div()
            .id("menu-overlay")
            .track_focus(&self.menu_focus)
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke.key;
                let Some(menu_id) = this.open_menu else {
                    return;
                };
                let count = Self::menu_item_count(menu_id);

                match key.as_str() {
                    "escape" => {
                        this.open_menu = None;
                        this.menu_focused_item = 0;
                        cx.notify();
                    }
                    "down" => {
                        this.menu_focused_item =
                            (this.menu_focused_item + 1).min(count.saturating_sub(1));
                        cx.notify();
                    }
                    "up" => {
                        this.menu_focused_item = this.menu_focused_item.saturating_sub(1);
                        cx.notify();
                    }
                    "right" => {
                        let next = match menu_id {
                            MenuId::App => MenuId::File,
                            MenuId::File => MenuId::View,
                            MenuId::View => MenuId::App,
                        };
                        this.open_menu = Some(next);
                        this.menu_focused_item = 0;
                        cx.notify();
                    }
                    "left" => {
                        let prev = match menu_id {
                            MenuId::App => MenuId::View,
                            MenuId::File => MenuId::App,
                            MenuId::View => MenuId::File,
                        };
                        this.open_menu = Some(prev);
                        this.menu_focused_item = 0;
                        cx.notify();
                    }
                    "enter" => {
                        this.execute_menu_item(menu_id, this.menu_focused_item, window, cx);
                    }
                    _ => {}
                }
            }))
            .child(
                div()
                    .id("menu-backdrop")
                    .absolute()
                    .inset_0()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.open_menu = None;
                            this.menu_focused_item = 0;
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

    fn render_main_content(
        &mut self,
        layout_mode: LayoutMode,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id("main-content")
            .flex_1()
            .flex()
            .flex_row()
            .overflow_hidden()
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if this.resize_drag.is_some() {
                    this.handle_resize_drag_move(
                        f32::from(event.position.x),
                        f32::from(event.position.y),
                    );
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.resize_drag.is_some() {
                        this.handle_resize_drag_end();
                        cx.notify();
                    }
                }),
            )
            .when(self.show_sidebar, |this| {
                this.child(self.render_sidebar(cx))
                    .child(self.render_resize_handle_v(
                        ResizeDrag::Sidebar {
                            start_x: 0.0,
                            initial_width: self.sidebar_width,
                        },
                        cx,
                    ))
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
                                    .h(px(self.file_view_height))
                                    .min_h(px(100.0))
                                    .flex_shrink_0()
                                    .child(self.file_view.clone()),
                            )
                            .child(self.render_resize_handle_h(cx))
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
                |this| {
                    this.child(self.render_resize_handle_v(
                        ResizeDrag::FileList {
                            start_x: 0.0,
                            initial_width: self.file_list_width,
                        },
                        cx,
                    ))
                    .child(self.render_file_list(cx))
                },
            )
    }

    pub(crate) fn render_resize_handle_v(
        &self,
        drag_variant: ResizeDrag,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let initial = drag_variant;
        div()
            .id(match initial {
                ResizeDrag::Sidebar { .. } => "resize-sidebar",
                ResizeDrag::FileList { .. } => "resize-filelist",
                ResizeDrag::TerminalSplit { .. } => "resize-terminal-split",
                _ => "resize-v",
            })
            .h_full()
            .w(px(12.0))
            .flex_shrink_0()
            .cursor_col_resize()
            .hover(|el| el.bg(rgb(BLUE)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    let x = f32::from(event.position.x);
                    this.resize_drag = Some(match initial {
                        ResizeDrag::Sidebar { .. } => ResizeDrag::Sidebar {
                            start_x: x,
                            initial_width: this.sidebar_width,
                        },
                        ResizeDrag::FileList { .. } => ResizeDrag::FileList {
                            start_x: x,
                            initial_width: this.file_list_width,
                        },
                        ResizeDrag::TerminalSplit { .. } => ResizeDrag::TerminalSplit {
                            start_x: x,
                            initial_ratio: this.terminal_split_ratio,
                        },
                        other => other,
                    });
                    cx.notify();
                }),
            )
    }

    fn render_resize_handle_h(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .id("resize-fileview-terminal")
            .w_full()
            .h(px(12.0))
            .flex_shrink_0()
            .cursor_row_resize()
            .hover(|el| el.bg(rgb(BLUE)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                    this.resize_drag = Some(ResizeDrag::FileViewTerminal {
                        start_y: f32::from(event.position.y),
                        initial_height: this.file_view_height,
                    });
                    cx.notify();
                }),
            )
    }

    fn handle_resize_drag_move(&mut self, current_x: f32, current_y: f32) {
        let drag = match self.resize_drag {
            Some(d) => d,
            None => return,
        };
        match drag {
            ResizeDrag::Sidebar {
                start_x,
                initial_width,
            } => {
                let new_width = (initial_width + (current_x - start_x)).clamp(120.0, 500.0);
                self.sidebar_width = new_width;
            }
            ResizeDrag::FileViewTerminal {
                start_y,
                initial_height,
            } => {
                let new_height = (initial_height + (current_y - start_y)).clamp(100.0, 800.0);
                self.file_view_height = new_height;
            }
            ResizeDrag::TerminalSplit {
                start_x,
                initial_ratio,
            } => {
                let container_width = if initial_ratio > 0.0 {
                    (start_x - 0.0) / initial_ratio
                } else {
                    1.0
                };
                if container_width > 0.0 {
                    let ratio_delta = (current_x - start_x) / container_width;
                    self.terminal_split_ratio = (initial_ratio + ratio_delta).clamp(0.2, 0.8);
                }
            }
            ResizeDrag::FileList {
                start_x,
                initial_width,
            } => {
                let new_width = (initial_width - (current_x - start_x)).clamp(120.0, 500.0);
                self.file_list_width = new_width;
            }
        }
    }

    fn handle_resize_drag_end(&mut self) {
        self.resize_drag = None;
    }
}
