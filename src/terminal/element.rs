//! Terminal element for GPUI rendering
//!
//! This module implements the custom GPUI Element for rendering terminal content.

use std::collections::HashMap;
use std::sync::Arc;

use super::{TerminalView, input_probe};
use super::view::{ansi_color_to_hsla, named_color_to_hsla};
use crate::theme::*;
use alacritty_terminal::term::cell::Flags as CellFlags;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, CursorShape, NamedColor};
use gpui::{
    App, Bounds, Element, ElementId, ElementInputHandler, Entity, Font, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Point, SharedString, Size, TextRun,
    TextStyle, UnderlineStyle, Window, fill, px, relative, rgb,
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

/// Terminal layout metadata for the paint phase.
/// Cell data is shared from `CachedContent` via `Arc` (zero-cost clone).
pub(super) struct TerminalLayout {
    /// Grid of cells shared from CachedContent (Arc::clone avoids deep copy)
    pub cells: Arc<Vec<Vec<super::view::CachedCell>>>,
    /// Cell dimensions
    pub cell_width: Pixels,
    pub line_height: Pixels,
    /// Cursor shape for this render pass
    pub cursor_shape: CursorShape,
    /// Cursor position in display coordinates (line, col)
    pub cursor: (i32, usize),
    /// Whether cursor should be visible
    pub cursor_visible: bool,
    /// Display offset for scrollback
    pub display_offset: i32,
    /// Current text selection
    pub selection: Option<super::view::TerminalSelection>,
    /// URL cell lookup (shared from view)
    pub url_cells: Arc<HashMap<(usize, usize), usize>>,
    /// Hovered URL index
    pub hovered_url_index: Option<usize>,
    /// Preedit text if any
    pub preedit_text: String,
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

#[inline]
fn url_underline(is_url: bool, color: Hsla) -> Option<UnderlineStyle> {
    if is_url {
        Some(UnderlineStyle {
            thickness: px(1.0),
            color: Some(color),
            wavy: false,
        })
    } else {
        None
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
    fg: Option<Hsla>,
    underline: bool,
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
            fg: None,
            underline: false,
            parked_spaces: 0,
        }
    }

    /// Reset state for a new row. Allocations are retained.
    fn reset_row(&mut self) {
        self.start_col = None;
        self.text.clear();
        self.runs.clear();
        self.fg = None;
        self.underline = false;
        self.parked_spaces = 0;
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
        self.runs.clear();
        self.fg = None;
    }

    fn park_space(&mut self) {
        if self.start_col.is_some() {
            self.parked_spaces += 1;
        }
    }

    /// Flush parked spaces into the current TextRun before a style change.
    /// Spaces are visually transparent; attributing them to the previous color
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

    /// Accumulate a regular character into the batch.
    fn push_char(&mut self, c: char, col_idx: usize, fg_color: Hsla, is_underline: bool) {
        self.drain_parked_spaces();

        if self.start_col.is_none() {
            self.start_col = Some(col_idx);
        }

        let style_matches = self
            .fg
            .map_or(false, |fg| fg == fg_color && self.underline == is_underline);

        if style_matches {
            self.runs.last_mut().unwrap().len += c.len_utf8();
        } else {
            self.runs.push(TextRun {
                len: c.len_utf8(),
                font: self.font.clone(),
                color: fg_color,
                background_color: None,
                underline: url_underline(is_underline, fg_color),
                strikethrough: None,
            });
            self.fg = Some(fg_color);
            self.underline = is_underline;
        }
        self.text.push(c);
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
                terminal.resize(cols, lines, cell_width_f32 as u16, line_height_f32 as u16);
            }
            // Update cell dimensions and content origin for mouse handling
            view.cell_width = cell_width_f32;
            view.cell_height = line_height_f32;
            view.content_origin = (origin_x, origin_y);
        });

        // Build layout data from terminal grid.
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
        window.paint_quad(fill(bounds, Hsla::from(rgb(BG_BASE))));

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
    /// Paint all terminal cells.
    ///
    /// Text rendering is batched per row: consecutive cells with the same foreground color
    /// and underline state are accumulated into a single `shape_line` call rather than
    /// calling it once per character. This reduces GPU text-shaping calls from O(cols×rows)
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

        // Pre-compute cursor display position
        let (cursor_disp_line, cursor_col) = layout.cursor;
        let selection = layout
            .selection
            .filter(|sel| sel.start != sel.end);

        for (line_idx, row) in layout.cells.iter().enumerate() {
            let y = origin.y + line_height * line_idx;
            let text_y = y + (line_height - font_size) / 2.0;
            let actual_line = line_idx as i32 - layout.display_offset;
            let is_cursor_line = layout.cursor_visible && line_idx as i32 == cursor_disp_line;

            batch.reset_row();

            for (col_idx, cell) in row.iter().enumerate() {
                if cell.flags.contains(CellFlags::WIDE_CHAR_SPACER) {
                    continue;
                }

                // Convert AnsiColor → Hsla, handling INVERSE flag
                let is_inverse = cell.flags.contains(CellFlags::INVERSE);
                let (cell_fg, cell_bg) = if is_inverse {
                    let fg = if cell.bg == AnsiColor::Named(NamedColor::Background) {
                        named_color_to_hsla(NamedColor::Background)
                    } else {
                        ansi_color_to_hsla(cell.bg)
                    };
                    (fg, Some(ansi_color_to_hsla(cell.fg)))
                } else {
                    let fg = ansi_color_to_hsla(cell.fg);
                    let bg = if cell.bg == AnsiColor::Named(NamedColor::Background) {
                        None
                    } else {
                        Some(ansi_color_to_hsla(cell.bg))
                    };
                    (fg, bg)
                };

                let x = origin.x + cell_width * col_idx;
                let is_wide = cell.flags.contains(CellFlags::WIDE_CHAR);

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

                let is_cursor = is_cursor_line && col_idx == cursor_col;
                let is_selected = selection
                    .map_or(false, |sel| sel.contains(actual_line, col_idx));

                // Paint base background (selection or cell background)
                let bg_color = if is_selected {
                    Some(Hsla::from(rgb(BLUE)))
                } else {
                    cell_bg
                };
                if let Some(bg) = bg_color {
                    window.paint_quad(fill(cell_bounds, bg));
                }

                // Paint cursor overlay according to configured shape
                if is_cursor {
                    let cursor_color = Hsla::from(rgb(ROSEWATER));
                    match layout.cursor_shape {
                        CursorShape::Block => {
                            window.paint_quad(fill(cell_bounds, cursor_color));
                        }
                        CursorShape::Hidden => {}
                        CursorShape::Underline => {
                            let thickness = px(2.0);
                            window.paint_quad(fill(
                                Bounds::new(
                                    Point::new(x, y + line_height - thickness),
                                    Size {
                                        width: render_width,
                                        height: thickness,
                                    },
                                ),
                                cursor_color,
                            ));
                        }
                        CursorShape::Beam => {
                            let thickness = px(2.0);
                            window.paint_quad(fill(
                                Bounds::new(
                                    Point::new(x, y),
                                    Size {
                                        width: thickness,
                                        height: line_height,
                                    },
                                ),
                                cursor_color,
                            ));
                        }
                        CursorShape::HollowBlock => {
                            let thickness = px(1.0);
                            window.paint_quad(fill(
                                Bounds::new(
                                    Point::new(x, y),
                                    Size {
                                        width: render_width,
                                        height: thickness,
                                    },
                                ),
                                cursor_color,
                            ));
                            window.paint_quad(fill(
                                Bounds::new(
                                    Point::new(x, y + line_height - thickness),
                                    Size {
                                        width: render_width,
                                        height: thickness,
                                    },
                                ),
                                cursor_color,
                            ));
                            window.paint_quad(fill(
                                Bounds::new(
                                    Point::new(x, y),
                                    Size {
                                        width: thickness,
                                        height: line_height,
                                    },
                                ),
                                cursor_color,
                            ));
                            window.paint_quad(fill(
                                Bounds::new(
                                    Point::new(x + render_width - thickness, y),
                                    Size {
                                        width: thickness,
                                        height: line_height,
                                    },
                                ),
                                cursor_color,
                            ));
                        }
                    }
                }

                // === Batched text rendering ===

                let c = if cell.c == '\0' { ' ' } else { cell.c };

                if c == ' ' {
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
                let is_block_cursor =
                    is_cursor && matches!(layout.cursor_shape, CursorShape::Block);
                let is_hidden_cursor =
                    is_cursor && matches!(layout.cursor_shape, CursorShape::Hidden);
                let fg_color = if is_block_cursor
                    || (is_selected && (!is_cursor || is_hidden_cursor))
                {
                    Hsla::from(rgb(BG_BASE))
                } else if is_url_hovered {
                    Hsla::from(rgb(TEAL))
                } else if is_url {
                    Hsla::from(rgb(BLUE))
                } else {
                    cell_fg
                };

                // Block elements (U+2580–U+259F): draw as filled rectangles instead of
                // font glyphs (same approach as Alacritty's builtin_font).
                // Flush the pending text batch first so paint order stays correct.
                if self.paint_block_element(
                    c,
                    Point::new(x, y),
                    render_width,
                    line_height,
                    fg_color,
                    window,
                ) {
                    batch.flush(text_y, window, cx);
                    continue;
                }

                // Wide characters need an explicit advance-width hint (2× cell_width).
                // Flush the current batch and shape the wide char individually.
                if is_wide {
                    batch.flush(text_y, window, cx);
                    let wtext: SharedString = c.to_string().into();
                    let wrun = [TextRun {
                        len: wtext.len(),
                        font: batch.font.clone(),
                        color: fg_color,
                        background_color: None,
                        underline: url_underline(is_url, fg_color),
                        strikethrough: None,
                    }];
                    let shaped = window
                        .text_system()
                        .shape_line(wtext, font_size, &wrun, Some(render_width));
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

                batch.push_char(c, col_idx, fg_color, is_url);
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
