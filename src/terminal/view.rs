//! Terminal view for GPUI rendering
//!
//! This module provides the main TerminalView struct and its implementation.

use super::Terminal;
use super::input_probe;
use crate::terminal::TerminalEvent;
use crate::terminal::element::{
    DEFAULT_CELL_HEIGHT, DEFAULT_CELL_WIDTH, MULTI_CLICK_THRESHOLD_MS, SCROLL_LINES_WHEEL,
    TERMINAL_PADDING, TerminalElement, TerminalLayout,
};
use crate::theme::{self, *};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point as AlacPoint};
use alacritty_terminal::term::{TermMode, cell::Flags as CellFlags};
use alacritty_terminal::vte::ansi::{Color as AnsiColor, CursorShape, NamedColor};
use gpui::prelude::FluentBuilder;
use gpui::{
    App, AsyncApp, Bounds, ClipboardItem, Context, EntityInputHandler, FocusHandle, Focusable,
    Hsla, InteractiveElement, IntoElement, MouseButton, MouseMoveEvent, ParentElement, Pixels,
    Point, Render, ScrollWheelEvent, Size, Styled, Subscription, UTF16Selection, WeakEntity,
    Window, div, px, rgb,
};
use regex::Regex;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

static URL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https?://[^\s\x00-\x1f\x7f<>"'\)\]]+"#).unwrap());

/// A URL detected in the terminal output.
/// Cell-to-URL mapping is stored separately in `TerminalView::url_cells`.
#[derive(Clone, Debug)]
pub(super) struct DetectedUrl {
    pub url: String,
}

/// Cached cell data from terminal grid.
/// Stores raw AnsiColor to keep cache updates cheap (no float conversion).
/// Color conversion to Hsla happens once per frame in paint_cells.
#[derive(Clone)]
pub(super) struct CachedCell {
    pub c: char,
    pub fg: AnsiColor,
    pub bg: AnsiColor,
    pub flags: CellFlags,
}

/// Cached terminal content snapshot.
/// Cells are wrapped in `Arc` so that `TerminalLayout` can share them
/// without a deep copy (Arc::clone is O(1) vs O(rows×cols) for Vec clone).
#[derive(Clone)]
struct CachedContent {
    cells: Arc<Vec<Vec<CachedCell>>>,
    /// Cursor position (line, column) in grid coordinates
    cursor: (i32, usize),
    cursor_shape: CursorShape,
    cursor_visible: bool,
    display_offset: i32,
    lines: usize,
}

/// Selection state for text selection in the terminal
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TerminalSelection {
    /// Start point (line, column)
    pub(super) start: (i32, usize),
    /// End point (line, column)
    pub(super) end: (i32, usize),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PendingCtrlCState {
    pub armed_at: Instant,
    pub rapid_tap_detected: bool,
    pub ctrl_c_released: bool,
}

#[derive(Clone, Debug)]
struct PendingAccessibilityCommit {
    text: String,
    armed_at: Instant,
}

impl TerminalSelection {
    /// Returns the selection normalized so start <= end
    fn normalized(&self) -> (i32, usize, i32, usize) {
        let (start_line, start_col) = self.start;
        let (end_line, end_col) = self.end;
        if start_line < end_line || (start_line == end_line && start_col <= end_col) {
            (start_line, start_col, end_line, end_col)
        } else {
            (end_line, end_col, start_line, start_col)
        }
    }

    /// Check if a position is within the selection
    pub(super) fn contains(&self, line: i32, col: usize) -> bool {
        let (start_line, start_col, end_line, end_col) = self.normalized();
        if line < start_line || line > end_line {
            return false;
        }
        if line == start_line && line == end_line {
            col >= start_col && col <= end_col
        } else if line == start_line {
            col >= start_col
        } else if line == end_line {
            col <= end_col
        } else {
            true
        }
    }
}

pub struct TerminalView {
    pub(super) terminal: Option<Arc<Terminal>>,
    pub(super) focus_handle: FocusHandle,
    pub(super) preedit_text: String,
    /// Error message if terminal creation failed
    error_message: Option<String>,
    /// Window title set by terminal application (via OSC 2)
    pub title: Option<String>,
    /// Whether bell was recently triggered (for UI visual feedback)
    pub bell_active: bool,
    /// Current text selection (if any)
    selection: Option<TerminalSelection>,
    /// Whether mouse is currently dragging for selection
    is_dragging: bool,
    /// Last click time for double/triple click detection
    last_click_time: Option<Instant>,
    /// Click count for multi-click detection
    click_count: u8,
    /// Cell dimensions for mouse position to cell conversion
    pub(super) cell_width: f32,
    pub(super) cell_height: f32,
    /// Terminal content origin for mouse coordinate conversion
    pub(super) content_origin: (f32, f32),
    /// Cached terminal content to ensure consistent state during rendering.
    /// Updated after all events are processed, used by build_layout().
    cached_content: Option<CachedContent>,
    /// URLs detected in the current terminal content
    pub(super) detected_urls: Vec<DetectedUrl>,
    /// (screen_line, col) → detected_urls index for O(1) URL lookup during layout build.
    /// Wrapped in Arc so build_layout can share without deep-copying the map every frame.
    pub(super) url_cells: Arc<HashMap<(usize, usize), usize>>,
    /// Index of the URL currently hovered with Ctrl held
    pub(super) hovered_url_index: Option<usize>,
    /// Whether Ctrl key is currently held (for lazy URL detection)
    pub(super) ctrl_held: bool,
    /// Pending Ctrl+C intent resolution.
    /// Ctrl+C is held until we can disambiguate:
    /// - normal manual Ctrl+C -> flush as SIGINT
    /// - automation Ctrl+C->paste chain -> canceled by paste
    pub(super) pending_ctrl_c: Option<PendingCtrlCState>,
    /// Caret bounds for platform text-input integrations (IME/voice tools).
    pub(super) input_cursor_bounds: Option<Bounds<Pixels>>,
    /// Lightweight text shadow exposed through InputHandler APIs.
    /// This is not terminal history; it only reflects recent committed input.
    input_shadow_text: String,
    /// Recently committed UTF-16 range in `input_shadow_text`.
    /// Some automation tools query selected range to verify paste success.
    recent_committed_range: Option<(Range<usize>, Instant)>,
    /// Pending accessibility text commit to confirm against terminal output.
    pending_accessibility_commit: Option<PendingAccessibilityCommit>,
    /// Focus-in subscription (kept alive for focus reporting)
    focus_in_subscription: Option<Subscription>,
    /// Focus-out subscription (kept alive for focus reporting)
    focus_out_subscription: Option<Subscription>,
}

impl TerminalView {
    pub(super) fn input_trace_enabled() -> bool {
        static ENABLED: LazyLock<bool> = LazyLock::new(|| {
            std::env::var("SASHIKI_TERMINAL_INPUT_TRACE")
                .map(|v| v != "0")
                .unwrap_or(false)
        });
        *ENABLED
    }

    fn input_trace_verbose_enabled() -> bool {
        static ENABLED: LazyLock<bool> = LazyLock::new(|| {
            std::env::var("SASHIKI_TERMINAL_INPUT_TRACE_VERBOSE")
                .map(|v| v != "0")
                .unwrap_or(false)
        });
        *ENABLED
    }

    fn should_emit_input_trace(message: &str) -> bool {
        if Self::input_trace_verbose_enabled() {
            return true;
        }
        message.starts_with("action ctrl-c")
            || message.starts_with("action ctrl-v")
            || message.starts_with("paste_text called")
            || message.starts_with("drop accessibility_commit timeout")
    }

    pub(super) fn trace_input_event(message: impl AsRef<str>) {
        let message = message.as_ref();
        if Self::input_trace_enabled() && Self::should_emit_input_trace(message) {
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            eprintln!("[terminal-input {ms}] {}", message);
        }
    }

    /// Create a new terminal with a specific working directory
    pub fn new_with_directory(
        working_directory: std::path::PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_internal(Some(working_directory), cx)
    }

    fn new_internal(working_directory: Option<std::path::PathBuf>, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        match Terminal::new(working_directory) {
            Ok((terminal, event_rx)) => {
                let terminal = Arc::new(terminal);

                // Event-based refresh: batch process all pending events before updating
                // This prevents catching intermediate states during rapid event sequences
                cx.spawn(
                    async move |this: WeakEntity<TerminalView>, cx: &mut AsyncApp| {
                        while let Ok(event) = event_rx.recv().await {
                            // Collect all pending events first
                            let mut events = vec![event];
                            while let Ok(e) = event_rx.try_recv() {
                                events.push(e);
                            }

                            let should_break = cx.update(|cx| {
                                if let Some(this) = this.upgrade() {
                                    this.update(cx, move |view, cx: &mut Context<TerminalView>| {
                                        let mut need_notify = false;
                                        let mut need_cache_refresh = false;

                                        for event in events {
                                            match event {
                                                TerminalEvent::Wakeup => {
                                                    need_cache_refresh = true;
                                                    need_notify = true;
                                                }
                                                TerminalEvent::Bell => {
                                                    view.bell_active = true;
                                                    need_notify = true;
                                                }
                                                TerminalEvent::Exit => {
                                                    // Terminal exited - handled elsewhere.
                                                    need_cache_refresh = true;
                                                    need_notify = true;
                                                }
                                                TerminalEvent::Title(title) => {
                                                    view.title = Some(title);
                                                    need_notify = true;
                                                }
                                                TerminalEvent::ClipboardStore(text) => {
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(text),
                                                    );
                                                }
                                                TerminalEvent::ClipboardLoad(formatter) => {
                                                    if let Some(content) = cx
                                                        .read_from_clipboard()
                                                        .and_then(|c| c.text())
                                                    {
                                                        let response = formatter(&content);
                                                        view.write_to_terminal(response.as_bytes());
                                                    } else {
                                                        let response = formatter("");
                                                        view.write_to_terminal(response.as_bytes());
                                                    }
                                                }
                                            }
                                        }

                                        if need_cache_refresh {
                                            view.update_content_cache();
                                        }
                                        if need_notify {
                                            cx.notify();
                                        }
                                    });
                                    false
                                } else {
                                    true
                                }
                            });
                            if should_break {
                                break;
                            }
                        }
                    },
                )
                .detach();

                let mut view = Self {
                    terminal: Some(terminal),
                    focus_handle,
                    preedit_text: String::new(),
                    error_message: None,
                    title: None,
                    bell_active: false,
                    selection: None,
                    is_dragging: false,
                    last_click_time: None,
                    click_count: 0,
                    cell_width: DEFAULT_CELL_WIDTH,
                    cell_height: DEFAULT_CELL_HEIGHT,
                    content_origin: (0.0, 0.0),
                    cached_content: None,
                    detected_urls: Vec::new(),
                    url_cells: Arc::new(HashMap::new()),
                    hovered_url_index: None,
                    ctrl_held: false,
                    pending_ctrl_c: None,
                    input_cursor_bounds: None,
                    input_shadow_text: String::new(),
                    recent_committed_range: None,
                    pending_accessibility_commit: None,
                    focus_in_subscription: None,
                    focus_out_subscription: None,
                };
                // Capture initial terminal state so build_layout always has cached data
                view.update_content_cache();
                view
            }
            Err(e) => Self {
                terminal: None,
                focus_handle,
                preedit_text: String::new(),
                error_message: Some(format!("Failed to create terminal: {}", e)),
                title: None,
                bell_active: false,
                selection: None,
                is_dragging: false,
                last_click_time: None,
                click_count: 0,
                cell_width: DEFAULT_CELL_WIDTH,
                cell_height: DEFAULT_CELL_HEIGHT,
                content_origin: (0.0, 0.0),
                cached_content: None,
                detected_urls: Vec::new(),
                url_cells: Arc::new(HashMap::new()),
                hovered_url_index: None,
                ctrl_held: false,
                pending_ctrl_c: None,
                input_cursor_bounds: None,
                input_shadow_text: String::new(),
                recent_committed_range: None,
                pending_accessibility_commit: None,
                focus_in_subscription: None,
                focus_out_subscription: None,
            },
        }
    }

    /// Shutdown the terminal by sending exit command to the shell
    pub fn shutdown(&self) {
        if let Some(ref terminal) = self.terminal {
            terminal.shutdown();
        }
    }

    /// Write text to the terminal (for pasting from file view)
    pub fn write_text(&self, text: &str) {
        self.write_to_terminal(text.as_bytes());
    }

    /// Write bytes to the terminal (used by action handlers)
    pub(super) fn write_to_terminal(&self, data: &[u8]) {
        if let Some(ref terminal) = self.terminal {
            terminal.write(data);
        }
    }

    /// Paste text to terminal, wrapping with bracketed paste sequences if enabled.
    /// Bracketed paste (DECSET 2004) lets the shell distinguish pasted text from typed text.
    pub(super) fn paste_text(&mut self, text: &str) {
        Self::trace_input_event(format!("paste_text called len={}", text.len()));
        self.cancel_pending_ctrl_c();

        if self.is_mode_set(TermMode::BRACKETED_PASTE) {
            // Strip ESC from pasted content to prevent escape sequence injection.
            let mut data = b"\x1b[200~".to_vec();
            let sanitized = text.replace('\x1b', "");
            data.extend_from_slice(sanitized.as_bytes());
            data.extend_from_slice(b"\x1b[201~");
            self.write_to_terminal(&data);
            self.update_input_shadow_text(&sanitized);
        } else {
            self.write_to_terminal(text.as_bytes());
            self.update_input_shadow_text(text);
        }
        self.arm_accessibility_commit(text);
    }

    pub(super) fn is_mode_set(&self, mode: TermMode) -> bool {
        self.terminal
            .as_ref()
            .map(|t| t.mode().contains(mode))
            .unwrap_or(false)
    }

    /// Number of lines to scroll per page (Shift+PageUp/Down).
    /// Uses current screen height minus 1 (standard terminal behavior),
    /// falling back to 10 lines if terminal size is unknown.
    pub(super) fn page_scroll_lines(&self) -> i32 {
        self.cached_content
            .as_ref()
            .map(|c| (c.lines as i32).saturating_sub(1).max(1))
            .unwrap_or(10)
    }

    /// Update cached content from terminal.
    /// Called after event processing to capture the complete terminal state.
    /// Similar to Zed's make_content() - captures all cells, cursor, and display state.
    pub(super) fn update_content_cache(&mut self) {
        let Some(ref terminal) = self.terminal else {
            return;
        };

        terminal.with_term(|term| {
            let render_content = term.renderable_content();
            let cursor_point = render_content.cursor.point;
            let cursor_shape = render_content.cursor.shape;
            let display_offset = render_content.display_offset as i32;

            let grid = term.grid();
            let cols = grid.columns();
            let lines = grid.screen_lines();

            // Copy raw cell data without color conversion.
            // Conversion to Hsla is deferred to paint_cells (runs once per frame).
            let mut cells = Vec::with_capacity(lines);
            for line_idx in 0..lines {
                let actual_line = line_idx as i32 - display_offset;
                let mut row = Vec::with_capacity(cols);
                for col_idx in 0..cols {
                    let point = AlacPoint::new(Line(actual_line), Column(col_idx));
                    let cell = &grid[point];
                    row.push(CachedCell {
                        c: cell.c,
                        fg: cell.fg,
                        bg: cell.bg,
                        flags: cell.flags,
                    });
                }
                cells.push(row);
            }

            let cursor_visible = term.mode().contains(TermMode::SHOW_CURSOR);

            self.cached_content = Some(CachedContent {
                cells: Arc::new(cells),
                cursor: (cursor_point.line.0, cursor_point.column.0),
                cursor_shape,
                cursor_visible,
                display_offset,
                lines,
            });
        });

        // Only run URL detection when Ctrl is held; URLs are only needed for Ctrl+click.
        // During AI streaming this avoids scanning all terminal rows on every wakeup.
        if self.ctrl_held {
            self.detect_urls_from_cache();
        }
        self.resolve_pending_accessibility_commit();
    }

    fn arm_accessibility_commit(&mut self, text: &str) {
        let sanitized: String = text
            .chars()
            .filter(|c| *c != '\0' && *c != '\x1b')
            .collect::<String>()
            .trim()
            .to_string();
        if sanitized.is_empty() {
            return;
        }

        Self::trace_input_event(format!(
            "arm accessibility_commit len={} text_preview={:?}",
            sanitized.chars().count(),
            sanitized.chars().take(16).collect::<String>()
        ));

        self.pending_accessibility_commit = Some(PendingAccessibilityCommit {
            text: sanitized.clone(),
            armed_at: Instant::now(),
        });
        // Keep compatibility with clients that expect immediate accessibility updates,
        // then send another notification once terminal output confirms the commit.
        input_probe::notify_accessibility_text_committed(&sanitized);
        Self::trace_input_event("accessibility_commit immediate notify");
    }

    fn resolve_pending_accessibility_commit(&mut self) {
        const ACCESSIBILITY_COMMIT_TIMEOUT: Duration = Duration::from_secs(3);

        let Some(pending) = self.pending_accessibility_commit.clone() else {
            return;
        };

        if self.visible_text_contains(&pending.text) {
            Self::trace_input_event(format!(
                "confirm accessibility_commit age_ms={} len={}",
                pending.armed_at.elapsed().as_millis(),
                pending.text.chars().count()
            ));
            input_probe::notify_accessibility_text_committed(&pending.text);
            self.pending_accessibility_commit = None;
            return;
        }

        if pending.armed_at.elapsed() >= ACCESSIBILITY_COMMIT_TIMEOUT {
            Self::trace_input_event(format!(
                "drop accessibility_commit timeout age_ms={} len={}",
                pending.armed_at.elapsed().as_millis(),
                pending.text.chars().count()
            ));
            self.pending_accessibility_commit = None;
        }
    }

    fn visible_text_contains(&self, needle: &str) -> bool {
        let Some(cached) = self.cached_content.as_ref() else {
            return false;
        };
        if needle.is_empty() {
            return false;
        }

        cached.cells.iter().any(|row| {
            let line: String = row
                .iter()
                // Skip NUL spacer cells used for wide chars (CJK), otherwise
                // "こんにちは" becomes "こ ん に ち は" and commit detection fails.
                .filter_map(|cell| (cell.c != '\0').then_some(cell.c))
                .collect();
            line.contains(needle)
        })
    }

    fn update_input_shadow_text(&mut self, text: &str) {
        let sanitized: String = text.chars().filter(|c| *c != '\0').collect();
        self.input_shadow_text = sanitized;
        let utf16_len = self.input_shadow_text.encode_utf16().count();
        self.recent_committed_range = if utf16_len > 0 {
            Some((0..utf16_len, Instant::now()))
        } else {
            None
        };
        Self::trace_input_event(format!("input_shadow_text updated utf16_len={utf16_len}"));
    }

    fn slice_utf16(text: &str, range: Range<usize>) -> String {
        if text.is_empty() || range.start >= range.end {
            return String::new();
        }

        let mut utf16_offset = 0usize;
        let mut byte_start: Option<usize> = None;
        let mut byte_end: Option<usize> = None;

        for (idx, ch) in text.char_indices() {
            let width = ch.len_utf16();
            let next = utf16_offset + width;

            if byte_start.is_none() && range.start <= utf16_offset {
                byte_start = Some(idx);
            }
            if byte_end.is_none() && range.end <= utf16_offset {
                byte_end = Some(idx);
                break;
            }
            if byte_start.is_none() && range.start < next {
                byte_start = Some(idx);
            }
            if byte_end.is_none() && range.end <= next {
                byte_end = Some(idx + ch.len_utf8());
                break;
            }

            utf16_offset = next;
        }

        let start = byte_start.unwrap_or(text.len());
        let end = byte_end.unwrap_or(text.len()).max(start);
        text[start..end].to_string()
    }

    /// Scan cached content for URLs using regex and record their screen positions.
    pub(super) fn detect_urls_from_cache(&mut self) {
        self.detected_urls.clear();

        let Some(ref cached) = self.cached_content else {
            self.url_cells = Arc::new(HashMap::new());
            return;
        };

        let mut url_cells = HashMap::new();
        // Reuse a single String buffer to avoid allocating one per row
        let mut line_text = String::new();

        for (line_idx, row) in cached.cells.iter().enumerate() {
            line_text.clear();
            line_text.extend(row.iter().map(|cell| if cell.c == '\0' { ' ' } else { cell.c }));

            for mat in URL_REGEX.find_iter(&line_text) {
                // Strip trailing punctuation that is commonly not part of URLs
                // (e.g. "Visit https://example.com." should not include the period)
                let url_str = mat
                    .as_str()
                    .trim_end_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?'));
                if url_str.len() <= "https://".len() {
                    continue;
                }

                // Convert byte offsets to column indices.
                // Because the line is built char-by-char from the grid, each char
                // maps 1:1 to a column only when all characters are single-byte.
                // Use char_indices for correct mapping.
                let start_col = line_text[..mat.start()].chars().count();
                let end_col = start_col + url_str.chars().count() - 1;

                let url_idx = self.detected_urls.len();
                self.detected_urls.push(DetectedUrl {
                    url: url_str.to_string(),
                });

                // Build O(1) cell lookup: store every (line, col) that belongs to this URL.
                // URLs are always single-line (detected per-line), so start.0 == end.0.
                for col in start_col..=end_col {
                    url_cells.insert((line_idx, col), url_idx);
                }
            }
        }
        self.url_cells = Arc::new(url_cells);
    }

    /// Get the text content of the current selection
    pub(super) fn get_selected_text(&self) -> Option<String> {
        let selection = self.selection?;
        let terminal = self.terminal.as_ref()?;

        let (start_line, start_col, end_line, end_col) = selection.normalized();
        let mut result = String::new();

        terminal.with_term(|term| {
            let content = term.grid();
            let cols = content.columns();
            let total_lines = content.screen_lines() as i32;
            let history = content.history_size() as i32;

            for line_idx in start_line..=end_line {
                // Selection is in grid coordinates: valid range is -history..screen_lines
                if line_idx < -history || line_idx >= total_lines {
                    continue;
                }

                let col_start = if line_idx == start_line { start_col } else { 0 };
                let col_end = if line_idx == end_line {
                    end_col.min(cols - 1)
                } else {
                    cols - 1
                };

                for col_idx in col_start..=col_end {
                    let point = AlacPoint::new(Line(line_idx), Column(col_idx));
                    let cell = &content[point];
                    let c = if cell.c == '\0' { ' ' } else { cell.c };
                    result.push(c);
                }

                // Add newline between lines (but not after the last line)
                if line_idx < end_line {
                    result.push('\n');
                }
            }
        });

        // Trim trailing whitespace from each line
        let result: String = result
            .lines()
            .map(|line| line.trim_end())
            .collect::<Vec<_>>()
            .join("\n");

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Clear the current text selection
    pub(super) fn clear_selection(&mut self) {
        self.selection = None;
    }

    fn send_mouse_event_to_pty(&self, button: u8, col: usize, row: usize, press: bool) {
        let col1 = col + 1;
        let row1 = row + 1;
        let seq = if self.is_mode_set(TermMode::SGR_MOUSE) {
            let suffix = if press { 'M' } else { 'm' };
            format!("\x1b[<{button};{col1};{row1}{suffix}")
        } else {
            // X10 mouse encoding: only works for col/row < 96 (byte would overflow readable range)
            if col1 > 95 || row1 > 95 {
                return;
            }
            let suffix = if press { 'M' } else { 'm' };
            let _ = suffix; // X10 has no release
            format!(
                "\x1b[M{}{}{}",
                (button + 32) as char,
                (col1 as u8 + 32) as char,
                (row1 as u8 + 32) as char
            )
        };
        self.write_to_terminal(seq.as_bytes());
    }

    // ========================================================================
    // Mouse handling
    // ========================================================================

    /// Convert mouse position (window coordinates) to cell coordinates
    fn position_to_cell(&self, x: f32, y: f32) -> (i32, usize) {
        // Subtract terminal content origin and padding to get relative position
        let x = (x - self.content_origin.0 - TERMINAL_PADDING).max(0.0);
        let y = (y - self.content_origin.1 - TERMINAL_PADDING).max(0.0);
        let col = (x / self.cell_width) as usize;
        let line = (y / self.cell_height) as i32;
        (line, col)
    }

    /// Handle Ctrl+click to open a URL under the cursor.
    /// Returns true if a URL was opened (so the caller can skip selection logic).
    fn try_open_url_at(&self, screen_line: usize, col: usize) -> bool {
        if let Some(&url_idx) = self.url_cells.get(&(screen_line, col)) {
            if let Some(url) = self.detected_urls.get(url_idx) {
                let _ = open::that(&url.url);
                return true;
            }
        }
        false
    }

    /// Update hovered URL index based on current mouse position and Ctrl state.
    fn update_hovered_url(&mut self, screen_line: usize, col: usize, ctrl: bool) {
        if ctrl {
            // If URL detection hasn't run yet (e.g. Ctrl was held before the window
            // received a modifiers-changed event), scan now as a fallback.
            if !self.ctrl_held {
                self.ctrl_held = true;
                self.detect_urls_from_cache();
            }
            self.hovered_url_index = self.url_cells.get(&(screen_line, col)).copied();
        } else {
            self.hovered_url_index = None;
        }
    }

    /// Handle mouse down event for selection
    fn handle_mouse_down(&mut self, x: f32, y: f32, ctrl: bool, cx: &mut Context<Self>) {
        let (screen_line, col) = self.position_to_cell(x, y);

        // If TUI app has mouse reporting enabled, forward the click to PTY.
        if !ctrl && self.is_mode_set(TermMode::MOUSE_REPORT_CLICK) {
            self.send_mouse_event_to_pty(0, col, screen_line as usize, true);
            return;
        }

        // Ctrl+click opens the URL under the cursor
        if ctrl && self.try_open_url_at(screen_line as usize, col) {
            return;
        }
        // Convert screen coordinates to grid coordinates so selection
        // remains stable when the viewport is scrolled back
        let display_offset = self
            .cached_content
            .as_ref()
            .map(|c| c.display_offset)
            .unwrap_or(0);
        let line = screen_line - display_offset;
        let now = Instant::now();

        // Detect double/triple click
        let is_multi_click = self
            .last_click_time
            .map(|t| now.duration_since(t).as_millis() < MULTI_CLICK_THRESHOLD_MS)
            .unwrap_or(false);

        if is_multi_click {
            self.click_count = (self.click_count % 3) + 1;
        } else {
            self.click_count = 1;
        }
        self.last_click_time = Some(now);

        match self.click_count {
            1 => {
                // Single click - start new selection
                self.selection = Some(TerminalSelection {
                    start: (line, col),
                    end: (line, col),
                });
                self.is_dragging = true;
            }
            2 => {
                // Double click - select word
                if let Some(ref terminal) = self.terminal {
                    let (word_start, word_end) = self.find_word_boundaries(terminal, line, col);
                    self.selection = Some(TerminalSelection {
                        start: (line, word_start),
                        end: (line, word_end),
                    });
                }
            }
            3 => {
                // Triple click - select line
                if let Some(ref terminal) = self.terminal {
                    let cols = terminal.with_term(|term| term.grid().columns());
                    self.selection = Some(TerminalSelection {
                        start: (line, 0),
                        end: (line, cols.saturating_sub(1)),
                    });
                }
            }
            _ => {}
        }

        cx.notify();
    }

    /// Find word boundaries at given position
    fn find_word_boundaries(&self, terminal: &Terminal, line: i32, col: usize) -> (usize, usize) {
        terminal.with_term(|term| {
            let content = term.grid();
            let cols = content.columns();
            let total_lines = content.screen_lines() as i32;
            let history = content.history_size() as i32;

            // line is in grid coordinates: valid range is -history..screen_lines
            if line < -history || line >= total_lines {
                return (col, col);
            }

            // Get character at position
            let get_char = |c: usize| -> char {
                if c >= cols {
                    return ' ';
                }
                let point = AlacPoint::new(Line(line), Column(c));
                let cell = &content[point];
                if cell.c == '\0' { ' ' } else { cell.c }
            };

            // Check if character is part of a word
            let is_word_char = |c: char| -> bool { c.is_alphanumeric() || c == '_' };

            let current_char = get_char(col);
            let is_word = is_word_char(current_char);

            // Find start of word/non-word sequence
            let mut start = col;
            while start > 0 {
                let prev_char = get_char(start - 1);
                if is_word_char(prev_char) != is_word {
                    break;
                }
                start -= 1;
            }

            // Find end of word/non-word sequence
            let mut end = col;
            while end < cols - 1 {
                let next_char = get_char(end + 1);
                if is_word_char(next_char) != is_word {
                    break;
                }
                end += 1;
            }

            (start, end)
        })
    }

    /// Handle mouse drag event for selection
    fn handle_mouse_drag(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        if !self.is_dragging {
            return;
        }

        let (screen_line, col) = self.position_to_cell(x, y);
        let display_offset = self
            .cached_content
            .as_ref()
            .map(|c| c.display_offset)
            .unwrap_or(0);
        let line = screen_line - display_offset;

        if let Some(ref mut selection) = self.selection {
            selection.end = (line, col);
        }

        cx.notify();
    }

    /// Handle mouse up event
    fn handle_mouse_up(&mut self, _cx: &mut Context<Self>) {
        if self.is_mode_set(TermMode::MOUSE_REPORT_CLICK) {
            // Release events are sent as button=3 in X10, or original button in SGR.
            // Since we always used left button (0) for down, send release for button 0.
            // We don't track the last pressed cell, so use cursor position as fallback for now.
            self.is_dragging = false;
            return;
        }

        self.is_dragging = false;

        // Clear selection if it's just a single click (no actual range selected)
        if let Some(ref selection) = self.selection {
            if selection.start == selection.end {
                self.selection = None;
            }
        }
    }

    /// Handle scroll wheel event
    fn handle_scroll(&mut self, delta_y: f32, cx: &mut Context<Self>) {
        // Read TermMode once to avoid acquiring FairMutex multiple times per scroll event.
        let mode = self.terminal.as_ref().map(|t| t.mode());

        let mouse_mode = mode
            .map(|m| m.intersects(TermMode::MOUSE_MODE))
            .unwrap_or(false);

        if mouse_mode {
            // Forward scroll to PTY as mouse button 64 (up) or 65 (down).
            let button = if delta_y > 0.0 { 64u8 } else { 65u8 };
            self.send_mouse_event_to_pty(button, 0, 0, true);
            return;
        }

        let alt_screen = mode
            .map(|m| m.contains(TermMode::ALT_SCREEN))
            .unwrap_or(false);
        let alt_scroll = mode
            .map(|m| m.contains(TermMode::ALTERNATE_SCROLL))
            .unwrap_or(false);

        if alt_screen && alt_scroll {
            // Send arrow keys instead of scrolling (for apps like less in alt screen).
            let lines = SCROLL_LINES_WHEEL;
            let key = if delta_y > 0.0 {
                b"\x1b[A" as &[u8]
            } else {
                b"\x1b[B" as &[u8]
            };
            for _ in 0..lines {
                self.write_to_terminal(key);
            }
            return;
        }

        if let Some(ref terminal) = self.terminal {
            // GPUI scroll: positive delta_y = wheel up = scroll back in history
            // alacritty Scroll::Delta: positive = scroll up (show older content)
            let lines = if delta_y > 0.0 {
                SCROLL_LINES_WHEEL
            } else {
                -SCROLL_LINES_WHEEL
            };
            terminal.scroll(alacritty_terminal::grid::Scroll::Delta(lines));
        } else {
            return;
        }
        // Scroll is a local operation (no PTY event), so we must
        // update the cache manually to reflect the new display_offset
        self.update_content_cache();
        cx.notify();
    }

    // ========================================================================
    // Layout building
    // ========================================================================

    /// Build terminal layout for the paint phase.
    /// Shares cell data from cache via Arc::clone (O(1) instead of O(rows×cols)).
    pub(super) fn build_layout(
        &self,
        cell_width: Pixels,
        line_height: Pixels,
    ) -> Option<TerminalLayout> {
        let cached = self.cached_content.as_ref()?;
        let display_cursor_line = cached.cursor.0 + cached.display_offset;
        Some(TerminalLayout {
            cells: Arc::clone(&cached.cells),
            cell_width,
            line_height,
            cursor_shape: cached.cursor_shape,
            cursor: (display_cursor_line, cached.cursor.1),
            cursor_visible: cached.cursor_visible,
            display_offset: cached.display_offset,
            selection: self.selection,
            url_cells: Arc::clone(&self.url_cells),
            hovered_url_index: self.hovered_url_index,
            preedit_text: self.preedit_text.clone(),
        })
    }

    /// Compute caret bounds used by IME/text-input integrations.
    ///
    /// This uses cursor coordinates from cached terminal content, independent of cursor
    /// visibility/blink state, so platform integrations always have a stable caret anchor.
    pub(super) fn compute_input_cursor_bounds(
        &self,
        origin: Point<Pixels>,
        cell_width: Pixels,
        line_height: Pixels,
    ) -> Option<Bounds<Pixels>> {
        let cached = self.cached_content.as_ref()?;
        let (cursor_line, cursor_col) = cached.cursor;
        let display_cursor_line = cursor_line + cached.display_offset;
        if display_cursor_line < 0 {
            return None;
        }
        let display_cursor_line = display_cursor_line as usize;
        let row = cached.cells.get(display_cursor_line)?;
        if cursor_col >= row.len() {
            return None;
        }

        let cell = &row[cursor_col];
        let width = if cell.flags.contains(CellFlags::WIDE_CHAR) {
            cell_width * 2.0
        } else {
            cell_width
        };

        Some(Bounds::new(
            Point::new(
                origin.x + cell_width * cursor_col,
                origin.y + line_height * display_cursor_line,
            ),
            Size {
                width,
                height: line_height,
            },
        ))
    }
}

// ============================================================================
// Color conversion (free functions, used by element.rs paint path)
// ============================================================================

/// Pre-computed HSLA values for named colors.
/// Eliminates per-cell RGB→HSLA conversion for the most common color type.
static NAMED_COLOR_TABLE: LazyLock<[Hsla; 20]> = LazyLock::new(|| {
    [
        Hsla::from(rgb(theme::ansi::BLACK)),          // 0: Black
        Hsla::from(rgb(theme::ansi::RED)),            // 1: Red
        Hsla::from(rgb(theme::ansi::GREEN)),          // 2: Green
        Hsla::from(rgb(theme::ansi::YELLOW)),         // 3: Yellow
        Hsla::from(rgb(theme::ansi::BLUE)),           // 4: Blue
        Hsla::from(rgb(theme::ansi::MAGENTA)),        // 5: Magenta
        Hsla::from(rgb(theme::ansi::CYAN)),           // 6: Cyan
        Hsla::from(rgb(theme::ansi::WHITE)),          // 7: White
        Hsla::from(rgb(theme::ansi::BRIGHT_BLACK)),   // 8: BrightBlack
        Hsla::from(rgb(theme::ansi::BRIGHT_RED)),     // 9: BrightRed
        Hsla::from(rgb(theme::ansi::BRIGHT_GREEN)),   // 10: BrightGreen
        Hsla::from(rgb(theme::ansi::BRIGHT_YELLOW)),  // 11: BrightYellow
        Hsla::from(rgb(theme::ansi::BRIGHT_BLUE)),    // 12: BrightBlue
        Hsla::from(rgb(theme::ansi::BRIGHT_MAGENTA)), // 13: BrightMagenta
        Hsla::from(rgb(theme::ansi::BRIGHT_CYAN)),    // 14: BrightCyan
        Hsla::from(rgb(theme::ansi::BRIGHT_WHITE)),   // 15: BrightWhite
        Hsla::from(rgb(theme::ansi::FOREGROUND)),     // 16: Foreground
        Hsla::from(rgb(theme::ansi::BACKGROUND)),     // 17: Background
        Hsla::from(rgb(theme::ansi::CURSOR)),         // 18: Cursor
        Hsla::from(rgb(theme::ansi::FOREGROUND)),     // 19: fallback
    ]
});

pub(super) fn ansi_color_to_hsla(color: AnsiColor) -> Hsla {
    match color {
        AnsiColor::Named(named) => named_color_to_hsla(named),
        AnsiColor::Spec(c) => Hsla::from(gpui::Rgba {
            r: c.r as f32 / 255.0,
            g: c.g as f32 / 255.0,
            b: c.b as f32 / 255.0,
            a: 1.0,
        }),
        AnsiColor::Indexed(idx) => indexed_color_to_hsla(idx),
    }
}

pub(super) fn named_color_to_hsla(color: NamedColor) -> Hsla {
    let idx = match color {
        NamedColor::Black => 0,
        NamedColor::Red => 1,
        NamedColor::Green => 2,
        NamedColor::Yellow => 3,
        NamedColor::Blue => 4,
        NamedColor::Magenta => 5,
        NamedColor::Cyan => 6,
        NamedColor::White => 7,
        NamedColor::BrightBlack => 8,
        NamedColor::BrightRed => 9,
        NamedColor::BrightGreen => 10,
        NamedColor::BrightYellow => 11,
        NamedColor::BrightBlue => 12,
        NamedColor::BrightMagenta => 13,
        NamedColor::BrightCyan => 14,
        NamedColor::BrightWhite => 15,
        NamedColor::Foreground => 16,
        NamedColor::Background => 17,
        NamedColor::Cursor => 18,
        _ => 19,
    };
    NAMED_COLOR_TABLE[idx]
}

fn indexed_color_to_hsla(idx: u8) -> Hsla {
    if idx < 16 {
        // First 16 indexed colors map to named colors
        NAMED_COLOR_TABLE[idx as usize]
    } else if idx < 232 {
        // 216 color cube (6x6x6)
        let i = idx - 16;
        let r = (i / 36) % 6;
        let g = (i / 6) % 6;
        let b = i % 6;
        let to_val = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
        Hsla::from(gpui::Rgba {
            r: to_val(r) as f32 / 255.0,
            g: to_val(g) as f32 / 255.0,
            b: to_val(b) as f32 / 255.0,
            a: 1.0,
        })
    } else {
        // 24 grayscale colors
        let gray = 8 + (idx - 232) * 10;
        Hsla::from(gpui::Rgba {
            r: gray as f32 / 255.0,
            g: gray as f32 / 255.0,
            b: gray as f32 / 255.0,
            a: 1.0,
        })
    }
}

// ============================================================================
// Trait implementations
// ============================================================================

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// IME input handler for terminal
impl EntityInputHandler for TerminalView {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let in_alt_screen = self.is_mode_set(TermMode::ALT_SCREEN);
        Self::trace_input_event(format!(
            "text_for_range req={:?} in_alt_screen={}",
            range_utf16, in_alt_screen
        ));
        if in_alt_screen {
            return None;
        }
        let total = self.input_shadow_text.encode_utf16().count();
        let start = range_utf16.start.min(total);
        let end = range_utf16.end.min(total);
        *actual_range = Some(start..end);
        let text = Self::slice_utf16(&self.input_shadow_text, start..end);
        Self::trace_input_event(format!(
            "text_for_range resp_len={} total_utf16={}",
            text.encode_utf16().count(),
            total
        ));
        Some(text)
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        const RECENT_COMMIT_SELECTION_TTL: Duration = Duration::from_secs(4);
        let in_alt_screen = self.is_mode_set(TermMode::ALT_SCREEN);
        Self::trace_input_event(format!("selected_text_range in_alt_screen={in_alt_screen}"));
        if in_alt_screen {
            None
        } else {
            if let Some((range, at)) = self.recent_committed_range.as_ref()
                && at.elapsed() <= RECENT_COMMIT_SELECTION_TTL
                && !range.is_empty()
            {
                Self::trace_input_event(format!(
                    "commit_probe selected_range={}..{} age_ms={}",
                    range.start,
                    range.end,
                    at.elapsed().as_millis()
                ));
                return Some(UTF16Selection {
                    range: range.clone(),
                    reversed: false,
                });
            }
            let len = self.input_shadow_text.encode_utf16().count();
            Some(UTF16Selection {
                range: len..len,
                reversed: false,
            })
        }
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Clear preedit and send committed text to terminal
        self.preedit_text.clear();
        if !text.is_empty() {
            Self::trace_input_event(format!("replace_text_in_range len={}", text.len()));
            self.cancel_pending_ctrl_c();
            self.write_to_terminal(text.as_bytes());
            self.update_input_shadow_text(text);
            self.arm_accessibility_commit(text);
            window.invalidate_character_coordinates();
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Update preedit text (IME composing state)
        self.preedit_text = new_text.to_string();
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let mut caret = self.input_cursor_bounds?;
        if range_utf16.start > 0 {
            caret.origin.x += px(self.cell_width * range_utf16.start as f32);
        }
        let x: f32 = caret.origin.x.into();
        let y: f32 = caret.origin.y.into();
        let w: f32 = caret.size.width.into();
        let h: f32 = caret.size.height.into();
        Self::trace_input_event(format!(
            "bounds_for_range start={} -> ({:.1},{:.1},{:.1},{:.1})",
            range_utf16.start, x, y, w, h
        ));
        Some(caret)
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }

    fn accepts_text_input(&self, _window: &mut Window, _cx: &mut Context<Self>) -> bool {
        Self::trace_input_event("accepts_text_input=true");
        true
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.focus_in_subscription.is_none() {
            let focus_handle = self.focus_handle.clone();
            self.focus_in_subscription =
                Some(cx.on_focus_in(&focus_handle, window, |this, _window, _cx| {
                    if this.is_mode_set(TermMode::FOCUS_IN_OUT) {
                        this.write_to_terminal(b"\x1b[I");
                    }
                }));
        }
        if self.focus_out_subscription.is_none() {
            let focus_handle = self.focus_handle.clone();
            self.focus_out_subscription =
                Some(
                    cx.on_focus_out(&focus_handle, window, |this, _event, _window, _cx| {
                        if this.is_mode_set(TermMode::FOCUS_IN_OUT) {
                            this.write_to_terminal(b"\x1b[O");
                        }
                        // Reset modifier tracking so ctrl_held doesn't get stuck when
                        // the window loses focus (no ModifiersChangedEvent is sent).
                        if this.ctrl_held {
                            this.ctrl_held = false;
                            this.detected_urls.clear();
                            this.url_cells = Arc::new(HashMap::new());
                            this.hovered_url_index = None;
                        }
                    }),
                );
        }

        // Show error message if terminal creation failed
        if let Some(ref error) = self.error_message {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(BG_BASE))
                .child(div().text_color(rgb(RED)).child(error.clone()))
                .into_any_element();
        }

        // Outer div handles focus, key context, and events
        // Uses flex_col layout so children can use flex_1 to fill
        div()
            .id("terminal-view")
            .key_context("Terminal")
            .track_focus(&self.focus_handle)
            .flex_1()
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .when_else(
                self.hovered_url_index.is_some(),
                |d: gpui::Stateful<gpui::Div>| d.cursor_pointer(),
                |d: gpui::Stateful<gpui::Div>| d.cursor_text(),
            )
            .on_key_down(cx.listener(Self::on_terminal_key_down))
            .on_key_up(cx.listener(Self::on_terminal_key_up))
            .on_modifiers_changed(cx.listener(Self::on_terminal_modifiers_changed))
            // Register action handlers for special keys
            .on_action(cx.listener(Self::on_enter))
            .on_action(cx.listener(Self::on_backspace))
            .on_action(cx.listener(Self::on_tab))
            .on_action(cx.listener(Self::on_escape))
            .on_action(cx.listener(Self::on_up))
            .on_action(cx.listener(Self::on_down))
            .on_action(cx.listener(Self::on_left))
            .on_action(cx.listener(Self::on_right))
            .on_action(cx.listener(Self::on_home))
            .on_action(cx.listener(Self::on_end))
            .on_action(cx.listener(Self::on_delete))
            .on_action(cx.listener(Self::on_page_up))
            .on_action(cx.listener(Self::on_page_down))
            .on_action(cx.listener(Self::on_insert))
            // Function keys
            .on_action(cx.listener(Self::on_f1))
            .on_action(cx.listener(Self::on_f2))
            .on_action(cx.listener(Self::on_f3))
            .on_action(cx.listener(Self::on_f4))
            .on_action(cx.listener(Self::on_f5))
            .on_action(cx.listener(Self::on_f6))
            .on_action(cx.listener(Self::on_f7))
            .on_action(cx.listener(Self::on_f8))
            .on_action(cx.listener(Self::on_f9))
            .on_action(cx.listener(Self::on_f10))
            .on_action(cx.listener(Self::on_f11))
            .on_action(cx.listener(Self::on_f12))
            // Control keys
            .on_action(cx.listener(Self::on_ctrl_a))
            .on_action(cx.listener(Self::on_ctrl_b))
            .on_action(cx.listener(Self::on_ctrl_c))
            .on_action(cx.listener(Self::on_ctrl_d))
            .on_action(cx.listener(Self::on_ctrl_e))
            .on_action(cx.listener(Self::on_ctrl_f))
            .on_action(cx.listener(Self::on_ctrl_g))
            .on_action(cx.listener(Self::on_ctrl_h))
            .on_action(cx.listener(Self::on_ctrl_i))
            .on_action(cx.listener(Self::on_ctrl_j))
            .on_action(cx.listener(Self::on_ctrl_k))
            .on_action(cx.listener(Self::on_ctrl_l))
            .on_action(cx.listener(Self::on_ctrl_m))
            .on_action(cx.listener(Self::on_ctrl_n))
            .on_action(cx.listener(Self::on_ctrl_o))
            .on_action(cx.listener(Self::on_ctrl_p))
            .on_action(cx.listener(Self::on_ctrl_q))
            .on_action(cx.listener(Self::on_ctrl_r))
            .on_action(cx.listener(Self::on_ctrl_s))
            .on_action(cx.listener(Self::on_ctrl_t))
            .on_action(cx.listener(Self::on_ctrl_u))
            .on_action(cx.listener(Self::on_ctrl_v))
            .on_action(cx.listener(Self::on_ctrl_w))
            .on_action(cx.listener(Self::on_ctrl_x))
            .on_action(cx.listener(Self::on_ctrl_y))
            .on_action(cx.listener(Self::on_ctrl_z))
            // Control+symbol keys
            .on_action(cx.listener(Self::on_ctrl_backslash))
            .on_action(cx.listener(Self::on_ctrl_bracket_right))
            .on_action(cx.listener(Self::on_ctrl_caret))
            .on_action(cx.listener(Self::on_ctrl_underscore))
            // Alt keys
            .on_action(cx.listener(Self::on_alt_b))
            .on_action(cx.listener(Self::on_alt_d))
            .on_action(cx.listener(Self::on_alt_f))
            .on_action(cx.listener(Self::on_alt_backspace))
            // Alt+arrow keys
            .on_action(cx.listener(Self::on_alt_up))
            .on_action(cx.listener(Self::on_alt_down))
            .on_action(cx.listener(Self::on_alt_left))
            .on_action(cx.listener(Self::on_alt_right))
            // Shift+arrow keys
            .on_action(cx.listener(Self::on_shift_up))
            .on_action(cx.listener(Self::on_shift_down))
            .on_action(cx.listener(Self::on_shift_left))
            .on_action(cx.listener(Self::on_shift_right))
            .on_action(cx.listener(Self::on_shift_home))
            .on_action(cx.listener(Self::on_shift_end))
            .on_action(cx.listener(Self::on_shift_insert))
            .on_action(cx.listener(Self::on_shift_page_up))
            .on_action(cx.listener(Self::on_shift_page_down))
            // Ctrl+arrow keys
            .on_action(cx.listener(Self::on_ctrl_up))
            .on_action(cx.listener(Self::on_ctrl_down))
            .on_action(cx.listener(Self::on_ctrl_left))
            .on_action(cx.listener(Self::on_ctrl_right))
            // Ctrl+Shift keys
            .on_action(cx.listener(Self::on_ctrl_shift_up))
            .on_action(cx.listener(Self::on_ctrl_shift_down))
            .on_action(cx.listener(Self::on_ctrl_shift_left))
            .on_action(cx.listener(Self::on_ctrl_shift_right))
            .on_action(cx.listener(Self::on_ctrl_shift_c))
            .on_action(cx.listener(Self::on_ctrl_shift_v))
            // Ctrl+Alt+arrow keys
            .on_action(cx.listener(Self::on_ctrl_alt_up))
            .on_action(cx.listener(Self::on_ctrl_alt_down))
            .on_action(cx.listener(Self::on_ctrl_alt_left))
            .on_action(cx.listener(Self::on_ctrl_alt_right))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    window.focus(&this.focus_handle, cx);
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    let ctrl = event.modifiers.control;
                    this.handle_mouse_down(x, y, ctrl, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                let x: f32 = event.position.x.into();
                let y: f32 = event.position.y.into();

                // Read TermMode once to avoid acquiring FairMutex multiple times per event.
                let mode = this.terminal.as_ref().map(|t| t.mode());

                // If MOUSE_MOTION mode: send every move; if MOUSE_DRAG: only send when dragging.
                if mode.map_or(false, |m| m.contains(TermMode::MOUSE_MOTION)) {
                    let (screen_line, col) = this.position_to_cell(x, y);
                    this.send_mouse_event_to_pty(32, col, screen_line as usize, true);
                    return;
                } else if this.is_dragging
                    && mode.map_or(false, |m| m.contains(TermMode::MOUSE_DRAG))
                {
                    let (screen_line, col) = this.position_to_cell(x, y);
                    this.send_mouse_event_to_pty(32, col, screen_line as usize, true);
                }

                if this.is_dragging {
                    this.handle_mouse_drag(x, y, cx);
                    return;
                }

                // URL hover check only when Ctrl is held
                if event.modifiers.control {
                    let (screen_line, col) = this.position_to_cell(x, y);
                    let prev = this.hovered_url_index;
                    this.update_hovered_url(screen_line as usize, col, true);
                    if this.hovered_url_index != prev {
                        cx.notify();
                    }
                } else if this.hovered_url_index.is_some() {
                    this.hovered_url_index = None;
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &gpui::MouseUpEvent, _window, cx| {
                    this.handle_mouse_up(cx);
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _window, cx| {
                let delta = event.delta.pixel_delta(Pixels::from(16.0));
                let y: f32 = delta.y.into();
                this.handle_scroll(y, cx);
            }))
            .child(
                // Wrapper div as flex container for proper layout propagation
                div()
                    .flex_1()
                    .w_full()
                    .flex()
                    .flex_col()
                    .bg(rgb(BG_BASE))
                    .child(TerminalElement::new(cx.entity())),
            )
            .into_any_element()
    }
}
