//! Terminal element for GPUI rendering
//!
//! This module implements the custom GPUI Element for rendering terminal content.

use std::collections::HashMap;
use std::sync::Arc;

use super::frame::{CellWidth, CursorStyle, FrameCell, FrameCursor, FrameRow, Rgb};
use super::{TerminalView, input_probe};
use crate::theme::*;
use gpui::{
    App, Bounds, Element, ElementId, ElementInputHandler, Entity, Font, FontStyle, FontWeight,
    GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, Pixels, Point, SharedString,
    Size, TextRun, TextStyle, UnderlineStyle, Window, fill, px, relative, rgb,
};

/// Padding around terminal content in pixels
pub(super) const TERMINAL_PADDING: f32 = 8.0;
/// Default cell width when font metrics unavailable
pub(super) const DEFAULT_CELL_WIDTH: f32 = 8.0;
/// Default cell height when font metrics unavailable
pub(super) const DEFAULT_CELL_HEIGHT: f32 = 16.0;
/// Lines to scroll per mouse wheel tick
pub(super) const SCROLL_LINES_WHEEL: i32 = 3;
/// Maximum milliseconds between clicks for multi-click detection
pub(super) const MULTI_CLICK_THRESHOLD_MS: u128 = 500;
/// Line height as a multiple of font size (1.4 is standard for terminal readability)
const LINE_HEIGHT_MULTIPLIER: f32 = 1.4;
/// Minimum element width in pixels to perform layout (avoids freezing on tiny resize)
const MIN_ELEMENT_WIDTH: f32 = 50.0;
/// Minimum element height in pixels to perform layout
const MIN_ELEMENT_HEIGHT: f32 = 40.0;
/// Minimum terminal columns (prevents degenerate grid)
const MIN_TERMINAL_COLS: u16 = 2;
/// Minimum terminal lines (prevents degenerate grid)
const MIN_TERMINAL_LINES: u16 = 2;
/// Alpha applied to faint (SGR 2) text
const FAINT_ALPHA: f32 = 0.6;

/// Terminal layout metadata for the paint phase.
/// Row data is shared from the current frame via `Arc` (zero-cost clone).
pub(super) struct TerminalLayout {
    /// Viewport rows shared from the frame (Arc::clone avoids a deep copy)
    pub rows: Arc<Vec<FrameRow>>,
    /// Cell dimensions
    pub cell_width: Pixels,
    pub line_height: Pixels,
    /// Cursor position and shape, absent while hidden or scrolled out of view
    pub cursor: Option<FrameCursor>,
    /// Default colors for cells that carry none
    pub foreground: Hsla,
    pub background: Hsla,
    pub cursor_color: Hsla,
    /// URL cell lookup (shared from view)
    pub url_cells: Arc<HashMap<(usize, usize), usize>>,
    /// Hovered URL index
    pub hovered_url_index: Option<usize>,
    /// Preedit text if any
    pub preedit_text: String,
}

pub(super) fn rgb_to_hsla(color: Rgb) -> Hsla {
    Hsla::from(gpui::Rgba {
        r: color.r as f32 / 255.0,
        g: color.g as f32 / 255.0,
        b: color.b as f32 / 255.0,
        a: 1.0,
    })
}

/// Custom element that renders terminal directly in paint phase
pub(super) struct TerminalElement {
    view: Entity<TerminalView>,
}

impl TerminalElement {
    pub fn new(view: Entity<TerminalView>) -> Self {
        Self { view }
    }
}

/// Style attributes a run of text is batched on.
#[derive(Clone, Copy, PartialEq)]
struct RunStyle {
    fg: Hsla,
    bold: bool,
    italic: bool,
    strikethrough: bool,
    underline: Option<UnderlineStyle>,
}

impl RunStyle {
    fn matches(&self, other: &RunStyle) -> bool {
        self.fg == other.fg
            && self.bold == other.bold
            && self.italic == other.italic
            && self.strikethrough == other.strikethrough
            && underline_eq(&self.underline, &other.underline)
    }
}

fn underline_eq(a: &Option<UnderlineStyle>, b: &Option<UnderlineStyle>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.thickness == b.thickness && a.color == b.color && a.wavy == b.wavy,
        _ => false,
    }
}

fn underline_style(
    kind: super::frame::Underline,
    color: Hsla,
    is_url: bool,
) -> Option<UnderlineStyle> {
    use super::frame::Underline;

    if is_url {
        return Some(UnderlineStyle {
            thickness: px(1.0),
            color: Some(color),
            wavy: false,
        });
    }

    match kind {
        Underline::None => None,
        Underline::Curly => Some(UnderlineStyle {
            thickness: px(1.0),
            color: Some(color),
            wavy: true,
        }),
        Underline::Double => Some(UnderlineStyle {
            thickness: px(2.0),
            color: Some(color),
            wavy: false,
        }),
        _ => Some(UnderlineStyle {
            thickness: px(1.0),
            color: Some(color),
            wavy: false,
        }),
    }
}

/// Accumulates consecutive same-style characters into a single `shape_line` call.
/// Reused across rows within a single `paint_cells` invocation.
struct TextBatch {
    // Constants for the paint call
    origin_x: Pixels,
    cell_width: Pixels,
    line_height: Pixels,
    font_size: Pixels,
    font: Font,
    // Per-segment mutable state
    start_col: Option<usize>,
    text: String,
    runs: Vec<TextRun>,
    style: Option<RunStyle>,
    /// Spaces after the last non-space char; flushed into the current run when the
    /// next visible char arrives. Prevents trailing whitespace in the batch string.
    parked_spaces: usize,
}

impl TextBatch {
    fn new(
        origin_x: Pixels,
        cell_width: Pixels,
        line_height: Pixels,
        font_size: Pixels,
        font: Font,
    ) -> Self {
        Self {
            origin_x,
            cell_width,
            line_height,
            font_size,
            font,
            start_col: None,
            text: String::new(),
            runs: Vec::new(),
            style: None,
            parked_spaces: 0,
        }
    }

    /// Reset state for a new row. Allocations are retained.
    fn reset_row(&mut self) {
        self.start_col = None;
        self.text.clear();
        self.runs.clear();
        self.style = None;
        self.parked_spaces = 0;
    }

    fn styled_font(&self, style: &RunStyle) -> Font {
        let mut font = self.font.clone();
        if style.bold {
            font.weight = FontWeight::BOLD;
        }
        if style.italic {
            font.style = FontStyle::Italic;
        }
        font
    }

    /// Shape and paint accumulated text, then reset segment state.
    /// Trailing parked spaces are discarded (their background is already painted per-cell).
    fn flush(&mut self, text_y: Pixels, window: &mut Window, cx: &mut App) {
        self.parked_spaces = 0;
        if let Some(start_col) = self.start_col.take() {
            if !self.text.is_empty() {
                let x = self.origin_x + self.cell_width * start_col;
                let text: SharedString = std::mem::take(&mut self.text).into();
                let shaped =
                    window
                        .text_system()
                        .shape_line(text, self.font_size, &self.runs, None);
                let _ = shaped.paint(
                    Point::new(x, text_y),
                    self.line_height,
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
        }
        self.text.clear();
        self.runs.clear();
        self.style = None;
    }

    fn park_space(&mut self) {
        if self.start_col.is_some() {
            self.parked_spaces += 1;
        }
    }

    /// Flush parked spaces into the current TextRun before a style change.
    /// Spaces are visually transparent; attributing them to the previous style
    /// has no visible effect.
    fn drain_parked_spaces(&mut self) {
        if self.parked_spaces > 0 {
            if let Some(last_run) = self.runs.last_mut() {
                last_run.len += self.parked_spaces;
                for _ in 0..self.parked_spaces {
                    self.text.push(' ');
                }
            }
            self.parked_spaces = 0;
        }
    }

    /// Accumulate one cell's text into the batch.
    fn push_text(&mut self, text: &str, col_idx: usize, style: RunStyle) {
        self.drain_parked_spaces();

        if self.start_col.is_none() {
            self.start_col = Some(col_idx);
        }

        if self.style.is_some_and(|current| current.matches(&style)) {
            if let Some(last_run) = self.runs.last_mut() {
                last_run.len += text.len();
            }
        } else {
            self.runs.push(TextRun {
                len: text.len(),
                font: self.styled_font(&style),
                color: style.fg,
                background_color: None,
                underline: style.underline,
                strikethrough: style.strikethrough.then(|| gpui::StrikethroughStyle {
                    thickness: px(1.0),
                    color: Some(style.fg),
                }),
            });
            self.style = Some(style);
        }
        self.text.push_str(text);
    }
}

/// State calculated during prepaint phase
pub(super) struct TerminalPrepaintState {
    pub layout: Option<TerminalLayout>,
    pub text_style: TextStyle,
    pub font_size: Pixels,
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = TerminalPrepaintState;

    fn id(&self) -> Option<ElementId> {
        Some("terminal-element".into())
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
        let style = gpui::Style {
            flex_grow: 1.0,
            size: gpui::Size {
                width: relative(1.).into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let layout_id = window.request_layout(style, None, cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let font_size = px(14.0);

        // Build text style for measuring and rendering
        let text_style = TextStyle {
            font_family: MONOSPACE_FONT.into(),
            font_size: font_size.into(),
            color: Hsla::from(rgb(TEXT)),
            ..Default::default()
        };

        // Calculate cell dimensions from font metrics
        let font_id = window.text_system().resolve_font(&text_style.font());
        let cell_width = window
            .text_system()
            .advance(font_id, font_size, 'M')
            .map(|s| s.width)
            .unwrap_or(px(DEFAULT_CELL_WIDTH));
        let line_height = font_size * LINE_HEIGHT_MULTIPLIER;

        // Minimum bounds check - skip resize if too small to avoid freezing
        let min_width = px(MIN_ELEMENT_WIDTH);
        let min_height = px(MIN_ELEMENT_HEIGHT);
        if bounds.size.width < min_width || bounds.size.height < min_height {
            self.view.update(cx, |view, _cx| {
                view.input_cursor_bounds = None;
            });
            return TerminalPrepaintState {
                layout: None,
                text_style,
                font_size,
            };
        }

        // Calculate terminal size from bounds
        let padding = px(TERMINAL_PADDING);
        let available_width = (bounds.size.width - padding * 2.0).max(cell_width);
        let available_height = (bounds.size.height - padding * 2.0).max(line_height);

        let cols = ((available_width / cell_width).floor() as u16).max(MIN_TERMINAL_COLS);
        let lines = ((available_height / line_height).floor() as u16).max(MIN_TERMINAL_LINES);

        // Resize terminal if needed and update view state for mouse handling
        let cell_width_f32: f32 = cell_width.into();
        let line_height_f32: f32 = line_height.into();
        let origin_x: f32 = bounds.origin.x.into();
        let origin_y: f32 = bounds.origin.y.into();
        self.view.update(cx, |view, _cx| {
            if let Some(ref terminal) = view.terminal {
                terminal.resize(
                    cols,
                    lines,
                    cell_width_f32 as u16,
                    line_height_f32 as u16,
                    TERMINAL_PADDING as u16,
                );
            }
            // Update cell dimensions and content origin for mouse handling
            view.cell_width = cell_width_f32;
            view.cell_height = line_height_f32;
            view.content_origin = (origin_x, origin_y);
        });

        // Build layout data from the current frame.
        let layout_origin = Point::new(bounds.origin.x + padding, bounds.origin.y + padding);
        let (layout, cursor_bounds) = {
            let view = self.view.read(cx);
            let layout = view.build_layout(cell_width, line_height);
            let cursor_bounds =
                view.compute_input_cursor_bounds(layout_origin, cell_width, line_height);
            (layout, cursor_bounds)
        };
        self.view.update(cx, |view, _cx| {
            view.input_cursor_bounds = cursor_bounds;
        });

        TerminalPrepaintState {
            layout,
            text_style,
            font_size,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let padding = px(TERMINAL_PADDING);
        let origin = Point::new(bounds.origin.x + padding, bounds.origin.y + padding);

        // Paint background
        let background = prepaint
            .layout
            .as_ref()
            .map(|layout| layout.background)
            .unwrap_or_else(|| Hsla::from(rgb(BG_BASE)));
        window.paint_quad(fill(bounds, background));

        // Paint terminal content
        if let Some(ref layout) = prepaint.layout {
            self.paint_cells(
                origin,
                layout,
                &prepaint.text_style,
                prepaint.font_size,
                window,
                cx,
            );
        }

        // Set up input handler
        let focus_handle = self.view.read(cx).focus_handle.clone();
        input_probe::ensure_window_uia_bridge(window);
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.view.clone()),
            cx,
        );
    }
}

impl TerminalElement {
    /// Resolve a cell's colors, applying the inverse attribute and the frame
    /// defaults. The background is `None` when the cell keeps the frame
    /// background, which is already painted.
    fn cell_colors(cell: &FrameCell, layout: &TerminalLayout) -> (Hsla, Option<Hsla>) {
        let fg = cell.fg.map(rgb_to_hsla).unwrap_or(layout.foreground);
        let bg = cell.bg.map(rgb_to_hsla);

        let (mut fg, bg) = if cell.attrs.inverse {
            (bg.unwrap_or(layout.background), Some(fg))
        } else {
            (fg, bg)
        };

        if cell.attrs.faint {
            fg.a *= FAINT_ALPHA;
        }
        (fg, bg)
    }

    /// Paint all terminal cells.
    ///
    /// Text rendering is batched per row: consecutive cells with the same style
    /// are accumulated into a single `shape_line` call rather than calling it
    /// once per character. This reduces GPU text-shaping calls from O(cols×rows)
    /// to O(style_changes×rows), typically a 10–40× reduction for typical terminal output.
    fn paint_cells(
        &self,
        origin: Point<Pixels>,
        layout: &TerminalLayout,
        text_style: &TextStyle,
        font_size: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        let cell_width = layout.cell_width;
        let line_height = layout.line_height;

        let mut batch = TextBatch::new(
            origin.x,
            cell_width,
            line_height,
            font_size,
            text_style.font(),
        );

        let selection_fg = Hsla::from(rgb(BG_BASE));
        let selection_bg = Hsla::from(rgb(BLUE));

        for (line_idx, row) in layout.rows.iter().enumerate() {
            let y = origin.y + line_height * line_idx;
            let text_y = y + (line_height - font_size) / 2.0;
            let cursor_col = layout
                .cursor
                .filter(|cursor| cursor.y as usize == line_idx)
                .map(|cursor| cursor.x as usize);

            batch.reset_row();

            for (col_idx, cell) in row.cells.iter().enumerate() {
                if cell.width == CellWidth::Spacer {
                    continue;
                }

                let (cell_fg, cell_bg) = Self::cell_colors(cell, layout);

                let x = origin.x + cell_width * col_idx;
                let is_wide = cell.width == CellWidth::Wide;
                let render_width = if is_wide {
                    cell_width * 2.0
                } else {
                    cell_width
                };

                let cell_bounds = Bounds::new(
                    Point::new(x, y),
                    Size {
                        width: render_width,
                        height: line_height,
                    },
                );

                let is_cursor = cursor_col == Some(col_idx);
                let is_selected = row.selection.is_some_and(|(start, end)| {
                    col_idx >= start as usize && col_idx <= end as usize
                });

                // Paint base background (selection or cell background)
                let bg_color = if is_selected {
                    Some(selection_bg)
                } else {
                    cell_bg
                };
                if let Some(bg) = bg_color {
                    window.paint_quad(fill(cell_bounds, bg));
                }

                // Paint cursor overlay according to configured shape
                if is_cursor {
                    self.paint_cursor(
                        layout.cursor.map(|cursor| cursor.style).unwrap_or_default(),
                        Point::new(x, y),
                        render_width,
                        line_height,
                        layout.cursor_color,
                        window,
                    );
                }

                // === Batched text rendering ===

                if cell.attrs.invisible {
                    batch.park_space();
                    continue;
                }

                let cluster = row.cluster(col_idx as u16);
                let c = cell.ch;

                if cluster.is_none() && c == ' ' {
                    batch.park_space();
                    continue;
                }

                // Compute URL state via O(1) HashMap lookup
                let (is_url, is_url_hovered) =
                    if let Some(&url_idx) = layout.url_cells.get(&(line_idx, col_idx)) {
                        (true, layout.hovered_url_index == Some(url_idx))
                    } else {
                        (false, false)
                    };

                // Compute effective foreground color (cursor / selection / URL overrides)
                let is_block_cursor = is_cursor
                    && matches!(
                        layout.cursor.map(|cursor| cursor.style),
                        Some(CursorStyle::Block)
                    );
                let fg_color = if is_block_cursor || (is_selected && !is_cursor) {
                    selection_fg
                } else if is_url_hovered {
                    Hsla::from(rgb(TEAL))
                } else if is_url {
                    Hsla::from(rgb(BLUE))
                } else {
                    cell_fg
                };

                let run_style = RunStyle {
                    fg: fg_color,
                    bold: cell.attrs.bold,
                    italic: cell.attrs.italic,
                    strikethrough: cell.attrs.strikethrough,
                    underline: underline_style(cell.attrs.underline, fg_color, is_url),
                };

                // Block elements (U+2580–U+259F): draw as filled rectangles instead of
                // font glyphs (same approach as Alacritty's builtin_font).
                // Flush the pending text batch first so paint order stays correct.
                if cluster.is_none()
                    && self.paint_block_element(
                        c,
                        Point::new(x, y),
                        render_width,
                        line_height,
                        fg_color,
                        window,
                    )
                {
                    batch.flush(text_y, window, cx);
                    continue;
                }

                // Wide characters need an explicit advance-width hint (2× cell_width).
                // Flush the current batch and shape the wide char individually.
                if is_wide {
                    batch.flush(text_y, window, cx);
                    let wtext: SharedString = match cluster {
                        Some(cluster) => cluster.to_string().into(),
                        None => c.to_string().into(),
                    };
                    let wrun = [TextRun {
                        len: wtext.len(),
                        font: batch.styled_font(&run_style),
                        color: fg_color,
                        background_color: None,
                        underline: run_style.underline,
                        strikethrough: run_style.strikethrough.then(|| gpui::StrikethroughStyle {
                            thickness: px(1.0),
                            color: Some(fg_color),
                        }),
                    }];
                    let shaped = window.text_system().shape_line(
                        wtext,
                        font_size,
                        &wrun,
                        Some(render_width),
                    );
                    let _ = shaped.paint(
                        Point::new(x, text_y),
                        line_height,
                        gpui::TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                    continue;
                }

                match cluster {
                    Some(cluster) => batch.push_text(cluster, col_idx, run_style),
                    None => {
                        let mut buffer = [0u8; 4];
                        batch.push_text(c.encode_utf8(&mut buffer), col_idx, run_style);
                    }
                }
            }

            batch.flush(text_y, window, cx);
        }

        // Paint preedit overlay if present
        if !layout.preedit_text.is_empty() {
            self.paint_preedit(
                origin,
                &layout.preedit_text,
                cell_width,
                line_height,
                text_style,
                font_size,
                window,
                cx,
            );
        }
    }

    fn paint_cursor(
        &self,
        style: CursorStyle,
        origin: Point<Pixels>,
        width: Pixels,
        height: Pixels,
        color: Hsla,
        window: &mut Window,
    ) {
        let (x, y) = (origin.x, origin.y);
        match style {
            CursorStyle::Block => {
                window.paint_quad(fill(Bounds::new(origin, Size { width, height }), color));
            }
            CursorStyle::Underline => {
                let thickness = px(2.0);
                window.paint_quad(fill(
                    Bounds::new(
                        Point::new(x, y + height - thickness),
                        Size {
                            width,
                            height: thickness,
                        },
                    ),
                    color,
                ));
            }
            CursorStyle::Bar => {
                let thickness = px(2.0);
                window.paint_quad(fill(
                    Bounds::new(
                        origin,
                        Size {
                            width: thickness,
                            height,
                        },
                    ),
                    color,
                ));
            }
            CursorStyle::BlockHollow => {
                let thickness = px(1.0);
                window.paint_quad(fill(
                    Bounds::new(
                        origin,
                        Size {
                            width,
                            height: thickness,
                        },
                    ),
                    color,
                ));
                window.paint_quad(fill(
                    Bounds::new(
                        Point::new(x, y + height - thickness),
                        Size {
                            width,
                            height: thickness,
                        },
                    ),
                    color,
                ));
                window.paint_quad(fill(
                    Bounds::new(
                        origin,
                        Size {
                            width: thickness,
                            height,
                        },
                    ),
                    color,
                ));
                window.paint_quad(fill(
                    Bounds::new(
                        Point::new(x + width - thickness, y),
                        Size {
                            width: thickness,
                            height,
                        },
                    ),
                    color,
                ));
            }
        }
    }

    /// Paint a block element character (U+2580-U+259F) as filled rectangles.
    /// Returns true if the character was handled, false otherwise.
    /// Based on Alacritty's builtin_font approach.
    fn paint_block_element(
        &self,
        c: char,
        origin: Point<Pixels>,
        width: Pixels,
        height: Pixels,
        color: Hsla,
        window: &mut Window,
    ) -> bool {
        match c {
            // Full block
            '\u{2588}' => {
                let bounds = Bounds::new(origin, Size { width, height });
                window.paint_quad(fill(bounds, color));
            }
            // Upper half block
            '\u{2580}' => {
                let bounds = Bounds::new(
                    origin,
                    Size {
                        width,
                        height: height * 0.5,
                    },
                );
                window.paint_quad(fill(bounds, color));
            }
            // Lower blocks: 1/8 through 7/8 (fraction derived from code point offset)
            '\u{2581}'..='\u{2587}' => {
                let eighths = (c as u32 - 0x2580) as f32 / 8.0;
                let h = height * eighths;
                let bounds = Bounds::new(
                    Point::new(origin.x, origin.y + height - h),
                    Size { width, height: h },
                );
                window.paint_quad(fill(bounds, color));
            }
            // Left blocks: 7/8 through 1/8 (fraction derived from code point offset)
            '\u{2589}'..='\u{258f}' => {
                let eighths = (0x2590 - c as u32) as f32 / 8.0;
                let w = width * eighths;
                let bounds = Bounds::new(origin, Size { width: w, height });
                window.paint_quad(fill(bounds, color));
            }
            // Right half block
            '\u{2590}' => {
                let w = width * 0.5;
                let bounds = Bounds::new(
                    Point::new(origin.x + w, origin.y),
                    Size { width: w, height },
                );
                window.paint_quad(fill(bounds, color));
            }
            // Shade characters: light/medium/dark (alpha derived from code point offset)
            '\u{2591}'..='\u{2593}' => {
                let mut shade = color;
                shade.a *= (c as u32 - 0x2590) as f32 / 4.0;
                let bounds = Bounds::new(origin, Size { width, height });
                window.paint_quad(fill(bounds, shade));
            }
            // Upper one eighth
            '\u{2594}' => {
                let h = height * 0.125;
                let bounds = Bounds::new(origin, Size { width, height: h });
                window.paint_quad(fill(bounds, color));
            }
            // Right one eighth
            '\u{2595}' => {
                let w = width * 0.125;
                let bounds = Bounds::new(
                    Point::new(origin.x + width - w, origin.y),
                    Size { width: w, height },
                );
                window.paint_quad(fill(bounds, color));
            }
            // Quadrant characters (U+2596-U+259F)
            '\u{2596}'..='\u{259f}' => {
                let half_w = width * 0.5;
                let half_h = height * 0.5;
                let mid_x = origin.x + half_w;
                let mid_y = origin.y + half_h;
                let half_size = Size {
                    width: half_w,
                    height: half_h,
                };

                // Each quadrant character is a combination of 4 quadrants
                let (tl, tr, bl, br) = match c {
                    '\u{2596}' => (false, false, true, false), // ▖ lower left
                    '\u{2597}' => (false, false, false, true), // ▗ lower right
                    '\u{2598}' => (true, false, false, false), // ▘ upper left
                    '\u{2599}' => (true, false, true, true),   // ▙ upper left + lower
                    '\u{259a}' => (true, false, false, true),  // ▚ upper left + lower right
                    '\u{259b}' => (true, true, true, false),   // ▛ upper + lower left
                    '\u{259c}' => (true, true, false, true),   // ▜ upper + lower right
                    '\u{259d}' => (false, true, false, false), // ▝ upper right
                    '\u{259e}' => (false, true, true, false),  // ▞ upper right + lower left
                    '\u{259f}' => (false, true, true, true),   // ▟ upper right + lower
                    _ => unreachable!(),
                };

                if tl {
                    window.paint_quad(fill(Bounds::new(origin, half_size), color));
                }
                if tr {
                    window.paint_quad(fill(
                        Bounds::new(Point::new(mid_x, origin.y), half_size),
                        color,
                    ));
                }
                if bl {
                    window.paint_quad(fill(
                        Bounds::new(Point::new(origin.x, mid_y), half_size),
                        color,
                    ));
                }
                if br {
                    window.paint_quad(fill(
                        Bounds::new(Point::new(mid_x, mid_y), half_size),
                        color,
                    ));
                }
            }
            _ => return false,
        }
        true
    }

    /// Paint IME preedit overlay
    fn paint_preedit(
        &self,
        origin: Point<Pixels>,
        preedit_text: &str,
        cell_width: Pixels,
        line_height: Pixels,
        text_style: &TextStyle,
        font_size: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        let cell_width_f32: f32 = cell_width.into();
        let preedit_padding = px(TERMINAL_PADDING);

        let mut style = text_style.clone();
        style.color = Hsla::from(rgb(YELLOW));

        let display_text = format!("IME: {}", preedit_text);
        // Use character count (not byte length) to handle multibyte characters.
        // Note: fullwidth CJK characters occupy 2 cells but are counted as 1 here;
        // a unicode-width crate would be needed for exact width.
        let char_count = display_text.chars().count();
        let preedit_width = px(char_count as f32 * cell_width_f32) + preedit_padding * 2.0;

        let preedit_bg = Bounds::new(
            Point::new(origin.x, origin.y),
            Size {
                width: preedit_width,
                height: line_height + preedit_padding,
            },
        );
        window.paint_quad(fill(preedit_bg, Hsla::from(rgb(BG_SURFACE0))));

        let text: SharedString = display_text.into();
        let runs = [TextRun {
            len: text.len(),
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];

        let shaped = window
            .text_system()
            .shape_line(text, font_size, &runs, None);
        let text_origin = Point::new(origin.x + preedit_padding, origin.y + preedit_padding / 2.0);
        let _ = shaped.paint(
            text_origin,
            line_height,
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        );
    }
}
