//! File view component for viewing files and diffs

use crate::git::ChangeType;
use crate::highlight::{HighlightedDoc, HighlightedLine};
use crate::theme::*;
use crate::ui::ChangeSection;
use gpui::{
    AnyElement, App, Context, DefiniteLength, EventEmitter, FocusHandle, Focusable,
    InteractiveText, IntoElement, MouseButton, ParentElement, Render, ScrollHandle,
    ScrollStrategy, Styled, StyledText, UniformListScrollHandle, Window, div, prelude::*, px, rgb,
    uniform_list,
};
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

/// Event to send text to terminal
#[derive(Debug, Clone)]
pub struct SendToTerminalEvent(pub String);

/// Event to request go-to-definition from a highlighted token click
#[derive(Debug, Clone)]
pub struct GotoDefinitionEvent {
    pub file_path: PathBuf,
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum StageSelectionKind {
    HunkAtLine(usize),
    LineRange { start: usize, end: usize },
}

#[derive(Debug, Clone, Copy)]
pub enum SelectionAction {
    Stage,
    Unstage,
    Discard,
}

/// Event to request staging a selected hunk or line range.
#[derive(Debug, Clone)]
pub struct StageSelectionEvent {
    pub file_path: PathBuf,
    pub section: ChangeSection,
    pub action: SelectionAction,
    pub kind: StageSelectionKind,
}

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
    old_line_num: Option<usize>,
    content: String,
    change_type: InlineChangeType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineChangeType {
    Unchanged,
    Added,
    Deleted,
}

struct DiffResizeDrag {
    start_x: f32,
    initial_ratio: f32,
}

/// File view component - read-only viewer
pub struct FileView {
    file_path: Option<PathBuf>,
    content: String,
    diff_content: Option<String>,
    is_binary: bool,
    highlight_content: Option<Arc<HighlightedDoc>>,
    highlight_old: Option<Arc<HighlightedDoc>>,
    highlight_new: Option<Arc<HighlightedDoc>>,
    mode: FileViewMode,
    focus_handle: FocusHandle,
    /// Rc-wrapped for cheap clones during render
    cached_added_lines: Rc<std::collections::HashSet<usize>>,
    /// Rc-wrapped hunk start line numbers (new-file side, 1-based)
    cached_hunk_start_lines: Rc<std::collections::HashSet<usize>>,
    /// Rc-wrapped hunk ranges (new-file side, 1-based inclusive)
    cached_hunk_ranges: Rc<Vec<(usize, usize)>>,
    /// Rc-wrapped for cheap clones during render (Before/left side)
    cached_left_lines: Rc<Vec<SplitDiffLine>>,
    /// Rc-wrapped for cheap clones during render (After/right side)
    cached_right_lines: Rc<Vec<SplitDiffLine>>,
    /// Rc-wrapped inline diff lines (recomputed only when diff changes)
    cached_inline_lines: Rc<Vec<InlineDiffLine>>,
    /// Shared scroll handle for synchronized split diff scrolling (uniform_list)
    diff_scroll_handle: UniformListScrollHandle,
    /// Scroll handle for content view (enables scroll_to_item for go-to-definition)
    content_scroll_handle: ScrollHandle,
    /// Scroll handle for inline diff view (uniform_list)
    inline_scroll_handle: UniformListScrollHandle,
    /// Target line to scroll to after opening a file (1-based)
    target_line: Option<usize>,
    current_change_section: Option<ChangeSection>,
    /// Change type of the currently displayed file (used to suppress hunk buttons for untracked files)
    current_change_type: Option<ChangeType>,
    /// Current hovered line number in diff views (new-file side, 1-based)
    hovered_line: Option<usize>,
    /// Selection anchor/focus line numbers for range staging in diff views (1-based).
    selected_line_anchor: Option<usize>,
    selected_line_focus: Option<usize>,
    diff_split_ratio: f32,
    diff_resize_drag: Option<DiffResizeDrag>,
}

impl FileView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            file_path: None,
            content: String::new(),
            diff_content: None,
            is_binary: false,
            highlight_content: None,
            highlight_old: None,
            highlight_new: None,
            mode: FileViewMode::Content,
            focus_handle: cx.focus_handle(),
            cached_added_lines: Rc::new(std::collections::HashSet::new()),
            cached_hunk_start_lines: Rc::new(std::collections::HashSet::new()),
            cached_hunk_ranges: Rc::new(Vec::new()),
            cached_left_lines: Rc::new(Vec::new()),
            cached_right_lines: Rc::new(Vec::new()),
            cached_inline_lines: Rc::new(Vec::new()),
            diff_scroll_handle: UniformListScrollHandle::new(),
            content_scroll_handle: ScrollHandle::new(),
            inline_scroll_handle: UniformListScrollHandle::new(),
            target_line: None,
            current_change_section: None,
            current_change_type: None,
            hovered_line: None,
            selected_line_anchor: None,
            selected_line_focus: None,
            diff_split_ratio: 0.5,
            diff_resize_drag: None,
        }
    }

    pub fn mode(&self) -> FileViewMode {
        self.mode
    }

    pub fn set_change_section(&mut self, section: Option<ChangeSection>) {
        self.current_change_section = section;
    }

    pub fn set_change_type(&mut self, change_type: Option<ChangeType>) {
        self.current_change_type = change_type;
    }

    pub fn set_target_line(&mut self, line: usize) {
        // DiffSplitはhunk周辺のみ表示するため、対象行が範囲外ならContentへフォールバック。
        // DiffInlineはファイル全行を表示するのでフォールバック不要。
        if self.mode == FileViewMode::DiffSplit {
            let in_diff = self
                .cached_right_lines
                .iter()
                .any(|l| l.new_line_num == Some(line));
            if !in_diff {
                self.mode = FileViewMode::Content;
            }
        }
        self.target_line = Some(line);
    }

    /// コードジャンプ時にジャンプ元のモードを復元する。
    /// open系メソッドが設定したモードを上書きし、diff表示に必要な状態を整える。
    pub fn restore_mode(&mut self, mode: FileViewMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        match mode {
            FileViewMode::DiffSplit | FileViewMode::DiffInline => {
                if self.diff_content.is_none() {
                    self.diff_content = Some(String::new());
                }
                self.update_diff_cache();
            }
            FileViewMode::Content => {
                // Content modeではdiff cacheは不要
            }
        }
    }

    pub fn open_file(&mut self, path: PathBuf) -> Result<(), std::io::Error> {
        self.content = std::fs::read_to_string(&path)?;
        self.file_path = Some(path);
        self.diff_content = None;
        self.mode = FileViewMode::Content;
        self.clear_highlight_state();
        self.clear_diff_cache();
        Ok(())
    }

    /// Open a file with pre-read content (avoids double file read)
    pub fn open_with_content(&mut self, path: PathBuf, content: String) {
        self.content = content;
        self.file_path = Some(path);
        self.diff_content = None;
        self.mode = FileViewMode::Content;
        self.clear_highlight_state();
        self.clear_diff_cache();
    }

    pub fn open_file_with_diff(
        &mut self,
        path: PathBuf,
        diff: String,
    ) -> Result<(), std::io::Error> {
        self.content = std::fs::read_to_string(&path)?;
        self.file_path = Some(path);
        self.diff_content = Some(diff);
        self.clear_highlight_state();
        if !self.is_diff_mode() {
            self.mode = FileViewMode::DiffSplit;
        }
        self.update_diff_cache();
        Ok(())
    }

    /// Open a file with pre-read content and diff (avoids double file read)
    pub fn open_with_diff_and_content(&mut self, path: PathBuf, content: String, diff: String) {
        self.content = content;
        self.file_path = Some(path);
        self.diff_content = Some(diff);
        self.clear_highlight_state();
        if !self.is_diff_mode() {
            self.mode = FileViewMode::DiffSplit;
        }
        self.update_diff_cache();
    }

    pub fn open_deleted_file_with_diff(&mut self, path: PathBuf, diff: String) {
        self.file_path = Some(path);
        self.content = String::new();
        self.diff_content = Some(diff);
        self.clear_highlight_state();
        if !self.is_diff_mode() {
            self.mode = FileViewMode::DiffSplit;
        }
        self.update_diff_cache();
    }

    pub fn set_highlights(
        &mut self,
        content: Option<Arc<HighlightedDoc>>,
        old: Option<Arc<HighlightedDoc>>,
        new: Option<Arc<HighlightedDoc>>,
    ) {
        self.highlight_content = content;
        self.highlight_old = old;
        self.highlight_new = new;
    }

    /// Open a file that is detected as binary (skip content reading)
    pub fn open_binary(&mut self, path: PathBuf) {
        self.file_path = Some(path);
        self.content.clear();
        self.diff_content = None;
        self.mode = FileViewMode::Content;
        self.is_binary = true;
        self.highlight_content = None;
        self.highlight_old = None;
        self.highlight_new = None;
        self.clear_diff_cache();
    }

    fn clear_highlight_state(&mut self) {
        self.is_binary = false;
        self.highlight_content = None;
        self.highlight_old = None;
        self.highlight_new = None;
        self.target_line = None;
        self.hovered_line = None;
        self.clear_line_selection();
    }

    fn clear_diff_cache(&mut self) {
        self.cached_added_lines = Rc::new(std::collections::HashSet::new());
        self.cached_hunk_start_lines = Rc::new(std::collections::HashSet::new());
        self.cached_hunk_ranges = Rc::new(Vec::new());
        self.cached_left_lines = Rc::new(Vec::new());
        self.cached_right_lines = Rc::new(Vec::new());
        self.cached_inline_lines = Rc::new(Vec::new());
    }

    fn update_diff_cache(&mut self) {
        self.cached_added_lines = Rc::new(self.compute_added_line_numbers());
        self.cached_hunk_start_lines = Rc::new(self.compute_hunk_start_lines());
        self.cached_hunk_ranges = Rc::new(self.compute_hunk_ranges());
        let (left, right) = self.compute_split_diff();
        self.cached_left_lines = Rc::new(left);
        self.cached_right_lines = Rc::new(right);
        self.cached_inline_lines = Rc::new(self.parse_diff_for_inline_view());
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

    fn clear_line_selection(&mut self) {
        self.selected_line_anchor = None;
        self.selected_line_focus = None;
    }

    fn select_diff_line(&mut self, line: usize, extend_selection: bool) {
        if extend_selection {
            if self.selected_line_anchor.is_none() {
                self.selected_line_anchor = Some(line);
            }
            self.selected_line_focus = Some(line);
        } else {
            self.selected_line_anchor = Some(line);
            self.selected_line_focus = Some(line);
        }
    }

    fn selected_line_range(&self) -> Option<(usize, usize)> {
        let anchor = self.selected_line_anchor?;
        let focus = self.selected_line_focus.unwrap_or(anchor);
        Some(if anchor <= focus {
            (anchor, focus)
        } else {
            (focus, anchor)
        })
    }

    fn selected_focus_line(&self) -> Option<usize> {
        self.selected_line_focus.or(self.selected_line_anchor)
    }

    fn is_line_selected(&self, line: usize) -> bool {
        self.selected_line_range()
            .is_some_and(|(start, end)| line >= start && line <= end)
    }

    fn request_stage_hunk(&mut self, line: usize, cx: &mut Context<Self>) {
        if let (Some(path), Some(section)) = (self.file_path.clone(), self.current_change_section) {
            cx.emit(StageSelectionEvent {
                file_path: path,
                section,
                action: SelectionAction::Stage,
                kind: StageSelectionKind::HunkAtLine(line),
            });
        }
    }

    fn request_stage_range(&mut self, start: usize, end: usize, cx: &mut Context<Self>) {
        if let (Some(path), Some(section)) = (self.file_path.clone(), self.current_change_section) {
            cx.emit(StageSelectionEvent {
                file_path: path,
                section,
                action: SelectionAction::Stage,
                kind: StageSelectionKind::LineRange { start, end },
            });
        }
    }

    fn request_unstage_hunk(&mut self, line: usize, cx: &mut Context<Self>) {
        if let (Some(path), Some(section)) = (self.file_path.clone(), self.current_change_section) {
            cx.emit(StageSelectionEvent {
                file_path: path,
                section,
                action: SelectionAction::Unstage,
                kind: StageSelectionKind::HunkAtLine(line),
            });
        }
    }

    fn request_unstage_range(&mut self, start: usize, end: usize, cx: &mut Context<Self>) {
        if let (Some(path), Some(section)) = (self.file_path.clone(), self.current_change_section) {
            cx.emit(StageSelectionEvent {
                file_path: path,
                section,
                action: SelectionAction::Unstage,
                kind: StageSelectionKind::LineRange { start, end },
            });
        }
    }

    fn request_discard_hunk(&mut self, line: usize, cx: &mut Context<Self>) {
        if let (Some(path), Some(section)) = (self.file_path.clone(), self.current_change_section) {
            cx.emit(StageSelectionEvent {
                file_path: path,
                section,
                action: SelectionAction::Discard,
                kind: StageSelectionKind::HunkAtLine(line),
            });
        }
    }

    fn request_discard_range(&mut self, start: usize, end: usize, cx: &mut Context<Self>) {
        if let (Some(path), Some(section)) = (self.file_path.clone(), self.current_change_section) {
            cx.emit(StageSelectionEvent {
                file_path: path,
                section,
                action: SelectionAction::Discard,
                kind: StageSelectionKind::LineRange { start, end },
            });
        }
    }

    pub fn close(&mut self) {
        self.file_path = None;
        self.content.clear();
        self.diff_content = None;
        self.current_change_section = None;
        self.current_change_type = None;
        self.clear_highlight_state();
        self.mode = FileViewMode::Content;
        self.clear_diff_cache();
        self.target_line = None;
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
        let mut deleted_at: Vec<(usize, usize, String)> = Vec::new();
        let mut old_line_num = 1usize;
        let mut new_line_num = 1usize;

        for line in diff.lines() {
            if line.starts_with("@@") {
                if let Some((old_start, new_start)) = Self::parse_hunk_header(line) {
                    old_line_num = old_start;
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
                deleted_at.push((new_line_num, old_line_num, content.to_string()));
                old_line_num += 1;
            } else if line.starts_with(' ') {
                old_line_num += 1;
                new_line_num += 1;
            }
            // Other lines (e.g. "\ No newline at end of file") are ignored
        }

        let mut deleted_idx = 0;
        for (i, content_line) in content_lines.iter().enumerate() {
            let line_num = i + 1;

            while deleted_idx < deleted_at.len() && deleted_at[deleted_idx].0 == line_num {
                result.push(InlineDiffLine {
                    line_num: None,
                    old_line_num: Some(deleted_at[deleted_idx].1),
                    content: deleted_at[deleted_idx].2.clone(),
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
                old_line_num: None,
                content: content_line.to_string(),
                change_type,
            });
        }

        while deleted_idx < deleted_at.len() {
            result.push(InlineDiffLine {
                line_num: None,
                old_line_num: Some(deleted_at[deleted_idx].1),
                content: deleted_at[deleted_idx].2.clone(),
                change_type: InlineChangeType::Deleted,
            });
            deleted_idx += 1;
        }

        result
    }

    fn compute_split_diff(&self) -> (Vec<SplitDiffLine>, Vec<SplitDiffLine>) {
        let diff = self.diff_content.as_deref().unwrap_or("");
        let content_lines: Vec<&str> = self.content.lines().collect();
        let mut left_lines: Vec<SplitDiffLine> = Vec::new();
        let mut right_lines: Vec<SplitDiffLine> = Vec::new();

        // Parse diff into hunks
        struct Hunk {
            new_start: usize,
            lines: Vec<(char, String)>,
        }

        let mut hunks: Vec<Hunk> = Vec::new();
        let mut current_hunk: Option<Hunk> = None;

        for line in diff.lines() {
            if line.starts_with("@@") {
                if let Some(hunk) = current_hunk.take() {
                    hunks.push(hunk);
                }
                if let Some((_, new_start)) = Self::parse_hunk_header(line) {
                    current_hunk = Some(Hunk {
                        new_start,
                        lines: Vec::new(),
                    });
                }
            } else if line.starts_with("---")
                || line.starts_with("+++")
                || line.starts_with("diff ")
            {
                // Skip diff metadata
            } else if let Some(ref mut hunk) = current_hunk {
                if let Some(stripped) = line.strip_prefix('+') {
                    hunk.lines.push(('+', stripped.to_string()));
                } else if let Some(stripped) = line.strip_prefix('-') {
                    hunk.lines.push(('-', stripped.to_string()));
                } else if line.starts_with(' ') {
                    hunk.lines.push((' ', line[1..].to_string()));
                } else if line.is_empty() {
                    hunk.lines.push((' ', String::new()));
                }
            }
        }
        if let Some(hunk) = current_hunk.take() {
            hunks.push(hunk);
        }

        if hunks.is_empty() {
            for (i, line) in content_lines.iter().enumerate() {
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

        let mut new_cursor = 1usize;
        let mut old_cursor = 1usize;

        for hunk in &hunks {
            // Fill context lines before this hunk from file content
            while new_cursor < hunk.new_start {
                let content = content_lines
                    .get(new_cursor - 1)
                    .unwrap_or(&"")
                    .to_string();
                left_lines.push(SplitDiffLine {
                    old_line_num: Some(old_cursor),
                    new_line_num: None,
                    content: content.clone(),
                    line_type: DiffLineType::Context,
                });
                right_lines.push(SplitDiffLine {
                    old_line_num: None,
                    new_line_num: Some(new_cursor),
                    content,
                    line_type: DiffLineType::Context,
                });
                new_cursor += 1;
                old_cursor += 1;
            }

            // Process hunk lines
            for (type_char, content) in &hunk.lines {
                match type_char {
                    '+' => {
                        left_lines.push(SplitDiffLine {
                            old_line_num: None,
                            new_line_num: None,
                            content: String::new(),
                            line_type: DiffLineType::Added,
                        });
                        right_lines.push(SplitDiffLine {
                            old_line_num: None,
                            new_line_num: Some(new_cursor),
                            content: content.clone(),
                            line_type: DiffLineType::Added,
                        });
                        new_cursor += 1;
                    }
                    '-' => {
                        left_lines.push(SplitDiffLine {
                            old_line_num: Some(old_cursor),
                            new_line_num: None,
                            content: content.clone(),
                            line_type: DiffLineType::Removed,
                        });
                        right_lines.push(SplitDiffLine {
                            old_line_num: None,
                            new_line_num: None,
                            content: String::new(),
                            line_type: DiffLineType::Removed,
                        });
                        old_cursor += 1;
                    }
                    _ => {
                        left_lines.push(SplitDiffLine {
                            old_line_num: Some(old_cursor),
                            new_line_num: None,
                            content: content.clone(),
                            line_type: DiffLineType::Context,
                        });
                        right_lines.push(SplitDiffLine {
                            old_line_num: None,
                            new_line_num: Some(new_cursor),
                            content: content.clone(),
                            line_type: DiffLineType::Context,
                        });
                        new_cursor += 1;
                        old_cursor += 1;
                    }
                }
            }
        }

        // Fill context lines after the last hunk
        while new_cursor <= content_lines.len() {
            let content = content_lines
                .get(new_cursor - 1)
                .unwrap_or(&"")
                .to_string();
            left_lines.push(SplitDiffLine {
                old_line_num: Some(old_cursor),
                new_line_num: None,
                content: content.clone(),
                line_type: DiffLineType::Context,
            });
            right_lines.push(SplitDiffLine {
                old_line_num: None,
                new_line_num: Some(new_cursor),
                content,
                line_type: DiffLineType::Context,
            });
            new_cursor += 1;
            old_cursor += 1;
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

    fn parse_hunk_header_with_counts(line: &str) -> Option<(usize, usize, usize, usize)> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            return None;
        }

        let old_part = parts[1].trim_start_matches('-');
        let new_part = parts[2].trim_start_matches('+');

        let mut old_iter = old_part.split(',');
        let mut new_iter = new_part.split(',');

        let old_start = old_iter.next()?.parse().ok()?;
        let old_count = old_iter.next().and_then(|s| s.parse().ok()).unwrap_or(1);
        let new_start = new_iter.next()?.parse().ok()?;
        let new_count = new_iter.next().and_then(|s| s.parse().ok()).unwrap_or(1);

        Some((old_start, old_count, new_start, new_count))
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
            } else if line.starts_with(' ') {
                new_line_num += 1;
            }
            // Other lines (e.g. "\ No newline at end of file") are ignored
        }

        added_lines
    }

    fn compute_hunk_start_lines(&self) -> std::collections::HashSet<usize> {
        let diff = self.diff_content.as_deref().unwrap_or("");
        let mut starts = std::collections::HashSet::new();
        for line in diff.lines() {
            if line.starts_with("@@")
                && let Some((_, new_start)) = Self::parse_hunk_header(line)
            {
                starts.insert(new_start);
            }
        }
        starts
    }

    fn compute_hunk_ranges(&self) -> Vec<(usize, usize)> {
        let diff = self.diff_content.as_deref().unwrap_or("");
        let mut ranges = Vec::new();

        for line in diff.lines() {
            if line.starts_with("@@")
                && let Some((_, _, new_start, new_count)) =
                    Self::parse_hunk_header_with_counts(line)
            {
                let new_end = if new_count == 0 {
                    new_start
                } else {
                    new_start + new_count.saturating_sub(1)
                };
                ranges.push((new_start, new_end));
            }
        }

        ranges
    }

    fn hunk_idx_for_line(&self, line: usize) -> Option<usize> {
        self.cached_hunk_ranges
            .iter()
            .position(|(start, end)| line >= *start && line <= *end)
    }

    fn highlighted_line_for_number<'a>(
        doc: Option<&'a HighlightedDoc>,
        line_num: Option<usize>,
    ) -> Option<&'a HighlightedLine> {
        let index = line_num?.checked_sub(1)?;
        doc.and_then(|d| d.lines.get(index))
    }

    fn styled_text_from_highlighted_line(
        content: &str,
        highlighted_line: &HighlightedLine,
    ) -> StyledText {
        let text = if content.is_empty() {
            " ".to_string()
        } else {
            content.to_string()
        };
        let len = text.len();
        let highlights = highlighted_line.spans.iter().filter_map(|span| {
            if span.range.start < span.range.end && span.range.end <= len {
                Some((span.range.clone(), span.style))
            } else {
                None
            }
        });

        StyledText::new(text).with_highlights(highlights)
    }

    /// Build InteractiveText so highlighted token spans are clickable and emit go-to-definition.
    fn render_interactive_text(
        &self,
        line_num: usize,
        content: &str,
        highlighted_line: &HighlightedLine,
        id_prefix: &'static str,
        cx: &mut Context<Self>,
    ) -> InteractiveText {
        let styled = Self::styled_text_from_highlighted_line(content, highlighted_line);
        let clickable_ranges: Vec<std::ops::Range<usize>> = highlighted_line
            .spans
            .iter()
            .filter(|span| span.range.start < span.range.end && span.range.end <= content.len())
            .map(|span| span.range.clone())
            .collect();
        // LSP uses UTF-16 code unit offsets; convert from UTF-8 byte offsets
        let span_starts: Vec<u32> = highlighted_line
            .spans
            .iter()
            .filter(|span| span.range.start < span.range.end && span.range.end <= content.len())
            .map(|span| content[..span.range.start].encode_utf16().count() as u32)
            .collect();

        let entity = cx.entity();
        let file_path = self.file_path.clone();

        InteractiveText::new((id_prefix, line_num), styled).on_click(
            clickable_ranges,
            move |range_ix, _window, cx| {
                if let Some(ref path) = file_path {
                    let character = span_starts.get(range_ix).copied().unwrap_or(0);
                    entity.update(cx, |_view, cx| {
                        cx.emit(GotoDefinitionEvent {
                            file_path: path.clone(),
                            line: line_num.saturating_sub(1) as u32,
                            character,
                        });
                    });
                }
            },
        )
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

    fn render_content(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.is_binary {
            return div()
                .id("file-content-scroll")
                .flex_1()
                .overflow_y_scroll()
                .bg(rgb(BG_BASE))
                .p_2()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(TEXT_MUTED))
                        .child("Binary file"),
                );
        }

        let lines: Vec<String> = self.content.lines().map(|s| s.to_string()).collect();
        let file_path = self.file_path.clone();
        let highlight_content = self.highlight_content.as_deref();

        if let Some(line) = self.target_line.take() {
            if !lines.is_empty() {
                let ix = line.saturating_sub(1).min(lines.len() - 1);
                self.content_scroll_handle.scroll_to_top_of_item(ix);
            }
        }

        div()
            .id("file-content-scroll")
            .flex_1()
            .overflow_scroll()
            .track_scroll(&self.content_scroll_handle)
            .bg(rgb(BG_BASE))
            .flex()
            .flex_col()
            .font_family(MONOSPACE_FONT)
            .text_sm()
            .line_height(px(20.0))
            .p_2()
            .children(lines.into_iter().enumerate().map(|(num, line)| {
                let line_num = num + 1;
                let path_for_click = file_path.clone();

                div()
                    .flex()
                    .whitespace_nowrap()
                    .child(
                        div()
                            .id(("content-line", line_num))
                            .w_12()
                            .flex_shrink_0()
                            .text_right()
                            .pr_2()
                            .bg(rgb(BG_MANTLE))
                            .text_color(rgb(TEXT_MUTED))
                            .cursor_pointer()
                            .hover(|el| el.text_color(rgb(BLUE)))
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(move |_this, _, _, cx| {
                                    if let Some(ref path) = path_for_click {
                                        let text =
                                            format!("`{}:{}`", path.to_string_lossy(), line_num);
                                        cx.emit(SendToTerminalEvent(text));
                                    }
                                }),
                            )
                            .child(format!("{}", line_num)),
                    )
                    .child({
                        if let Some(highlighted_line) =
                            Self::highlighted_line_for_number(highlight_content, Some(line_num))
                        {
                            div().flex_1().pl_2().text_color(rgb(TEXT)).child(
                                self.render_interactive_text(
                                    line_num,
                                    &line,
                                    highlighted_line,
                                    "content-text",
                                    cx,
                                ),
                            )
                        } else {
                            div()
                                .flex_1()
                                .pl_2()
                                .text_color(rgb(TEXT))
                                .child(if line.is_empty() {
                                    " ".to_string()
                                } else {
                                    line
                                })
                        }
                    })
            }))
    }

    fn render_inline_diff(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let lines = self.cached_inline_lines.clone();

        if let Some(target) = self.target_line.take() {
            if let Some(ix) = lines.iter().position(|l| l.line_num == Some(target)) {
                self.inline_scroll_handle
                    .scroll_to_item(ix, ScrollStrategy::Top);
            }
        }

        let item_count = lines.len();

        uniform_list(
            "inline-diff-scroll",
            item_count,
            cx.processor(|this: &mut Self, range: Range<usize>, _window: &mut Window, cx: &mut Context<Self>| {
                let lines = &this.cached_inline_lines;
                let highlight_new = this.highlight_new.as_deref();
                let highlight_old = this.highlight_old.as_deref();

                range
                    .map(|idx| {
                        let line = &lines[idx];
                        let (bg_color, text_color, opacity) = match line.change_type {
                            InlineChangeType::Added => {
                                (Some(rgb(DIFF_ADDED_BG)), rgb(GREEN), 1.0)
                            }
                            InlineChangeType::Deleted => {
                                (Some(rgb(DIFF_REMOVED_BG)), rgb(RED), 0.6)
                            }
                            InlineChangeType::Unchanged => (None, rgb(TEXT), 1.0),
                        };
                        let prefix = match line.change_type {
                            InlineChangeType::Added => "+",
                            InlineChangeType::Deleted => "-",
                            InlineChangeType::Unchanged => " ",
                        };
                        let gutter_strip_color = match line.change_type {
                            InlineChangeType::Added => rgb(GREEN),
                            InlineChangeType::Deleted => rgb(RED),
                            InlineChangeType::Unchanged => rgb(BG_MANTLE),
                        };

                        // Build content element before calling build_diff_row
                        // (each borrows cx sequentially, not simultaneously)
                        let highlight_line = match line.change_type {
                            InlineChangeType::Deleted => {
                                Self::highlighted_line_for_number(
                                    highlight_old,
                                    line.old_line_num,
                                )
                            }
                            _ => {
                                Self::highlighted_line_for_number(
                                    highlight_new,
                                    line.line_num,
                                )
                            }
                        };
                        let content = if let Some(hl) = highlight_line {
                            if line.change_type == InlineChangeType::Deleted {
                                div().flex_1().pl_2().text_color(text_color).child(
                                    Self::styled_text_from_highlighted_line(
                                        &line.content, hl,
                                    ),
                                )
                            } else {
                                let ln = line.line_num.unwrap_or(0);
                                div().flex_1().pl_2().text_color(text_color).child(
                                    this.render_interactive_text(
                                        ln, &line.content, hl, "inline-diff", cx,
                                    ),
                                )
                            }
                        } else {
                            let text = if line.content.is_empty() {
                                " ".to_string()
                            } else {
                                line.content.clone()
                            };
                            div().flex_1().pl_2().text_color(text_color).child(text)
                        };

                        this.build_diff_row(
                            idx, line.line_num, bg_color, text_color,
                            gutter_strip_color, prefix, opacity, content, cx,
                        )
                    })
                    .collect()
            }),
        )
        .flex_1()
        .bg(rgb(BG_BASE))
        .font_family(MONOSPACE_FONT)
        .text_sm()
        .line_height(px(20.0))
        .p_2()
        .track_scroll(&self.inline_scroll_handle)
    }

    fn render_diff(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(line) = self.target_line.take() {
            if let Some(ix) = self
                .cached_right_lines
                .iter()
                .position(|l| l.new_line_num == Some(line))
            {
                self.diff_scroll_handle
                    .scroll_to_item(ix, ScrollStrategy::Top);
            }
        }

        let ratio = self.diff_split_ratio;
        let line_count = self.cached_left_lines.len();

        div()
            .id("diff-view")
            .flex_1()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(BG_BASE))
            .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                if this.diff_resize_drag.is_some() {
                    this.handle_diff_resize_move(f32::from(event.position.x));
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.diff_resize_drag.is_some() {
                        this.handle_diff_resize_end();
                        cx.notify();
                    }
                }),
            )
            // Header row
            .child(
                div()
                    .flex()
                    .flex_row()
                    .child(
                        div()
                            .w(DefiniteLength::Fraction(ratio))
                            .min_w_0()
                            .h_6()
                            .flex_shrink_0()
                            .px_2()
                            .flex()
                            .items_center()
                            .bg(rgb(BG_MANTLE))
                            .text_xs()
                            .text_color(rgb(RED))
                            .child("Before (HEAD)"),
                    )
                    .child(self.render_diff_resize_handle(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_6()
                            .flex_shrink_0()
                            .px_2()
                            .flex()
                            .items_center()
                            .bg(rgb(BG_MANTLE))
                            .text_xs()
                            .text_color(rgb(GREEN))
                            .child("After (Working)"),
                    ),
            )
            // Scrollable content with virtual scrolling
            .child(
                uniform_list(
                    "diff-scroll",
                    line_count,
                    cx.processor(
                        |this: &mut Self,
                         range: Range<usize>,
                         _window: &mut Window,
                         cx: &mut Context<Self>| {
                            let ratio = this.diff_split_ratio;
                            let highlight_old = this.highlight_old.as_deref();
                            let highlight_new = this.highlight_new.as_deref();

                            range
                                .map(|idx| {
                                    let left_line = &this.cached_left_lines[idx];
                                    let right_line = &this.cached_right_lines[idx];

                                    div()
                                        .w_full()
                                        .h(px(20.0))
                                        .relative()
                                        .child(
                                            div()
                                                .absolute()
                                                .left_0()
                                                .top_0()
                                                .bottom_0()
                                                .w(DefiniteLength::Fraction(ratio))
                                                .overflow_hidden()
                                                .pl_2()
                                                .child(
                                                    Self::render_highlighted_diff_line(
                                                        left_line,
                                                        true,
                                                        highlight_old,
                                                    ),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .absolute()
                                                .top_0()
                                                .bottom_0()
                                                .right_0()
                                                .w(DefiniteLength::Fraction(1.0 - ratio))
                                                .overflow_hidden()
                                                .pr_2()
                                                .child(
                                                    this.render_diff_line_right(
                                                        idx,
                                                        right_line,
                                                        highlight_new,
                                                        cx,
                                                    ),
                                                ),
                                        )
                                })
                                .collect()
                        },
                    ),
                )
                .flex_1()
                .font_family(MONOSPACE_FONT)
                .text_sm()
                .line_height(px(20.0))
                .track_scroll(&self.diff_scroll_handle),
            )
    }

    fn render_highlighted_diff_line(
        line: &SplitDiffLine,
        is_left: bool,
        highlight_doc: Option<&HighlightedDoc>,
    ) -> impl IntoElement {
        let (bg_color, text_color) = match line.line_type {
            DiffLineType::Added => (Some(rgb(DIFF_ADDED_BG)), rgb(GREEN)),
            DiffLineType::Removed => (Some(rgb(DIFF_REMOVED_BG)), rgb(RED)),
            DiffLineType::Context => (None, rgb(TEXT)),
        };

        let gutter_strip_color = match line.line_type {
            DiffLineType::Added => rgb(GREEN),
            DiffLineType::Removed => rgb(RED),
            DiffLineType::Context => rgb(BG_MANTLE),
        };

        let line_num = if is_left {
            line.old_line_num
        } else {
            line.new_line_num
        };

        let content = if line.content.is_empty() {
            " ".to_string()
        } else {
            line.content.clone()
        };
        let highlighted_line = Self::highlighted_line_for_number(highlight_doc, line_num);

        let prefix = match line.line_type {
            DiffLineType::Removed => "-",
            _ => " ",
        };

        div()
            .flex()
            .flex_row()
            .whitespace_nowrap()
            .when_some(bg_color, |el, color| el.bg(color))
            .child(div().w(px(3.0)).flex_shrink_0().bg(gutter_strip_color))
            .child(
                div()
                    .w(px(40.0))
                    .flex_shrink_0()
                    .text_right()
                    .pr_2()
                    .bg(rgb(BG_MANTLE))
                    .text_color(rgb(TEXT_MUTED))
                    .child(line_num.map(|n| n.to_string()).unwrap_or_default()),
            )
            // Spacers matching right side's stage/discard/prefix columns (16px × 3)
            .child(div().w(px(16.0)).flex_shrink_0())
            .child(div().w(px(16.0)).flex_shrink_0())
            .child(
                div()
                    .w(px(16.0))
                    .flex_shrink_0()
                    .text_center()
                    .text_color(text_color)
                    .child(prefix),
            )
            .child(if let Some(highlighted_line) = highlighted_line {
                div()
                    .flex_1()
                    .min_w_0()
                    .pl_2()
                    .text_color(text_color)
                    .child(Self::styled_text_from_highlighted_line(
                        &content,
                        highlighted_line,
                    ))
            } else {
                div()
                    .flex_1()
                    .min_w_0()
                    .pl_2()
                    .text_color(text_color)
                    .child(content)
            })
    }

    /// Build an interactive diff row with gutter strip, line number, stage/discard buttons,
    /// prefix symbol, and caller-provided content element.
    /// Used by both inline diff and split diff (right side) to avoid duplicating ~120 lines.
    fn build_diff_row(
        &self,
        idx: usize,
        line_num: Option<usize>,
        bg_color: Option<gpui::Rgba>,
        text_color: gpui::Rgba,
        gutter_strip_color: gpui::Rgba,
        prefix: &'static str,
        opacity: f32,
        content: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let is_selected_line = line_num.is_some_and(|n| self.is_line_selected(n));
        let hovered_hunk_idx = self.hovered_line.and_then(|h| self.hunk_idx_for_line(h));
        let line_hunk_idx = line_num.and_then(|n| self.hunk_idx_for_line(n));
        let is_hovered_hunk = line_hunk_idx
            .zip(hovered_hunk_idx)
            .is_some_and(|(a, b)| a == b);
        let selected_range = self
            .selected_line_range()
            .filter(|(start, end)| start != end);
        let is_staged_section = matches!(self.current_change_section, Some(ChangeSection::Staged));
        let is_unstaged_section =
            matches!(self.current_change_section, Some(ChangeSection::Unstaged));
        let stage_line = line_num.unwrap_or(1);
        let is_untracked =
            matches!(self.current_change_type, Some(ChangeType::Added)) && is_unstaged_section;
        let can_show_stage = is_hovered_hunk && line_num.is_some() && !is_untracked;
        let can_show_discard =
            is_hovered_hunk && is_unstaged_section && line_num.is_some() && !is_untracked;
        let stage_symbol = if is_staged_section { "-" } else { "+" };
        let path_for_click = self.file_path.clone();

        div()
            .flex()
            .whitespace_nowrap()
            .when_some(bg_color, |el, color| el.bg(color))
            .when(is_selected_line, |el| el.bg(rgb(BG_SURFACE2)))
            .opacity(opacity)
            .on_mouse_move(cx.listener(move |this, _: &gpui::MouseMoveEvent, _, cx| {
                if this.hovered_line != line_num {
                    this.hovered_line = line_num;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .w(px(3.0))
                    .h_full()
                    .flex_shrink_0()
                    .bg(gutter_strip_color),
            )
            .child(
                div()
                    .id(("diff-line", idx))
                    .w(px(40.0))
                    .flex_shrink_0()
                    .text_right()
                    .pr_2()
                    .bg(if is_selected_line {
                        rgb(BG_SURFACE2)
                    } else {
                        rgb(BG_MANTLE)
                    })
                    .text_color(if is_selected_line {
                        rgb(TEXT)
                    } else {
                        rgb(TEXT_MUTED)
                    })
                    .when(line_num.is_some(), |el| {
                        el.cursor_pointer()
                            .hover(|el| el.text_color(rgb(BLUE)))
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(
                            move |this, event: &gpui::MouseDownEvent, _, cx| {
                                if let Some(num) = line_num {
                                    this.select_diff_line(num, event.modifiers.shift);
                                    cx.notify();
                                }
                            },
                        ),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |_this, _, _, cx| {
                            if let (Some(path), Some(num)) = (&path_for_click, line_num) {
                                let text =
                                    format!("`{}:{}`", path.to_string_lossy(), num);
                                cx.emit(SendToTerminalEvent(text));
                            }
                        }),
                    )
                    .child(line_num.map(|n| n.to_string()).unwrap_or_default()),
            )
            .child(
                div()
                    .id(("diff-stage", idx))
                    .w(px(16.0))
                    .flex_shrink_0()
                    .text_center()
                    .text_xs()
                    .text_color(if is_staged_section {
                        rgb(BLUE)
                    } else {
                        rgb(GREEN)
                    })
                    .when(can_show_stage, |el| {
                        el.cursor_pointer()
                            .hover(|d| d.bg(rgb(BG_SURFACE1)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some((start, end)) = selected_range {
                                    if is_staged_section {
                                        this.request_unstage_range(start, end, cx);
                                    } else {
                                        this.request_stage_range(start, end, cx);
                                    }
                                } else if is_staged_section {
                                    this.request_unstage_hunk(stage_line, cx);
                                } else {
                                    this.request_stage_hunk(stage_line, cx);
                                }
                            }))
                    })
                    .child(if can_show_stage { stage_symbol } else { " " }),
            )
            .child(
                div()
                    .id(("diff-discard", idx))
                    .w(px(16.0))
                    .flex_shrink_0()
                    .text_center()
                    .text_xs()
                    .text_color(rgb(RED))
                    .when(can_show_discard, |el| {
                        el.cursor_pointer()
                            .hover(|d| d.bg(rgb(BG_SURFACE1)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some((start, end)) = selected_range {
                                    this.request_discard_range(start, end, cx);
                                } else {
                                    this.request_discard_hunk(stage_line, cx);
                                }
                            }))
                    })
                    .child(if can_show_discard { "x" } else { " " }),
            )
            .child(
                div()
                    .w(px(16.0))
                    .flex_shrink_0()
                    .text_center()
                    .text_color(text_color)
                    .child(prefix),
            )
            .child(content)
    }

    /// Render a right-side (After/Working) diff line with InteractiveText for go-to-definition.
    fn render_diff_line_right(
        &self,
        idx: usize,
        line: &SplitDiffLine,
        highlight_doc: Option<&HighlightedDoc>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (bg_color, text_color) = match line.line_type {
            DiffLineType::Added => (Some(rgb(DIFF_ADDED_BG)), rgb(GREEN)),
            DiffLineType::Removed => (Some(rgb(DIFF_REMOVED_BG)), rgb(RED)),
            DiffLineType::Context => (None, rgb(TEXT)),
        };
        let gutter_strip_color = match line.line_type {
            DiffLineType::Added => rgb(GREEN),
            DiffLineType::Removed => rgb(RED),
            DiffLineType::Context => rgb(BG_MANTLE),
        };
        let prefix = match line.line_type {
            DiffLineType::Added => "+",
            DiffLineType::Removed => "-",
            DiffLineType::Context => " ",
        };
        let line_num = line.new_line_num;
        let content = if line.content.is_empty() {
            " ".to_string()
        } else {
            line.content.clone()
        };
        let highlighted_line = Self::highlighted_line_for_number(highlight_doc, line_num);

        let content_element =
            if let (Some(hl), Some(ln)) = (highlighted_line, line_num) {
                div()
                    .flex_1()
                    .min_w_0()
                    .pl_2()
                    .text_color(text_color)
                    .child(self.render_interactive_text(ln, &content, hl, "diff-right", cx))
            } else if let Some(hl) = highlighted_line {
                div()
                    .flex_1()
                    .min_w_0()
                    .pl_2()
                    .text_color(text_color)
                    .child(Self::styled_text_from_highlighted_line(&content, hl))
            } else {
                div()
                    .flex_1()
                    .min_w_0()
                    .pl_2()
                    .text_color(text_color)
                    .child(content)
            };

        self.build_diff_row(
            idx, line_num, bg_color, text_color, gutter_strip_color, prefix, 1.0,
            content_element, cx,
        )
        .into_any_element()
    }

    fn render_diff_resize_handle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("diff-resize-handle")
            .h_full()
            .w(px(4.0))
            .flex_shrink_0()
            .cursor_col_resize()
            .hover(|el| el.bg(rgb(BLUE)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                    this.diff_resize_drag = Some(DiffResizeDrag {
                        start_x: f32::from(event.position.x),
                        initial_ratio: this.diff_split_ratio,
                    });
                    cx.notify();
                }),
            )
    }

    fn handle_diff_resize_move(&mut self, current_x: f32) {
        if let Some(ref drag) = self.diff_resize_drag {
            let container_width = if drag.initial_ratio > 0.0 {
                (drag.start_x - 0.0) / drag.initial_ratio
            } else {
                1.0
            };
            if container_width > 0.0 {
                let ratio_delta = (current_x - drag.start_x) / container_width;
                self.diff_split_ratio = (drag.initial_ratio + ratio_delta).clamp(0.2, 0.8);
            }
        }
    }

    fn handle_diff_resize_end(&mut self) {
        self.diff_resize_drag = None;
    }
}

impl Focusable for FileView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<SendToTerminalEvent> for FileView {}
impl EventEmitter<GotoDefinitionEvent> for FileView {}
impl EventEmitter<StageSelectionEvent> for FileView {}

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
                } else if line.starts_with(' ') {
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
