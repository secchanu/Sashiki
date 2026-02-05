//! Dialog rendering

use crate::app::SashikiApp;
use crate::theme::*;
use gpui::{
    AnyElement, Context, IntoElement, KeyDownEvent, ParentElement, Styled, div, prelude::*, rgb,
    rgba,
};

impl SashikiApp {
    pub fn render_create_dialog(&self, cx: &Context<Self>) -> AnyElement {
        let input_value = self.create_branch_input.clone();

        div()
            .id("create-dialog-container")
            .track_focus(&self.create_dialog_focus)
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke.key;
                if key == "escape" {
                    this.close_create_dialog(window, cx);
                } else if key == "enter" {
                    this.submit_create_worktree(window, cx);
                } else if key == "backspace" {
                    this.create_branch_input.pop();
                    cx.notify();
                } else if let Some(c) = key.chars().next()
                    && key.chars().count() == 1
                    && (c.is_alphanumeric() || matches!(c, '-' | '_' | '/' | '.' | '@'))
                {
                    this.create_branch_input.push(c);
                    cx.notify();
                }
            }))
            .child(
                div()
                    .id("create-dialog-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.close_create_dialog(window, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("create-dialog")
                            .occlude()
                            .w_80()
                            .bg(rgb(BG_BASE))
                            .border_1()
                            .border_color(rgb(BG_SURFACE1))
                            .rounded_md()
                            .shadow_lg()
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .text_color(rgb(TEXT))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("Create Worktree"),
                            )
                            .child(
                                div()
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .child(
                                        div()
                                            .text_color(rgb(TEXT_SECONDARY))
                                            .text_sm()
                                            .child("Enter branch name:"),
                                    )
                                    .child(
                                        div()
                                            .id("branch-input")
                                            .w_full()
                                            .px_3()
                                            .py_2()
                                            .bg(rgb(BG_SURFACE0))
                                            .border_1()
                                            .border_color(rgb(BLUE))
                                            .rounded_sm()
                                            .cursor_text()
                                            .text_color(if input_value.is_empty() {
                                                rgb(TEXT_MUTED)
                                            } else {
                                                rgb(TEXT)
                                            })
                                            .text_sm()
                                            .child(if input_value.is_empty() {
                                                "feature/my-branch".to_string()
                                            } else {
                                                format!("{}_", input_value)
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_color(rgb(TEXT_MUTED))
                                            .text_xs()
                                            .child("If the branch doesn't exist, it will be created from HEAD."),
                                    ),
                            )
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_t_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .flex()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id("cancel-create")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.close_create_dialog(window, cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("submit-create")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(GREEN))
                                            .hover(|el| el.bg(rgb(TEAL)))
                                            .text_xs()
                                            .text_color(rgb(BG_BASE))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.submit_create_worktree(window, cx);
                                            }))
                                            .child("Create"),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub fn render_delete_dialog(&self, target_index: usize, cx: &Context<Self>) -> AnyElement {
        let target_name = self
            .session_manager
            .sessions()
            .get(target_index)
            .map(|s| s.name().to_string())
            .unwrap_or_default();

        div()
            .id("delete-confirm-container")
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let key = &event.keystroke.key;
                if key == "escape" {
                    this.close_delete_dialog(cx);
                } else if key == "enter" {
                    this.confirm_delete_worktree(cx);
                }
            }))
            .child(
                div()
                    .id("delete-confirm-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.close_delete_dialog(cx);
                        }),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("delete-confirm-dialog")
                            .occlude()
                            .w_96()
                            .bg(rgb(BG_BASE))
                            .border_1()
                            .border_color(rgb(RED))
                            .rounded_md()
                            .shadow_lg()
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .text_color(rgb(RED))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("Delete Worktree"),
                            )
                            .child(
                                div()
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .child(div().text_color(rgb(TEXT)).text_sm().child(format!(
                                        "Are you sure you want to delete \"{}\"?",
                                        target_name
                                    )))
                                    .child(div().text_color(rgb(YELLOW)).text_xs().child(
                                        "This will remove the worktree directory and its contents.",
                                    )),
                            )
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_t_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .flex()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id("cancel-delete")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.close_delete_dialog(cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("confirm-delete")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(RED))
                                            .hover(|el| el.bg(rgb(MAROON)))
                                            .text_xs()
                                            .text_color(rgb(BG_BASE))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.confirm_delete_worktree(cx);
                                            }))
                                            .child("Delete"),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub fn render_error_dialog(&self, message: &str, cx: &Context<Self>) -> AnyElement {
        let message = message.to_string();

        div()
            .id("error-dialog-container")
            .absolute()
            .inset_0()
            .child(
                div()
                    .id("error-dialog-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.close_error_dialog(cx);
                        }),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("error-dialog")
                            .occlude()
                            .w_96()
                            .bg(rgb(BG_BASE))
                            .border_1()
                            .border_color(rgb(RED))
                            .rounded_md()
                            .shadow_lg()
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .text_color(rgb(RED))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("Error"),
                            )
                            .child(div().p_4().text_color(rgb(TEXT)).text_sm().child(message))
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_t_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .flex()
                                    .justify_end()
                                    .child(
                                        div()
                                            .id("dismiss-error")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.close_error_dialog(cx);
                                            }))
                                            .child("OK"),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub fn render_deleting_dialog(&self) -> AnyElement {
        div()
            .id("deleting-dialog-container")
            .absolute()
            .inset_0()
            .child(
                div()
                    .id("deleting-dialog-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY)),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("deleting-dialog")
                            .occlude()
                            .w_64()
                            .bg(rgb(BG_BASE))
                            .border_1()
                            .border_color(rgb(YELLOW))
                            .rounded_md()
                            .shadow_lg()
                            .p_4()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .text_color(rgb(YELLOW))
                                    .text_sm()
                                    .child("Deleting worktree..."),
                            )
                            .child(
                                div()
                                    .text_color(rgb(TEXT_MUTED))
                                    .text_xs()
                                    .child("Please wait"),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub fn render_creating_dialog(
        &self,
        branch: &str,
        steps: &[String],
        current_step: usize,
    ) -> AnyElement {
        let branch = branch.to_string();

        let mut body = div().p_4().flex().flex_col().gap_2();

        for (i, step) in steps.iter().enumerate() {
            let (icon, color) = if i < current_step {
                // Completed
                ("OK ", GREEN)
            } else if i == current_step {
                // Running
                (">> ", YELLOW)
            } else {
                // Pending
                ("   ", TEXT_MUTED)
            };

            body = body.child(
                div()
                    .flex()
                    .gap_2()
                    .text_xs()
                    .child(div().text_color(rgb(color)).child(icon))
                    .child(div().text_color(rgb(color)).child(step.clone())),
            );
        }

        div()
            .id("creating-dialog-container")
            .absolute()
            .inset_0()
            .child(
                div()
                    .id("creating-dialog-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY)),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("creating-dialog")
                            .occlude()
                            .w_80()
                            .bg(rgb(BG_BASE))
                            .border_1()
                            .border_color(rgb(GREEN))
                            .rounded_md()
                            .shadow_lg()
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .text_color(rgb(GREEN))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_sm()
                                    .child(format!("Creating \"{}\"", branch)),
                            )
                            .child(body),
                    ),
            )
            .into_any_element()
    }

    pub fn render_template_settings_dialog(&self, cx: &Context<Self>) -> AnyElement {
        let template = self.template_edit.clone().unwrap_or_default();
        let input_value = self.settings_input.clone();
        let active_section = self.settings_active_section;

        div()
            .id("template-settings-container")
            .track_focus(&self.settings_dialog_focus)
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let key = &event.keystroke.key;
                if key == "escape" {
                    this.close_template_settings(cx);
                } else if key == "enter" {
                    this.add_template_item(cx);
                } else if key == "tab" {
                    this.settings_active_section =
                        (this.settings_active_section + 1) % 4;
                    this.settings_input.clear();
                    cx.notify();
                } else if key == "backspace" {
                    this.settings_input.pop();
                    cx.notify();
                } else if let Some(c) = key.chars().next()
                    && key.chars().count() == 1
                {
                    this.settings_input.push(c);
                    cx.notify();
                }
            }))
            .child(
                div()
                    .id("template-settings-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.close_template_settings(cx);
                        }),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .id("template-settings-dialog")
                            .occlude()
                            .w_96()
                            .bg(rgb(BG_BASE))
                            .border_1()
                            .border_color(rgb(BLUE))
                            .rounded_md()
                            .shadow_lg()
                            // Header
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .text_color(rgb(BLUE))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("Session Template"),
                            )
                            // Body
                            .child(
                                div()
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_4()
                                    .child(Self::render_template_section(
                                        "Pre-create Commands",
                                        &template.pre_create_commands,
                                        "e.g. git pull --ff-only",
                                        0,
                                        active_section,
                                        &input_value,
                                        cx,
                                    ))
                                    .child(Self::render_template_section(
                                        "Files to Copy (glob)",
                                        &template.file_copies,
                                        "e.g. .env",
                                        1,
                                        active_section,
                                        &input_value,
                                        cx,
                                    ))
                                    .child(Self::render_template_section(
                                        "Post-create Commands",
                                        &template.post_create_commands,
                                        "e.g. npm install",
                                        2,
                                        active_section,
                                        &input_value,
                                        cx,
                                    ))
                                    .child(Self::render_workdir_section(
                                        &template.working_directory,
                                        3,
                                        active_section,
                                        &input_value,
                                        cx,
                                    )),
                            )
                            // Footer
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_t_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .flex()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_color(rgb(TEXT_MUTED))
                                            .text_xs()
                                            .child("Tab: switch section"),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .id("cancel-settings")
                                                    .px_4()
                                                    .py_2()
                                                    .cursor_pointer()
                                                    .rounded_sm()
                                                    .bg(rgb(BG_SURFACE1))
                                                    .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                                    .text_xs()
                                                    .text_color(rgb(TEXT))
                                                    .on_click(cx.listener(
                                                        |this, _, _, cx| {
                                                            this.close_template_settings(cx);
                                                        },
                                                    ))
                                                    .child("Cancel"),
                                            )
                                            .child(
                                                div()
                                                    .id("save-settings")
                                                    .px_4()
                                                    .py_2()
                                                    .cursor_pointer()
                                                    .rounded_sm()
                                                    .bg(rgb(GREEN))
                                                    .hover(|el| el.bg(rgb(TEAL)))
                                                    .text_xs()
                                                    .text_color(rgb(BG_BASE))
                                                    .on_click(cx.listener(
                                                        |this, _, _, cx| {
                                                            this.save_template_settings(cx);
                                                        },
                                                    ))
                                                    .child("Save"),
                                            ),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_template_section(
        title: &str,
        items: &[String],
        placeholder: &str,
        section_index: usize,
        active_section: usize,
        input_value: &str,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let is_active = section_index == active_section;
        let title = title.to_string();
        let placeholder = placeholder.to_string();
        let input_value = input_value.to_string();

        let mut section = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_color(if is_active {
                        rgb(BLUE)
                    } else {
                        rgb(TEXT_SECONDARY)
                    })
                    .text_xs()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(title),
            );

        // Existing items
        for (i, item) in items.iter().enumerate() {
            let item_text = item.clone();
            let remove_idx = i;
            let sec = section_index;
            section = section.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .bg(rgb(BG_SURFACE0))
                    .rounded_sm()
                    .child(
                        div()
                            .text_color(rgb(TEXT))
                            .text_xs()
                            .child(item_text),
                    )
                    .child(
                        div()
                            .id(("remove-item", section_index * 100 + i))
                            .px_1()
                            .cursor_pointer()
                            .text_color(rgb(TEXT_MUTED))
                            .hover(|el| el.text_color(rgb(RED)))
                            .text_xs()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove_template_item(sec, remove_idx, cx);
                            }))
                            .child("x"),
                    ),
            );
        }

        // Input field (only shown for active section)
        if is_active {
            section = section.child(
                div()
                    .flex()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .px_2()
                            .py_1()
                            .bg(rgb(BG_SURFACE0))
                            .border_1()
                            .border_color(rgb(BLUE))
                            .rounded_sm()
                            .text_xs()
                            .text_color(if input_value.is_empty() {
                                rgb(TEXT_MUTED)
                            } else {
                                rgb(TEXT)
                            })
                            .child(if input_value.is_empty() {
                                placeholder
                            } else {
                                input_value
                            }),
                    ),
            );
        }

        section
    }

    fn render_workdir_section(
        working_directory: &Option<String>,
        section_index: usize,
        active_section: usize,
        input_value: &str,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let is_active = section_index == active_section;
        let current_value = working_directory.clone().unwrap_or_default();
        let input_value = input_value.to_string();
        let input_is_empty = input_value.is_empty();

        let mut section = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_color(if is_active {
                        rgb(BLUE)
                    } else {
                        rgb(TEXT_SECONDARY)
                    })
                    .text_xs()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child("Working Directory"),
            );

        if !is_active {
            // Show current value
            if !current_value.is_empty() {
                section = section.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_2()
                        .py_1()
                        .bg(rgb(BG_SURFACE0))
                        .rounded_sm()
                        .child(div().text_color(rgb(TEXT)).text_xs().child(current_value.clone()))
                        .child(
                            div()
                                .id("clear-workdir")
                                .px_1()
                                .cursor_pointer()
                                .text_color(rgb(TEXT_MUTED))
                                .hover(|el| el.text_color(rgb(RED)))
                                .text_xs()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(ref mut config) = this.template_edit {
                                        config.working_directory = None;
                                    }
                                    cx.notify();
                                }))
                                .child("x"),
                        ),
                );
            } else {
                section = section.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_color(rgb(TEXT_MUTED))
                        .text_xs()
                        .child(". (worktree root)"),
                );
            }
        } else {
            // Editable input
            let display = if input_is_empty {
                if current_value.is_empty() {
                    "e.g. packages/frontend".to_string()
                } else {
                    current_value
                }
            } else {
                input_value
            };

            section = section.child(
                div()
                    .px_2()
                    .py_1()
                    .bg(rgb(BG_SURFACE0))
                    .border_1()
                    .border_color(rgb(BLUE))
                    .rounded_sm()
                    .text_xs()
                    .text_color(if input_is_empty {
                        rgb(TEXT_MUTED)
                    } else {
                        rgb(TEXT)
                    })
                    .child(display),
            );
        }

        section
    }
}
