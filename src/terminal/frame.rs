//! Owned terminal snapshot handed from the VT thread to the renderer.
//!
//! libghostty-vt handles are `!Send`, so the terminal state cannot be read
//! directly from the UI thread. The VT thread converts each render snapshot
//! into these plain types instead.

use std::sync::Arc;

/// 24-bit color resolved against the terminal palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl From<libghostty_vt::style::RgbColor> for Rgb {
    fn from(value: libghostty_vt::style::RgbColor) -> Self {
        Self {
            r: value.r,
            g: value.g,
            b: value.b,
        }
    }
}

impl From<Rgb> for libghostty_vt::style::RgbColor {
    fn from(value: Rgb) -> Self {
        Self {
            r: value.r,
            g: value.g,
            b: value.b,
        }
    }
}

impl Rgb {
    pub const fn from_u32(value: u32) -> Self {
        Self {
            r: ((value >> 16) & 0xff) as u8,
            g: ((value >> 8) & 0xff) as u8,
            b: (value & 0xff) as u8,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Underline {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

impl From<libghostty_vt::style::Underline> for Underline {
    fn from(value: libghostty_vt::style::Underline) -> Self {
        use libghostty_vt::style::Underline as U;
        match value {
            U::None => Self::None,
            U::Single => Self::Single,
            U::Double => Self::Double,
            U::Curly => Self::Curly,
            U::Dotted => Self::Dotted,
            U::Dashed => Self::Dashed,
            _ => Self::Single,
        }
    }
}

/// Cell width class. Spacer cells are not drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CellWidth {
    #[default]
    Narrow,
    Wide,
    Spacer,
}

/// Style attributes that the renderer batches on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellAttrs {
    pub bold: bool,
    pub italic: bool,
    pub faint: bool,
    pub inverse: bool,
    pub invisible: bool,
    pub strikethrough: bool,
    pub underline: Underline,
}

/// A single rendered cell. `None` colors mean "use the frame default".
#[derive(Clone, Debug)]
pub struct FrameCell {
    /// Base codepoint, or a space for empty cells.
    pub ch: char,
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub attrs: CellAttrs,
    pub width: CellWidth,
}

impl Default for FrameCell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: None,
            bg: None,
            attrs: CellAttrs::default(),
            width: CellWidth::Narrow,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FrameRow {
    pub cells: Vec<FrameCell>,
    /// Full grapheme clusters by column, for the rare cells that have
    /// combining codepoints. Cells absent here render as `FrameCell::ch`.
    pub clusters: Vec<(u16, Box<str>)>,
    /// Selected column range, inclusive on both ends.
    pub selection: Option<(u16, u16)>,
}

impl FrameRow {
    pub fn cluster(&self, col: u16) -> Option<&str> {
        if self.clusters.is_empty() {
            return None;
        }
        self.clusters
            .iter()
            .find(|(c, _)| *c == col)
            .map(|(_, text)| text.as_ref())
    }

    /// Row text with spacer cells removed, used for URL detection and
    /// accessibility probes.
    pub fn text(&self) -> String {
        let mut out = String::with_capacity(self.cells.len());
        for (col, cell) in self.cells.iter().enumerate() {
            if cell.width == CellWidth::Spacer {
                continue;
            }
            match self.cluster(col as u16) {
                Some(cluster) => out.push_str(cluster),
                None => out.push(cell.ch),
            }
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorStyle {
    #[default]
    Block,
    BlockHollow,
    Underline,
    Bar,
}

impl From<libghostty_vt::render::CursorVisualStyle> for CursorStyle {
    fn from(value: libghostty_vt::render::CursorVisualStyle) -> Self {
        use libghostty_vt::render::CursorVisualStyle as S;
        match value {
            S::Block => Self::Block,
            S::BlockHollow => Self::BlockHollow,
            S::Underline => Self::Underline,
            S::Bar => Self::Bar,
            _ => Self::Block,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FrameCursor {
    /// Viewport column.
    pub x: u16,
    /// Viewport row.
    pub y: u16,
    pub style: CursorStyle,
}

/// A complete viewport snapshot. Coordinates are viewport-relative, so the
/// renderer never has to account for the scrollback offset.
#[derive(Clone, Debug)]
pub struct Frame {
    pub rows: Arc<Vec<FrameRow>>,
    /// `None` while the cursor is hidden or scrolled out of the viewport.
    pub cursor: Option<FrameCursor>,
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor_color: Rgb,
    pub alt_screen: bool,
    pub mouse_tracking: bool,
    pub has_selection: bool,
}

impl Frame {
    pub fn line_count(&self) -> usize {
        self.rows.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(ch: char, width: CellWidth) -> FrameCell {
        FrameCell {
            ch,
            width,
            ..FrameCell::default()
        }
    }

    #[test]
    fn row_text_skips_spacers_and_uses_clusters() {
        let row = FrameRow {
            cells: vec![
                cell('あ', CellWidth::Wide),
                cell(' ', CellWidth::Spacer),
                cell('e', CellWidth::Narrow),
            ],
            clusters: vec![(2, "é".into())],
            selection: None,
        };
        assert_eq!(row.text(), "あé");
    }
}
