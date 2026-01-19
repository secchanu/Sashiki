//! File viewer component for viewing and editing files

use crate::theme::*;
use gpui::{
    actions, div, prelude::*, rgb, App, Bounds, Context, Element, ElementId,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId,
    InspectorElementId, IntoElement, KeyBinding, LayoutId, MouseButton, ParentElement, Pixels,
    Render, Styled, UTF16Selection, Window,
};
use std::ops::Range;
use std::path::PathBuf;

// Define actions for editor special keys
actions!(
    file_viewer,
    [
        EditorEnter,
        EditorBackspace,
        EditorUp,
        EditorDown,
        EditorLeft,
        EditorRight,
        EditorHome,
        EditorEnd,
        EditorDelete,
        EditorEscape,
        EditorTab,
    ]
);

/// View mode for the file viewer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileViewMode {
    /// View file content
    Content,
    /// View git diff
    Diff,
    /// Edit file
    Edit,
}

/// Diff view style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffViewStyle {
    #[default]
    Unified,
    SideBySide,
}

/// Parsed diff line for split view
#[derive(Debug, Clone)]
struct ParsedDiffLine {
    old_line_num: Option<usize>,
    new_line_num: Option<usize>,
    content: String,
    line_type: DiffLineType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffLineType {
    Context,
    Added,
    Removed,
    Header,
    HunkHeader,
}

/// File viewer component
pub struct FileViewer {
    file_path: Option<PathBuf>,
    content: String,
    diff_content: Option<String>,
    mode: FileViewMode,
    diff_view_style: DiffViewStyle,
    modified: bool,
    focus_handle: FocusHandle,
    cursor_line: usize,
    cursor_col: usize,
    preedit_text: String,
    /// Error message to display to user (e.g., save failure)
    error_message: Option<String>,
}

impl FileViewer {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            file_path: None,
            content: String::new(),
            diff_content: None,
            mode: FileViewMode::Content,
            diff_view_style: DiffViewStyle::default(),
            modified: false,
            focus_handle: cx.focus_handle(),
            cursor_line: 0,
            cursor_col: 0,
            preedit_text: String::new(),
            error_message: None,
        }
    }

    /// Bind editor key actions
    pub fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("enter", EditorEnter, Some("FileViewer")),
            KeyBinding::new("backspace", EditorBackspace, Some("FileViewer")),
            KeyBinding::new("up", EditorUp, Some("FileViewer")),
            KeyBinding::new("down", EditorDown, Some("FileViewer")),
            KeyBinding::new("left", EditorLeft, Some("FileViewer")),
            KeyBinding::new("right", EditorRight, Some("FileViewer")),
            KeyBinding::new("home", EditorHome, Some("FileViewer")),
            KeyBinding::new("end", EditorEnd, Some("FileViewer")),
            KeyBinding::new("delete", EditorDelete, Some("FileViewer")),
            KeyBinding::new("escape", EditorEscape, Some("FileViewer")),
            KeyBinding::new("tab", EditorTab, Some("FileViewer")),
        ]);
    }

    // Action handlers
    fn on_editor_enter(&mut self, _: &EditorEnter, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == FileViewMode::Edit {
            self.insert_text("\n");
            cx.notify();
        }
    }

    fn on_editor_backspace(&mut self, _: &EditorBackspace, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == FileViewMode::Edit {
            self.backspace();
            cx.notify();
        }
    }

    fn on_editor_up(&mut self, _: &EditorUp, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == FileViewMode::Edit {
            self.move_cursor(-1, 0);
            cx.notify();
        }
    }

    fn on_editor_down(&mut self, _: &EditorDown, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == FileViewMode::Edit {
            self.move_cursor(1, 0);
            cx.notify();
        }
    }

    fn on_editor_left(&mut self, _: &EditorLeft, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == FileViewMode::Edit {
            self.move_cursor(0, -1);
            cx.notify();
        }
    }

    fn on_editor_right(&mut self, _: &EditorRight, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == FileViewMode::Edit {
            self.move_cursor(0, 1);
            cx.notify();
        }
    }

    fn on_editor_home(&mut self, _: &EditorHome, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == FileViewMode::Edit {
            self.cursor_col = 0;
            cx.notify();
        }
    }

    fn on_editor_end(&mut self, _: &EditorEnd, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == FileViewMode::Edit {
            let lines: Vec<&str> = self.content.lines().collect();
            if self.cursor_line < lines.len() {
                self.cursor_col = lines[self.cursor_line].chars().count();
            }
            cx.notify();
        }
    }

    fn on_editor_delete(&mut self, _: &EditorDelete, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == FileViewMode::Edit {
            // Move right then backspace to delete character after cursor
            self.move_cursor(0, 1);
            self.backspace();
            cx.notify();
        }
    }

    fn on_editor_escape(&mut self, _: &EditorEscape, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == FileViewMode::Edit {
            self.mode = FileViewMode::Content;
            cx.notify();
        }
    }

    fn on_editor_tab(&mut self, _: &EditorTab, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode == FileViewMode::Edit {
            self.insert_text("    "); // 4 spaces for tab
            cx.notify();
        }
    }

    /// Open a file for viewing
    pub fn open_file(&mut self, path: PathBuf) -> Result<(), std::io::Error> {
        let content = std::fs::read_to_string(&path)?;
        self.file_path = Some(path);
        self.content = content;
        self.diff_content = None;
        self.mode = FileViewMode::Content;
        self.modified = false;
        self.cursor_line = 0;
        self.cursor_col = 0;
        self.preedit_text.clear();
        Ok(())
    }

    /// Open a file with diff content
    pub fn open_file_with_diff(
        &mut self,
        path: PathBuf,
        diff: String,
    ) -> Result<(), std::io::Error> {
        let content = std::fs::read_to_string(&path)?;
        self.file_path = Some(path);
        self.content = content;
        self.diff_content = Some(diff);
        self.mode = FileViewMode::Diff;
        self.modified = false;
        self.cursor_line = 0;
        self.cursor_col = 0;
        self.preedit_text.clear();
        Ok(())
    }

    /// Open a deleted file with diff content (file doesn't exist on disk)
    pub fn open_deleted_file_with_diff(&mut self, path: PathBuf, diff: String) {
        self.file_path = Some(path);
        self.content = String::new(); // No content for deleted files
        self.diff_content = Some(diff);
        self.mode = FileViewMode::Diff;
        self.modified = false;
        self.cursor_line = 0;
        self.cursor_col = 0;
        self.preedit_text.clear();
    }

    /// Switch to content view
    pub fn view_content(&mut self) {
        self.mode = FileViewMode::Content;
    }

    /// Switch to diff view
    pub fn view_diff(&mut self) {
        if self.diff_content.is_some() {
            self.mode = FileViewMode::Diff;
        }
    }

    /// Switch to edit mode
    pub fn start_edit(&mut self) {
        self.mode = FileViewMode::Edit;
    }

    /// Toggle diff view style
    pub fn toggle_diff_style(&mut self) {
        self.diff_view_style = match self.diff_view_style {
            DiffViewStyle::Unified => DiffViewStyle::SideBySide,
            DiffViewStyle::SideBySide => DiffViewStyle::Unified,
        };
    }

    /// Save the file
    pub fn save(&mut self) {
        self.error_message = None;
        if let Some(path) = &self.file_path {
            match std::fs::write(path, &self.content) {
                Ok(()) => self.modified = false,
                Err(e) => self.error_message = Some(format!("Failed to save: {}", e)),
            }
        }
    }

    /// Dismiss error message
    pub fn dismiss_error(&mut self) {
        self.error_message = None;
    }

    /// Close the file
    pub fn close(&mut self) {
        self.file_path = None;
        self.content.clear();
        self.diff_content = None;
        self.mode = FileViewMode::Content;
        self.modified = false;
        self.preedit_text.clear();
        self.error_message = None;
    }

    /// Insert text at cursor position (for edit mode)
    pub fn insert_text(&mut self, text: &str) {
        let lines: Vec<&str> = self.content.lines().collect();

        if self.cursor_line >= lines.len() {
            self.content.push_str(text);
        } else {
            let mut new_content = String::new();
            for (i, line) in lines.iter().enumerate() {
                if i == self.cursor_line {
                    let col = self.cursor_col.min(line.len());
                    new_content.push_str(&line[..col]);
                    new_content.push_str(text);
                    new_content.push_str(&line[col..]);
                } else {
                    new_content.push_str(line);
                }
                new_content.push('\n');
            }
            self.content = new_content.trim_end_matches('\n').to_string();
        }

        // Move cursor after inserted text
        for c in text.chars() {
            if c == '\n' {
                self.cursor_line += 1;
                self.cursor_col = 0;
            } else {
                self.cursor_col += 1;
            }
        }

        self.modified = true;
    }

    /// Delete character before cursor
    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            let lines: Vec<&str> = self.content.lines().collect();
            if self.cursor_line < lines.len() {
                let line = lines[self.cursor_line];
                let char_count = line.chars().count();
                let col = self.cursor_col.min(char_count);
                if col > 0 {
                    let mut new_content = String::new();
                    for (i, l) in lines.iter().enumerate() {
                        if i == self.cursor_line {
                            // Convert character position to byte position
                            let byte_pos_before: usize = l.char_indices()
                                .take(col - 1)
                                .map(|(_, c)| c.len_utf8())
                                .sum();
                            let byte_pos_after: usize = l.char_indices()
                                .take(col)
                                .map(|(_, c)| c.len_utf8())
                                .sum();
                            new_content.push_str(&l[..byte_pos_before]);
                            new_content.push_str(&l[byte_pos_after..]);
                        } else {
                            new_content.push_str(l);
                        }
                        new_content.push('\n');
                    }
                    self.content = new_content.trim_end_matches('\n').to_string();
                    self.cursor_col -= 1;
                    self.modified = true;
                }
            }
        } else if self.cursor_line > 0 {
            // Join with previous line
            let lines: Vec<&str> = self.content.lines().collect();
            let prev_line_char_count = lines[self.cursor_line - 1].chars().count();
            let mut new_content = String::new();
            for (i, l) in lines.iter().enumerate() {
                if i == self.cursor_line - 1 {
                    new_content.push_str(l);
                    if self.cursor_line < lines.len() {
                        new_content.push_str(lines[self.cursor_line]);
                    }
                } else if i != self.cursor_line {
                    new_content.push_str(l);
                    new_content.push('\n');
                }
            }
            self.content = new_content.trim_end_matches('\n').to_string();
            self.cursor_line -= 1;
            self.cursor_col = prev_line_char_count;
            self.modified = true;
        }
    }

    /// Move cursor
    pub fn move_cursor(&mut self, line_delta: i32, col_delta: i32) {
        let lines: Vec<&str> = self.content.lines().collect();

        // Move line
        let new_line = (self.cursor_line as i32 + line_delta).max(0) as usize;
        self.cursor_line = new_line.min(lines.len().saturating_sub(1));

        // Get current line character count (not byte length)
        let char_count = lines.get(self.cursor_line).map(|l| l.chars().count()).unwrap_or(0);

        // Move column
        let new_col = (self.cursor_col as i32 + col_delta).max(0) as usize;
        self.cursor_col = new_col.min(char_count);
    }

    /// Parse unified diff into structured lines for side-by-side view
    fn parse_diff_for_split_view(&self) -> (Vec<ParsedDiffLine>, Vec<ParsedDiffLine>) {
        let diff = self.diff_content.as_deref().unwrap_or("");
        let mut left_lines: Vec<ParsedDiffLine> = Vec::new();
        let mut right_lines: Vec<ParsedDiffLine> = Vec::new();

        let mut old_line_num = 1usize;
        let mut new_line_num = 1usize;

        for line in diff.lines() {
            if line.starts_with("@@") {
                // Parse hunk header: @@ -start,count +start,count @@
                if let Some((old_start, new_start)) = Self::parse_hunk_header(line) {
                    old_line_num = old_start;
                    new_line_num = new_start;
                }
                left_lines.push(ParsedDiffLine {
                    old_line_num: None,
                    new_line_num: None,
                    content: line.to_string(),
                    line_type: DiffLineType::HunkHeader,
                });
                right_lines.push(ParsedDiffLine {
                    old_line_num: None,
                    new_line_num: None,
                    content: line.to_string(),
                    line_type: DiffLineType::HunkHeader,
                });
            } else if line.starts_with("---")
                || line.starts_with("+++")
                || line.starts_with("diff ")
            {
                // Header lines
                left_lines.push(ParsedDiffLine {
                    old_line_num: None,
                    new_line_num: None,
                    content: line.to_string(),
                    line_type: DiffLineType::Header,
                });
                right_lines.push(ParsedDiffLine {
                    old_line_num: None,
                    new_line_num: None,
                    content: line.to_string(),
                    line_type: DiffLineType::Header,
                });
            } else if let Some(stripped) = line.strip_prefix('+') {
                // Added line - only on right
                left_lines.push(ParsedDiffLine {
                    old_line_num: None,
                    new_line_num: None,
                    content: String::new(),
                    line_type: DiffLineType::Added,
                });
                right_lines.push(ParsedDiffLine {
                    old_line_num: None,
                    new_line_num: Some(new_line_num),
                    content: stripped.to_string(),
                    line_type: DiffLineType::Added,
                });
                new_line_num += 1;
            } else if let Some(stripped) = line.strip_prefix('-') {
                // Removed line - only on left
                left_lines.push(ParsedDiffLine {
                    old_line_num: Some(old_line_num),
                    new_line_num: None,
                    content: stripped.to_string(),
                    line_type: DiffLineType::Removed,
                });
                right_lines.push(ParsedDiffLine {
                    old_line_num: None,
                    new_line_num: None,
                    content: String::new(),
                    line_type: DiffLineType::Removed,
                });
                old_line_num += 1;
            } else if line.starts_with(' ') || line.is_empty() {
                // Context line - both sides
                let content = if line.is_empty() {
                    ""
                } else {
                    &line[1..]
                };
                left_lines.push(ParsedDiffLine {
                    old_line_num: Some(old_line_num),
                    new_line_num: None,
                    content: content.to_string(),
                    line_type: DiffLineType::Context,
                });
                right_lines.push(ParsedDiffLine {
                    old_line_num: None,
                    new_line_num: Some(new_line_num),
                    content: content.to_string(),
                    line_type: DiffLineType::Context,
                });
                old_line_num += 1;
                new_line_num += 1;
            }
        }

        (left_lines, right_lines)
    }

    fn parse_hunk_header(line: &str) -> Option<(usize, usize)> {
        // Parse @@ -old_start,old_count +new_start,new_count @@
        let parts: Vec<&str> = line.split(' ').collect();
        if parts.len() >= 3 {
            let old_part = parts[1].trim_start_matches('-');
            let new_part = parts[2].trim_start_matches('+');

            let old_start = old_part.split(',').next()?.parse().ok()?;
            let new_start = new_part.split(',').next()?.parse().ok()?;

            Some((old_start, new_start))
        } else {
            None
        }
    }

    fn render_toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let file_name = self
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("No file");

        let has_diff = self.diff_content.is_some();
        let mode = self.mode;
        let modified = self.modified;
        let diff_style = self.diff_view_style;

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
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .child(file_name.to_string()),
                    )
                    .when(modified, |el| {
                        el.child(
                            div()
                                .text_xs()
                                .text_color(rgb(YELLOW))
                                .child("[Modified]"),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    // Content button
                    .child(
                        div()
                            .id("view-content")
                            .px_2()
                            .py_1()
                            .cursor_pointer()
                            .rounded_sm()
                            .when(mode == FileViewMode::Content, |el| el.bg(rgb(BG_SURFACE1)))
                            .hover(|el| el.bg(rgb(BG_SURFACE1)))
                            .text_xs()
                            .text_color(rgb(TEXT))
                            .on_click(cx.listener(|this, _, _, _cx| {
                                this.view_content();
                            }))
                            .child("Content"),
                    )
                    // Diff button (only if diff available)
                    .when(has_diff, |el| {
                        el.child(
                            div()
                                .id("view-diff")
                                .px_2()
                                .py_1()
                                .cursor_pointer()
                                .rounded_sm()
                                .when(mode == FileViewMode::Diff, |d| d.bg(rgb(BG_SURFACE1)))
                                .hover(|d| d.bg(rgb(BG_SURFACE1)))
                                .text_xs()
                                .text_color(rgb(TEXT))
                                .on_click(cx.listener(|this, _, _, _cx| {
                                    this.view_diff();
                                }))
                                .child("Diff"),
                        )
                    })
                    // Diff style toggle (only in diff mode)
                    .when(has_diff && mode == FileViewMode::Diff, |el| {
                        el.child(
                            div()
                                .id("toggle-diff-style")
                                .px_2()
                                .py_1()
                                .cursor_pointer()
                                .rounded_sm()
                                .bg(rgb(BG_SURFACE0))
                                .hover(|d| d.bg(rgb(BG_SURFACE1)))
                                .text_xs()
                                .text_color(rgb(BLUE))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_diff_style();
                                    cx.notify();
                                }))
                                .child(if diff_style == DiffViewStyle::SideBySide {
                                    "Unified"
                                } else {
                                    "Split"
                                }),
                        )
                    })
                    // Edit button
                    .child(
                        div()
                            .id("edit-file")
                            .px_2()
                            .py_1()
                            .cursor_pointer()
                            .rounded_sm()
                            .when(mode == FileViewMode::Edit, |el| {
                                el.bg(rgb(BLUE)).text_color(rgb(BG_BASE))
                            })
                            .when(mode != FileViewMode::Edit, |el| {
                                el.hover(|e| e.bg(rgb(BG_SURFACE1)))
                                    .text_color(rgb(BLUE))
                            })
                            .text_xs()
                            .on_click(cx.listener(|this, _, _, _cx| {
                                this.start_edit();
                            }))
                            .child("Edit"),
                    )
                    // Save button (only in edit mode)
                    .when(mode == FileViewMode::Edit && modified, |el| {
                        el.child(
                            div()
                                .id("save-file")
                                .px_2()
                                .py_1()
                                .cursor_pointer()
                                .rounded_sm()
                                .bg(rgb(GREEN))
                                .text_color(rgb(BG_BASE))
                                .text_xs()
                                .hover(|e| e.bg(rgb(TEAL)))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.save();
                                    cx.notify();
                                }))
                                .child("Save"),
                        )
                    })
                    // Close button
                    .child(
                        div()
                            .id("close-file")
                            .px_2()
                            .py_1()
                            .cursor_pointer()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .hover(|el| el.text_color(rgb(RED)))
                            .on_click(cx.listener(|this, _, _, _cx| {
                                this.close();
                            }))
                            .child("Close"),
                    ),
            )
    }

    fn render_content(&self) -> impl IntoElement {
        let lines: Vec<(usize, &str)> = self.content.lines().enumerate().collect();

        div()
            .flex_1()
            .overflow_hidden()
            .bg(rgb(BG_BASE))
            .p_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .font_family("Consolas")
                    .text_sm()
                    .children(lines.into_iter().map(|(num, line)| {
                        div()
                            .flex()
                            .child(
                                div()
                                    .w_12()
                                    .text_right()
                                    .pr_2()
                                    .text_color(rgb(TEXT_MUTED))
                                    .child(format!("{}", num + 1)),
                            )
                            .child(
                                div().flex_1().text_color(rgb(TEXT)).child(
                                    if line.is_empty() {
                                        " ".to_string()
                                    } else {
                                        line.to_string()
                                    },
                                ),
                            )
                    })),
            )
    }

    fn render_diff(&self) -> impl IntoElement {
        let diff = self.diff_content.as_deref().unwrap_or("");
        let lines: Vec<&str> = diff.lines().collect();

        div()
            .flex_1()
            .overflow_hidden()
            .bg(rgb(BG_BASE))
            .p_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .font_family("Consolas")
                    .text_sm()
                    .children(lines.into_iter().map(|line| {
                        let (bg_color, text_color) =
                            if line.starts_with('+') && !line.starts_with("+++") {
                                (Some(rgb(DIFF_ADDED_BG)), rgb(GREEN))
                            } else if line.starts_with('-') && !line.starts_with("---") {
                                (Some(rgb(DIFF_REMOVED_BG)), rgb(RED))
                            } else if line.starts_with("@@") {
                                (None, rgb(BLUE))
                            } else {
                                (None, rgb(TEXT_MUTED))
                            };

                        div()
                            .px_2()
                            .when_some(bg_color, |el, color| el.bg(color))
                            .text_color(text_color)
                            .child(if line.is_empty() {
                                " ".to_string()
                            } else {
                                line.to_string()
                            })
                    })),
            )
    }

    fn render_split_diff(&self) -> impl IntoElement {
        let (left_lines, right_lines) = self.parse_diff_for_split_view();

        div()
            .flex_1()
            .flex()
            .flex_row()
            .overflow_hidden()
            .bg(rgb(BG_BASE))
            // Left panel (old/before)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .border_r_1()
                    .border_color(rgb(BG_SURFACE0))
                    // Header
                    .child(
                        div()
                            .h_6()
                            .px_2()
                            .flex()
                            .items_center()
                            .bg(rgb(BG_MANTLE))
                            .text_xs()
                            .text_color(rgb(RED))
                            .child("Before (HEAD)"),
                    )
                    // Content
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .p_2()
                            .font_family("Consolas")
                            .text_sm()
                            .children(left_lines.iter().map(|line| {
                                Self::render_diff_line(line, true)
                            })),
                    ),
            )
            // Right panel (new/after)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    // Header
                    .child(
                        div()
                            .h_6()
                            .px_2()
                            .flex()
                            .items_center()
                            .bg(rgb(BG_MANTLE))
                            .text_xs()
                            .text_color(rgb(GREEN))
                            .child("After (Working)"),
                    )
                    // Content
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .p_2()
                            .font_family("Consolas")
                            .text_sm()
                            .children(right_lines.iter().map(|line| {
                                Self::render_diff_line(line, false)
                            })),
                    ),
            )
    }

    fn render_diff_line(line: &ParsedDiffLine, is_left: bool) -> impl IntoElement {
        let (bg_color, text_color) = match line.line_type {
            DiffLineType::Added => (Some(rgb(DIFF_ADDED_BG)), rgb(GREEN)),
            DiffLineType::Removed => (Some(rgb(DIFF_REMOVED_BG)), rgb(RED)),
            DiffLineType::HunkHeader => (None, rgb(BLUE)),
            DiffLineType::Header => (None, rgb(TEXT_MUTED)),
            DiffLineType::Context => (None, rgb(TEXT)),
        };

        let line_num = if is_left {
            line.old_line_num
        } else {
            line.new_line_num
        };

        div()
            .flex()
            .flex_row()
            .when_some(bg_color, |el, color| el.bg(color))
            .child(
                // Line number
                div()
                    .w_10()
                    .text_right()
                    .pr_2()
                    .text_color(rgb(BG_SURFACE1))
                    .child(line_num.map(|n| n.to_string()).unwrap_or_default()),
            )
            .child(
                // Content
                div()
                    .flex_1()
                    .text_color(text_color)
                    .child(if line.content.is_empty() {
                        " ".to_string()
                    } else {
                        line.content.clone()
                    }),
            )
    }

    fn render_editor(&self) -> impl IntoElement {
        let lines: Vec<(usize, &str)> = self.content.lines().enumerate().collect();
        let cursor_line = self.cursor_line;
        let cursor_col = self.cursor_col;
        let preedit = self.preedit_text.clone();

        div()
            .flex_1()
            .overflow_hidden()
            .bg(rgb(BG_BASE))
            .p_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .font_family("Consolas")
                    .text_sm()
                    .children(lines.into_iter().map(|(num, line)| {
                        let is_cursor_line = num == cursor_line;

                        div()
                            .flex()
                            .when(is_cursor_line, |el| el.bg(rgb(BG_SURFACE0)))
                            .child(
                                div()
                                    .w_12()
                                    .text_right()
                                    .pr_2()
                                    .text_color(if is_cursor_line {
                                        rgb(BLUE)
                                    } else {
                                        rgb(TEXT_MUTED)
                                    })
                                    .child(format!("{}", num + 1)),
                            )
                            .child({
                                let char_count = line.chars().count();
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_row()
                                    .text_color(rgb(TEXT))
                                    .when(
                                        is_cursor_line && cursor_col <= char_count,
                                        |el| {
                                            let col = cursor_col.min(char_count);
                                            // Convert character position to byte position for slicing
                                            let before = if col == 0 { "" } else {
                                                &line[..line.char_indices().nth(col).map(|(i, _)| i).unwrap_or(line.len())]
                                            };
                                            let cursor_char = line.chars().nth(col).unwrap_or(' ');
                                            let after_start = line
                                                .char_indices()
                                                .nth(col + 1)
                                                .map(|(i, _)| i)
                                                .unwrap_or(line.len());
                                            let after = &line[after_start..];

                                            el.child(div().child(before.to_string()))
                                                .child(
                                                    div()
                                                        .bg(rgb(ROSEWATER))
                                                        .text_color(rgb(BG_BASE))
                                                        .child(cursor_char.to_string()),
                                                )
                                                .child(div().child(after.to_string()))
                                        },
                                    )
                                    .when(
                                        !(is_cursor_line && cursor_col <= char_count),
                                        |el| {
                                            el.child(if line.is_empty() {
                                                " ".to_string()
                                            } else {
                                                line.to_string()
                                            })
                                        },
                                    )
                            })
                    }))
                    // Show preedit text
                    .when(!preedit.is_empty(), |this| {
                        this.child(
                            div()
                                .absolute()
                                .bg(rgb(BG_SURFACE0))
                                .text_color(rgb(YELLOW))
                                .px_2()
                                .py_1()
                                .rounded_sm()
                                .child(format!("IME: {}", preedit)),
                        )
                    }),
            )
    }
}

impl Focusable for FileViewer {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// IME input handler for file viewer - implements EntityInputHandler
impl EntityInputHandler for FileViewer {
    fn text_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        Some(String::new())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: 0..0,
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        if self.preedit_text.is_empty() {
            None
        } else {
            Some(0..self.preedit_text.encode_utf16().count())
        }
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.preedit_text.clear();
    }

    fn replace_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode == FileViewMode::Edit {
            self.preedit_text.clear();
            if !text.is_empty() {
                self.insert_text(text);
            }
            cx.notify();
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode == FileViewMode::Edit {
            self.preedit_text = new_text.to_string();
            cx.notify();
        }
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(bounds)
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        Some(0)
    }
}

/// Custom element that handles input during paint phase
struct FileViewerElement {
    view: Entity<FileViewer>,
    content: gpui::AnyElement,
}

impl IntoElement for FileViewerElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for FileViewerElement {
    type RequestLayoutState = gpui::AnyElement;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some("file-viewer-input-element".into())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut content = std::mem::replace(&mut self.content, gpui::Empty.into_any_element());
        let layout_id = content.request_layout(window, cx);
        (layout_id, content)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        content: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        content.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        content: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        content.paint(window, cx);

        // Only set up input handler when in edit mode and focused
        let view_ref = self.view.read(cx);
        let is_edit_mode = view_ref.mode == FileViewMode::Edit;
        let focus_handle = view_ref.focus_handle.clone();

        if is_edit_mode && focus_handle.is_focused(window) {
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.view.clone()),
                cx,
            );
        }
    }
}

impl Render for FileViewer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_file = self.file_path.is_some();
        let is_edit_mode = self.mode == FileViewMode::Edit;
        let has_error = self.error_message.is_some();
        let error_msg = self.error_message.clone().unwrap_or_default();
        let focus_handle = self.focus_handle.clone();

        let content = div()
            .id("file-viewer")
            .key_context("FileViewer")
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(BG_BASE))
            .relative()
            // Register action handlers (only active in edit mode due to handler checks)
            .on_action(cx.listener(Self::on_editor_enter))
            .on_action(cx.listener(Self::on_editor_backspace))
            .on_action(cx.listener(Self::on_editor_up))
            .on_action(cx.listener(Self::on_editor_down))
            .on_action(cx.listener(Self::on_editor_left))
            .on_action(cx.listener(Self::on_editor_right))
            .on_action(cx.listener(Self::on_editor_home))
            .on_action(cx.listener(Self::on_editor_end))
            .on_action(cx.listener(Self::on_editor_delete))
            .on_action(cx.listener(Self::on_editor_escape))
            .on_action(cx.listener(Self::on_editor_tab))
            .when(is_edit_mode, |el| {
                el.on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                    window.focus(&focus_handle, cx);
                })
            })
            .when(has_file, |el| {
                el.child(self.render_toolbar(cx)).child(
                    match self.mode {
                        FileViewMode::Content => self.render_content().into_any_element(),
                        FileViewMode::Diff => {
                            if self.diff_view_style == DiffViewStyle::SideBySide {
                                self.render_split_diff().into_any_element()
                            } else {
                                self.render_diff().into_any_element()
                            }
                        }
                        FileViewMode::Edit => self.render_editor().into_any_element(),
                    },
                )
            })
            .when(!has_file, |el| {
                el.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(TEXT_MUTED))
                        .child("Select a file to view"),
                )
            })
            // Error message toast
            .when(has_error, |el| {
                el.child(
                    div()
                        .id("file-viewer-error")
                        .absolute()
                        .bottom_4()
                        .left_4()
                        .right_4()
                        .px_4()
                        .py_2()
                        .bg(rgb(RED))
                        .text_color(rgb(BG_BASE))
                        .rounded_md()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_sm()
                                .child(error_msg),
                        )
                        .child(
                            div()
                                .id("dismiss-file-error")
                                .cursor_pointer()
                                .px_2()
                                .text_sm()
                                .hover(|e| e.text_color(rgb(ROSEWATER)))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.dismiss_error();
                                    cx.notify();
                                }))
                                .child("×"),
                        ),
                )
            })
            .into_any_element();

        FileViewerElement {
            view: cx.entity(),
            content,
        }
    }
}
