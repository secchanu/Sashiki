//! Dialog rendering

use crate::app::SashikiApp;
use crate::git::StashMode;
use crate::theme::*;
use gpui::{
    AnyElement, ClipboardItem, Context, HighlightStyle, Hsla, IntoElement, KeyDownEvent,
    ParentElement, Styled, StyledText, div, prelude::*, px, rgb, rgba,
};

impl SashikiApp {
    pub fn render_create_dialog(&self, cx: &Context<Self>) -> AnyElement {
        let input_value = self.create_branch_input.clone();
        let cursor = self.create_branch_cursor.min(input_value.chars().count());
        let selection_anchor = self.create_branch_selection_anchor;

        div()
            .id("create-dialog-container")
            .track_focus(&self.create_dialog_focus)
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke.key;
                let primary_mod =
                    event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
                if key == "escape" {
                    this.close_create_dialog(window, cx);
                } else if key == "enter" {
                    this.submit_create_worktree(window, cx);
                } else if primary_mod && key == "a" {
                    select_all_text(
                        &this.create_branch_input,
                        &mut this.create_branch_cursor,
                        &mut this.create_branch_selection_anchor,
                    );
                    cx.notify();
                } else if primary_mod && key == "c" {
                    copy_selection_to_clipboard(
                        &this.create_branch_input,
                        this.create_branch_cursor,
                        this.create_branch_selection_anchor,
                        cx,
                    );
                } else if primary_mod && key == "x" {
                    if cut_selection_to_clipboard(
                        &mut this.create_branch_input,
                        &mut this.create_branch_cursor,
                        &mut this.create_branch_selection_anchor,
                        cx,
                    ) {
                        cx.notify();
                    }
                } else if primary_mod && key == "v" {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        let normalized = normalize_single_line_text(&text);
                        replace_selection_with_text(
                            &mut this.create_branch_input,
                            &mut this.create_branch_cursor,
                            &mut this.create_branch_selection_anchor,
                            &normalized,
                        );
                        cx.notify();
                    }
                } else if key == "backspace" {
                    if backspace_text(
                        &mut this.create_branch_input,
                        &mut this.create_branch_cursor,
                        &mut this.create_branch_selection_anchor,
                    ) {
                        cx.notify();
                    }
                } else if key == "delete" {
                    if delete_text(
                        &mut this.create_branch_input,
                        &mut this.create_branch_cursor,
                        &mut this.create_branch_selection_anchor,
                    ) {
                        cx.notify();
                    }
                } else if key == "left" {
                    let current = this.create_branch_cursor;
                    move_text_cursor(
                        &mut this.create_branch_cursor,
                        &mut this.create_branch_selection_anchor,
                        current.saturating_sub(1),
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "right" {
                    let current = this.create_branch_cursor;
                    let next = (current + 1).min(this.create_branch_input.chars().count());
                    move_text_cursor(
                        &mut this.create_branch_cursor,
                        &mut this.create_branch_selection_anchor,
                        next,
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "home" {
                    move_text_cursor(
                        &mut this.create_branch_cursor,
                        &mut this.create_branch_selection_anchor,
                        0,
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "end" {
                    let end = this.create_branch_input.chars().count();
                    move_text_cursor(
                        &mut this.create_branch_cursor,
                        &mut this.create_branch_selection_anchor,
                        end,
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "space" {
                    replace_selection_with_text(
                        &mut this.create_branch_input,
                        &mut this.create_branch_cursor,
                        &mut this.create_branch_selection_anchor,
                        " ",
                    );
                    cx.notify();
                } else if let Some(c) = key.chars().next()
                    && key.chars().count() == 1
                    && !primary_mod
                    && !event.keystroke.modifiers.alt
                    && (c.is_alphanumeric() || matches!(c, '-' | '_' | '/' | '.' | '@'))
                {
                    replace_selection_with_text(
                        &mut this.create_branch_input,
                        &mut this.create_branch_cursor,
                        &mut this.create_branch_selection_anchor,
                        &c.to_string(),
                    );
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
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    window.focus(&this.create_dialog_focus, cx);
                                }),
                            )
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
                                                StyledText::new("feature/my-branch")
                                            } else {
                                                render_text_with_selection_and_caret(
                                                    &input_value,
                                                    cursor,
                                                    selection_anchor,
                                                )
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

    pub fn render_commit_dialog(&self, cx: &Context<Self>) -> AnyElement {
        let message = self.commit_message_input.clone();
        let cursor = self.commit_message_cursor.min(message.chars().count());
        let selection_anchor = self.commit_message_selection_anchor;
        let amend_mode = self.commit_amend_mode;

        div()
            .id("commit-dialog-container")
            .track_focus(&self.commit_dialog_focus)
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke.key;
                let primary_mod =
                    event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
                if key == "escape" {
                    this.close_commit_dialog(window, cx);
                } else if key == "enter" && !event.keystroke.modifiers.shift {
                    this.submit_commit(window, cx);
                } else if key == "enter" {
                    replace_selection_with_text(
                        &mut this.commit_message_input,
                        &mut this.commit_message_cursor,
                        &mut this.commit_message_selection_anchor,
                        "\n",
                    );
                    cx.notify();
                } else if primary_mod && key == "a" {
                    select_all_text(
                        &this.commit_message_input,
                        &mut this.commit_message_cursor,
                        &mut this.commit_message_selection_anchor,
                    );
                    cx.notify();
                } else if primary_mod && key == "c" {
                    copy_selection_to_clipboard(
                        &this.commit_message_input,
                        this.commit_message_cursor,
                        this.commit_message_selection_anchor,
                        cx,
                    );
                } else if primary_mod && key == "x" {
                    if cut_selection_to_clipboard(
                        &mut this.commit_message_input,
                        &mut this.commit_message_cursor,
                        &mut this.commit_message_selection_anchor,
                        cx,
                    ) {
                        cx.notify();
                    }
                } else if primary_mod && key == "v" {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        replace_selection_with_text(
                            &mut this.commit_message_input,
                            &mut this.commit_message_cursor,
                            &mut this.commit_message_selection_anchor,
                            &text,
                        );
                        cx.notify();
                    }
                } else if key == "backspace" {
                    if backspace_text(
                        &mut this.commit_message_input,
                        &mut this.commit_message_cursor,
                        &mut this.commit_message_selection_anchor,
                    ) {
                        cx.notify();
                    }
                } else if key == "delete" {
                    if delete_text(
                        &mut this.commit_message_input,
                        &mut this.commit_message_cursor,
                        &mut this.commit_message_selection_anchor,
                    ) {
                        cx.notify();
                    }
                } else if key == "left" {
                    let current = this.commit_message_cursor;
                    move_text_cursor(
                        &mut this.commit_message_cursor,
                        &mut this.commit_message_selection_anchor,
                        current.saturating_sub(1),
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "right" {
                    let current = this.commit_message_cursor;
                    let next = (current + 1).min(this.commit_message_input.chars().count());
                    move_text_cursor(
                        &mut this.commit_message_cursor,
                        &mut this.commit_message_selection_anchor,
                        next,
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "up" {
                    let current = this.commit_message_cursor;
                    let (line, col) = cursor_to_line_col(&this.commit_message_input, current);
                    let next = if line == 0 {
                        0
                    } else {
                        line_col_to_cursor(&this.commit_message_input, line - 1, col)
                    };
                    move_text_cursor(
                        &mut this.commit_message_cursor,
                        &mut this.commit_message_selection_anchor,
                        next,
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "down" {
                    let current = this.commit_message_cursor;
                    let (line, col) = cursor_to_line_col(&this.commit_message_input, current);
                    let next = line_col_to_cursor(&this.commit_message_input, line + 1, col);
                    move_text_cursor(
                        &mut this.commit_message_cursor,
                        &mut this.commit_message_selection_anchor,
                        next,
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "home" {
                    let current = this.commit_message_cursor;
                    let (line, _) = cursor_to_line_col(&this.commit_message_input, current);
                    let next = line_col_to_cursor(&this.commit_message_input, line, 0);
                    move_text_cursor(
                        &mut this.commit_message_cursor,
                        &mut this.commit_message_selection_anchor,
                        next,
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "end" {
                    let current = this.commit_message_cursor;
                    let (line, _) = cursor_to_line_col(&this.commit_message_input, current);
                    let next = line_col_to_cursor(&this.commit_message_input, line, usize::MAX);
                    move_text_cursor(
                        &mut this.commit_message_cursor,
                        &mut this.commit_message_selection_anchor,
                        next,
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "space" {
                    replace_selection_with_text(
                        &mut this.commit_message_input,
                        &mut this.commit_message_cursor,
                        &mut this.commit_message_selection_anchor,
                        " ",
                    );
                    cx.notify();
                } else if let Some(c) = key.chars().next()
                    && key.chars().count() == 1
                    && !primary_mod
                    && !event.keystroke.modifiers.alt
                {
                    replace_selection_with_text(
                        &mut this.commit_message_input,
                        &mut this.commit_message_cursor,
                        &mut this.commit_message_selection_anchor,
                        &c.to_string(),
                    );
                    cx.notify();
                }
            }))
            .child(
                div()
                    .id("commit-dialog-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.close_commit_dialog(window, cx);
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
                            .id("commit-dialog")
                            .occlude()
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    window.focus(&this.commit_dialog_focus, cx);
                                }),
                            )
                            .w_96()
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
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_color(rgb(GREEN))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .child(if amend_mode {
                                                "Amend Last Commit"
                                            } else {
                                                "Create Commit"
                                            }),
                                    )
                                    .child(
                                        // Amendモード切り替えボタン
                                        div()
                                            .id("commit-amend-toggle")
                                            .px_2()
                                            .py_1()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .when(amend_mode, |el| el.bg(rgb(BG_SURFACE2)))
                                            .when(!amend_mode, |el| el.bg(rgb(BG_SURFACE0)))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(if amend_mode {
                                                rgb(YELLOW)
                                            } else {
                                                rgb(TEXT_MUTED)
                                            })
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                if this.commit_amend_mode {
                                                    // amendモード解除: メッセージをクリア
                                                    this.commit_amend_mode = false;
                                                    this.commit_message_input.clear();
                                                    this.commit_message_cursor = 0;
                                                    this.commit_message_selection_anchor = None;
                                                    cx.notify();
                                                } else {
                                                    // amendモードへ: 最後のコミットメッセージをプリフィル
                                                    this.open_amend_dialog(window, cx);
                                                }
                                            }))
                                            .child("Amend"),
                                    ),
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
                                            .child("Commit message"),
                                    )
                                    .child(
                                        div()
                                            .id("commit-message-input")
                                            .w_full()
                                            .min_h(gpui::px(80.0))
                                            .px_3()
                                            .py_2()
                                            .bg(rgb(BG_SURFACE0))
                                            .border_1()
                                            .border_color(rgb(BLUE))
                                            .rounded_sm()
                                            .text_color(if message.is_empty() {
                                                rgb(TEXT_MUTED)
                                            } else {
                                                rgb(TEXT)
                                            })
                                            .text_sm()
                                            .child(if message.is_empty() {
                                                StyledText::new("feat: summarize reviewed changes")
                                            } else {
                                                render_text_with_selection_and_caret(
                                                    &message,
                                                    cursor,
                                                    selection_anchor,
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_color(rgb(TEXT_MUTED))
                                            .text_xs()
                                            .child("Enter to commit. Shift+Enter for newline."),
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
                                            .id("cancel-commit")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.close_commit_dialog(window, cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("submit-commit")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(GREEN))
                                            .hover(|el| el.bg(rgb(TEAL)))
                                            .text_xs()
                                            .text_color(rgb(BG_BASE))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.submit_commit(window, cx);
                                            }))
                                            .child("Commit"),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_stash_mode_btn(
        label: &str,
        is_active: bool,
        cx: &Context<Self>,
        handler: impl Fn(&mut Self, &mut gpui::Context<Self>) + 'static,
    ) -> impl IntoElement {
        let label = label.to_string();
        let id = format!("stash-mode-{}", label);
        div()
            .id(id)
            .px_2()
            .py_0p5()
            .cursor_pointer()
            .rounded_sm()
            .when(is_active, |el| el.bg(rgb(BLUE)).text_color(rgb(BG_BASE)))
            .when(!is_active, |el| {
                el.bg(rgb(BG_SURFACE1))
                    .text_color(rgb(TEXT_MUTED))
                    .hover(|b| b.bg(rgb(BG_SURFACE2)))
            })
            .text_xs()
            .on_click(cx.listener(move |this, _, _, cx| handler(this, cx)))
            .child(label)
    }

    pub fn render_stash_dialog(&self, cx: &Context<Self>) -> AnyElement {
        let message = self.stash_message_input.clone();
        let cursor = self.stash_message_cursor.min(message.chars().count());
        let selection_anchor = self.stash_message_selection_anchor;
        let entries = self.stash_entries.clone();

        div()
            .id("stash-dialog-container")
            .track_focus(&self.stash_dialog_focus)
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke.key;
                let primary_mod =
                    event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
                if key == "escape" {
                    this.close_stash_dialog(window, cx);
                } else if key == "enter" {
                    this.create_stash(cx);
                } else if primary_mod && key == "a" {
                    select_all_text(
                        &this.stash_message_input,
                        &mut this.stash_message_cursor,
                        &mut this.stash_message_selection_anchor,
                    );
                    cx.notify();
                } else if primary_mod && key == "c" {
                    copy_selection_to_clipboard(
                        &this.stash_message_input,
                        this.stash_message_cursor,
                        this.stash_message_selection_anchor,
                        cx,
                    );
                } else if primary_mod && key == "x" {
                    if cut_selection_to_clipboard(
                        &mut this.stash_message_input,
                        &mut this.stash_message_cursor,
                        &mut this.stash_message_selection_anchor,
                        cx,
                    ) {
                        cx.notify();
                    }
                } else if primary_mod && key == "v" {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        let normalized = normalize_single_line_text(&text);
                        replace_selection_with_text(
                            &mut this.stash_message_input,
                            &mut this.stash_message_cursor,
                            &mut this.stash_message_selection_anchor,
                            &normalized,
                        );
                        cx.notify();
                    }
                } else if key == "backspace" {
                    if backspace_text(
                        &mut this.stash_message_input,
                        &mut this.stash_message_cursor,
                        &mut this.stash_message_selection_anchor,
                    ) {
                        cx.notify();
                    }
                } else if key == "delete" {
                    if delete_text(
                        &mut this.stash_message_input,
                        &mut this.stash_message_cursor,
                        &mut this.stash_message_selection_anchor,
                    ) {
                        cx.notify();
                    }
                } else if key == "left" {
                    let current = this.stash_message_cursor;
                    move_text_cursor(
                        &mut this.stash_message_cursor,
                        &mut this.stash_message_selection_anchor,
                        current.saturating_sub(1),
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "right" {
                    let current = this.stash_message_cursor;
                    let next = (current + 1).min(this.stash_message_input.chars().count());
                    move_text_cursor(
                        &mut this.stash_message_cursor,
                        &mut this.stash_message_selection_anchor,
                        next,
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "home" {
                    move_text_cursor(
                        &mut this.stash_message_cursor,
                        &mut this.stash_message_selection_anchor,
                        0,
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "end" {
                    let end = this.stash_message_input.chars().count();
                    move_text_cursor(
                        &mut this.stash_message_cursor,
                        &mut this.stash_message_selection_anchor,
                        end,
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "space" {
                    replace_selection_with_text(
                        &mut this.stash_message_input,
                        &mut this.stash_message_cursor,
                        &mut this.stash_message_selection_anchor,
                        " ",
                    );
                    cx.notify();
                } else if let Some(c) = key.chars().next()
                    && key.chars().count() == 1
                    && !primary_mod
                    && !event.keystroke.modifiers.alt
                {
                    replace_selection_with_text(
                        &mut this.stash_message_input,
                        &mut this.stash_message_cursor,
                        &mut this.stash_message_selection_anchor,
                        &c.to_string(),
                    );
                    cx.notify();
                }
            }))
            .child(
                div()
                    .id("stash-dialog-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.close_stash_dialog(window, cx);
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
                            .id("stash-dialog")
                            .occlude()
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    window.focus(&this.stash_dialog_focus, cx);
                                }),
                            )
                            .w_96()
                            .bg(rgb(BG_BASE))
                            .border_1()
                            .border_color(rgb(BLUE))
                            .rounded_md()
                            .shadow_lg()
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .text_color(rgb(BLUE))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("Stash"),
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
                                            .text_xs()
                                            .child("Create stash (optional message)"),
                                    )
                                    .child(
                                        div()
                                            .id("stash-message-input")
                                            .w_full()
                                            .px_3()
                                            .py_2()
                                            .bg(rgb(BG_SURFACE0))
                                            .border_1()
                                            .border_color(rgb(BLUE))
                                            .rounded_sm()
                                            .text_color(if message.is_empty() {
                                                rgb(TEXT_MUTED)
                                            } else {
                                                rgb(TEXT)
                                            })
                                            .text_sm()
                                            .child(if message.is_empty() {
                                                StyledText::new("wip: save current changes")
                                            } else {
                                                render_text_with_selection_and_caret(
                                                    &message,
                                                    cursor,
                                                    selection_anchor,
                                                )
                                            }),
                                    )
                                    // スタッシュ範囲選択: [All] [Staged] [+Untracked]
                                    .child({
                                        let mode = self.stash_mode;
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(TEXT_MUTED))
                                                    .child("Scope:"),
                                            )
                                            .child(Self::render_stash_mode_btn(
                                                "All",
                                                mode == StashMode::All,
                                                cx,
                                                |this, cx| {
                                                    this.stash_mode = StashMode::All;
                                                    cx.notify();
                                                },
                                            ))
                                            .child(Self::render_stash_mode_btn(
                                                "Staged",
                                                mode == StashMode::Staged,
                                                cx,
                                                |this, cx| {
                                                    this.stash_mode = StashMode::Staged;
                                                    cx.notify();
                                                },
                                            ))
                                            .child(Self::render_stash_mode_btn(
                                                "+Untracked",
                                                mode == StashMode::IncludeUntracked,
                                                cx,
                                                |this, cx| {
                                                    this.stash_mode = StashMode::IncludeUntracked;
                                                    cx.notify();
                                                },
                                            ))
                                    })
                                    .child(
                                        div()
                                            .id("stash-create-button")
                                            .px_3()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(GREEN))
                                            .hover(|el| el.bg(rgb(TEAL)))
                                            .text_xs()
                                            .text_color(rgb(BG_BASE))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.create_stash(cx);
                                            }))
                                            .child("Create Stash"),
                                    )
                                    .child(
                                        div()
                                            .pt_2()
                                            .border_t_1()
                                            .border_color(rgb(BG_SURFACE0))
                                            .text_color(rgb(TEXT_SECONDARY))
                                            .text_xs()
                                            .child(format!("Stashes ({})", entries.len())),
                                    )
                                    .child({
                                        let mut list = div().flex().flex_col().gap_1();
                                        if entries.is_empty() {
                                            list = list.child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(TEXT_MUTED))
                                                    .child("No stashes"),
                                            );
                                        } else {
                                            for (idx, entry) in entries.into_iter().enumerate() {
                                                let apply_ref = entry.reference.clone();
                                                let pop_ref = entry.reference.clone();
                                                let drop_ref = entry.reference.clone();
                                                let expand_ref = entry.reference.clone();
                                                let is_expanded = self
                                                    .stash_expanded_entries
                                                    .contains(&entry.reference);
                                                let files = self
                                                    .stash_entry_files
                                                    .get(&entry.reference)
                                                    .cloned()
                                                    .unwrap_or_default();

                                                let mut entry_div = div()
                                                    .px_2()
                                                    .py_2()
                                                    .rounded_sm()
                                                    .bg(rgb(BG_SURFACE0))
                                                    .flex()
                                                    .flex_col()
                                                    .gap_1();

                                                // ヘッダ行: ▸/▾ + reference + message + ボタン群
                                                entry_div = entry_div.child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .justify_between()
                                                        .gap_2()
                                                        .child(
                                                            div()
                                                                .id(("stash-expand", idx))
                                                                .flex_1()
                                                                .min_w_0()
                                                                .flex()
                                                                .items_center()
                                                                .gap_1()
                                                                .cursor_pointer()
                                                                .on_click(cx.listener(
                                                                    move |this, _, _, cx| {
                                                                        if this
                                                                            .stash_expanded_entries
                                                                            .contains(&expand_ref)
                                                                        {
                                                                            this.stash_expanded_entries
                                                                                .remove(&expand_ref);
                                                                        } else {
                                                                            this.stash_expanded_entries
                                                                                .insert(expand_ref.clone());
                                                                        }
                                                                        cx.notify();
                                                                    },
                                                                ))
                                                                .child(
                                                                    div()
                                                                        .text_xs()
                                                                        .text_color(rgb(TEXT_MUTED))
                                                                        .child(if is_expanded {
                                                                            "▾"
                                                                        } else {
                                                                            "▸"
                                                                        }),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .flex()
                                                                        .flex_col()
                                                                        .min_w_0()
                                                                        .child(
                                                                            div()
                                                                                .text_xs()
                                                                                .text_color(
                                                                                    rgb(BLUE),
                                                                                )
                                                                                .child(
                                                                                    entry.reference,
                                                                                ),
                                                                        )
                                                                        .child(
                                                                            div()
                                                                                .text_xs()
                                                                                .text_color(
                                                                                    rgb(TEXT),
                                                                                )
                                                                                .truncate()
                                                                                .child(
                                                                                    entry.message,
                                                                                ),
                                                                        ),
                                                                ),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .gap_1()
                                                                .child(
                                                                    div()
                                                                        .id(("apply-stash", idx))
                                                                        .px_2()
                                                                        .py_1()
                                                                        .cursor_pointer()
                                                                        .rounded_sm()
                                                                        .bg(rgb(BG_SURFACE1))
                                                                        .hover(|el| {
                                                                            el.bg(rgb(BG_SURFACE2))
                                                                        })
                                                                        .text_xs()
                                                                        .text_color(rgb(GREEN))
                                                                        .on_click(cx.listener(
                                                                            move |this, _, _, cx| {
                                                                                this.open_stash_apply_confirm(
                                                                                    apply_ref.clone(),
                                                                                    cx,
                                                                                );
                                                                            },
                                                                        ))
                                                                        .child("Apply"),
                                                                )
                                                                .child({
                                                                    div()
                                                                        .id(("pop-stash", idx))
                                                                        .px_2()
                                                                        .py_1()
                                                                        .cursor_pointer()
                                                                        .rounded_sm()
                                                                        .bg(rgb(BG_SURFACE1))
                                                                        .hover(|el| {
                                                                            el.bg(rgb(BG_SURFACE2))
                                                                        })
                                                                        .text_xs()
                                                                        .text_color(rgb(TEAL))
                                                                        .on_click(cx.listener(
                                                                            move |this, _, _, cx| {
                                                                                this.open_stash_pop_confirm(
                                                                                    pop_ref.clone(),
                                                                                    cx,
                                                                                );
                                                                            },
                                                                        ))
                                                                        .child("Pop")
                                                                })
                                                                .child(
                                                                    div()
                                                                        .id(("drop-stash", idx))
                                                                        .px_2()
                                                                        .py_1()
                                                                        .cursor_pointer()
                                                                        .rounded_sm()
                                                                        .bg(rgb(BG_SURFACE1))
                                                                        .hover(|el| {
                                                                            el.bg(rgb(BG_SURFACE2))
                                                                        })
                                                                        .text_xs()
                                                                        .text_color(rgb(RED))
                                                                        .on_click(cx.listener(
                                                                            move |this, _, _, cx| {
                                                                                this.open_stash_drop_confirm(
                                                                                    drop_ref.clone(),
                                                                                    cx,
                                                                                );
                                                                            },
                                                                        ))
                                                                        .child("Drop"),
                                                                ),
                                                        ),
                                                );

                                                // 展開時にファイル一覧を表示
                                                if is_expanded {
                                                    for (status, path) in &files {
                                                        let color = match status.as_str() {
                                                            "A" => rgb(GREEN),
                                                            "D" => rgb(RED),
                                                            _ => rgb(YELLOW),
                                                        };
                                                        entry_div = entry_div.child(
                                                            div()
                                                                .pl(px(20.))
                                                                .py_0p5()
                                                                .flex()
                                                                .items_center()
                                                                .gap_1()
                                                                .text_xs()
                                                                .child(
                                                                    div()
                                                                        .w_3()
                                                                        .text_color(color)
                                                                        .child(status.clone()),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .text_color(rgb(TEXT_MUTED))
                                                                        .truncate()
                                                                        .child(path.clone()),
                                                                ),
                                                        );
                                                    }
                                                }

                                                list = list.child(entry_div);
                                            }
                                        }
                                        list
                                    }),
                            )
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
                                            .id("close-stash")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.close_stash_dialog(window, cx);
                                            }))
                                            .child("Close"),
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

    pub fn render_close_group_dialog(&self, group_index: usize, cx: &Context<Self>) -> AnyElement {
        let group_name = self
            .session_manager
            .groups()
            .get(group_index)
            .map(|g| g.name().to_string())
            .unwrap_or_default();

        div()
            .id("close-group-container")
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let key = &event.keystroke.key;
                if key == "escape" {
                    this.close_close_group_dialog(cx);
                } else if key == "enter" {
                    this.confirm_close_group(cx);
                }
            }))
            .child(
                div()
                    .id("close-group-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.close_close_group_dialog(cx);
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
                            .id("close-group-dialog")
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
                                    .child("Close Group"),
                            )
                            .child(div().p_4().flex().flex_col().gap_3().child(
                                div().text_color(rgb(TEXT)).text_sm().child(format!(
                                    "Close \"{}\"? All sessions will be closed.",
                                    group_name
                                )),
                            ))
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
                                            .id("cancel-close-group")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.close_close_group_dialog(cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("confirm-close-group")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(RED))
                                            .hover(|el| el.bg(rgb(MAROON)))
                                            .text_xs()
                                            .text_color(rgb(BG_BASE))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.confirm_close_group(cx);
                                            }))
                                            .child("Close"),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub fn render_discard_confirm_dialog(
        &self,
        path: &std::path::Path,
        change_type: crate::git::ChangeType,
        cx: &Context<Self>,
    ) -> AnyElement {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_else(|| path.to_str().unwrap_or(""));
        let is_untracked = change_type == crate::git::ChangeType::Added;
        let title = if is_untracked {
            "Delete File"
        } else {
            "Discard Changes"
        };
        let message = if is_untracked {
            format!(
                "Delete \"{}\"?\nThis will permanently remove the file from disk.",
                file_name
            )
        } else {
            format!(
                "Discard changes to \"{}\"?\nThis will revert the file to its last committed state.",
                file_name
            )
        };

        div()
            .id("discard-confirm-container")
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let key = &event.keystroke.key;
                if key == "escape" {
                    this.close_discard_confirm(cx);
                } else if key == "enter" {
                    this.confirm_discard_file(cx);
                }
            }))
            .child(
                div()
                    .id("discard-confirm-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.close_discard_confirm(cx);
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
                            .id("discard-confirm-dialog")
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
                                    .child(title),
                            )
                            .child(div().p_4().flex().flex_col().gap_3().children(
                                message.lines().map(|line| {
                                    if line.starts_with("This will") {
                                        div()
                                            .text_color(rgb(YELLOW))
                                            .text_xs()
                                            .child(line.to_string())
                                    } else {
                                        div()
                                            .text_color(rgb(TEXT))
                                            .text_sm()
                                            .child(line.to_string())
                                    }
                                }),
                            ))
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
                                            .id("cancel-discard")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.close_discard_confirm(cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("confirm-discard")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(RED))
                                            .hover(|el| el.bg(rgb(MAROON)))
                                            .text_xs()
                                            .text_color(rgb(BG_BASE))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.confirm_discard_file(cx);
                                            }))
                                            .child(if is_untracked { "Delete" } else { "Discard" }),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub fn render_discard_all_confirm_dialog(&self, cx: &Context<Self>) -> AnyElement {
        div()
            .id("discard-all-confirm-container")
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let key = &event.keystroke.key;
                if key == "escape" {
                    this.close_discard_all_confirm(cx);
                } else if key == "enter" {
                    this.confirm_discard_all(cx);
                }
            }))
            .child(
                div()
                    .id("discard-all-confirm-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.close_discard_all_confirm(cx);
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
                            .id("discard-all-confirm-dialog")
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
                                    .child("Discard All Changes"),
                            )
                            .child(
                                div()
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_color(rgb(TEXT))
                                            .text_sm()
                                            .child("Discard all unstaged changes?"),
                                    )
                                    .child(
                                        div()
                                            .text_color(rgb(YELLOW))
                                            .text_xs()
                                            .child("This will revert all tracked files and delete all untracked files. This cannot be undone."),
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
                                            .id("cancel-discard-all")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.close_discard_all_confirm(cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("confirm-discard-all")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(RED))
                                            .hover(|el| el.bg(rgb(MAROON)))
                                            .text_xs()
                                            .text_color(rgb(BG_BASE))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.confirm_discard_all(cx);
                                            }))
                                            .child("Discard All"),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Confirmation dialog for Smart Commit: no staged changes, offer to stage all and commit.
    pub fn render_smart_commit_confirm_dialog(&self, cx: &Context<Self>) -> AnyElement {
        div()
            .id("smart-commit-confirm-container")
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke.key;
                if key == "escape" {
                    this.cancel_smart_commit(window, cx);
                } else if key == "enter" {
                    this.confirm_smart_commit(window, cx);
                }
            }))
            .child(
                div()
                    .id("smart-commit-confirm-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.cancel_smart_commit(window, cx);
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
                            .id("smart-commit-confirm-dialog")
                            .occlude()
                            .w_96()
                            .bg(rgb(BG_BASE))
                            .border_1()
                            .border_color(rgb(BLUE))
                            .rounded_md()
                            .shadow_lg()
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .text_color(rgb(BLUE))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("No Staged Changes"),
                            )
                            .child(
                                div()
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_color(rgb(TEXT))
                                            .text_sm()
                                            .child("There are no staged changes to commit."),
                                    )
                                    .child(
                                        div().text_color(rgb(TEXT_SECONDARY)).text_xs().child(
                                            "Would you like to stage all changes and commit?",
                                        ),
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
                                            .id("cancel-smart-commit")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.cancel_smart_commit(window, cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("confirm-smart-commit")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(GREEN))
                                            .hover(|el| el.bg(rgb(TEAL)))
                                            .text_xs()
                                            .text_color(rgb(BG_BASE))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.confirm_smart_commit(window, cx);
                                            }))
                                            .child("Stage All & Commit"),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub fn render_undo_commit_confirm_dialog(&self, cx: &Context<Self>) -> AnyElement {
        div()
            .id("undo-commit-confirm-container")
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let key = &event.keystroke.key;
                if key == "escape" {
                    this.close_undo_commit_confirm(cx);
                } else if key == "enter" {
                    this.confirm_undo_last_commit(cx);
                }
            }))
            .child(
                div()
                    .id("undo-commit-confirm-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.close_undo_commit_confirm(cx);
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
                            .id("undo-commit-confirm-dialog")
                            .occlude()
                            .w_96()
                            .bg(rgb(BG_BASE))
                            .border_1()
                            .border_color(rgb(YELLOW))
                            .rounded_md()
                            .shadow_lg()
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .text_color(rgb(YELLOW))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("Undo Last Commit"),
                            )
                            .child(
                                div()
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_color(rgb(TEXT))
                                            .text_sm()
                                            .child("Undo the last commit?"),
                                    )
                                    .child(
                                        div()
                                            .text_color(rgb(TEXT_SECONDARY))
                                            .text_xs()
                                            .child("The commit will be removed, but all changes will be kept as staged (git reset --soft HEAD~1)."),
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
                                            .id("cancel-undo-commit")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.close_undo_commit_confirm(cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("confirm-undo-commit")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(YELLOW))
                                            .hover(|el| el.bg(rgb(PEACH)))
                                            .text_xs()
                                            .text_color(rgb(BG_BASE))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.confirm_undo_last_commit(cx);
                                            }))
                                            .child("Undo Commit"),
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
        let selection_anchors = self.settings_selection_anchors;
        let group_name = self
            .session_manager
            .groups()
            .get(self.settings_group_index)
            .map(|g| g.name().to_string())
            .unwrap_or_default();

        div()
            .id("template-settings-container")
            .track_focus(&self.settings_dialog_focus)
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke.key;
                let sec = this.settings_active_section;
                let primary_mod =
                    event.keystroke.modifiers.control || event.keystroke.modifiers.platform;

                if key == "escape" {
                    this.close_template_settings(window, cx);
                } else if primary_mod && key == "s" {
                    this.save_template_settings(window, cx);
                } else if primary_mod && key == "a" {
                    select_all_text(
                        &this.settings_inputs[sec],
                        &mut this.settings_cursors[sec],
                        &mut this.settings_selection_anchors[sec],
                    );
                    cx.notify();
                } else if primary_mod && key == "c" {
                    copy_selection_to_clipboard(
                        &this.settings_inputs[sec],
                        this.settings_cursors[sec],
                        this.settings_selection_anchors[sec],
                        cx,
                    );
                } else if primary_mod && key == "x" {
                    if cut_selection_to_clipboard(
                        &mut this.settings_inputs[sec],
                        &mut this.settings_cursors[sec],
                        &mut this.settings_selection_anchors[sec],
                        cx,
                    ) {
                        cx.notify();
                    }
                } else if primary_mod && key == "v" {
                    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                        let inserted = if sec == 4 {
                            normalize_single_line_text(&text)
                        } else {
                            text
                        };
                        replace_selection_with_text(
                            &mut this.settings_inputs[sec],
                            &mut this.settings_cursors[sec],
                            &mut this.settings_selection_anchors[sec],
                            &inserted,
                        );
                        cx.notify();
                    }
                } else if key == "tab" {
                    if event.keystroke.modifiers.shift {
                        this.settings_active_section = if sec == 0 { 4 } else { sec - 1 };
                    } else {
                        this.settings_active_section = (sec + 1) % 5;
                    }
                    cx.notify();
                } else if key == "enter" {
                    if sec == 4 {
                        this.save_template_settings(window, cx);
                        return;
                    }
                    replace_selection_with_text(
                        &mut this.settings_inputs[sec],
                        &mut this.settings_cursors[sec],
                        &mut this.settings_selection_anchors[sec],
                        "\n",
                    );
                    cx.notify();
                } else if key == "backspace" {
                    if backspace_text(
                        &mut this.settings_inputs[sec],
                        &mut this.settings_cursors[sec],
                        &mut this.settings_selection_anchors[sec],
                    ) {
                        cx.notify();
                    }
                } else if key == "delete" {
                    if delete_text(
                        &mut this.settings_inputs[sec],
                        &mut this.settings_cursors[sec],
                        &mut this.settings_selection_anchors[sec],
                    ) {
                        cx.notify();
                    }
                } else if key == "left" {
                    let current = this.settings_cursors[sec];
                    move_text_cursor(
                        &mut this.settings_cursors[sec],
                        &mut this.settings_selection_anchors[sec],
                        current.saturating_sub(1),
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "right" {
                    let char_count = this.settings_inputs[sec].chars().count();
                    let cursor = this.settings_cursors[sec];
                    move_text_cursor(
                        &mut this.settings_cursors[sec],
                        &mut this.settings_selection_anchors[sec],
                        (cursor + 1).min(char_count),
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "up" {
                    let cursor = this.settings_cursors[sec];
                    let text = &this.settings_inputs[sec];
                    let (line, col) = cursor_to_line_col(text, cursor);
                    let new_cursor = if line > 0 {
                        line_col_to_cursor(text, line - 1, col)
                    } else {
                        0
                    };
                    move_text_cursor(
                        &mut this.settings_cursors[sec],
                        &mut this.settings_selection_anchors[sec],
                        new_cursor,
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "down" {
                    let cursor = this.settings_cursors[sec];
                    let text = &this.settings_inputs[sec];
                    let (line, col) = cursor_to_line_col(text, cursor);
                    let new_cursor = line_col_to_cursor(text, line + 1, col);
                    move_text_cursor(
                        &mut this.settings_cursors[sec],
                        &mut this.settings_selection_anchors[sec],
                        new_cursor,
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "home" {
                    let cursor = this.settings_cursors[sec];
                    let text = &this.settings_inputs[sec];
                    let (line, _) = cursor_to_line_col(text, cursor);
                    move_text_cursor(
                        &mut this.settings_cursors[sec],
                        &mut this.settings_selection_anchors[sec],
                        line_col_to_cursor(text, line, 0),
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "end" {
                    let cursor = this.settings_cursors[sec];
                    let text = &this.settings_inputs[sec];
                    let (line, _) = cursor_to_line_col(text, cursor);
                    move_text_cursor(
                        &mut this.settings_cursors[sec],
                        &mut this.settings_selection_anchors[sec],
                        line_col_to_cursor(text, line, usize::MAX),
                        event.keystroke.modifiers.shift,
                    );
                    cx.notify();
                } else if key == "space" {
                    replace_selection_with_text(
                        &mut this.settings_inputs[sec],
                        &mut this.settings_cursors[sec],
                        &mut this.settings_selection_anchors[sec],
                        " ",
                    );
                    cx.notify();
                } else if let Some(c) = key.chars().next()
                    && key.chars().count() == 1
                    && !primary_mod
                    && !event.keystroke.modifiers.alt
                {
                    replace_selection_with_text(
                        &mut this.settings_inputs[sec],
                        &mut this.settings_cursors[sec],
                        &mut this.settings_selection_anchors[sec],
                        &c.to_string(),
                    );
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
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|this, _, window, cx| {
                                    window.focus(&this.settings_dialog_focus, cx);
                                }),
                            )
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
                                    .child(format!("Session Template — {}", group_name)),
                            )
                            // Body
                            .child(
                                div()
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .child(Self::render_template_group_header(
                                        "Create-time Actions",
                                    ))
                                    .child(Self::render_textarea_section(
                                        "Pre-create Commands",
                                        "e.g. git pull --ff-only",
                                        &inputs[0],
                                        cursors[0],
                                        selection_anchors[0],
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
                                        selection_anchors[1],
                                        1,
                                        active_section,
                                        true,
                                        cx,
                                    ))
                                    .child(Self::render_textarea_section(
                                        "Files to Sync (glob)",
                                        "e.g. .env.local",
                                        &inputs[2],
                                        cursors[2],
                                        selection_anchors[2],
                                        2,
                                        active_section,
                                        true,
                                        cx,
                                    ))
                                    .child(
                                        div()
                                            .text_color(rgb(TEXT_MUTED))
                                            .text_xs()
                                            .child(
                                                "Creates hard links when possible. Falls back to copy if linking fails.",
                                            ),
                                    )
                                    .child(Self::render_textarea_section(
                                        "Post-create Commands",
                                        "e.g. npm install",
                                        &inputs[3],
                                        cursors[3],
                                        selection_anchors[3],
                                        3,
                                        active_section,
                                        true,
                                        cx,
                                    ))
                                    .child(
                                        div()
                                            .mt_2()
                                            .pt_3()
                                            .border_t_1()
                                            .border_color(rgb(BG_SURFACE0))
                                            .child(Self::render_template_group_header(
                                                "Session Defaults",
                                            )),
                                    )
                                    .child(Self::render_textarea_section(
                                        "Default Working Directory",
                                        ".",
                                        &inputs[4],
                                        cursors[4],
                                        selection_anchors[4],
                                        4,
                                        active_section,
                                        false,
                                        cx,
                                    ))
                                    .child(
                                        div()
                                            .text_color(rgb(TEXT_MUTED))
                                            .text_xs()
                                            .child("Relative path from worktree root."),
                                    ),
                            )
                            // Footer
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
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.close_template_settings(window, cx);
                                                    }))
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
                                                    .on_click(cx.listener(|this, _, window, cx| {
                                                        this.save_template_settings(window, cx);
                                                    }))
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
        selection_anchor: Option<usize>,
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
            let rendered = if is_active {
                insert_caret_marker(content, cursor)
            } else {
                content.to_string()
            };
            let highlights = if is_active {
                selection_ranges_in_display_text(content, cursor, selection_anchor)
            } else {
                Vec::new()
            };
            let selection_style = selection_highlight_style();

            let mut line_start = 0usize;
            for line in rendered.split('\n') {
                let line_end = line_start + line.len();
                let line_text = if line.is_empty() {
                    " ".to_string()
                } else {
                    line.to_string()
                };

                let mut styled = StyledText::new(line_text);
                let line_highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = highlights
                    .iter()
                    .filter_map(|range| {
                        let start = range.start.max(line_start);
                        let end = range.end.min(line_end);
                        if start < end {
                            Some(((start - line_start)..(end - line_start), selection_style))
                        } else {
                            None
                        }
                    })
                    .collect();
                if !line_highlights.is_empty() {
                    styled = styled.with_highlights(line_highlights);
                }

                textarea = textarea.child(div().text_xs().text_color(rgb(TEXT)).child(styled));
                line_start = line_end.saturating_add(1);
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

    fn render_template_group_header(title: &str) -> impl IntoElement {
        div().flex().items_center().child(
            div()
                .text_color(rgb(TEXT_SECONDARY))
                .text_xs()
                .font_weight(gpui::FontWeight::BOLD)
                .child(title.to_string()),
        )
    }

    pub fn render_amend_commit_confirm_dialog(&self, cx: &Context<Self>) -> AnyElement {
        div()
            .id("amend-commit-confirm-container")
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let key = &event.keystroke.key;
                if key == "escape" {
                    this.cancel_amend_commit(window, cx);
                } else if key == "enter" {
                    this.confirm_amend_commit(window, cx);
                }
            }))
            .child(
                div()
                    .id("amend-commit-confirm-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.cancel_amend_commit(window, cx);
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
                            .id("amend-commit-confirm-dialog")
                            .occlude()
                            .w_80()
                            .bg(rgb(BG_BASE))
                            .border_1()
                            .border_color(rgb(YELLOW))
                            .rounded_md()
                            .shadow_lg()
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .text_color(rgb(YELLOW))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("Amend Last Commit"),
                            )
                            .child(
                                div()
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_color(rgb(TEXT))
                                            .text_sm()
                                            .child("Replace the last commit with updated content?"),
                                    )
                                    .child(div().text_color(rgb(TEXT_SECONDARY)).text_xs().child(
                                        "If already pushed, a force push will be required.",
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
                                            .id("cancel-amend-commit")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.cancel_amend_commit(window, cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("confirm-amend-commit")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(YELLOW))
                                            .hover(|el| el.bg(rgb(PEACH)))
                                            .text_xs()
                                            .text_color(rgb(BG_BASE))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.confirm_amend_commit(window, cx);
                                            }))
                                            .child("Amend"),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub fn render_discard_hunk_confirm_dialog(&self, cx: &Context<Self>) -> AnyElement {
        div()
            .id("discard-hunk-confirm-container")
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let key = &event.keystroke.key;
                if key == "escape" {
                    this.cancel_discard_hunk(cx);
                } else if key == "enter" {
                    this.confirm_discard_hunk(cx);
                }
            }))
            .child(
                div()
                    .id("discard-hunk-confirm-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.cancel_discard_hunk(cx);
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
                            .id("discard-hunk-confirm-dialog")
                            .occlude()
                            .w_80()
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
                                    .child("Discard Changes"),
                            )
                            .child(
                                div().p_4().child(
                                    div().text_color(rgb(TEXT)).text_sm().child(
                                        "Discard this hunk? The changes cannot be recovered.",
                                    ),
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
                                            .id("cancel-discard-hunk")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.cancel_discard_hunk(cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("confirm-discard-hunk")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(RED))
                                            .hover(|el| el.bg(rgb(MAROON)))
                                            .text_xs()
                                            .text_color(rgb(BG_BASE))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.confirm_discard_hunk(cx);
                                            }))
                                            .child("Discard"),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub fn render_stash_apply_confirm_dialog(
        &self,
        reference: &str,
        cx: &Context<Self>,
    ) -> AnyElement {
        let body = format!("Apply {}?", reference);
        div()
            .id("stash-apply-confirm-container")
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let key = &event.keystroke.key;
                if key == "escape" {
                    this.cancel_stash_apply(cx);
                } else if key == "enter" {
                    this.confirm_apply_stash(cx);
                }
            }))
            .child(
                div()
                    .id("stash-apply-confirm-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.cancel_stash_apply(cx);
                    })),
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
                            .id("stash-apply-confirm-dialog")
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
                                    .child("Apply Stash"),
                            )
                            .child(
                                div()
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(div().text_color(rgb(TEXT)).text_sm().child(body))
                                    .child(
                                        div()
                                            .text_color(rgb(TEXT_SECONDARY))
                                            .text_xs()
                                            .child("Changes will be applied to the working tree. The stash entry is kept."),
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
                                            .id("cancel-stash-apply")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.cancel_stash_apply(cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("confirm-stash-apply")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(GREEN))
                                            .hover(|el| el.bg(rgb(TEAL)))
                                            .text_xs()
                                            .text_color(rgb(BG_BASE))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.confirm_apply_stash(cx);
                                            }))
                                            .child("Apply"),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub fn render_stash_pop_confirm_dialog(
        &self,
        reference: &str,
        cx: &Context<Self>,
    ) -> AnyElement {
        let body = format!("Pop {}?", reference);
        div()
            .id("stash-pop-confirm-container")
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let key = &event.keystroke.key;
                if key == "escape" {
                    this.cancel_stash_pop(cx);
                } else if key == "enter" {
                    this.confirm_pop_stash(cx);
                }
            }))
            .child(
                div()
                    .id("stash-pop-confirm-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, _, _, cx| {
                        this.cancel_stash_pop(cx);
                    })),
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
                            .id("stash-pop-confirm-dialog")
                            .occlude()
                            .w_80()
                            .bg(rgb(BG_BASE))
                            .border_1()
                            .border_color(rgb(TEAL))
                            .rounded_md()
                            .shadow_lg()
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .border_b_1()
                                    .border_color(rgb(BG_SURFACE0))
                                    .text_color(rgb(TEAL))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("Pop Stash"),
                            )
                            .child(
                                div()
                                    .p_4()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(div().text_color(rgb(TEXT)).text_sm().child(body))
                                    .child(
                                        div()
                                            .text_color(rgb(TEXT_SECONDARY))
                                            .text_xs()
                                            .child("Changes will be applied and the stash entry will be removed (git stash pop)."),
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
                                            .id("cancel-stash-pop")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.cancel_stash_pop(cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("confirm-stash-pop")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(TEAL))
                                            .hover(|el| el.bg(rgb(SAPPHIRE)))
                                            .text_xs()
                                            .text_color(rgb(BG_BASE))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.confirm_pop_stash(cx);
                                            }))
                                            .child("Pop"),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub fn render_stash_drop_confirm_dialog(
        &self,
        reference: &str,
        cx: &Context<Self>,
    ) -> AnyElement {
        let body = format!("Delete {}? This cannot be undone.", reference);
        div()
            .id("stash-drop-confirm-container")
            .absolute()
            .inset_0()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                let key = &event.keystroke.key;
                if key == "escape" {
                    this.cancel_stash_drop(cx);
                } else if key == "enter" {
                    this.confirm_drop_stash(cx);
                }
            }))
            .child(
                div()
                    .id("stash-drop-confirm-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(rgba(OVERLAY))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.cancel_stash_drop(cx);
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
                            .id("stash-drop-confirm-dialog")
                            .occlude()
                            .w_80()
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
                                    .child("Drop Stash"),
                            )
                            .child(
                                div()
                                    .p_4()
                                    .child(div().text_color(rgb(TEXT)).text_sm().child(body)),
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
                                            .id("cancel-stash-drop")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(BG_SURFACE1))
                                            .hover(|el| el.bg(rgb(BG_SURFACE2)))
                                            .text_xs()
                                            .text_color(rgb(TEXT))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.cancel_stash_drop(cx);
                                            }))
                                            .child("Cancel"),
                                    )
                                    .child(
                                        div()
                                            .id("confirm-stash-drop")
                                            .px_4()
                                            .py_2()
                                            .cursor_pointer()
                                            .rounded_sm()
                                            .bg(rgb(RED))
                                            .hover(|el| el.bg(rgb(MAROON)))
                                            .text_xs()
                                            .text_color(rgb(BG_BASE))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.confirm_drop_stash(cx);
                                            }))
                                            .child("Drop"),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    // === Commit dropdown overlay ===

    /// Commitスプリットボタンの▾を押したときに表示するドロップダウン。
    /// アプリメニューと同じオーバーレイ方式で表示し、外側クリックで閉じる。
    pub fn render_commit_dropdown_overlay(&self, cx: &Context<Self>) -> AnyElement {
        div()
            .id("commit-dropdown-overlay")
            .absolute()
            .inset_0()
            // 外側クリックで閉じる透明バックドロップ
            .child(
                div()
                    .id("commit-dropdown-backdrop")
                    .absolute()
                    .inset_0()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.commit_dropdown_open = false;
                            cx.notify();
                        }),
                    ),
            )
            // ドロップダウン本体 (ヘッダーの真下・右端に配置)
            .child(
                div()
                    .absolute()
                    .top(gpui::px(32.))
                    .right(gpui::px(4.))
                    .child(
                        div()
                            .id("commit-dropdown-menu")
                            .occlude()
                            .min_w_40()
                            .bg(rgb(BG_BASE))
                            .border_1()
                            .border_color(rgb(BG_SURFACE1))
                            .rounded_sm()
                            .shadow_lg()
                            .py_1()
                            .child(Self::render_commit_dropdown_item(
                                "Commit",
                                cx,
                                |this, window, cx| {
                                    this.commit_dropdown_open = false;
                                    this.open_commit_dialog(window, cx);
                                },
                            ))
                            .child(Self::render_commit_dropdown_item(
                                "Commit (Amend...)",
                                cx,
                                |this, window, cx| {
                                    this.commit_dropdown_open = false;
                                    this.open_amend_dialog(window, cx);
                                },
                            ))
                            .child(div().my_1().mx_2().h_px().bg(rgb(BG_SURFACE1)))
                            .child(Self::render_commit_dropdown_item(
                                "Undo Last Commit",
                                cx,
                                |this, _, cx| {
                                    this.commit_dropdown_open = false;
                                    this.open_undo_commit_confirm(cx);
                                },
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_commit_dropdown_item(
        label: &str,
        cx: &Context<Self>,
        handler: impl Fn(&mut Self, &mut gpui::Window, &mut gpui::Context<Self>) + 'static,
    ) -> impl gpui::IntoElement {
        let label_owned = label.to_string();
        div()
            .id(label_owned.clone())
            .w_full()
            .px_3()
            .py_1()
            .cursor_pointer()
            .hover(|b| b.bg(rgb(BG_SURFACE1)))
            .text_xs()
            .text_color(rgb(TEXT))
            .on_click(cx.listener(move |this, _, window, cx| {
                handler(this, window, cx);
            }))
            .child(label_owned)
    }
}

fn normalize_single_line_text(text: &str) -> String {
    text.chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect()
}

fn render_text_with_selection_and_caret(
    text: &str,
    cursor: usize,
    selection_anchor: Option<usize>,
) -> StyledText {
    let rendered = insert_caret_marker(text, cursor);
    let highlights = selection_ranges_in_display_text(text, cursor, selection_anchor)
        .into_iter()
        .map(|range| (range, selection_highlight_style()));
    StyledText::new(rendered).with_highlights(highlights)
}

fn selection_highlight_style() -> HighlightStyle {
    HighlightStyle {
        background_color: Some(Hsla::from(rgba(0x74a9e455))),
        ..HighlightStyle::default()
    }
}

fn insert_caret_marker(text: &str, cursor: usize) -> String {
    let cursor = cursor.min(text.chars().count());
    let byte_pos = char_to_byte_offset(text, cursor);
    let (before, after) = text.split_at(byte_pos);
    format!("{}|{}", before, after)
}

fn selection_ranges_in_display_text(
    text: &str,
    cursor: usize,
    selection_anchor: Option<usize>,
) -> Vec<std::ops::Range<usize>> {
    let Some((start, end)) = selected_char_range(cursor, selection_anchor) else {
        return Vec::new();
    };

    let cursor = cursor.min(text.chars().count());
    let display_start = if start >= cursor { start + 1 } else { start };
    let display_end = if end > cursor { end + 1 } else { end };
    let rendered = insert_caret_marker(text, cursor);
    let byte_start = char_to_byte_offset(&rendered, display_start);
    let byte_end = char_to_byte_offset(&rendered, display_end);
    if byte_start < byte_end {
        vec![byte_start..byte_end]
    } else {
        Vec::new()
    }
}

fn selected_char_range(cursor: usize, selection_anchor: Option<usize>) -> Option<(usize, usize)> {
    let anchor = selection_anchor?;
    if anchor == cursor {
        None
    } else {
        Some((anchor.min(cursor), anchor.max(cursor)))
    }
}

fn move_text_cursor(
    cursor: &mut usize,
    selection_anchor: &mut Option<usize>,
    next_cursor: usize,
    selecting: bool,
) {
    if selecting {
        if selection_anchor.is_none() {
            *selection_anchor = Some(*cursor);
        }
        *cursor = next_cursor;
    } else {
        *cursor = next_cursor;
        *selection_anchor = None;
    }
}

fn select_all_text(text: &str, cursor: &mut usize, selection_anchor: &mut Option<usize>) {
    *selection_anchor = Some(0);
    *cursor = text.chars().count();
}

fn slice_char_range(text: &str, start: usize, end: usize) -> String {
    let byte_start = char_to_byte_offset(text, start);
    let byte_end = char_to_byte_offset(text, end);
    text[byte_start..byte_end].to_string()
}

fn copy_selection_to_clipboard(
    text: &str,
    cursor: usize,
    selection_anchor: Option<usize>,
    cx: &mut Context<SashikiApp>,
) -> bool {
    if let Some((start, end)) = selected_char_range(cursor, selection_anchor) {
        let selected = slice_char_range(text, start, end);
        cx.write_to_clipboard(ClipboardItem::new_string(selected));
        true
    } else {
        false
    }
}

fn delete_selected_text(
    text: &mut String,
    cursor: &mut usize,
    selection_anchor: &mut Option<usize>,
) -> bool {
    if let Some((start, end)) = selected_char_range(*cursor, *selection_anchor) {
        let byte_start = char_to_byte_offset(text, start);
        let byte_end = char_to_byte_offset(text, end);
        text.replace_range(byte_start..byte_end, "");
        *cursor = start;
        *selection_anchor = None;
        true
    } else {
        false
    }
}

fn cut_selection_to_clipboard(
    text: &mut String,
    cursor: &mut usize,
    selection_anchor: &mut Option<usize>,
    cx: &mut Context<SashikiApp>,
) -> bool {
    if let Some((start, end)) = selected_char_range(*cursor, *selection_anchor) {
        let selected = slice_char_range(text, start, end);
        cx.write_to_clipboard(ClipboardItem::new_string(selected));
        delete_selected_text(text, cursor, selection_anchor)
    } else {
        false
    }
}

fn replace_selection_with_text(
    text: &mut String,
    cursor: &mut usize,
    selection_anchor: &mut Option<usize>,
    inserted: &str,
) -> bool {
    let had_selection = delete_selected_text(text, cursor, selection_anchor);
    if inserted.is_empty() {
        return had_selection;
    }

    let byte_pos = char_to_byte_offset(text, *cursor);
    text.insert_str(byte_pos, inserted);
    *cursor += inserted.chars().count();
    *selection_anchor = None;
    true
}

fn backspace_text(
    text: &mut String,
    cursor: &mut usize,
    selection_anchor: &mut Option<usize>,
) -> bool {
    if delete_selected_text(text, cursor, selection_anchor) {
        return true;
    }
    if *cursor == 0 {
        return false;
    }
    let byte_start = char_to_byte_offset(text, *cursor - 1);
    let byte_end = char_to_byte_offset(text, *cursor);
    text.replace_range(byte_start..byte_end, "");
    *cursor -= 1;
    *selection_anchor = None;
    true
}

fn delete_text(
    text: &mut String,
    cursor: &mut usize,
    selection_anchor: &mut Option<usize>,
) -> bool {
    if delete_selected_text(text, cursor, selection_anchor) {
        return true;
    }
    let len = text.chars().count();
    if *cursor >= len {
        return false;
    }
    let byte_start = char_to_byte_offset(text, *cursor);
    let byte_end = char_to_byte_offset(text, *cursor + 1);
    text.replace_range(byte_start..byte_end, "");
    *selection_anchor = None;
    true
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
