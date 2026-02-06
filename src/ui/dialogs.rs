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
        let active_section = self.settings_active_section;
        let inputs: Vec<String> = self.settings_inputs.iter().cloned().collect();
        let cursors = self.settings_cursors;

        div()
            .id("template-settings-container")
            .track_focus(&self.settings_dialog_focus)
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke.key;
                let sec = this.settings_active_section;

                if key == "escape" {
                    this.close_template_settings(window, cx);
                } else if event.keystroke.modifiers.control && key == "s" {
                    this.save_template_settings(window, cx);
                } else if key == "tab" {
                    if event.keystroke.modifiers.shift {
                        this.settings_active_section = if sec == 0 { 3 } else { sec - 1 };
                    } else {
                        this.settings_active_section = (sec + 1) % 4;
                    }
                    cx.notify();
                } else if key == "enter" {
                    let cursor = this.settings_cursors[sec];
                    let byte_pos = char_to_byte_offset(&this.settings_inputs[sec], cursor);
                    this.settings_inputs[sec].insert(byte_pos, '\n');
                    this.settings_cursors[sec] = cursor + 1;
                    cx.notify();
                } else if key == "backspace" {
                    let cursor = this.settings_cursors[sec];
                    if cursor > 0 {
                        let byte_pos =
                            char_to_byte_offset(&this.settings_inputs[sec], cursor - 1);
                        this.settings_inputs[sec].remove(byte_pos);
                        this.settings_cursors[sec] = cursor - 1;
                    }
                    cx.notify();
                } else if key == "delete" {
                    let cursor = this.settings_cursors[sec];
                    let char_count = this.settings_inputs[sec].chars().count();
                    if cursor < char_count {
                        let byte_pos =
                            char_to_byte_offset(&this.settings_inputs[sec], cursor);
                        this.settings_inputs[sec].remove(byte_pos);
                    }
                    cx.notify();
                } else if key == "left" {
                    this.settings_cursors[sec] =
                        this.settings_cursors[sec].saturating_sub(1);
                    cx.notify();
                } else if key == "right" {
                    let char_count = this.settings_inputs[sec].chars().count();
                    let cursor = this.settings_cursors[sec];
                    this.settings_cursors[sec] = (cursor + 1).min(char_count);
                    cx.notify();
                } else if key == "up" {
                    let cursor = this.settings_cursors[sec];
                    let text = &this.settings_inputs[sec];
                    let (line, col) = cursor_to_line_col(text, cursor);
                    if line > 0 {
                        this.settings_cursors[sec] =
                            line_col_to_cursor(text, line - 1, col);
                    }
                    cx.notify();
                } else if key == "down" {
                    let cursor = this.settings_cursors[sec];
                    let text = &this.settings_inputs[sec];
                    let (line, col) = cursor_to_line_col(text, cursor);
                    let new_cursor = line_col_to_cursor(text, line + 1, col);
                    this.settings_cursors[sec] = new_cursor;
                    cx.notify();
                } else if key == "home" {
                    let cursor = this.settings_cursors[sec];
                    let text = &this.settings_inputs[sec];
                    let (line, _) = cursor_to_line_col(text, cursor);
                    this.settings_cursors[sec] = line_col_to_cursor(text, line, 0);
                    cx.notify();
                } else if key == "end" {
                    let cursor = this.settings_cursors[sec];
                    let text = &this.settings_inputs[sec];
                    let (line, _) = cursor_to_line_col(text, cursor);
                    this.settings_cursors[sec] =
                        line_col_to_cursor(text, line, usize::MAX);
                    cx.notify();
                } else if key == "space" {
                    let cursor = this.settings_cursors[sec];
                    let byte_pos = char_to_byte_offset(&this.settings_inputs[sec], cursor);
                    this.settings_inputs[sec].insert(byte_pos, ' ');
                    this.settings_cursors[sec] = cursor + 1;
                    cx.notify();
                } else if let Some(c) = key.chars().next()
                    && key.chars().count() == 1
                {
                    let cursor = this.settings_cursors[sec];
                    let byte_pos = char_to_byte_offset(&this.settings_inputs[sec], cursor);
                    this.settings_inputs[sec].insert(byte_pos, c);
                    this.settings_cursors[sec] = cursor + 1;
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
                        cx.listener(|this, _, window, cx| {
                            this.close_template_settings(window, cx);
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
                                    .gap_3()
                                    .child(Self::render_textarea_section(
                                        "Pre-create Commands",
                                        "e.g. git pull --ff-only",
                                        &inputs[0],
                                        cursors[0],
                                        0,
                                        active_section,
                                        true,
                                        cx,
                                    ))
                                    .child(Self::render_textarea_section(
                                        "Files to Copy (glob)",
                                        "e.g. .env",
                                        &inputs[1],
                                        cursors[1],
                                        1,
                                        active_section,
                                        true,
                                        cx,
                                    ))
                                    .child(Self::render_textarea_section(
                                        "Post-create Commands",
                                        "e.g. npm install",
                                        &inputs[2],
                                        cursors[2],
                                        2,
                                        active_section,
                                        true,
                                        cx,
                                    ))
                                    .child(Self::render_textarea_section(
                                        "Working Directory",
                                        "e.g. packages/frontend",
                                        &inputs[3],
                                        cursors[3],
                                        3,
                                        active_section,
                                        false,
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
                                            .child("Tab: switch section / Ctrl+S: save"),
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
                                                        |this, _, window, cx| {
                                                            this.close_template_settings(window, cx);
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
                                                        |this, _, window, cx| {
                                                            this.save_template_settings(window, cx);
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

    fn render_textarea_section(
        title: &str,
        placeholder: &str,
        content: &str,
        cursor: usize,
        section_index: usize,
        active_section: usize,
        multiline: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let is_active = section_index == active_section;
        let title = title.to_string();
        let is_empty = content.is_empty();
        let sec = section_index;
        let cursor = cursor.min(content.chars().count());

        let min_height = if multiline {
            gpui::px(72.)
        } else {
            gpui::px(26.)
        };

        let mut textarea = div()
            .id(("textarea-section", section_index))
            .w_full()
            .min_h(min_height)
            .px_2()
            .py_1()
            .bg(rgb(BG_SURFACE0))
            .border_1()
            .border_color(if is_active {
                rgb(BLUE)
            } else {
                rgb(BG_SURFACE1)
            })
            .rounded_sm()
            .cursor_text()
            .flex()
            .flex_col()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.settings_active_section = sec;
                cx.notify();
            }));

        if is_empty {
            if is_active {
                textarea = textarea.child(
                    div()
                        .text_xs()
                        .text_color(rgb(TEXT_MUTED))
                        .child(format!("|{}", placeholder)),
                );
            } else {
                textarea = textarea.child(
                    div()
                        .text_xs()
                        .text_color(rgb(TEXT_MUTED))
                        .child(placeholder.to_string()),
                );
            }
        } else {
            let lines: Vec<&str> = content.split('\n').collect();
            let (cursor_line, cursor_col) = cursor_to_line_col(content, cursor);

            for (line_idx, line) in lines.iter().enumerate() {
                let display = if is_active && line_idx == cursor_line {
                    let col = cursor_col.min(line.chars().count());
                    let byte_pos = line
                        .char_indices()
                        .nth(col)
                        .map(|(i, _)| i)
                        .unwrap_or(line.len());
                    let (before, after) = line.split_at(byte_pos);
                    format!("{}|{}", before, after)
                } else if line.is_empty() {
                    " ".to_string()
                } else {
                    line.to_string()
                };

                textarea = textarea.child(
                    div().text_xs().text_color(rgb(TEXT)).child(display),
                );
            }
        }

        div()
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
            )
            .child(textarea)
    }
}

/// Get (line, col) from a char-based cursor position in text.
fn cursor_to_line_col(text: &str, cursor: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    for (i, c) in text.chars().enumerate() {
        if i == cursor {
            return (line, col);
        }
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Get char-based cursor position from (line, col).
/// Clamps col to the end of the target line if it exceeds the line length.
fn line_col_to_cursor(text: &str, target_line: usize, target_col: usize) -> usize {
    let mut line = 0;
    let mut col = 0;
    for (i, c) in text.chars().enumerate() {
        if line == target_line && col == target_col {
            return i;
        }
        if c == '\n' {
            if line == target_line {
                return i;
            }
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    if line == target_line {
        text.chars().count()
    } else {
        // target_line is beyond last line, clamp to end of text
        text.chars().count()
    }
}

/// Convert a char offset to a byte offset in a string.
fn char_to_byte_offset(text: &str, char_offset: usize) -> usize {
    text.char_indices()
        .nth(char_offset)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}
