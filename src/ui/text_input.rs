//! Dialog text input state that participates in GPUI's IME system.
//!
//! Each dialog input field is an `Entity<TextInput>`, following the official GPUI pattern
//! from `gpui/examples/input.rs`. The entity handles its own text, cursor, selection, and
//! IME preedit state independently.

use gpui::{
    Bounds, Context, EntityInputHandler, HighlightStyle, Hsla, Pixels, Point, Size, StyledText,
    UTF16Selection, Window, px, rgba,
};

/// Text input state for a single dialog field.
///
/// Used as `Entity<TextInput>` to participate in GPUI's IME system via
/// `ElementInputHandler::new(bounds, entity)` registered during paint.
pub struct TextInput {
    pub text: String,
    pub cursor: usize,
    pub selection_anchor: Option<usize>,
    pub preedit: String,
    /// Bounds of the visible input field, updated each frame by the dialog's
    /// inner canvas. Used by `bounds_for_range` to position the IME candidate window.
    pub input_bounds: Option<Bounds<Pixels>>,
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            selection_anchor: None,
            preedit: String::new(),
            input_bounds: None,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn selection_anchor(&self) -> Option<usize> {
        self.selection_anchor
    }

    pub fn preedit(&self) -> &str {
        &self.preedit
    }

    /// Clear all state (text, cursor, selection, preedit).
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.selection_anchor = None;
        self.preedit.clear();
    }

    /// Set text content and move cursor to end.
    pub fn set_text(&mut self, text: String) {
        self.cursor = text.chars().count();
        self.selection_anchor = None;
        self.preedit.clear();
        self.text = text;
    }

    /// Insert `text` at the cursor position, replacing any active selection.
    pub fn insert(&mut self, text: &str) {
        replace_selection_with_text(
            &mut self.text,
            &mut self.cursor,
            &mut self.selection_anchor,
            text,
        );
    }

    /// Delete the character before the cursor (or the selection). Returns `true` if changed.
    pub fn backspace(&mut self) -> bool {
        backspace_text(&mut self.text, &mut self.cursor, &mut self.selection_anchor)
    }

    /// Delete the character after the cursor (or the selection). Returns `true` if changed.
    pub fn delete(&mut self) -> bool {
        delete_text(&mut self.text, &mut self.cursor, &mut self.selection_anchor)
    }

    /// Move cursor to `pos`, optionally extending the selection (`shift = true`).
    pub fn move_cursor(&mut self, pos: usize, shift: bool) {
        if shift {
            if self.selection_anchor.is_none() {
                self.selection_anchor = Some(self.cursor);
            }
            self.cursor = pos;
        } else {
            self.cursor = pos;
            self.selection_anchor = None;
        }
    }

    /// Select all text (anchor at 0, cursor at end).
    pub fn select_all(&mut self) {
        self.selection_anchor = Some(0);
        self.cursor = self.text.chars().count();
    }

    /// Return the currently selected text, or `None` if there is no selection.
    pub fn get_selected_text(&self) -> Option<String> {
        let (start, end) = selected_char_range(self.cursor, self.selection_anchor)?;
        Some(slice_char_range(&self.text, start, end))
    }

    /// Delete the selection and return it. Returns `None` if there is no selection.
    pub fn cut_selection(&mut self) -> Option<String> {
        let selected = self.get_selected_text()?;
        delete_selected_text(&mut self.text, &mut self.cursor, &mut self.selection_anchor);
        Some(selected)
    }

    /// Move cursor up one line.
    pub fn cursor_up(&mut self, shift: bool) {
        let (line, col) = cursor_to_line_col(&self.text, self.cursor);
        let new_cursor = if line == 0 {
            0
        } else {
            line_col_to_cursor(&self.text, line - 1, col)
        };
        self.move_cursor(new_cursor, shift);
    }

    /// Move cursor down one line.
    pub fn cursor_down(&mut self, shift: bool) {
        let (line, col) = cursor_to_line_col(&self.text, self.cursor);
        let new_cursor = line_col_to_cursor(&self.text, line + 1, col);
        self.move_cursor(new_cursor, shift);
    }

    /// Move cursor to the start of the current line.
    pub fn cursor_home(&mut self, shift: bool) {
        let (line, _) = cursor_to_line_col(&self.text, self.cursor);
        let new_cursor = line_col_to_cursor(&self.text, line, 0);
        self.move_cursor(new_cursor, shift);
    }

    /// Move cursor to the end of the current line.
    pub fn cursor_end(&mut self, shift: bool) {
        let (line, _) = cursor_to_line_col(&self.text, self.cursor);
        let new_cursor = line_col_to_cursor(&self.text, line, usize::MAX);
        self.move_cursor(new_cursor, shift);
    }

    /// Handle a non-character text editing keystroke (navigation, deletion, etc.).
    ///
    /// Returns `true` if the key was consumed (state may have changed).
    ///
    /// Printable character input (including space) is handled by the IME system
    /// via `EntityInputHandler::replace_text_in_range`, not here.
    ///
    /// - `multiline`: when `true`, Up/Down/Enter are handled as navigation/newline.
    pub fn handle_editing_key(&mut self, key: &str, shift: bool, multiline: bool) -> bool {
        match key {
            "backspace" => self.backspace(),
            "delete" => self.delete(),
            "left" => {
                let pos = self.cursor.saturating_sub(1);
                self.move_cursor(pos, shift);
                true
            }
            "right" => {
                let pos = (self.cursor + 1).min(self.text.chars().count());
                self.move_cursor(pos, shift);
                true
            }
            "home" => {
                self.cursor_home(shift);
                true
            }
            "end" => {
                self.cursor_end(shift);
                true
            }
            "up" if multiline => {
                self.cursor_up(shift);
                true
            }
            "down" if multiline => {
                self.cursor_down(shift);
                true
            }
            "enter" if multiline => {
                self.insert("\n");
                true
            }
            _ => false,
        }
    }

    /// Render the input text with cursor marker (`|`), selection highlight, and IME preedit.
    pub fn render_styled(&self) -> StyledText {
        render_text_with_preedit(
            &self.text,
            self.cursor,
            self.selection_anchor,
            &self.preedit,
        )
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: std::ops::Range<usize>,
        actual_range: &mut Option<std::ops::Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let total: usize = self.text.chars().map(|c| c.len_utf16()).sum();
        let start = range_utf16.start.min(total);
        let end = range_utf16.end.min(total);
        *actual_range = Some(start..end);
        Some(slice_utf16(&self.text, start..end))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let cursor_utf16 = char_cursor_to_utf16(&self.text, self.cursor);
        if let Some(anchor) = self.selection_anchor {
            let anchor_utf16 = char_cursor_to_utf16(&self.text, anchor);
            let (start, end, reversed) = if anchor <= self.cursor {
                (anchor_utf16, cursor_utf16, false)
            } else {
                (cursor_utf16, anchor_utf16, true)
            };
            Some(UTF16Selection {
                range: start..end,
                reversed,
            })
        } else {
            Some(UTF16Selection {
                range: cursor_utf16..cursor_utf16,
                reversed: false,
            })
        }
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<std::ops::Range<usize>> {
        if self.preedit.is_empty() {
            None
        } else {
            let cursor_utf16 = char_cursor_to_utf16(&self.text, self.cursor);
            let preedit_len: usize = self.preedit.chars().map(|c| c.len_utf16()).sum();
            Some(cursor_utf16..cursor_utf16 + preedit_len)
        }
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.preedit.clear();
    }

    fn replace_text_in_range(
        &mut self,
        _range_utf16: Option<std::ops::Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preedit.clear();
        if !text.is_empty() {
            self.insert(text);
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<std::ops::Range<usize>>,
        new_text: &str,
        _new_selected_range_utf16: Option<std::ops::Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preedit = new_text.to_string();
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: std::ops::Range<usize>,
        _element_bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let ib = self.input_bounds?;
        // Estimate character width from the input font (text_sm ≈ 14px, mono ≈ 8.4px)
        let font_size = px(14.0);
        let font_id = window.text_system().resolve_font(
            &gpui::TextStyle {
                font_family: crate::theme::MONOSPACE_FONT.into(),
                font_size: font_size.into(),
                ..Default::default()
            }
            .font(),
        );
        let char_width = window
            .text_system()
            .advance(font_id, font_size, 'M')
            .map(|s| s.width)
            .unwrap_or(px(8.4));
        let offset_x = char_width * range_utf16.start as f32;
        Some(Bounds {
            origin: Point::new(ib.origin.x + offset_x, ib.origin.y),
            size: Size {
                width: px(1.0),
                height: ib.size.height,
            },
        })
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

// === Rendering helpers (pub(crate) for use in ui/dialogs.rs) ===

/// Render text with cursor marker (`|`), selection highlight, and optional IME preedit underline.
pub(crate) fn render_text_with_preedit(
    text: &str,
    cursor: usize,
    selection_anchor: Option<usize>,
    preedit: &str,
) -> StyledText {
    if preedit.is_empty() {
        return render_text_with_selection_and_caret(text, cursor, selection_anchor);
    }

    // Build display string: before_cursor + "|" + preedit + after_cursor
    let cursor = cursor.min(text.chars().count());
    let byte_pos = char_to_byte_offset(text, cursor);
    let (before, after) = text.split_at(byte_pos);
    let display = format!("{}|{}{}", before, preedit, after);

    // Underline the preedit region
    let preedit_start = before.len() + 1; // +1 for "|"
    let preedit_end = preedit_start + preedit.len();
    let underline = HighlightStyle {
        underline: Some(gpui::UnderlineStyle {
            thickness: gpui::px(1.0),
            color: None,
            wavy: false,
        }),
        ..Default::default()
    };
    StyledText::new(display)
        .with_highlights(std::iter::once((preedit_start..preedit_end, underline)))
}

pub(crate) fn render_text_with_selection_and_caret(
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

pub(crate) fn selection_highlight_style() -> HighlightStyle {
    HighlightStyle {
        background_color: Some(Hsla::from(rgba(0x74a9e455))),
        ..HighlightStyle::default()
    }
}

pub(crate) fn insert_caret_marker(text: &str, cursor: usize) -> String {
    let cursor = cursor.min(text.chars().count());
    let byte_pos = char_to_byte_offset(text, cursor);
    let (before, after) = text.split_at(byte_pos);
    format!("{}|{}", before, after)
}

pub(crate) fn selection_ranges_in_display_text(
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

pub(crate) fn selected_char_range(
    cursor: usize,
    selection_anchor: Option<usize>,
) -> Option<(usize, usize)> {
    let anchor = selection_anchor?;
    if anchor == cursor {
        None
    } else {
        Some((anchor.min(cursor), anchor.max(cursor)))
    }
}

/// Normalize pasted text for single-line fields (replace newlines with spaces).
pub(crate) fn normalize_single_line_text(text: &str) -> String {
    text.chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect()
}

pub(crate) fn char_to_byte_offset(text: &str, char_offset: usize) -> usize {
    text.char_indices()
        .nth(char_offset)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

// === Private text manipulation helpers ===

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

fn slice_char_range(text: &str, start: usize, end: usize) -> String {
    let byte_start = char_to_byte_offset(text, start);
    let byte_end = char_to_byte_offset(text, end);
    text[byte_start..byte_end].to_string()
}

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
                return i; // end of target line (clamped)
            }
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    // target_line is at or beyond last line, or col exceeds line length
    text.chars().count()
}

fn char_cursor_to_utf16(text: &str, char_cursor: usize) -> usize {
    text.chars().take(char_cursor).map(|c| c.len_utf16()).sum()
}

fn slice_utf16(text: &str, range: std::ops::Range<usize>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut units = 0;
    let mut start_char = None;
    let mut end_char = None;
    for (i, &c) in chars.iter().enumerate() {
        if units == range.start {
            start_char = Some(i);
        }
        if units == range.end {
            end_char = Some(i);
            break;
        }
        units += c.len_utf16();
    }
    if units == range.end {
        end_char = Some(chars.len());
    }
    let s = start_char.unwrap_or(chars.len());
    let e = end_char.unwrap_or(chars.len());
    chars[s..e].iter().collect()
}
