//! Terminal view for GPUI rendering
//!
//! This module provides the main TerminalView struct and its implementation.

use super::Terminal;
use crate::terminal::element::{
    CellData, DEFAULT_CELL_HEIGHT, DEFAULT_CELL_WIDTH, MULTI_CLICK_THRESHOLD_MS,
    SCROLL_LINES_WHEEL, TERMINAL_PADDING, TerminalElement, TerminalLayout,
};
use crate::theme::{self, *};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point as AlacPoint};
use alacritty_terminal::term::cell::Flags as CellFlags;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};
use gpui::{
    App, AsyncApp, Bounds, Context, EntityInputHandler, FocusHandle, Focusable, Hsla,
    InteractiveElement, IntoElement, MouseButton, MouseMoveEvent, ParentElement, Pixels, Render,
    ScrollWheelEvent, Styled, UTF16Selection, WeakEntity, Window, div, rgb,
};
use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;

/// Cached cell data from terminal grid.
/// Copied from alacritty_terminal to ensure consistent state during rendering.
#[derive(Clone)]
struct CachedCell {
    c: char,
    fg: AnsiColor,
    bg: AnsiColor,
    flags: CellFlags,
}

/// Cached terminal content snapshot.
/// Similar to Zed's TerminalContent, this captures the entire terminal state
/// at a specific point in time to prevent rendering intermediate states.
#[derive(Clone)]
struct CachedContent {
    /// Grid of cells (rows x cols)
    cells: Vec<Vec<CachedCell>>,
    /// Cursor position (line, column)
    cursor: (i32, usize),
    /// Whether cursor should be visible (SHOW_CURSOR mode)
    cursor_visible: bool,
    /// Display offset for scrollback
    display_offset: i32,
    /// Number of lines
    lines: usize,
}

/// Selection state for text selection in the terminal
#[derive(Clone, Copy, Debug, PartialEq)]
struct TerminalSelection {
    /// Start point (line, column)
    start: (i32, usize),
    /// End point (line, column)
    end: (i32, usize),
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
    fn contains(&self, line: i32, col: usize) -> bool {
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
}

impl TerminalView {
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
                        while let Ok(_event) = event_rx.recv().await {
                            // Drain any additional pending events before updating
                            // This ensures we process all events in a batch
                            while event_rx.try_recv().is_ok() {}

                            let should_break = cx.update(|cx| {
                                if let Some(this) = this.upgrade() {
                                    this.update(cx, |view, cx: &mut Context<TerminalView>| {
                                        // Update content cache after all events processed
                                        view.update_content_cache();
                                        cx.notify();
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
                    selection: None,
                    is_dragging: false,
                    last_click_time: None,
                    click_count: 0,
                    cell_width: DEFAULT_CELL_WIDTH,
                    cell_height: DEFAULT_CELL_HEIGHT,
                    content_origin: (0.0, 0.0),
                    cached_content: None,
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
                selection: None,
                is_dragging: false,
                last_click_time: None,
                click_count: 0,
                cell_width: DEFAULT_CELL_WIDTH,
                cell_height: DEFAULT_CELL_HEIGHT,
                content_origin: (0.0, 0.0),
                cached_content: None,
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
            let display_offset = render_content.display_offset as i32;

            let grid = term.grid();
            let cols = grid.columns();
            let lines = grid.screen_lines();

            // Copy all cell data from the grid
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

            let cursor_visible = term
                .mode()
                .contains(alacritty_terminal::term::TermMode::SHOW_CURSOR);

            self.cached_content = Some(CachedContent {
                cells,
                cursor: (cursor_point.line.0, cursor_point.column.0),
                cursor_visible,
                display_offset,
                lines,
            });
        });
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

    /// Handle mouse down event for selection
    fn handle_mouse_down(&mut self, x: f32, y: f32, cx: &mut Context<Self>) {
        let (screen_line, col) = self.position_to_cell(x, y);
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
    // Color conversion
    // ========================================================================

    fn ansi_color_to_hsla(color: AnsiColor) -> Hsla {
        match color {
            AnsiColor::Named(named) => Self::named_color_to_hsla(named),
            AnsiColor::Spec(rgb) => Hsla::from(gpui::Rgba {
                r: rgb.r as f32 / 255.0,
                g: rgb.g as f32 / 255.0,
                b: rgb.b as f32 / 255.0,
                a: 1.0,
            }),
            AnsiColor::Indexed(idx) => Self::indexed_color_to_hsla(idx),
        }
    }

    fn named_color_to_hsla(color: NamedColor) -> Hsla {
        let rgb_val = match color {
            NamedColor::Black => theme::ansi::BLACK,
            NamedColor::Red => theme::ansi::RED,
            NamedColor::Green => theme::ansi::GREEN,
            NamedColor::Yellow => theme::ansi::YELLOW,
            NamedColor::Blue => theme::ansi::BLUE,
            NamedColor::Magenta => theme::ansi::MAGENTA,
            NamedColor::Cyan => theme::ansi::CYAN,
            NamedColor::White => theme::ansi::WHITE,
            NamedColor::BrightBlack => theme::ansi::BRIGHT_BLACK,
            NamedColor::BrightRed => theme::ansi::BRIGHT_RED,
            NamedColor::BrightGreen => theme::ansi::BRIGHT_GREEN,
            NamedColor::BrightYellow => theme::ansi::BRIGHT_YELLOW,
            NamedColor::BrightBlue => theme::ansi::BRIGHT_BLUE,
            NamedColor::BrightMagenta => theme::ansi::BRIGHT_MAGENTA,
            NamedColor::BrightCyan => theme::ansi::BRIGHT_CYAN,
            NamedColor::BrightWhite => theme::ansi::BRIGHT_WHITE,
            NamedColor::Foreground => theme::ansi::FOREGROUND,
            NamedColor::Background => theme::ansi::BACKGROUND,
            NamedColor::Cursor => theme::ansi::CURSOR,
            _ => theme::ansi::FOREGROUND,
        };
        Hsla::from(rgb(rgb_val))
    }

    fn indexed_color_to_hsla(idx: u8) -> Hsla {
        if idx < 16 {
            let named = match idx {
                0 => NamedColor::Black,
                1 => NamedColor::Red,
                2 => NamedColor::Green,
                3 => NamedColor::Yellow,
                4 => NamedColor::Blue,
                5 => NamedColor::Magenta,
                6 => NamedColor::Cyan,
                7 => NamedColor::White,
                8 => NamedColor::BrightBlack,
                9 => NamedColor::BrightRed,
                10 => NamedColor::BrightGreen,
                11 => NamedColor::BrightYellow,
                12 => NamedColor::BrightBlue,
                13 => NamedColor::BrightMagenta,
                14 => NamedColor::BrightCyan,
                15 => NamedColor::BrightWhite,
                _ => NamedColor::Foreground,
            };
            Self::named_color_to_hsla(named)
        } else if idx < 232 {
            // 216 color cube (6x6x6)
            let idx = idx - 16;
            let r = (idx / 36) % 6;
            let g = (idx / 6) % 6;
            let b = idx % 6;
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

    // ========================================================================
    // Layout building
    // ========================================================================

    /// Build terminal layout data for paint phase rendering.
    /// Always uses cached content for consistent state (like Zed's approach).
    /// Cache is initialized at terminal creation and updated on every event.
    pub(super) fn build_layout(
        &self,
        cell_width: Pixels,
        line_height: Pixels,
    ) -> Option<TerminalLayout> {
        let cached = self.cached_content.as_ref()?;
        Some(self.build_layout_from_cache(cached, cell_width, line_height))
    }

    /// Build layout from cached content (consistent state)
    fn build_layout_from_cache(
        &self,
        cached: &CachedContent,
        cell_width: Pixels,
        line_height: Pixels,
    ) -> TerminalLayout {
        let selection = self.selection;
        let (cursor_line, cursor_col) = cached.cursor;
        let cursor_visible = cached.cursor_visible;
        let display_offset = cached.display_offset;

        // Convert cursor to display coordinates
        let display_cursor_line = cursor_line + display_offset;

        let mut cells: Vec<Vec<CellData>> = Vec::with_capacity(cached.lines);

        for (line_idx, cached_row) in cached.cells.iter().enumerate() {
            let actual_line = line_idx as i32 - display_offset;
            let is_cursor_line = line_idx as i32 == display_cursor_line;

            let mut row_cells: Vec<CellData> = Vec::with_capacity(cached_row.len());

            for (col_idx, cached_cell) in cached_row.iter().enumerate() {
                let is_inverse = cached_cell.flags.contains(CellFlags::INVERSE);

                // Swap fg/bg when INVERSE flag is set (used by TUI apps for software cursors)
                let (fg, bg) = if is_inverse {
                    let fg = if cached_cell.bg == AnsiColor::Named(NamedColor::Background) {
                        Self::named_color_to_hsla(NamedColor::Background)
                    } else {
                        Self::ansi_color_to_hsla(cached_cell.bg)
                    };
                    let bg = Some(Self::ansi_color_to_hsla(cached_cell.fg));
                    (fg, bg)
                } else {
                    let fg = Self::ansi_color_to_hsla(cached_cell.fg);
                    let bg = if cached_cell.bg == AnsiColor::Named(NamedColor::Background) {
                        None
                    } else {
                        Some(Self::ansi_color_to_hsla(cached_cell.bg))
                    };
                    (fg, bg)
                };

                // Only show cursor if SHOW_CURSOR mode is enabled
                let is_cursor = cursor_visible && is_cursor_line && col_idx == cursor_col;
                let is_selected = selection
                    .filter(|sel| sel.start != sel.end)
                    .map(|sel| sel.contains(actual_line, col_idx))
                    .unwrap_or(false);

                let c = if cached_cell.c == ' ' || cached_cell.c == '\0' {
                    ' '
                } else {
                    cached_cell.c
                };

                let is_wide_char = cached_cell.flags.contains(CellFlags::WIDE_CHAR);
                let is_wide_spacer = cached_cell.flags.contains(CellFlags::WIDE_CHAR_SPACER);

                row_cells.push(CellData {
                    c,
                    fg,
                    bg,
                    is_cursor,
                    is_selected,
                    is_wide_char,
                    is_wide_spacer,
                });
            }

            cells.push(row_cells);
        }

        TerminalLayout {
            cells,
            cell_width,
            line_height,
            preedit_text: self.preedit_text.clone(),
        }
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
        // Clear preedit and send committed text to terminal
        self.preedit_text.clear();
        if !text.is_empty() {
            self.write_to_terminal(text.as_bytes());
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
        _range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        // Return bounds for IME candidate window positioning
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

impl Render for TerminalView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .cursor_text()
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
                    this.handle_mouse_down(x, y, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.is_dragging {
                    let x: f32 = event.position.x.into();
                    let y: f32 = event.position.y.into();
                    this.handle_mouse_drag(x, y, cx);
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
