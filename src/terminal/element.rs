//! Terminal element for GPUI rendering
//!
//! This module implements the custom GPUI Element for rendering terminal content.

use super::TerminalView;
use crate::theme::*;
use gpui::{
    App, Bounds, Element, ElementId, ElementInputHandler, Entity, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, Point, SharedString, Size, TextRun,
    TextStyle, Window, fill, px, relative, rgb,
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

/// Terminal cell data for paint phase rendering
#[derive(Clone)]
pub(super) struct CellData {
    /// Character to display
    pub c: char,
    /// Foreground color
    pub fg: Hsla,
    /// Background color (None = transparent)
    pub bg: Option<Hsla>,
    /// Is this the cursor position
    pub is_cursor: bool,
    /// Is this cell selected
    pub is_selected: bool,
    /// Whether this cell is a wide character (occupies 2 cells)
    pub is_wide_char: bool,
    /// Whether this cell is a spacer for a wide character (should skip rendering)
    pub is_wide_spacer: bool,
}

/// Cached terminal layout for paint phase
pub(super) struct TerminalLayout {
    /// Grid of cells (rows x cols)
    pub cells: Vec<Vec<CellData>>,
    /// Cell dimensions
    pub cell_width: Pixels,
    pub line_height: Pixels,
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

        // Build layout data from terminal grid
        let layout = self.view.read(cx).build_layout(cell_width, line_height);

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
        if focus_handle.is_focused(window) {
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.view.clone()),
                cx,
            );
        }
    }
}

impl TerminalElement {
    /// Paint all terminal cells
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

        for (line_idx, row) in layout.cells.iter().enumerate() {
            let y = origin.y + line_height * line_idx;

            for (col_idx, cell) in row.iter().enumerate() {
                // Skip wide character spacers - they're handled by the previous cell
                if cell.is_wide_spacer {
                    continue;
                }

                let x = origin.x + cell_width * col_idx;

                // Wide characters occupy 2 cells in terminal grid
                let render_width = if cell.is_wide_char {
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

                // Paint background
                let bg_color = if cell.is_cursor {
                    Some(Hsla::from(rgb(ROSEWATER)))
                } else if cell.is_selected {
                    Some(Hsla::from(rgb(BLUE)))
                } else {
                    cell.bg
                };

                if let Some(bg) = bg_color {
                    window.paint_quad(fill(cell_bounds, bg));
                }

                // Paint character
                if cell.c != ' ' {
                    let fg_color = if cell.is_cursor || cell.is_selected {
                        Hsla::from(rgb(BG_BASE))
                    } else {
                        cell.fg
                    };

                    // Block elements (U+2580-U+259F): draw as filled rectangles
                    // instead of font glyphs to ensure gap-free rendering
                    // (same approach as Alacritty's builtin_font)
                    if self.paint_block_element(
                        cell.c,
                        Point::new(x, y),
                        render_width,
                        line_height,
                        fg_color,
                        window,
                    ) {
                        continue;
                    }

                    let mut style = text_style.clone();
                    style.color = fg_color;

                    let text: SharedString = cell.c.to_string().into();
                    let runs = [TextRun {
                        len: text.len(),
                        font: style.font(),
                        color: style.color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }];

                    let shaped =
                        window
                            .text_system()
                            .shape_line(text, font_size, &runs, Some(render_width));
                    // Center text vertically in cell
                    let text_y = y + (line_height - font_size) / 2.0;
                    let text_origin = Point::new(x, text_y);
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
