//! File view component for viewing files and diffs

use crate::git::ChangeType;
use crate::highlight::{HighlightedDoc, HighlightedLine};
use crate::theme::*;
use crate::ui::ChangeSection;
use gpui::{
    AnyElement, App, Context, DefiniteLength, EventEmitter, FocusHandle, Focusable,
    HighlightStyle, Hsla, InteractiveText, IntoElement, MouseButton, ParentElement, Render,
    ScrollHandle, ScrollStrategy, Stateful, Styled, StyledText, UniformListScrollHandle, Window,
    div, prelude::*, px, rgb, rgba, uniform_list,
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
#[allow(dead_code)]
pub enum StageSelectionKind {
    HunkAtLine(usize),
    LineRange { start: usize, end: usize },
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
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
    /// 文字レベルdiff: 変更された文字の UTF-8 byte range
    char_changes: Vec<std::ops::Range<usize>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffLineType {
    Context,
    Added,
    Removed,
    /// Split view のフィラー行（対面パネルの追加/削除に対応する空行）
    Filler,
}

/// Split diff の1行を表すenum
#[derive(Debug, Clone)]
enum SplitRow {
    HunkHeader(String),
    CollapseInfo { hidden_count: usize, collapse_id: usize },
    Line { left: SplitDiffLine, right: SplitDiffLine },
}

/// Line info for inline diff view
#[derive(Debug, Clone)]
struct InlineDiffLine {
    line_num: Option<usize>,
    old_line_num: Option<usize>,
    content: String,
    change_type: InlineChangeType,
    /// 文字レベルdiff: 変更された文字の UTF-8 byte range
    char_changes: Vec<std::ops::Range<usize>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineChangeType {
    Unchanged,
    Added,
    Deleted,
}

/// Inline diff の1行を表すenum
#[derive(Debug, Clone)]
enum InlineRow {
    HunkHeader(String),
    CollapseInfo { hidden_count: usize, collapse_id: usize },
    Line(InlineDiffLine),
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
    /// Split diff rows: HunkHeader / CollapseInfo / Line を統合
    cached_split_rows: Rc<Vec<SplitRow>>,
    /// Inline diff rows: HunkHeader / CollapseInfo / Line を統合
    cached_inline_rows: Rc<Vec<InlineRow>>,
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
    diff_split_ratio: f32,
    diff_resize_drag: Option<DiffResizeDrag>,
    /// ビューポート幅（ウィンドウ論理ピクセル）。render時に更新し、
    /// マウスハンドラでコンテンツ領域のx座標推定に使用する。
    viewport_width: f32,
    /// 展開されたコンテキスト折りたたみセクションの ID セット
    expanded_collapses: std::collections::HashSet<usize>,
    /// ツールバー表示用の相対パス（worktree rootからの相対）
    display_path: Option<String>,
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
            cached_split_rows: Rc::new(Vec::new()),
            cached_inline_rows: Rc::new(Vec::new()),
            diff_scroll_handle: UniformListScrollHandle::new(),
            content_scroll_handle: ScrollHandle::new(),
            inline_scroll_handle: UniformListScrollHandle::new(),
            target_line: None,
            current_change_section: None,
            current_change_type: None,
            hovered_line: None,
            diff_split_ratio: 0.5,
            diff_resize_drag: None,
            viewport_width: 0.0,
            expanded_collapses: std::collections::HashSet::new(),
            display_path: None,
        }
    }

    pub fn mode(&self) -> FileViewMode {
        self.mode
    }

    pub fn set_display_path(&mut self, path: String) {
        self.display_path = Some(path);
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
            let in_diff = self.cached_split_rows.iter().any(|row| {
                matches!(row, SplitRow::Line { right, .. } if right.new_line_num == Some(line))
            });
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
    }

    fn clear_diff_cache(&mut self) {
        self.cached_added_lines = Rc::new(std::collections::HashSet::new());
        self.cached_hunk_start_lines = Rc::new(std::collections::HashSet::new());
        self.cached_hunk_ranges = Rc::new(Vec::new());
        self.cached_split_rows = Rc::new(Vec::new());
        self.cached_inline_rows = Rc::new(Vec::new());
        self.expanded_collapses.clear();
    }

    fn update_diff_cache(&mut self) {
        self.cached_added_lines = Rc::new(self.compute_added_line_numbers());
        self.cached_hunk_start_lines = Rc::new(self.compute_hunk_start_lines());
        self.cached_hunk_ranges = Rc::new(self.compute_hunk_ranges());
        self.cached_split_rows = Rc::new(self.compute_split_rows());
        self.cached_inline_rows = Rc::new(self.compute_inline_rows());
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

    /// Diff統計（追加行数, 削除行数）を返す。
    fn diff_stats(&self) -> (usize, usize) {
        let mut added = 0usize;
        let mut removed = 0usize;
        for row in self.cached_inline_rows.iter() {
            if let InlineRow::Line(line) = row {
                match line.change_type {
                    InlineChangeType::Added => added += 1,
                    InlineChangeType::Deleted => removed += 1,
                    InlineChangeType::Unchanged => {}
                }
            }
        }
        (added, removed)
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

    /// 2行を比較し、それぞれの変更文字の UTF-8 byte range を返す。
    /// 共通 prefix/suffix を除去し、残りの「変更コア」を強調範囲とする。
    fn compute_char_diff(
        old: &str,
        new: &str,
    ) -> (Vec<std::ops::Range<usize>>, Vec<std::ops::Range<usize>>) {
        // 共通 prefix の byte 長を計算
        let prefix_bytes = old
            .char_indices()
            .zip(new.char_indices())
            .take_while(|((_, a), (_, b))| a == b)
            .last()
            .map(|((i, c), _)| i + c.len_utf8())
            .unwrap_or(0);

        let old_tail = &old[prefix_bytes..];
        let new_tail = &new[prefix_bytes..];

        // 共通 suffix の byte 長を計算（残り部分の末尾から）
        let suffix_chars = old_tail
            .chars()
            .rev()
            .zip(new_tail.chars().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let old_suffix_bytes: usize = old_tail
            .chars()
            .rev()
            .take(suffix_chars)
            .map(|c| c.len_utf8())
            .sum();
        let new_suffix_bytes: usize = new_tail
            .chars()
            .rev()
            .take(suffix_chars)
            .map(|c| c.len_utf8())
            .sum();

        let old_change_end = old.len() - old_suffix_bytes;
        let new_change_end = new.len() - new_suffix_bytes;

        let old_ranges = if prefix_bytes < old_change_end {
            vec![prefix_bytes..old_change_end]
        } else {
            vec![]
        };
        let new_ranges = if prefix_bytes < new_change_end {
            vec![prefix_bytes..new_change_end]
        } else {
            vec![]
        };

        (old_ranges, new_ranges)
    }

    /// Hunk内の pending Remove/Add 行をflushしてSplitRowに追加する。
    /// 1:1でペアリングできる行には compute_char_diff を適用する。
    fn flush_split_pending(
        pending_removes: &mut Vec<(usize, String)>, // (old_line_num, content)
        pending_adds: &mut Vec<(usize, String)>,    // (new_line_num, content)
        rows: &mut Vec<SplitRow>,
    ) {
        let pair_count = pending_removes.len().min(pending_adds.len());

        for i in 0..pair_count {
            let (old_char_changes, new_char_changes) =
                Self::compute_char_diff(&pending_removes[i].1, &pending_adds[i].1);
            rows.push(SplitRow::Line {
                left: SplitDiffLine {
                    old_line_num: Some(pending_removes[i].0),
                    new_line_num: None,
                    content: pending_removes[i].1.clone(),
                    line_type: DiffLineType::Removed,
                    char_changes: old_char_changes,
                },
                right: SplitDiffLine {
                    old_line_num: None,
                    new_line_num: Some(pending_adds[i].0),
                    content: pending_adds[i].1.clone(),
                    line_type: DiffLineType::Added,
                    char_changes: new_char_changes,
                },
            });
        }
        // ペアにならない余りの Remove 行（右パネルはフィラー）
        for i in pair_count..pending_removes.len() {
            rows.push(SplitRow::Line {
                left: SplitDiffLine {
                    old_line_num: Some(pending_removes[i].0),
                    new_line_num: None,
                    content: pending_removes[i].1.clone(),
                    line_type: DiffLineType::Removed,
                    char_changes: vec![],
                },
                right: SplitDiffLine {
                    old_line_num: None,
                    new_line_num: None,
                    content: String::new(),
                    line_type: DiffLineType::Filler,
                    char_changes: vec![],
                },
            });
        }
        // ペアにならない余りの Add 行（左パネルはフィラー）
        for i in pair_count..pending_adds.len() {
            rows.push(SplitRow::Line {
                left: SplitDiffLine {
                    old_line_num: None,
                    new_line_num: None,
                    content: String::new(),
                    line_type: DiffLineType::Filler,
                    char_changes: vec![],
                },
                right: SplitDiffLine {
                    old_line_num: None,
                    new_line_num: Some(pending_adds[i].0),
                    content: pending_adds[i].1.clone(),
                    line_type: DiffLineType::Added,
                    char_changes: vec![],
                },
            });
        }

        pending_removes.clear();
        pending_adds.clear();
    }

    /// Split diff 用の行リストを計算する。
    /// Hunk 前後 CONTEXT_LINES 行のみ表示し、それ以外は CollapseInfo で省略する。
    fn compute_split_rows(&self) -> Vec<SplitRow> {
        const CONTEXT: usize = 3;
        let diff = self.diff_content.as_deref().unwrap_or("");
        let content_lines: Vec<&str> = self.content.lines().collect();
        let total_lines = content_lines.len();

        // diff テキストをパースして Hunk リストを構築
        struct ParsedHunk {
            header: String,
            new_start: usize,
            lines: Vec<(char, String)>,
        }

        let mut hunks: Vec<ParsedHunk> = Vec::new();
        let mut current_hunk: Option<ParsedHunk> = None;

        for raw_line in diff.lines() {
            if raw_line.starts_with("@@") {
                if let Some(h) = current_hunk.take() {
                    hunks.push(h);
                }
                if let Some((_, new_start)) = Self::parse_hunk_header(raw_line) {
                    current_hunk = Some(ParsedHunk {
                        header: raw_line.to_string(),
                        new_start,
                        lines: Vec::new(),
                    });
                }
            } else if raw_line.starts_with("---")
                || raw_line.starts_with("+++")
                || raw_line.starts_with("diff ")
            {
                // diff メタデータをスキップ
            } else if let Some(ref mut hunk) = current_hunk {
                if let Some(s) = raw_line.strip_prefix('+') {
                    hunk.lines.push(('+', s.to_string()));
                } else if let Some(s) = raw_line.strip_prefix('-') {
                    hunk.lines.push(('-', s.to_string()));
                } else if raw_line.starts_with(' ') {
                    hunk.lines.push((' ', raw_line[1..].to_string()));
                } else if raw_line.is_empty() {
                    hunk.lines.push((' ', String::new()));
                }
            }
        }
        if let Some(h) = current_hunk.take() {
            hunks.push(h);
        }

        // Hunk がなければファイル全行をコンテキスト表示
        if hunks.is_empty() {
            return content_lines
                .iter()
                .enumerate()
                .map(|(i, line)| {
                    let n = i + 1;
                    SplitRow::Line {
                        left: SplitDiffLine {
                            old_line_num: Some(n),
                            new_line_num: None,
                            content: line.to_string(),
                            line_type: DiffLineType::Context,
                            char_changes: vec![],
                        },
                        right: SplitDiffLine {
                            old_line_num: None,
                            new_line_num: Some(n),
                            content: line.to_string(),
                            line_type: DiffLineType::Context,
                            char_changes: vec![],
                        },
                    }
                })
                .collect();
        }

        let mut rows: Vec<SplitRow> = Vec::new();
        let mut next_new = 1usize; // 次に表示する new_line_num
        let mut next_old = 1usize;
        let mut collapse_counter = 0usize;

        for (hunk_idx, hunk) in hunks.iter().enumerate() {
            // このHunkの前コンテキスト開始行 (最低 next_new から始める)
            let ctx_start = hunk.new_start.saturating_sub(CONTEXT).max(next_new).max(1);

            // 省略行の挿入: next_new から ctx_start - 1 まで hidden
            if ctx_start > next_new {
                let hidden = ctx_start - next_new;
                let cid = collapse_counter;
                collapse_counter += 1;
                if self.expanded_collapses.contains(&cid) {
                    // 展開: hidden 行をコンテキストとして表示
                    for ln in next_new..ctx_start {
                        let content = content_lines.get(ln - 1).unwrap_or(&"").to_string();
                        rows.push(SplitRow::Line {
                            left: SplitDiffLine {
                                old_line_num: Some(next_old),
                                new_line_num: None,
                                content: content.clone(),
                                line_type: DiffLineType::Context,
                                char_changes: vec![],
                            },
                            right: SplitDiffLine {
                                old_line_num: None,
                                new_line_num: Some(ln),
                                content,
                                line_type: DiffLineType::Context,
                                char_changes: vec![],
                            },
                        });
                        next_old += 1;
                    }
                } else {
                    rows.push(SplitRow::CollapseInfo { hidden_count: hidden, collapse_id: cid });
                }
                next_new = ctx_start;
                if !self.expanded_collapses.contains(&cid) {
                    next_old = next_old + hidden;
                }
            }

            // HunkHeader 行
            rows.push(SplitRow::HunkHeader(hunk.header.clone()));

            // Hunk 前のコンテキスト行 (ctx_start .. hunk.new_start-1)
            while next_new < hunk.new_start {
                let content = content_lines.get(next_new - 1).unwrap_or(&"").to_string();
                rows.push(SplitRow::Line {
                    left: SplitDiffLine {
                        old_line_num: Some(next_old),
                        new_line_num: None,
                        content: content.clone(),
                        line_type: DiffLineType::Context,
                        char_changes: vec![],
                    },
                    right: SplitDiffLine {
                        old_line_num: None,
                        new_line_num: Some(next_new),
                        content,
                        line_type: DiffLineType::Context,
                        char_changes: vec![],
                    },
                });
                next_new += 1;
                next_old += 1;
            }

            // Hunk 内の行を処理。連続する Remove/Add ブロックをバッファしてペアリング
            let mut pending_removes: Vec<(usize, String)> = Vec::new();
            let mut pending_adds: Vec<(usize, String)> = Vec::new();

            for (type_char, content) in &hunk.lines {
                match type_char {
                    '-' => {
                        // 新しい Remove ブロック前に先行する Add があればまずflush
                        if !pending_adds.is_empty() && pending_removes.is_empty() {
                            Self::flush_split_pending(
                                &mut pending_removes,
                                &mut pending_adds,
                                &mut rows,
                            );
                        }
                        pending_removes.push((next_old, content.clone()));
                        next_old += 1;
                    }
                    '+' => {
                        pending_adds.push((next_new, content.clone()));
                        next_new += 1;
                    }
                    _ => {
                        // コンテキスト行が来たら pending をflushしてから通常行を追加
                        Self::flush_split_pending(
                            &mut pending_removes,
                            &mut pending_adds,
                            &mut rows,
                        );
                        rows.push(SplitRow::Line {
                            left: SplitDiffLine {
                                old_line_num: Some(next_old),
                                new_line_num: None,
                                content: content.clone(),
                                line_type: DiffLineType::Context,
                                char_changes: vec![],
                            },
                            right: SplitDiffLine {
                                old_line_num: None,
                                new_line_num: Some(next_new),
                                content: content.clone(),
                                line_type: DiffLineType::Context,
                                char_changes: vec![],
                            },
                        });
                        next_new += 1;
                        next_old += 1;
                    }
                }
            }
            // Hunk 末尾の pending をflush
            Self::flush_split_pending(&mut pending_removes, &mut pending_adds, &mut rows);

            // Hunk 後のコンテキスト行: 次のHunkの ctx_start まで or total_lines まで最大CONTEXT行
            let next_hunk_ctx_start = hunks
                .get(hunk_idx + 1)
                .map(|h| h.new_start.saturating_sub(CONTEXT).max(1))
                .unwrap_or(total_lines + 1);
            let ctx_end = (next_new + CONTEXT - 1).min(next_hunk_ctx_start - 1).min(total_lines);
            while next_new <= ctx_end {
                let content = content_lines.get(next_new - 1).unwrap_or(&"").to_string();
                rows.push(SplitRow::Line {
                    left: SplitDiffLine {
                        old_line_num: Some(next_old),
                        new_line_num: None,
                        content: content.clone(),
                        line_type: DiffLineType::Context,
                        char_changes: vec![],
                    },
                    right: SplitDiffLine {
                        old_line_num: None,
                        new_line_num: Some(next_new),
                        content,
                        line_type: DiffLineType::Context,
                        char_changes: vec![],
                    },
                });
                next_new += 1;
                next_old += 1;
            }
        }

        // 最後のHunk後に残りの行が省略される場合
        if next_new <= total_lines {
            let hidden = total_lines - next_new + 1;
            let cid = collapse_counter;
            if self.expanded_collapses.contains(&cid) {
                for ln in next_new..=total_lines {
                    let content = content_lines.get(ln - 1).unwrap_or(&"").to_string();
                    rows.push(SplitRow::Line {
                        left: SplitDiffLine {
                            old_line_num: Some(next_old),
                            new_line_num: None,
                            content: content.clone(),
                            line_type: DiffLineType::Context,
                            char_changes: vec![],
                        },
                        right: SplitDiffLine {
                            old_line_num: None,
                            new_line_num: Some(ln),
                            content,
                            line_type: DiffLineType::Context,
                            char_changes: vec![],
                        },
                    });
                    next_old += 1;
                }
            } else {
                rows.push(SplitRow::CollapseInfo { hidden_count: hidden, collapse_id: cid });
            }
        }

        rows
    }

    /// Inline diff 用の行リストを計算する。
    /// Hunk 前後 CONTEXT_LINES 行のみ表示し、それ以外は CollapseInfo で省略する。
    fn compute_inline_rows(&self) -> Vec<InlineRow> {
        const CONTEXT: usize = 3;
        let diff = self.diff_content.as_deref().unwrap_or("");
        let content_lines: Vec<&str> = self.content.lines().collect();
        let total_lines = content_lines.len();

        // Hunk パース（compute_split_rows と同じ構造）
        struct ParsedHunk {
            header: String,
            new_start: usize,
            lines: Vec<(char, String)>,
        }

        let mut hunks: Vec<ParsedHunk> = Vec::new();
        let mut current_hunk: Option<ParsedHunk> = None;

        for raw_line in diff.lines() {
            if raw_line.starts_with("@@") {
                if let Some(h) = current_hunk.take() {
                    hunks.push(h);
                }
                if let Some((_, new_start)) = Self::parse_hunk_header(raw_line) {
                    current_hunk = Some(ParsedHunk {
                        header: raw_line.to_string(),
                        new_start,
                        lines: Vec::new(),
                    });
                }
            } else if raw_line.starts_with("---")
                || raw_line.starts_with("+++")
                || raw_line.starts_with("diff ")
            {
            } else if let Some(ref mut hunk) = current_hunk {
                if let Some(s) = raw_line.strip_prefix('+') {
                    hunk.lines.push(('+', s.to_string()));
                } else if let Some(s) = raw_line.strip_prefix('-') {
                    hunk.lines.push(('-', s.to_string()));
                } else if raw_line.starts_with(' ') {
                    hunk.lines.push((' ', raw_line[1..].to_string()));
                } else if raw_line.is_empty() {
                    hunk.lines.push((' ', String::new()));
                }
            }
        }
        if let Some(h) = current_hunk.take() {
            hunks.push(h);
        }

        if hunks.is_empty() {
            // 差分なし: ファイル全行を Unchanged で返す
            return content_lines
                .iter()
                .enumerate()
                .map(|(i, line)| {
                    InlineRow::Line(InlineDiffLine {
                        line_num: Some(i + 1),
                        old_line_num: None,
                        content: line.to_string(),
                        change_type: InlineChangeType::Unchanged,
                        char_changes: vec![],
                    })
                })
                .collect();
        }

        let mut rows: Vec<InlineRow> = Vec::new();
        let mut next_new = 1usize;
        let mut next_old = 1usize;
        let mut collapse_counter = 0usize;

        for (hunk_idx, hunk) in hunks.iter().enumerate() {
            let ctx_start = hunk.new_start.saturating_sub(CONTEXT).max(next_new).max(1);

            // 省略行
            if ctx_start > next_new {
                let hidden = ctx_start - next_new;
                let cid = collapse_counter;
                collapse_counter += 1;
                if self.expanded_collapses.contains(&cid) {
                    for ln in next_new..ctx_start {
                        let old_ln = next_old + (ln - next_new);
                        let content = content_lines.get(ln - 1).unwrap_or(&"").to_string();
                        rows.push(InlineRow::Line(InlineDiffLine {
                            line_num: Some(ln),
                            old_line_num: Some(old_ln),
                            content,
                            change_type: InlineChangeType::Unchanged,
                            char_changes: vec![],
                        }));
                    }
                } else {
                    rows.push(InlineRow::CollapseInfo { hidden_count: hidden, collapse_id: cid });
                }
                next_new = ctx_start;
                next_old += hidden;
            }

            // HunkHeader
            rows.push(InlineRow::HunkHeader(hunk.header.clone()));

            // Hunk 前コンテキスト
            while next_new < hunk.new_start {
                let content = content_lines.get(next_new - 1).unwrap_or(&"").to_string();
                rows.push(InlineRow::Line(InlineDiffLine {
                    line_num: Some(next_new),
                    old_line_num: Some(next_old),
                    content,
                    change_type: InlineChangeType::Unchanged,
                    char_changes: vec![],
                }));
                next_new += 1;
                next_old += 1;
            }

            // Hunk 内の行を処理。Remove/Add をバッファしてchar_diffをペアリング
            let mut pending_removes: Vec<(usize, String)> = Vec::new();
            let mut pending_adds: Vec<(usize, String)> = Vec::new();

            let flush_inline = |pending_removes: &mut Vec<(usize, String)>,
                                pending_adds: &mut Vec<(usize, String)>,
                                rows: &mut Vec<InlineRow>| {
                let pair_count = pending_removes.len().min(pending_adds.len());
                // ペアリング可能な行に char_diff を適用
                for i in 0..pending_removes.len() {
                    let (old_changes, _) = if i < pair_count {
                        Self::compute_char_diff(&pending_removes[i].1, &pending_adds[i].1)
                    } else {
                        (vec![], vec![])
                    };
                    rows.push(InlineRow::Line(InlineDiffLine {
                        line_num: None,
                        old_line_num: Some(pending_removes[i].0),
                        content: pending_removes[i].1.clone(),
                        change_type: InlineChangeType::Deleted,
                        char_changes: old_changes,
                    }));
                }
                for i in 0..pending_adds.len() {
                    let (_, new_changes) = if i < pair_count {
                        Self::compute_char_diff(&pending_removes[i].1, &pending_adds[i].1)
                    } else {
                        (vec![], vec![])
                    };
                    rows.push(InlineRow::Line(InlineDiffLine {
                        line_num: Some(pending_adds[i].0),
                        old_line_num: None,
                        content: pending_adds[i].1.clone(),
                        change_type: InlineChangeType::Added,
                        char_changes: new_changes,
                    }));
                }
                pending_removes.clear();
                pending_adds.clear();
            };

            for (type_char, content) in &hunk.lines {
                match type_char {
                    '-' => {
                        if !pending_adds.is_empty() && pending_removes.is_empty() {
                            flush_inline(&mut pending_removes, &mut pending_adds, &mut rows);
                        }
                        pending_removes.push((next_old, content.clone()));
                        next_old += 1;
                    }
                    '+' => {
                        pending_adds.push((next_new, content.clone()));
                        next_new += 1;
                    }
                    _ => {
                        flush_inline(&mut pending_removes, &mut pending_adds, &mut rows);
                        let line_content =
                            content_lines.get(next_new - 1).map_or(content.as_str(), |v| v);
                        rows.push(InlineRow::Line(InlineDiffLine {
                            line_num: Some(next_new),
                            old_line_num: Some(next_old),
                            content: line_content.to_string(),
                            change_type: InlineChangeType::Unchanged,
                            char_changes: vec![],
                        }));
                        next_new += 1;
                        next_old += 1;
                    }
                }
            }
            flush_inline(&mut pending_removes, &mut pending_adds, &mut rows);

            // Hunk 後のコンテキスト行
            let next_hunk_ctx_start = hunks
                .get(hunk_idx + 1)
                .map(|h| h.new_start.saturating_sub(CONTEXT).max(1))
                .unwrap_or(total_lines + 1);
            let ctx_end = (next_new + CONTEXT - 1).min(next_hunk_ctx_start - 1).min(total_lines);
            while next_new <= ctx_end {
                let content = content_lines.get(next_new - 1).unwrap_or(&"").to_string();
                rows.push(InlineRow::Line(InlineDiffLine {
                    line_num: Some(next_new),
                    old_line_num: Some(next_old),
                    content,
                    change_type: InlineChangeType::Unchanged,
                    char_changes: vec![],
                }));
                next_new += 1;
                next_old += 1;
            }
        }

        // 末尾省略
        if next_new <= total_lines {
            let hidden = total_lines - next_new + 1;
            let cid = collapse_counter;
            if self.expanded_collapses.contains(&cid) {
                for ln in next_new..=total_lines {
                    let old_ln = next_old + (ln - next_new);
                    let content = content_lines.get(ln - 1).unwrap_or(&"").to_string();
                    rows.push(InlineRow::Line(InlineDiffLine {
                        line_num: Some(ln),
                        old_line_num: Some(old_ln),
                        content,
                        change_type: InlineChangeType::Unchanged,
                        char_changes: vec![],
                    }));
                }
            } else {
                rows.push(InlineRow::CollapseInfo { hidden_count: hidden, collapse_id: cid });
            }
        }

        rows
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

    /// シンタックスハイライトと文字レベルdiffを重ねた StyledText を生成する。
    /// char_changes の範囲に背景色ハイライトを上乗せする。
    /// GPUI の StyledText::compute_runs はハイライトがソート済み・非重複であることを前提とするため、
    /// シンタックスハイライトと char_changes 範囲を境界分割してマージする。
    fn styled_text_with_char_changes(
        content: &str,
        highlight_line: Option<&HighlightedLine>,
        char_changes: &[std::ops::Range<usize>],
        char_highlight_rgba: u32,
    ) -> StyledText {
        let text = if content.is_empty() {
            " ".to_string()
        } else {
            content.to_string()
        };
        let len = text.len();

        let syntax_spans: Vec<(std::ops::Range<usize>, HighlightStyle)> = if let Some(hl) = highlight_line {
            hl.spans
                .iter()
                .filter(|s| s.range.start < s.range.end && s.range.end <= len)
                .map(|s| (s.range.clone(), s.style))
                .collect()
        } else {
            vec![]
        };

        if char_changes.is_empty() {
            return StyledText::new(text).with_highlights(syntax_spans.into_iter());
        }

        let char_bg = Hsla::from(rgba(char_highlight_rgba));

        // 全境界点を収集してソート（シンタックスspan + char_changes の開始/終了）
        let mut boundaries = std::collections::BTreeSet::new();
        boundaries.insert(0usize);
        boundaries.insert(len);
        for (range, _) in &syntax_spans {
            boundaries.insert(range.start);
            boundaries.insert(range.end);
        }
        for range in char_changes {
            if range.start <= len {
                boundaries.insert(range.start.min(len));
            }
            if range.end <= len {
                boundaries.insert(range.end.min(len));
            }
        }

        let pts: Vec<usize> = boundaries.into_iter().collect();
        let mut merged: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();

        for window in pts.windows(2) {
            let seg_start = window[0];
            let seg_end = window[1];
            if seg_start >= seg_end {
                continue;
            }

            // このセグメントに適用されるシンタックススタイルを検索
            let syntax_style = syntax_spans
                .iter()
                .find(|(r, _)| r.start <= seg_start && seg_end <= r.end)
                .map(|(_, style)| *style);

            // このセグメントが char_changes 範囲内かどうか
            let in_char_change = char_changes
                .iter()
                .any(|r| r.start <= seg_start && seg_end <= r.end);

            let style = match (syntax_style, in_char_change) {
                (Some(mut s), true) => {
                    s.background_color = Some(char_bg);
                    s
                }
                (Some(s), false) => s,
                (None, true) => HighlightStyle {
                    background_color: Some(char_bg),
                    ..Default::default()
                },
                (None, false) => continue, // スタイルなし → デフォルトに任せる
            };

            merged.push((seg_start..seg_end, style));
        }

        StyledText::new(text).with_highlights(merged.into_iter())
    }

    /// Hunk ヘッダー行を青背景でレンダリングする（split/inline 共通）。
    fn render_hunk_header_row(text: &str) -> gpui::Div {
        div()
            .w_full()
            .h(px(20.0))
            .flex()
            .flex_row()
            .items_center()
            .bg(rgb(DIFF_HUNK_HEADER_BG))
            // ガター構造を維持: strip(3px) + old行番号(40px) + new行番号(40px) + prefix(16px)
            .child(div().w(px(3.0)).flex_shrink_0().bg(rgb(DIFF_HUNK_HEADER_BG)))
            .child(div().w(px(40.0)).flex_shrink_0())
            .child(div().w(px(40.0)).flex_shrink_0())
            .child(div().w(px(16.0)).flex_shrink_0())
            .child(
                div()
                    .flex_1()
                    .pl_2()
                    .text_xs()
                    .text_color(rgb(DIFF_HUNK_HEADER_FG))
                    .font_family(MONOSPACE_FONT)
                    .child(text.to_string()),
            )
    }

    /// 省略行バナーをレンダリングする（split/inline 共通）。
    fn render_collapse_row(&self, hidden_count: usize, collapse_id: usize, cx: &mut Context<Self>) -> Stateful<gpui::Div> {
        div()
            .id(("collapse-row", collapse_id))
            .w_full()
            .h(px(20.0))
            .flex()
            .flex_row()
            .items_center()
            .bg(rgb(DIFF_COLLAPSE_BG))
            .cursor_pointer()
            .hover(|el| el.bg(rgb(BG_SURFACE1)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.expanded_collapses.insert(collapse_id);
                    this.update_diff_cache();
                    cx.notify();
                }),
            )
            // ガター構造を維持: strip(3px) + old行番号(40px) + new行番号(40px) + prefix(16px)
            .child(div().w(px(3.0)).flex_shrink_0().bg(rgb(DIFF_COLLAPSE_BG)))
            .child(div().w(px(40.0)).flex_shrink_0())
            .child(div().w(px(40.0)).flex_shrink_0())
            .child(div().w(px(16.0)).flex_shrink_0())
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap_1()
                    .pl_2()
                    .text_xs()
                    .text_color(rgb(BLUE))
                    .child("⊕")
                    .child(
                        div()
                            .text_color(rgb(TEXT_MUTED))
                            .child(format!("{hidden_count} hidden lines")),
                    ),
            )
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
        let file_display = self
            .display_path
            .clone()
            .or_else(|| {
                self.file_path.as_ref().map(|p| {
                    let components: Vec<_> = p.components().rev().take(2).collect();
                    components
                        .into_iter()
                        .rev()
                        .map(|c| c.as_os_str().to_string_lossy().to_string())
                        .collect::<Vec<_>>()
                        .join("/")
                })
            })
            .unwrap_or_else(|| "No file".to_string());

        let mode = self.mode;
        let has_diff = self.diff_content.is_some();
        let is_diff_view = has_diff && self.is_diff_mode();
        let (added, removed) = if is_diff_view { self.diff_stats() } else { (0, 0) };

        div()
            .h_8()
            .px_3()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .bg(rgb(BG_BASE))
            .border_b_1()
            .border_color(rgb(BG_SURFACE0))
            .relative()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(TEXT))
                            .child(file_display.clone()),
                    )
                    .when(is_diff_view, |el| {
                        el.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_xs()
                                .child(
                                    div()
                                        .text_color(rgb(GREEN))
                                        .child(format!("+{added}")),
                                )
                                .child(
                                    div()
                                        .text_color(rgb(RED))
                                        .child(format!("-{removed}")),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(is_diff_view, |el| {
                        el.child(
                            div()
                                .id("toggle-diff-display")
                                .px_3()
                                .py(px(3.0))
                                .cursor_pointer()
                                .rounded_sm()
                                .bg(rgb(BG_SURFACE0))
                                .hover(|d| d.bg(rgb(BG_SURFACE1)))
                                .text_xs()
                                .text_color(rgb(MAUVE))
                                .on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.toggle_diff_display_mode();
                                        cx.notify();
                                    }),
                                )
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
                            .px_3()
                            .py(px(3.0))
                            .cursor_pointer()
                            .text_xs()
                            .text_color(rgb(TEXT_MUTED))
                            .hover(|el| el.text_color(rgb(RED)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.close();
                                    cx.notify();
                                }),
                            )
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
        let rows = self.cached_inline_rows.clone();

        if let Some(target) = self.target_line.take() {
            if let Some(ix) = rows.iter().position(|row| {
                matches!(row, InlineRow::Line(l) if l.line_num == Some(target))
            }) {
                self.inline_scroll_handle
                    .scroll_to_item(ix, ScrollStrategy::Top);
            }
        }

        let item_count = rows.len();

        uniform_list(
            "inline-diff-scroll",
            item_count,
            cx.processor(|this: &mut Self, range: Range<usize>, _window: &mut Window, cx: &mut Context<Self>| {
                let rows = this.cached_inline_rows.clone();
                let highlight_new = this.highlight_new.as_deref();
                let highlight_old = this.highlight_old.as_deref();

                range
                    .map(|idx| {
                        match &rows[idx] {
                            InlineRow::HunkHeader(text) => {
                                Self::render_hunk_header_row(text).into_any_element()
                            }
                            InlineRow::CollapseInfo { hidden_count, collapse_id } => {
                                this.render_collapse_row(*hidden_count, *collapse_id, cx).into_any_element()
                            }
                            InlineRow::Line(line) => {
                                let (bg_color, text_color, opacity) = match line.change_type {
                                    InlineChangeType::Added => (Some(rgb(DIFF_ADDED_BG)), rgb(TEXT), 1.0),
                                    InlineChangeType::Deleted => (Some(rgb(DIFF_REMOVED_BG)), rgb(TEXT), 1.0),
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
                                let gutter_bg = match line.change_type {
                                    InlineChangeType::Added => rgb(DIFF_ADDED_GUTTER_BG),
                                    InlineChangeType::Deleted => rgb(DIFF_REMOVED_GUTTER_BG),
                                    InlineChangeType::Unchanged => rgb(BG_MANTLE),
                                };
                                let char_highlight_rgba = match line.change_type {
                                    InlineChangeType::Added => DIFF_ADDED_WORD_BG,
                                    InlineChangeType::Deleted => DIFF_REMOVED_WORD_BG,
                                    InlineChangeType::Unchanged => 0,
                                };
                                let highlight_line = match line.change_type {
                                    InlineChangeType::Deleted => Self::highlighted_line_for_number(highlight_old, line.old_line_num),
                                    _ => Self::highlighted_line_for_number(highlight_new, line.line_num),
                                };
                                let char_changes = &line.char_changes;
                                let content = if !char_changes.is_empty() {
                                    div().pl_2().text_color(text_color).child(
                                        Self::styled_text_with_char_changes(
                                            &line.content, highlight_line, char_changes, char_highlight_rgba,
                                        ),
                                    )
                                } else if line.change_type == InlineChangeType::Deleted {
                                    div().pl_2().text_color(text_color).child(
                                        Self::styled_text_with_char_changes(
                                            &line.content, highlight_line, &[], char_highlight_rgba,
                                        ),
                                    )
                                } else if let (Some(hl), Some(ln)) = (highlight_line, line.line_num) {
                                    div().pl_2().text_color(text_color).child(
                                        this.render_interactive_text(ln, &line.content, hl, "inline-diff", cx),
                                    )
                                } else {
                                    div().pl_2().text_color(text_color).child(
                                        Self::styled_text_with_char_changes(
                                            &line.content, None, &[], char_highlight_rgba,
                                        ),
                                    )
                                };

                                this.build_diff_row(
                                    idx, line.line_num, Some(line.old_line_num),
                                    bg_color, gutter_bg,
                                    gutter_strip_color, prefix, opacity, content,
                                    &line.content, cx,
                                ).into_any_element()
                            }
                        }
                    })
                    .collect()
            }),
        )
        .flex_1()
        .bg(rgb(BG_BASE))
        .font_family(MONOSPACE_FONT)
        .text_sm()
        .line_height(px(20.0))
        .track_scroll(&self.inline_scroll_handle)
    }

    fn render_diff(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(line) = self.target_line.take() {
            if let Some(ix) = self.cached_split_rows.iter().position(|row| {
                matches!(row, SplitRow::Line { right, .. } if right.new_line_num == Some(line))
            }) {
                self.diff_scroll_handle
                    .scroll_to_item(ix, ScrollStrategy::Top);
            }
        }

        let ratio = self.diff_split_ratio;
        let row_count = self.cached_split_rows.len();

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
            // 左右パネルのラベルヘッダー
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
            // 仮想スクロールリスト
            .child(
                uniform_list(
                    "diff-scroll",
                    row_count,
                    cx.processor(
                        |this: &mut Self,
                         range: Range<usize>,
                         _window: &mut Window,
                         cx: &mut Context<Self>| {
                            let ratio = this.diff_split_ratio;
                            let rows = this.cached_split_rows.clone();
                            let highlight_old = this.highlight_old.as_deref();
                            let highlight_new = this.highlight_new.as_deref();

                            range
                                .map(|idx| match &rows[idx] {
                                    SplitRow::HunkHeader(text) => {
                                        // 両側に青背景のHunkヘッダー行を表示
                                        // ガター構造: strip(3px) + line_num(40px) + prefix(16px) + text
                                        div()
                                            .w_full()
                                            .h(px(20.0))
                                            .flex()
                                            .flex_row()
                                            .bg(rgb(DIFF_HUNK_HEADER_BG))
                                            .child(
                                                div()
                                                    .w(DefiniteLength::Fraction(ratio))
                                                    .overflow_hidden()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .child(div().w(px(3.0)).flex_shrink_0())
                                                    .child(div().w(px(40.0)).flex_shrink_0())
                                                    .child(div().w(px(16.0)).flex_shrink_0())
                                                    .child(div().flex_1().pl_2().text_xs()
                                                        .font_family(MONOSPACE_FONT)
                                                        .text_color(rgb(DIFF_HUNK_HEADER_FG))
                                                        .child(text.clone())),
                                            )
                                            .child(div().w(px(4.0)).flex_shrink_0().bg(rgb(BG_SURFACE0)))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .overflow_hidden()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .child(div().w(px(3.0)).flex_shrink_0())
                                                    .child(div().w(px(40.0)).flex_shrink_0())
                                                    .child(div().w(px(16.0)).flex_shrink_0())
                                                    .child(div().flex_1().pl_2().text_xs()
                                                        .font_family(MONOSPACE_FONT)
                                                        .text_color(rgb(DIFF_HUNK_HEADER_FG))
                                                        .child(text.clone())),
                                            )
                                            .into_any_element()
                                    }
                                    SplitRow::CollapseInfo { hidden_count, collapse_id } => {
                                        let cid = *collapse_id;
                                        // ガター構造: strip(3px) + line_num(40px) + prefix(16px) + content
                                        div()
                                            .id(("split-collapse", cid))
                                            .w_full()
                                            .h(px(20.0))
                                            .flex()
                                            .flex_row()
                                            .bg(rgb(DIFF_COLLAPSE_BG))
                                            .cursor_pointer()
                                            .hover(|el| el.bg(rgb(BG_SURFACE1)))
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(move |this, _, _, cx| {
                                                    this.expanded_collapses.insert(cid);
                                                    this.update_diff_cache();
                                                    cx.notify();
                                                }),
                                            )
                                            .items_center()
                                            .child(
                                                div()
                                                    .w(DefiniteLength::Fraction(ratio))
                                                    .overflow_hidden()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .child(div().w(px(3.0)).flex_shrink_0())
                                                    .child(div().w(px(40.0)).flex_shrink_0())
                                                    .child(div().w(px(16.0)).flex_shrink_0())
                                                    .child(
                                                        div().flex_1().pl_2().flex().items_center().gap_1().text_xs()
                                                            .child(div().text_color(rgb(BLUE)).child("⊕"))
                                                            .child(div().text_color(rgb(TEXT_MUTED))
                                                                .child(format!("{} hidden lines", hidden_count))),
                                                    ),
                                            )
                                            .child(div().w(px(4.0)).flex_shrink_0().bg(rgb(BG_SURFACE0)))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .overflow_hidden()
                                                    .flex()
                                                    .flex_row()
                                                    .items_center()
                                                    .child(div().w(px(3.0)).flex_shrink_0())
                                                    .child(div().w(px(40.0)).flex_shrink_0())
                                                    .child(div().w(px(16.0)).flex_shrink_0())
                                                    .child(
                                                        div().flex_1().pl_2().flex().items_center().gap_1().text_xs()
                                                            .child(div().text_color(rgb(BLUE)).child("⊕"))
                                                            .child(div().text_color(rgb(TEXT_MUTED))
                                                                .child(format!("{} hidden lines", hidden_count))),
                                                    ),
                                            )
                                            .into_any_element()
                                    }
                                    SplitRow::Line { left, right } => {
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
                                                    .child(
                                                        Self::render_split_left_line(
                                                            left,
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
                                                    .child(
                                                        this.render_split_right_line(
                                                            idx,
                                                            right,
                                                            highlight_new,
                                                            cx,
                                                        ),
                                                    ),
                                            )
                                            .into_any_element()
                                    }
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

    /// Split diff の左側（Before/削除）パネルの1行をレンダリングする。
    fn render_split_left_line(
        line: &SplitDiffLine,
        highlight_doc: Option<&HighlightedDoc>,
    ) -> impl IntoElement {
        let (bg_color, text_color) = match line.line_type {
            DiffLineType::Added => (Some(rgb(DIFF_ADDED_BG)), rgb(TEXT)),
            DiffLineType::Removed => (Some(rgb(DIFF_REMOVED_BG)), rgb(TEXT)),
            DiffLineType::Context => (None, rgb(TEXT)),
            DiffLineType::Filler => (Some(rgb(DIFF_FILLER_BG)), rgb(TEXT_MUTED)),
        };
        let gutter_strip_color = match line.line_type {
            DiffLineType::Added => rgb(GREEN),
            DiffLineType::Removed => rgb(RED),
            DiffLineType::Context | DiffLineType::Filler => rgb(BG_MANTLE),
        };
        let gutter_bg = match line.line_type {
            DiffLineType::Added => rgb(DIFF_ADDED_GUTTER_BG),
            DiffLineType::Removed => rgb(DIFF_REMOVED_GUTTER_BG),
            DiffLineType::Context => rgb(BG_MANTLE),
            DiffLineType::Filler => rgb(DIFF_FILLER_BG),
        };
        let prefix = match line.line_type {
            DiffLineType::Removed => "-",
            DiffLineType::Added => "+",
            DiffLineType::Context | DiffLineType::Filler => " ",
        };
        let char_highlight = match line.line_type {
            DiffLineType::Removed => DIFF_REMOVED_WORD_BG,
            DiffLineType::Added => DIFF_ADDED_WORD_BG,
            DiffLineType::Context | DiffLineType::Filler => 0,
        };
        let line_num = line.old_line_num;
        let highlighted_line = Self::highlighted_line_for_number(highlight_doc, line_num);

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
                    .bg(gutter_bg)
                    .text_color(rgb(TEXT_MUTED))
                    .child(line_num.map(|n| n.to_string()).unwrap_or_default()),
            )
            // 右側の prefix 列に合わせたスペーサー
            .child(
                div()
                    .w(px(16.0))
                    .flex_shrink_0()
                    .text_center()
                    .text_color(match prefix {
                        "+" => rgb(GREEN),
                        "-" => rgb(RED),
                        _ => rgb(TEXT_MUTED),
                    })
                    .child(prefix),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .pl_2()
                    .text_color(text_color)
                    .child(Self::styled_text_with_char_changes(
                        &line.content,
                        highlighted_line,
                        &line.char_changes,
                        char_highlight,
                    )),
            )
    }

    /// Build an interactive diff row with gutter strip, line number, stage/discard buttons,
    /// prefix symbol, and caller-provided content element.
    /// Used by both inline diff and split diff (right side) to avoid duplicating ~120 lines.
    /// `old_line_num`: inline view で旧ファイル側の行番号カラムを追加表示する場合に指定。
    /// None → カラム非表示（split right）、Some(None) → 空カラム、Some(Some(n)) → 行番号n表示。
    fn build_diff_row(
        &self,
        idx: usize,
        line_num: Option<usize>,
        old_line_num: Option<Option<usize>>,
        bg_color: Option<gpui::Rgba>,
        gutter_bg: gpui::Rgba,
        gutter_strip_color: gpui::Rgba,
        prefix: &'static str,
        opacity: f32,
        content: impl IntoElement,
        content_text: &str,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        let hovered_hunk_idx = self.hovered_line.and_then(|h| self.hunk_idx_for_line(h));
        let line_hunk_idx = line_num.and_then(|n| self.hunk_idx_for_line(n));
        let is_hovered_hunk = line_hunk_idx
            .zip(hovered_hunk_idx)
            .is_some_and(|(a, b)| a == b);
        let is_untracked =
            matches!(self.current_change_type, Some(ChangeType::Added))
                && matches!(self.current_change_section, Some(ChangeSection::Unstaged));
        let show_hunk_highlight = is_hovered_hunk && !is_untracked;

        let content_text_rc: Rc<str> = Rc::from(content_text);

        div()
            .id(("diff-row", idx))
            .flex()
            .whitespace_nowrap()
            .when_some(bg_color, |el, color| el.bg(color))
            .when(show_hunk_highlight && bg_color.is_none(), |el| {
                el.bg(rgb(BG_SURFACE0))
            })
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
                    .bg(if show_hunk_highlight {
                        rgb(BLUE)
                    } else {
                        gutter_strip_color
                    }),
            )
            .when_some(old_line_num, |el, oln| {
                el.child(
                    div()
                        .w(px(40.0))
                        .flex_shrink_0()
                        .text_right()
                        .pr_2()
                        .bg(gutter_bg)
                        .text_color(rgb(TEXT_MUTED))
                        .child(oln.map(|n| n.to_string()).unwrap_or_default()),
                )
            })
            .child(
                div()
                    .id(("diff-line", idx))
                    .w(px(40.0))
                    .flex_shrink_0()
                    .text_right()
                    .pr_2()
                    .bg(gutter_bg)
                    .text_color(rgb(TEXT_MUTED))
                    .when(line_num.is_some(), |el| {
                        el.cursor_pointer()
                            .hover(|el| el.text_color(rgb(BLUE)))
                    })
                    // 右クリック行番号: path:line をターミナルに送信
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(
                            move |this, _, _, cx| {
                                if let Some(num) = line_num {
                                    if let Some(ref path) = this.display_path {
                                        cx.emit(SendToTerminalEvent(
                                            format!("`{}:{}`", path, num),
                                        ));
                                    }
                                }
                            },
                        ),
                    )
                    .child(line_num.map(|n| n.to_string()).unwrap_or_default()),
            )
            .child(
                div()
                    .w(px(16.0))
                    .flex_shrink_0()
                    .text_center()
                    .text_color(match prefix {
                        "+" => rgb(GREEN),
                        "-" => rgb(RED),
                        _ => rgb(TEXT_MUTED),
                    })
                    .child(prefix),
            )
            .child(
                div()
                    .id(("diff-content", idx))
                    .flex_1()
                    .min_w_0()
                    .cursor_pointer()
                    // 右クリックコンテンツ: クリック位置の識別子をターミナルに送信
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener({
                            let text = content_text_rc.clone();
                            move |this, event: &gpui::MouseDownEvent, _, cx| {
                                // split右パネルのコンテンツ左端を推定
                                let fixed_ui = 540.0; // sidebar(224) + handle(4) + handle(4) + filelist(308)
                                let diff_area = this.viewport_width - fixed_ui;
                                let gutter = 67.0; // strip(3) + line_num(40) + prefix(16) + pl_2(8)
                                let content_x = 228.0 + diff_area * this.diff_split_ratio + gutter;

                                let word = extract_word_at_click(
                                    &text,
                                    f32::from(event.position.x),
                                    content_x,
                                );
                                if !word.is_empty() {
                                    cx.emit(SendToTerminalEvent(format!("`{}`", word)));
                                }
                            }
                        }),
                    )
                    .child(content),
            )
    }



    /// Split diff の右側（After/追加）パネルの1行をレンダリングする。
    /// インタラクティブ操作（ステージング・go-to-definition）を含む。
    fn render_split_right_line(
        &self,
        idx: usize,
        line: &SplitDiffLine,
        highlight_doc: Option<&HighlightedDoc>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (bg_color, text_color) = match line.line_type {
            DiffLineType::Added => (Some(rgb(DIFF_ADDED_BG)), rgb(TEXT)),
            DiffLineType::Removed => (Some(rgb(DIFF_REMOVED_BG)), rgb(TEXT)),
            DiffLineType::Context => (None, rgb(TEXT)),
            DiffLineType::Filler => (Some(rgb(DIFF_FILLER_BG)), rgb(TEXT_MUTED)),
        };
        let gutter_strip_color = match line.line_type {
            DiffLineType::Added => rgb(GREEN),
            DiffLineType::Removed => rgb(RED),
            DiffLineType::Context | DiffLineType::Filler => rgb(BG_MANTLE),
        };
        let gutter_bg = match line.line_type {
            DiffLineType::Added => rgb(DIFF_ADDED_GUTTER_BG),
            DiffLineType::Removed => rgb(DIFF_REMOVED_GUTTER_BG),
            DiffLineType::Context => rgb(BG_MANTLE),
            DiffLineType::Filler => rgb(DIFF_FILLER_BG),
        };
        let prefix = match line.line_type {
            DiffLineType::Added => "+",
            DiffLineType::Removed => "-",
            DiffLineType::Context | DiffLineType::Filler => " ",
        };
        let char_highlight = match line.line_type {
            DiffLineType::Added => DIFF_ADDED_WORD_BG,
            DiffLineType::Removed => DIFF_REMOVED_WORD_BG,
            DiffLineType::Context | DiffLineType::Filler => 0,
        };
        let line_num = line.new_line_num;
        let highlighted_line = Self::highlighted_line_for_number(highlight_doc, line_num);

        // 追加行かつgo-to-definition可能な場合はInteractiveTextを使う
        // レイアウト (flex_1/min_w_0) は build_diff_row 側で制御するため、ここでは表示属性のみ設定
        let content_element = if line.char_changes.is_empty() {
            if let (Some(hl), Some(ln)) = (highlighted_line, line_num) {
                div()
                    .pl_2()
                    .text_color(text_color)
                    .child(self.render_interactive_text(ln, &line.content, hl, "diff-right", cx))
            } else {
                div()
                    .pl_2()
                    .text_color(text_color)
                    .child(Self::styled_text_with_char_changes(
                        &line.content, highlighted_line, &[], 0,
                    ))
            }
        } else {
            div()
                .pl_2()
                .text_color(text_color)
                .child(Self::styled_text_with_char_changes(
                    &line.content,
                    highlighted_line,
                    &line.char_changes,
                    char_highlight,
                ))
        };

        self.build_diff_row(
            idx, line_num, None, bg_color, gutter_bg, gutter_strip_color, prefix, 1.0,
            content_element, &line.content, cx,
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.viewport_width = f32::from(window.viewport_size().width);
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

/// ウィンドウx座標からテキスト内の識別子（単語）を推定して返す。
/// content_origin_x: コンテンツ領域の左端ウィンドウx座標。
fn extract_word_at_click(text: &str, click_x: f32, content_origin_x: f32) -> String {
    // text_xs() = 12px モノスペースフォント、文字幅 ≈ 7.2px
    let char_width: f32 = 7.2;
    let local_x = click_x - content_origin_x;
    if local_x < 0.0 {
        return String::new();
    }
    let char_idx = (local_x / char_width).floor() as usize;

    if char_idx >= text.len() {
        return String::new();
    }

    let bytes = text.as_bytes();
    let is_ident_char = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b':' || b == b'.';

    if !is_ident_char(bytes[char_idx]) {
        return String::new();
    }

    let start = (0..char_idx)
        .rev()
        .take_while(|&i| is_ident_char(bytes[i]))
        .last()
        .unwrap_or(char_idx);
    let end = (char_idx..bytes.len())
        .take_while(|&i| is_ident_char(bytes[i]))
        .last()
        .map(|i| i + 1)
        .unwrap_or(char_idx + 1);

    text[start..end].to_string()
}
