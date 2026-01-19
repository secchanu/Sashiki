//! File view component for viewing files and diffs

use crate::theme::*;
use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, IntoElement, MouseButton, ParentElement,
    Render, ScrollHandle, Styled, Window, div, prelude::*, rgb,
};
use std::path::PathBuf;
use std::rc::Rc;

/// Event to send text to terminal
#[derive(Debug, Clone)]
pub struct SendToTerminalEvent(pub String);

/// View mode for the file view
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileViewMode {
    /// Display file content as-is
    #[default]
    Content,
    /// Display diff in split view (old on left, new on right)
    DiffSplit,
    /// Display diff inline (additions/deletions marked in content)
    DiffInline,
}

/// Diff line for split view (side-by-side display)
#[derive(Debug, Clone)]
struct SplitDiffLine {
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
}

/// Line info for inline diff view
#[derive(Debug, Clone)]
struct InlineDiffLine {
    line_num: Option<usize>,
    content: String,
    change_type: InlineChangeType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineChangeType {
    Unchanged,
    Added,
    Deleted,
}

/// File view component - read-only viewer
pub struct FileView {
    file_path: Option<PathBuf>,
    content: String,
    diff_content: Option<String>,
    mode: FileViewMode,
    focus_handle: FocusHandle,
    /// Rc-wrapped for cheap clones during render
    cached_added_lines: Rc<std::collections::HashSet<usize>>,
    /// Rc-wrapped for cheap clones during render (Before/left side)
    cached_left_lines: Rc<Vec<SplitDiffLine>>,
    /// Rc-wrapped for cheap clones during render (After/right side)
    cached_right_lines: Rc<Vec<SplitDiffLine>>,
    /// Shared scroll handle for synchronized split diff scrolling
    diff_scroll_handle: ScrollHandle,
}

impl FileView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            file_path: None,
            content: String::new(),
            diff_content: None,
            mode: FileViewMode::Content,
            focus_handle: cx.focus_handle(),
            cached_added_lines: Rc::new(std::collections::HashSet::new()),
            cached_left_lines: Rc::new(Vec::new()),
            cached_right_lines: Rc::new(Vec::new()),
            diff_scroll_handle: ScrollHandle::new(),
        }
    }

    pub fn open_file(&mut self, path: PathBuf) -> Result<(), std::io::Error> {
        self.content = std::fs::read_to_string(&path)?;
        self.file_path = Some(path);
        self.diff_content = None;
        self.mode = FileViewMode::Content;
        self.clear_diff_cache();
        Ok(())
    }

    pub fn open_file_with_diff(
        &mut self,
        path: PathBuf,
        diff: String,
    ) -> Result<(), std::io::Error> {
        self.content = std::fs::read_to_string(&path)?;
        self.file_path = Some(path);
        self.diff_content = Some(diff);
        self.mode = FileViewMode::DiffSplit;
        self.update_diff_cache();
        Ok(())
    }

    pub fn open_deleted_file_with_diff(&mut self, path: PathBuf, diff: String) {
        self.file_path = Some(path);
        self.content = String::new();
        self.diff_content = Some(diff);
        self.mode = FileViewMode::DiffSplit;
        self.update_diff_cache();
    }

    fn clear_diff_cache(&mut self) {
        self.cached_added_lines = Rc::new(std::collections::HashSet::new());
        self.cached_left_lines = Rc::new(Vec::new());
        self.cached_right_lines = Rc::new(Vec::new());
    }

    fn update_diff_cache(&mut self) {
        self.cached_added_lines = Rc::new(self.compute_added_line_numbers());
        let (left, right) = self.compute_split_diff();
        self.cached_left_lines = Rc::new(left);
        self.cached_right_lines = Rc::new(right);
    }

    /// Toggle between DiffSplit and DiffInline modes (only when viewing diff)
    pub fn toggle_diff_display_mode(&mut self) {
        self.mode = match self.mode {
            FileViewMode::DiffSplit => FileViewMode::DiffInline,
            FileViewMode::DiffInline => FileViewMode::DiffSplit,
            FileViewMode::Content => FileViewMode::Content,
        };
    }

    /// Check if currently in a diff mode
    pub fn is_diff_mode(&self) -> bool {
        matches!(
            self.mode,
            FileViewMode::DiffSplit | FileViewMode::DiffInline
        )
    }

    pub fn close(&mut self) {
        self.file_path = None;
        self.content.clear();
        self.diff_content = None;
        self.mode = FileViewMode::Content;
        self.clear_diff_cache();
    }

    /// Parse diff to create inline view lines.
    ///
    /// Algorithm:
    /// 1. First pass: scan diff to identify added lines and their positions,
    ///    and collect deleted lines with their insertion points
    /// 2. Second pass: iterate through file content, inserting deleted lines
    ///    at their original positions and marking added lines
    fn parse_diff_for_inline_view(&self) -> Vec<InlineDiffLine> {
        let diff = self.diff_content.as_deref().unwrap_or("");
        let content_lines: Vec<&str> = self.content.lines().collect();
        let mut result: Vec<InlineDiffLine> = Vec::new();

        let mut added_lines: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let mut deleted_at: Vec<(usize, String)> = Vec::new();
        let mut new_line_num = 1usize;

        for line in diff.lines() {
            if line.starts_with("@@") {
                if let Some((_, new_start)) = Self::parse_hunk_header(line) {
                    new_line_num = new_start;
                }
            } else if line.starts_with("---")
                || line.starts_with("+++")
                || line.starts_with("diff ")
            {
            } else if line.starts_with('+') && !line.starts_with("+++") {
                added_lines.insert(new_line_num);
                new_line_num += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
                let content = line.strip_prefix('-').unwrap_or(line);
                deleted_at.push((new_line_num, content.to_string()));
            } else if line.starts_with(' ') || (!line.starts_with('@') && !line.is_empty()) {
                new_line_num += 1;
            }
        }

        let mut deleted_idx = 0;
        for (i, content_line) in content_lines.iter().enumerate() {
            let line_num = i + 1;

            while deleted_idx < deleted_at.len() && deleted_at[deleted_idx].0 == line_num {
                result.push(InlineDiffLine {
                    line_num: None,
                    content: deleted_at[deleted_idx].1.clone(),
                    change_type: InlineChangeType::Deleted,
                });
                deleted_idx += 1;
            }

            let change_type = if added_lines.contains(&line_num) {
                InlineChangeType::Added
            } else {
                InlineChangeType::Unchanged
            };

            result.push(InlineDiffLine {
                line_num: Some(line_num),
                content: content_line.to_string(),
                change_type,
            });
        }

        while deleted_idx < deleted_at.len() {
            result.push(InlineDiffLine {
                line_num: None,
                content: deleted_at[deleted_idx].1.clone(),
                change_type: InlineChangeType::Deleted,
            });
            deleted_idx += 1;
        }

        result
    }

    fn compute_split_diff(&self) -> (Vec<SplitDiffLine>, Vec<SplitDiffLine>) {
        let diff = self.diff_content.as_deref().unwrap_or("");
        let mut left_lines: Vec<SplitDiffLine> = Vec::new();
        let mut right_lines: Vec<SplitDiffLine> = Vec::new();

        // If diff is empty or has no actual changes, show file content as context
        let has_changes = diff.lines().any(|line| {
            line.starts_with('+') && !line.starts_with("+++")
                || line.starts_with('-') && !line.starts_with("---")
        });

        if !has_changes {
            for (i, line) in self.content.lines().enumerate() {
                let line_num = i + 1;
                let parsed = SplitDiffLine {
                    old_line_num: Some(line_num),
                    new_line_num: Some(line_num),
                    content: line.to_string(),
                    line_type: DiffLineType::Context,
                };
                left_lines.push(parsed.clone());
                right_lines.push(parsed);
            }
            return (left_lines, right_lines);
        }

        let mut old_line_num = 1usize;
        let mut new_line_num = 1usize;

        for line in diff.lines() {
            if line.starts_with("@@") {
                // Parse hunk header to update line numbers, but don't display it
                if let Some((old_start, new_start)) = Self::parse_hunk_header(line) {
                    old_line_num = old_start;
                    new_line_num = new_start;
                }
            } else if line.starts_with("---")
                || line.starts_with("+++")
                || line.starts_with("diff ")
            {
                // Skip diff metadata headers
            } else if let Some(stripped) = line.strip_prefix('+') {
                left_lines.push(SplitDiffLine {
                    old_line_num: None,
                    new_line_num: None,
                    content: String::new(),
                    line_type: DiffLineType::Added,
                });
                right_lines.push(SplitDiffLine {
                    old_line_num: None,
                    new_line_num: Some(new_line_num),
                    content: stripped.to_string(),
                    line_type: DiffLineType::Added,
                });
                new_line_num += 1;
            } else if let Some(stripped) = line.strip_prefix('-') {
                left_lines.push(SplitDiffLine {
                    old_line_num: Some(old_line_num),
                    new_line_num: None,
                    content: stripped.to_string(),
                    line_type: DiffLineType::Removed,
                });
                right_lines.push(SplitDiffLine {
                    old_line_num: None,
                    new_line_num: None,
                    content: String::new(),
                    line_type: DiffLineType::Removed,
                });
                old_line_num += 1;
            } else if line.starts_with(' ') || line.is_empty() {
                let content = if line.is_empty() { "" } else { &line[1..] };
                left_lines.push(SplitDiffLine {
                    old_line_num: Some(old_line_num),
                    new_line_num: None,
                    content: content.to_string(),
                    line_type: DiffLineType::Context,
                });
                right_lines.push(SplitDiffLine {
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

    pub(crate) fn parse_hunk_header(line: &str) -> Option<(usize, usize)> {
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

    fn compute_added_line_numbers(&self) -> std::collections::HashSet<usize> {
        let diff = self.diff_content.as_deref().unwrap_or("");
        let mut added_lines = std::collections::HashSet::new();
        let mut new_line_num = 1usize;

        for line in diff.lines() {
            if line.starts_with("@@") {
                if let Some((_, new_start)) = Self::parse_hunk_header(line) {
                    new_line_num = new_start;
                }
            } else if line.starts_with('+') && !line.starts_with("+++") {
                added_lines.insert(new_line_num);
                new_line_num += 1;
            } else if line.starts_with('-') && !line.starts_with("---") {
            } else if line.starts_with(' ') || (!line.starts_with('@') && !line.is_empty()) {
                new_line_num += 1;
            }
        }

        added_lines
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let file_name = self
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("No file");

        let mode = self.mode;
        let has_diff = self.diff_content.is_some();

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
                    .text_sm()
                    .text_color(rgb(TEXT))
                    .child(file_name.to_string()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .when(has_diff && self.is_diff_mode(), |el| {
                        el.child(
                            div()
                                .id("toggle-diff-display")
                                .px_2()
                                .py_1()
                                .cursor_pointer()
                                .rounded_sm()
                                .bg(rgb(BG_SURFACE0))
                                .hover(|d| d.bg(rgb(BG_SURFACE1)))
                                .text_xs()
                                .text_color(rgb(MAUVE))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_diff_display_mode();
                                    cx.notify();
                                }))
                                .child(if mode == FileViewMode::DiffSplit {
                                    "Inline"
                                } else {
                                    "Split"
                                }),
                        )
                    })
                    .child(
                        div()
                            .id("close-file")
                            .px_2()
                            .py_1()
                            .cursor_pointer()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .hover(|el| el.text_color(rgb(RED)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close();
                                cx.notify();
                            }))
                            .child("Close"),
                    ),
            )
    }

    fn render_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let lines: Vec<String> = self.content.lines().map(|s| s.to_string()).collect();
        let file_path = self.file_path.clone();

        div()
            .id("file-content-scroll")
            .flex_1()
            .overflow_y_scroll()
            .bg(rgb(BG_BASE))
            .p_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .font_family(MONOSPACE_FONT)
                    .text_sm()
                    .children(lines.into_iter().enumerate().map(|(num, line)| {
                        let line_num = num + 1;
                        let path_for_click = file_path.clone();

                        div()
                            .flex()
                            .child(
                                div()
                                    .id(("content-line", line_num))
                                    .w_12()
                                    .flex_shrink_0()
                                    .text_right()
                                    .pr_2()
                                    .text_color(rgb(TEXT_MUTED))
                                    .cursor_pointer()
                                    .hover(|el| el.text_color(rgb(BLUE)))
                                    .on_mouse_down(
                                        MouseButton::Right,
                                        cx.listener(move |_this, _, _, cx| {
                                            if let Some(ref path) = path_for_click {
                                                let text = format!(
                                                    "`{}:{}`",
                                                    path.to_string_lossy(),
                                                    line_num
                                                );
                                                cx.emit(SendToTerminalEvent(text));
                                            }
                                        }),
                                    )
                                    .child(format!("{}", line_num)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_color(rgb(TEXT))
                                    .child(if line.is_empty() {
                                        " ".to_string()
                                    } else {
                                        line
                                    }),
                            )
                    })),
            )
    }

    fn render_inline_diff(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let lines = self.parse_diff_for_inline_view();
        let file_path = self.file_path.clone();

        div()
            .id("inline-diff-scroll")
            .flex_1()
            .overflow_y_scroll()
            .bg(rgb(BG_BASE))
            .p_2()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .font_family(MONOSPACE_FONT)
                    .text_sm()
                    .children(lines.into_iter().enumerate().map(|(idx, line)| {
                        let (bg_color, text_color, opacity) = match line.change_type {
                            InlineChangeType::Added => (Some(rgb(DIFF_ADDED_BG)), rgb(GREEN), 1.0),
                            InlineChangeType::Deleted => {
                                (Some(rgb(DIFF_REMOVED_BG)), rgb(RED), 0.6)
                            }
                            InlineChangeType::Unchanged => (None, rgb(TEXT), 1.0),
                        };

                        let line_num_str = line
                            .line_num
                            .map(|n| format!("{:>4}", n))
                            .unwrap_or_else(|| "    ".to_string());

                        let prefix = match line.change_type {
                            InlineChangeType::Added => "+",
                            InlineChangeType::Deleted => "-",
                            InlineChangeType::Unchanged => " ",
                        };

                        let path_for_click = file_path.clone();
                        let line_num_for_click = line.line_num;

                        div()
                            .flex()
                            .when_some(bg_color, |el, color| el.bg(color))
                            .opacity(opacity)
                            .child(
                                div()
                                    .id(("inline-diff-line", idx))
                                    .w_12()
                                    .flex_shrink_0()
                                    .text_right()
                                    .pr_2()
                                    .text_color(rgb(TEXT_MUTED))
                                    .when(line_num_for_click.is_some(), |el| {
                                        el.cursor_pointer().hover(|el| el.text_color(rgb(BLUE)))
                                    })
                                    .on_mouse_down(
                                        MouseButton::Right,
                                        cx.listener(move |_this, _, _, cx| {
                                            if let (Some(path), Some(num)) =
                                                (&path_for_click, line_num_for_click)
                                            {
                                                let text =
                                                    format!("`{}:{}`", path.to_string_lossy(), num);
                                                cx.emit(SendToTerminalEvent(text));
                                            }
                                        }),
                                    )
                                    .child(line_num_str),
                            )
                            .child(
                                div()
                                    .w_4()
                                    .flex_shrink_0()
                                    .text_color(text_color)
                                    .child(prefix),
                            )
                            .child(div().flex_1().text_color(text_color).child(
                                if line.content.is_empty() {
                                    " ".to_string()
                                } else {
                                    line.content
                                },
                            ))
                    })),
            )
    }

    fn render_diff(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        let left_lines = self.cached_left_lines.clone();
        let right_lines = self.cached_right_lines.clone();
        let scroll_handle = self.diff_scroll_handle.clone();

        // Pair up left and right lines for synchronized rendering
        let line_count = left_lines.len().max(right_lines.len());
        let line_pairs: Vec<_> = (0..line_count)
            .map(|i| (left_lines.get(i).cloned(), right_lines.get(i).cloned()))
            .collect();

        div()
            .flex_1()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(BG_BASE))
            // Fixed header row
            .child(
                div()
                    .h_6()
                    .flex_shrink_0()
                    .flex()
                    .flex_row()
                    .child(
                        div()
                            .w_1_2()
                            .min_w_0()
                            .px_2()
                            .flex()
                            .items_center()
                            .bg(rgb(BG_MANTLE))
                            .border_r_1()
                            .border_color(rgb(BG_SURFACE0))
                            .text_xs()
                            .text_color(rgb(RED))
                            .child("Before (HEAD)"),
                    )
                    .child(
                        div()
                            .w_1_2()
                            .min_w_0()
                            .px_2()
                            .flex()
                            .items_center()
                            .bg(rgb(BG_MANTLE))
                            .text_xs()
                            .text_color(rgb(GREEN))
                            .child("After (Working)"),
                    ),
            )
            // Synchronized scrolling content - row by row
            .child(
                div()
                    .id("diff-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .track_scroll(&scroll_handle)
                    .p_2()
                    .font_family(MONOSPACE_FONT)
                    .text_sm()
                    .children(
                        line_pairs
                            .into_iter()
                            .map(|(left, right)| Self::render_diff_row(left, right)),
                    ),
            )
    }

    fn render_diff_row(
        left: Option<SplitDiffLine>,
        right: Option<SplitDiffLine>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .child(Self::render_diff_half(left, true))
            .child(Self::render_diff_half(right, false))
    }

    fn render_diff_half(line: Option<SplitDiffLine>, is_left: bool) -> impl IntoElement {
        let empty_line = SplitDiffLine {
            old_line_num: None,
            new_line_num: None,
            content: String::new(),
            line_type: DiffLineType::Context,
        };
        let line = line.unwrap_or(empty_line);

        let (bg_color, text_color) = match line.line_type {
            DiffLineType::Added => (Some(rgb(DIFF_ADDED_BG)), rgb(GREEN)),
            DiffLineType::Removed => (Some(rgb(DIFF_REMOVED_BG)), rgb(RED)),
            DiffLineType::Context => (None, rgb(TEXT)),
        };

        let line_num = if is_left {
            line.old_line_num
        } else {
            line.new_line_num
        };

        let content = if line.content.is_empty() {
            " ".to_string()
        } else {
            line.content
        };

        let mut el = div().w_1_2().min_w_0().flex().flex_row();

        if is_left {
            el = el.border_r_1().border_color(rgb(BG_SURFACE0));
        }

        el.when_some(bg_color, |el, color| el.bg(color))
            .child(
                div()
                    .w_10()
                    .flex_shrink_0()
                    .text_right()
                    .pr_2()
                    .text_color(rgb(TEXT_MUTED))
                    .child(line_num.map(|n| n.to_string()).unwrap_or_default()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_color(text_color)
                    .child(content),
            )
    }
}

impl Focusable for FileView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<SendToTerminalEvent> for FileView {}

impl Render for FileView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_file = self.file_path.is_some();

        let content_element = if has_file {
            match self.mode {
                FileViewMode::Content => self.render_content(cx).into_any_element(),
                FileViewMode::DiffSplit => self.render_diff(cx).into_any_element(),
                FileViewMode::DiffInline => self.render_inline_diff(cx).into_any_element(),
            }
        } else {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(TEXT_MUTED))
                .child("Select a file to view")
                .into_any_element()
        };

        div()
            .id("file-view")
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(BG_BASE))
            .when(has_file, |el| el.child(self.render_toolbar(cx)))
            .child(content_element)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== FileViewMode tests =====

    #[test]
    fn test_file_view_mode_default() {
        let mode = FileViewMode::default();
        assert_eq!(mode, FileViewMode::Content);
    }

    // ===== parse_hunk_header tests =====

    #[test]
    fn test_parse_hunk_header_basic() {
        let result = FileView::parse_hunk_header("@@ -1 +1 @@");
        assert_eq!(result, Some((1, 1)));
    }

    #[test]
    fn test_parse_hunk_header_with_count() {
        let result = FileView::parse_hunk_header("@@ -10,5 +20,7 @@ function name");
        assert_eq!(result, Some((10, 20)));
    }

    #[test]
    fn test_parse_hunk_header_larger_numbers() {
        let result = FileView::parse_hunk_header("@@ -100,50 +200,60 @@");
        assert_eq!(result, Some((100, 200)));
    }

    #[test]
    fn test_parse_hunk_header_invalid_missing_parts() {
        let result = FileView::parse_hunk_header("@@ -1 @@");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_hunk_header_invalid_not_hunk() {
        let result = FileView::parse_hunk_header("diff --git a/file b/file");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_hunk_header_invalid_non_numeric() {
        let result = FileView::parse_hunk_header("@@ -abc +def @@");
        assert_eq!(result, None);
    }

    // ===== DiffLineType tests =====

    #[test]
    fn test_diff_line_type_equality() {
        assert_eq!(DiffLineType::Context, DiffLineType::Context);
        assert_ne!(DiffLineType::Added, DiffLineType::Removed);
    }

    // ===== InlineChangeType tests =====

    #[test]
    fn test_inline_change_type_equality() {
        assert_eq!(InlineChangeType::Unchanged, InlineChangeType::Unchanged);
        assert_ne!(InlineChangeType::Added, InlineChangeType::Deleted);
    }

    // ===== Integration-style tests (using struct directly) =====

    /// Helper to create a FileView-like struct for testing diff parsing
    struct DiffTestHelper {
        #[allow(dead_code)]
        content: String,
        diff_content: Option<String>,
    }

    impl DiffTestHelper {
        fn new(content: &str, diff: &str) -> Self {
            Self {
                content: content.to_string(),
                diff_content: Some(diff.to_string()),
            }
        }

        fn compute_added_line_numbers(&self) -> std::collections::HashSet<usize> {
            let diff = self.diff_content.as_deref().unwrap_or("");
            let mut added_lines = std::collections::HashSet::new();
            let mut new_line_num = 1usize;

            for line in diff.lines() {
                if line.starts_with("@@") {
                    if let Some((_, new_start)) = FileView::parse_hunk_header(line) {
                        new_line_num = new_start;
                    }
                } else if line.starts_with('+') && !line.starts_with("+++") {
                    added_lines.insert(new_line_num);
                    new_line_num += 1;
                } else if line.starts_with('-') && !line.starts_with("---") {
                    // Deleted lines don't advance new_line_num
                } else if line.starts_with(' ') || (!line.starts_with('@') && !line.is_empty()) {
                    new_line_num += 1;
                }
            }

            added_lines
        }
    }

    #[test]
    fn test_compute_added_line_numbers_simple_add() {
        let helper = DiffTestHelper::new(
            "line1\nline2\nline3",
            "@@ -1,2 +1,3 @@\n line1\n+line2\n line3",
        );

        let added = helper.compute_added_line_numbers();

        assert!(added.contains(&2));
        assert!(!added.contains(&1));
        assert!(!added.contains(&3));
    }

    #[test]
    fn test_compute_added_line_numbers_multiple_adds() {
        let helper = DiffTestHelper::new("a\nb\nc\nd", "@@ -1,2 +1,4 @@\n a\n+b\n+c\n d");

        let added = helper.compute_added_line_numbers();

        assert!(!added.contains(&1));
        assert!(added.contains(&2));
        assert!(added.contains(&3));
        assert!(!added.contains(&4));
    }

    #[test]
    fn test_compute_added_line_numbers_with_deletion() {
        let helper = DiffTestHelper::new("new_line", "@@ -1,1 +1,1 @@\n-old_line\n+new_line");

        let added = helper.compute_added_line_numbers();

        assert!(added.contains(&1));
    }

    #[test]
    fn test_compute_added_line_numbers_empty_diff() {
        let helper = DiffTestHelper {
            content: "unchanged".to_string(),
            diff_content: Some("".to_string()),
        };

        let added = helper.compute_added_line_numbers();

        assert!(added.is_empty());
    }

    #[test]
    fn test_compute_added_line_numbers_no_additions() {
        let helper = DiffTestHelper::new("remaining", "@@ -1,2 +1,1 @@\n-deleted\n remaining");

        let added = helper.compute_added_line_numbers();

        assert!(added.is_empty());
    }

    #[test]
    fn test_compute_added_line_numbers_multiple_hunks() {
        let helper = DiffTestHelper::new(
            "a\nb\nc\nd\ne\nf",
            "@@ -1,2 +1,3 @@\n a\n+b\n c\n@@ -4,2 +5,3 @@\n d\n+e\n f",
        );

        let added = helper.compute_added_line_numbers();

        assert!(added.contains(&2));
        assert!(added.contains(&6));
    }
}
