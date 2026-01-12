//! Terminal state management
//!
//! Implements a proper terminal grid with ANSI escape sequence support.

use iced::Color;
use std::collections::VecDeque;
use unicode_width::UnicodeWidthChar;
use vte::{Params, Perform};

// Scrollback configuration
const MAX_SCROLLBACK_LINES: usize = 5000;

/// Helper to create Color from RGB values (0-255)
const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

/// Transparent color
const TRANSPARENT: Color = Color {
    r: 0.0,
    g: 0.0,
    b: 0.0,
    a: 0.0,
};

/// ANSI color palette (16 basic colors)
const ANSI_COLORS: [Color; 16] = [
    rgb(0, 0, 0),       // Black
    rgb(205, 49, 49),   // Red
    rgb(13, 188, 121),  // Green
    rgb(229, 229, 16),  // Yellow
    rgb(36, 114, 200),  // Blue
    rgb(188, 63, 188),  // Magenta
    rgb(17, 168, 205),  // Cyan
    rgb(229, 229, 229), // White
    // Bright variants
    rgb(102, 102, 102), // Bright Black
    rgb(241, 76, 76),   // Bright Red
    rgb(35, 209, 139),  // Bright Green
    rgb(245, 245, 67),  // Bright Yellow
    rgb(59, 142, 234),  // Bright Blue
    rgb(214, 112, 214), // Bright Magenta
    rgb(41, 184, 219),  // Bright Cyan
    rgb(255, 255, 255), // Bright White
];

/// Cell attributes (color, style)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellAttrs {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

impl Default for CellAttrs {
    fn default() -> Self {
        Self {
            fg: rgb(229, 229, 229), // Default foreground
            bg: TRANSPARENT,        // Default background
            bold: false,
            italic: false,
            underline: false,
            inverse: false,
        }
    }
}

/// A single terminal cell
#[derive(Debug, Clone, Copy)]
pub struct Cell {
    pub c: char,
    /// Cell attributes (future: colored terminal rendering)
    #[allow(dead_code)]
    pub attrs: CellAttrs,
    /// Width of the character (1 for narrow, 2 for wide/CJK)
    /// Future: proper CJK character width handling in rendering
    #[allow(dead_code)]
    pub width: u8,
    /// True if this cell is the continuation of a wide character
    pub is_continuation: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            attrs: CellAttrs::default(),
            width: 1,
            is_continuation: false,
        }
    }
}

/// Terminal cursor state
#[derive(Debug, Clone, Copy)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
    /// Whether cursor is visible (future: cursor blinking in rendering)
    #[allow(dead_code)]
    pub visible: bool,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            row: 0,
            col: 0,
            visible: true,
        }
    }
}

/// Terminal state with grid and scrollback
pub struct TerminalState {
    /// Current visible grid
    grid: Vec<Vec<Cell>>,
    /// Scrollback buffer
    scrollback: VecDeque<Vec<Cell>>,
    /// Maximum scrollback lines
    max_scrollback: usize,
    /// Terminal dimensions
    pub rows: usize,
    pub cols: usize,
    /// Cursor state
    pub cursor: Cursor,
    /// Current cell attributes
    current_attrs: CellAttrs,
    /// Saved cursor position (for DECSC/DECRC)
    saved_cursor: Option<Cursor>,
    /// Scroll region (top, bottom) - 0-indexed
    scroll_region: (usize, usize),
    /// Scroll offset for viewing scrollback
    pub scroll_offset: usize,
    /// Window title (set via OSC sequences)
    pub title: Option<String>,
}

impl TerminalState {
    pub fn new(rows: usize, cols: usize) -> Self {
        let grid = vec![vec![Cell::default(); cols]; rows];
        Self {
            grid,
            scrollback: VecDeque::new(),
            max_scrollback: MAX_SCROLLBACK_LINES,
            rows,
            cols,
            cursor: Cursor::default(),
            current_attrs: CellAttrs::default(),
            saved_cursor: None,
            scroll_region: (0, rows.saturating_sub(1)),
            scroll_offset: 0,
            title: None,
        }
    }

    /// Resize the terminal
    pub fn resize(&mut self, new_rows: usize, new_cols: usize) {
        if new_rows == self.rows && new_cols == self.cols {
            return;
        }

        // Create new grid
        let mut new_grid = vec![vec![Cell::default(); new_cols]; new_rows];

        // Copy existing content
        for (r, row) in self.grid.iter().enumerate().take(new_rows) {
            for (c, cell) in row.iter().enumerate().take(new_cols) {
                new_grid[r][c] = *cell;
            }
        }

        self.grid = new_grid;
        self.rows = new_rows;
        self.cols = new_cols;
        self.scroll_region = (0, new_rows.saturating_sub(1));

        // Clamp cursor
        self.cursor.row = self.cursor.row.min(new_rows.saturating_sub(1));
        self.cursor.col = self.cursor.col.min(new_cols.saturating_sub(1));
    }

    /// Get visible rows for rendering
    pub fn visible_rows(&self) -> impl Iterator<Item = &Vec<Cell>> {
        self.grid.iter()
    }

    /// Scroll up (add to scrollback)
    fn scroll_up(&mut self) {
        let (top, bottom) = self.scroll_region;

        if top < self.grid.len() && bottom < self.grid.len() && top <= bottom {
            // Save top line to scrollback (take ownership to avoid clone)
            let line = std::mem::take(&mut self.grid[top]);
            self.scrollback.push_back(line);

            // Trim scrollback if needed
            while self.scrollback.len() > self.max_scrollback {
                self.scrollback.pop_front();
            }

            // Shift lines up within scroll region using rotate (no clones)
            self.grid[top..=bottom].rotate_left(1);

            // Clear bottom line (which now contains the taken empty row)
            self.grid[bottom] = vec![Cell::default(); self.cols];
        }
    }

    /// Scroll down within scroll region
    fn scroll_down(&mut self) {
        let (top, bottom) = self.scroll_region;

        if top < self.grid.len() && bottom < self.grid.len() && top <= bottom {
            // Shift lines down within scroll region using rotate (no clones)
            self.grid[top..=bottom].rotate_right(1);

            // Clear top line
            self.grid[top] = vec![Cell::default(); self.cols];
        }
    }

    /// Put a character at cursor position
    fn put_char(&mut self, c: char) {
        // Get character width using unicode-width
        let char_width = c.width().unwrap_or(1) as u8;

        if self.cursor.row < self.rows && self.cursor.col < self.cols {
            // If this is a wide character and we're at the last column,
            // wrap to next line first
            if char_width == 2 && self.cursor.col == self.cols - 1 {
                self.grid[self.cursor.row][self.cursor.col] = Cell::default();
                self.cursor.col = 0;
                self.cursor.row += 1;
                if self.cursor.row > self.scroll_region.1 {
                    self.cursor.row = self.scroll_region.1;
                    self.scroll_up();
                }
            }

            if self.cursor.row < self.rows && self.cursor.col < self.cols {
                self.grid[self.cursor.row][self.cursor.col] = Cell {
                    c,
                    attrs: self.current_attrs,
                    width: char_width,
                    is_continuation: false,
                };

                // For wide characters, mark the next cell as continuation
                if char_width == 2 && self.cursor.col + 1 < self.cols {
                    self.grid[self.cursor.row][self.cursor.col + 1] = Cell {
                        c: ' ',
                        attrs: self.current_attrs,
                        width: 0,
                        is_continuation: true,
                    };
                }
            }
        }

        self.cursor.col += char_width as usize;

        // Handle line wrap
        if self.cursor.col >= self.cols {
            self.cursor.col = 0;
            self.cursor.row += 1;
            if self.cursor.row > self.scroll_region.1 {
                self.cursor.row = self.scroll_region.1;
                self.scroll_up();
            }
        }
    }

    /// Newline
    fn newline(&mut self) {
        self.cursor.row += 1;
        if self.cursor.row > self.scroll_region.1 {
            self.cursor.row = self.scroll_region.1;
            self.scroll_up();
        }
    }

    /// Carriage return
    fn carriage_return(&mut self) {
        self.cursor.col = 0;
    }

    /// Backspace
    fn backspace(&mut self) {
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        }
    }

    /// Tab
    fn tab(&mut self) {
        // Move to next tab stop (every 8 columns)
        let next_tab = (self.cursor.col / 8 + 1) * 8;
        self.cursor.col = next_tab.min(self.cols - 1);
    }

    /// Clear from cursor to end of line
    fn clear_to_eol(&mut self) {
        if self.cursor.row < self.rows {
            for c in self.cursor.col..self.cols {
                self.grid[self.cursor.row][c] = Cell::default();
            }
        }
    }

    /// Clear from cursor to end of screen
    fn clear_to_eos(&mut self) {
        self.clear_to_eol();
        for r in (self.cursor.row + 1)..self.rows {
            for c in 0..self.cols {
                self.grid[r][c] = Cell::default();
            }
        }
    }

    /// Clear entire screen
    fn clear_screen(&mut self) {
        for r in 0..self.rows {
            for c in 0..self.cols {
                self.grid[r][c] = Cell::default();
            }
        }
    }

    /// Clear from start of screen to cursor
    fn clear_to_bos(&mut self) {
        for r in 0..self.cursor.row {
            for c in 0..self.cols {
                self.grid[r][c] = Cell::default();
            }
        }
        for c in 0..=self.cursor.col.min(self.cols - 1) {
            self.grid[self.cursor.row][c] = Cell::default();
        }
    }

    /// Erase characters from cursor
    fn erase_chars(&mut self, n: usize) {
        if self.cursor.row < self.rows {
            for i in 0..n {
                let col = self.cursor.col + i;
                if col < self.cols {
                    self.grid[self.cursor.row][col] = Cell::default();
                }
            }
        }
    }

    /// Delete lines at cursor
    fn delete_lines(&mut self, n: usize) {
        let (_, bottom) = self.scroll_region;
        for _ in 0..n {
            if self.cursor.row <= bottom {
                // Shift lines up
                for r in self.cursor.row..bottom {
                    self.grid[r] = self.grid[r + 1].clone();
                }
                self.grid[bottom] = vec![Cell::default(); self.cols];
            }
        }
    }

    /// Insert lines at cursor
    fn insert_lines(&mut self, n: usize) {
        let (_, bottom) = self.scroll_region;
        for _ in 0..n {
            if self.cursor.row <= bottom {
                // Shift lines down
                for r in (self.cursor.row + 1..=bottom).rev() {
                    self.grid[r] = self.grid[r - 1].clone();
                }
                self.grid[self.cursor.row] = vec![Cell::default(); self.cols];
            }
        }
    }

    /// Parse SGR (Select Graphic Rendition) parameters
    fn apply_sgr(&mut self, params: &Params) {
        let mut iter = params.iter();

        while let Some(param) = iter.next() {
            let code = param.first().copied().unwrap_or(0);

            match code {
                0 => self.current_attrs = CellAttrs::default(),
                1 => self.current_attrs.bold = true,
                3 => self.current_attrs.italic = true,
                4 => self.current_attrs.underline = true,
                7 => self.current_attrs.inverse = true,
                22 => self.current_attrs.bold = false,
                23 => self.current_attrs.italic = false,
                24 => self.current_attrs.underline = false,
                27 => self.current_attrs.inverse = false,
                // Foreground colors
                30..=37 => {
                    self.current_attrs.fg = ANSI_COLORS[(code - 30) as usize];
                }
                38 => {
                    // Extended foreground color
                    if let Some(sub) = iter.next() {
                        match sub.first().copied().unwrap_or(0) {
                            5 => {
                                // 256 color mode
                                if let Some(idx) = iter.next() {
                                    let idx = idx.first().copied().unwrap_or(0) as usize;
                                    self.current_attrs.fg = color_from_256(idx);
                                }
                            }
                            2 => {
                                // True color mode
                                let r = iter.next().and_then(|p| p.first().copied()).unwrap_or(0);
                                let g = iter.next().and_then(|p| p.first().copied()).unwrap_or(0);
                                let b = iter.next().and_then(|p| p.first().copied()).unwrap_or(0);
                                self.current_attrs.fg = rgb(r as u8, g as u8, b as u8);
                            }
                            _ => {}
                        }
                    }
                }
                39 => self.current_attrs.fg = CellAttrs::default().fg, // Default fg
                // Background colors
                40..=47 => {
                    self.current_attrs.bg = ANSI_COLORS[(code - 40) as usize];
                }
                48 => {
                    // Extended background color
                    if let Some(sub) = iter.next() {
                        match sub.first().copied().unwrap_or(0) {
                            5 => {
                                // 256 color mode
                                if let Some(idx) = iter.next() {
                                    let idx = idx.first().copied().unwrap_or(0) as usize;
                                    self.current_attrs.bg = color_from_256(idx);
                                }
                            }
                            2 => {
                                // True color mode
                                let r = iter.next().and_then(|p| p.first().copied()).unwrap_or(0);
                                let g = iter.next().and_then(|p| p.first().copied()).unwrap_or(0);
                                let b = iter.next().and_then(|p| p.first().copied()).unwrap_or(0);
                                self.current_attrs.bg = rgb(r as u8, g as u8, b as u8);
                            }
                            _ => {}
                        }
                    }
                }
                49 => self.current_attrs.bg = CellAttrs::default().bg, // Default bg
                // Bright foreground colors
                90..=97 => {
                    self.current_attrs.fg = ANSI_COLORS[(code - 90 + 8) as usize];
                }
                // Bright background colors
                100..=107 => {
                    self.current_attrs.bg = ANSI_COLORS[(code - 100 + 8) as usize];
                }
                _ => {}
            }
        }
    }

    /// Reset scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }
}

/// Convert 256-color index to Color
fn color_from_256(idx: usize) -> Color {
    if idx < 16 {
        ANSI_COLORS[idx]
    } else if idx < 232 {
        // 216 color cube (6x6x6)
        let idx = idx - 16;
        let r = (idx / 36) % 6;
        let g = (idx / 6) % 6;
        let b = idx % 6;
        let r = if r > 0 { r * 40 + 55 } else { 0 };
        let g = if g > 0 { g * 40 + 55 } else { 0 };
        let b = if b > 0 { b * 40 + 55 } else { 0 };
        rgb(r as u8, g as u8, b as u8)
    } else {
        // Grayscale (24 shades)
        let gray = (idx - 232) * 10 + 8;
        rgb(gray as u8, gray as u8, gray as u8)
    }
}

/// VTE performer implementation
impl Perform for TerminalState {
    fn print(&mut self, c: char) {
        self.put_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x08 => self.backspace(),        // BS
            0x09 => self.tab(),              // HT
            0x0A | 0x0B | 0x0C => self.newline(), // LF, VT, FF
            0x0D => self.carriage_return(),  // CR
            0x07 => {}                        // BEL - ignore
            _ => {}
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {
        // DCS sequences - not commonly needed
    }

    fn put(&mut self, _byte: u8) {
        // DCS data
    }

    fn unhook(&mut self) {
        // End DCS
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        // OSC sequences: handle window title (codes 0, 1, 2)
        if params.is_empty() {
            return;
        }

        // First param is the OSC code as ASCII digits
        let code = std::str::from_utf8(params[0])
            .ok()
            .and_then(|s| s.parse::<u8>().ok());

        match code {
            // OSC 0: Set icon name and window title
            // OSC 1: Set icon name
            // OSC 2: Set window title
            Some(0 | 1 | 2) if params.len() > 1 => {
                if let Ok(title) = std::str::from_utf8(params[1]) {
                    self.title = Some(title.to_string());
                }
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], _ignore: bool, action: char) {
        let param = |idx: usize, default: usize| -> usize {
            params
                .iter()
                .nth(idx)
                .and_then(|p| p.first().copied())
                .map(|v| if v == 0 { default } else { v as usize })
                .unwrap_or(default)
        };

        match action {
            // Cursor movement
            'A' => {
                // CUU - Cursor Up
                let n = param(0, 1);
                self.cursor.row = self.cursor.row.saturating_sub(n);
            }
            'B' => {
                // CUD - Cursor Down
                let n = param(0, 1);
                self.cursor.row = (self.cursor.row + n).min(self.rows - 1);
            }
            'C' => {
                // CUF - Cursor Forward
                let n = param(0, 1);
                self.cursor.col = (self.cursor.col + n).min(self.cols - 1);
            }
            'D' => {
                // CUB - Cursor Back
                let n = param(0, 1);
                self.cursor.col = self.cursor.col.saturating_sub(n);
            }
            'E' => {
                // CNL - Cursor Next Line
                let n = param(0, 1);
                self.cursor.row = (self.cursor.row + n).min(self.rows - 1);
                self.cursor.col = 0;
            }
            'F' => {
                // CPL - Cursor Previous Line
                let n = param(0, 1);
                self.cursor.row = self.cursor.row.saturating_sub(n);
                self.cursor.col = 0;
            }
            'G' => {
                // CHA - Cursor Character Absolute
                let n = param(0, 1);
                self.cursor.col = (n - 1).min(self.cols - 1);
            }
            'H' | 'f' => {
                // CUP/HVP - Cursor Position
                let row = param(0, 1).saturating_sub(1);
                let col = param(1, 1).saturating_sub(1);
                self.cursor.row = row.min(self.rows - 1);
                self.cursor.col = col.min(self.cols - 1);
            }
            'J' => {
                // ED - Erase in Display
                match param(0, 0) {
                    0 => self.clear_to_eos(),
                    1 => self.clear_to_bos(),
                    2 | 3 => self.clear_screen(),
                    _ => {}
                }
            }
            'K' => {
                // EL - Erase in Line
                match param(0, 0) {
                    0 => self.clear_to_eol(),
                    1 => {
                        // Clear from start of line to cursor
                        for c in 0..=self.cursor.col.min(self.cols - 1) {
                            self.grid[self.cursor.row][c] = Cell::default();
                        }
                    }
                    2 => {
                        // Clear entire line
                        for c in 0..self.cols {
                            self.grid[self.cursor.row][c] = Cell::default();
                        }
                    }
                    _ => {}
                }
            }
            'L' => {
                // IL - Insert Lines
                let n = param(0, 1);
                self.insert_lines(n);
            }
            'M' => {
                // DL - Delete Lines
                let n = param(0, 1);
                self.delete_lines(n);
            }
            'P' => {
                // DCH - Delete Characters
                let n = param(0, 1);
                if self.cursor.row < self.rows {
                    let row = &mut self.grid[self.cursor.row];
                    for i in self.cursor.col..self.cols {
                        if i + n < self.cols {
                            row[i] = row[i + n];
                        } else {
                            row[i] = Cell::default();
                        }
                    }
                }
            }
            'X' => {
                // ECH - Erase Characters
                let n = param(0, 1);
                self.erase_chars(n);
            }
            'd' => {
                // VPA - Vertical Position Absolute
                let n = param(0, 1);
                self.cursor.row = (n - 1).min(self.rows - 1);
            }
            'm' => {
                // SGR - Select Graphic Rendition
                self.apply_sgr(params);
            }
            'r' => {
                // DECSTBM - Set Scrolling Region
                let top = param(0, 1).saturating_sub(1);
                let bottom = param(1, self.rows).saturating_sub(1);
                if top < bottom && bottom < self.rows {
                    self.scroll_region = (top, bottom);
                    self.cursor.row = 0;
                    self.cursor.col = 0;
                }
            }
            's' => {
                // DECSC - Save Cursor
                self.saved_cursor = Some(self.cursor);
            }
            'u' => {
                // DECRC - Restore Cursor
                if let Some(saved) = self.saved_cursor {
                    self.cursor = saved;
                }
            }
            'S' => {
                // SU - Scroll Up
                let n = param(0, 1);
                for _ in 0..n {
                    self.scroll_up();
                }
            }
            'T' => {
                // SD - Scroll Down
                let n = param(0, 1);
                for _ in 0..n {
                    self.scroll_down();
                }
            }
            '@' => {
                // ICH - Insert Characters
                let n = param(0, 1);
                if self.cursor.row < self.rows {
                    let row = &mut self.grid[self.cursor.row];
                    for i in (self.cursor.col + n..self.cols).rev() {
                        row[i] = row[i - n];
                    }
                    for i in self.cursor.col..(self.cursor.col + n).min(self.cols) {
                        row[i] = Cell::default();
                    }
                }
            }
            _ => {
                // Unhandled CSI sequence
            }
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'7' => {
                // DECSC - Save Cursor
                self.saved_cursor = Some(self.cursor);
            }
            b'8' => {
                // DECRC - Restore Cursor
                if let Some(saved) = self.saved_cursor {
                    self.cursor = saved;
                }
            }
            b'D' => {
                // IND - Index (move cursor down, scroll if needed)
                if self.cursor.row >= self.scroll_region.1 {
                    self.scroll_up();
                } else {
                    self.cursor.row += 1;
                }
            }
            b'E' => {
                // NEL - Next Line
                self.cursor.col = 0;
                if self.cursor.row >= self.scroll_region.1 {
                    self.scroll_up();
                } else {
                    self.cursor.row += 1;
                }
            }
            b'M' => {
                // RI - Reverse Index (move cursor up, scroll if needed)
                if self.cursor.row <= self.scroll_region.0 {
                    self.scroll_down();
                } else {
                    self.cursor.row -= 1;
                }
            }
            b'c' => {
                // RIS - Full Reset
                *self = TerminalState::new(self.rows, self.cols);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_state_creation() {
        let state = TerminalState::new(24, 80);
        assert_eq!(state.rows, 24);
        assert_eq!(state.cols, 80);
        assert_eq!(state.cursor.row, 0);
        assert_eq!(state.cursor.col, 0);
    }

    #[test]
    fn test_put_char() {
        let mut state = TerminalState::new(24, 80);
        state.put_char('A');
        assert_eq!(state.grid[0][0].c, 'A');
        assert_eq!(state.cursor.col, 1);
    }

    #[test]
    fn test_wide_character() {
        let mut state = TerminalState::new(24, 80);

        // Wide character (CJK)
        state.put_char('日');
        assert_eq!(state.grid[0][0].c, '日');
        assert_eq!(state.grid[0][0].width, 2);
        assert!(!state.grid[0][0].is_continuation);
        assert_eq!(state.grid[0][1].c, ' ');
        assert!(state.grid[0][1].is_continuation);
        assert_eq!(state.cursor.col, 2);

        // Another wide character
        state.put_char('本');
        assert_eq!(state.grid[0][2].c, '本');
        assert_eq!(state.cursor.col, 4);

        // Narrow character after wide
        state.put_char('A');
        assert_eq!(state.grid[0][4].c, 'A');
        assert_eq!(state.grid[0][4].width, 1);
        assert!(!state.grid[0][4].is_continuation);
        assert_eq!(state.cursor.col, 5);
    }

    #[test]
    fn test_wide_character_at_line_end() {
        // Width 10, put wide char at col 9 should wrap
        let mut state = TerminalState::new(24, 10);
        state.cursor.col = 9;

        // Wide character at last column should wrap to next line
        state.put_char('日');
        assert_eq!(state.cursor.row, 1);
        assert_eq!(state.cursor.col, 2);
        assert_eq!(state.grid[1][0].c, '日');
        assert!(state.grid[1][1].is_continuation);
    }

    #[test]
    fn test_newline() {
        let mut state = TerminalState::new(24, 80);
        state.cursor.col = 5;
        state.newline();
        assert_eq!(state.cursor.row, 1);
    }

    #[test]
    fn test_carriage_return() {
        let mut state = TerminalState::new(24, 80);
        state.cursor.col = 10;
        state.carriage_return();
        assert_eq!(state.cursor.col, 0);
    }

    #[test]
    fn test_clear_screen() {
        let mut state = TerminalState::new(24, 80);
        state.put_char('A');
        state.put_char('B');
        state.clear_screen();
        assert_eq!(state.grid[0][0].c, ' ');
        assert_eq!(state.grid[0][1].c, ' ');
    }

    #[test]
    fn test_resize() {
        let mut state = TerminalState::new(24, 80);
        state.put_char('A');
        state.resize(30, 100);
        assert_eq!(state.rows, 30);
        assert_eq!(state.cols, 100);
        assert_eq!(state.grid[0][0].c, 'A');
    }

    #[test]
    fn test_color_from_256() {
        // Basic colors
        assert_eq!(color_from_256(0), ANSI_COLORS[0]);
        assert_eq!(color_from_256(15), ANSI_COLORS[15]);

        // 216 color cube
        let c = color_from_256(16);
        assert_eq!(c, rgb(0, 0, 0));

        // Grayscale
        let g = color_from_256(232);
        assert_eq!(g, rgb(8, 8, 8));
    }
}
