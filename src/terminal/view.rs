//! Terminal view for GPUI rendering

use super::Terminal;
use crate::theme::{self, *};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};
use gpui::{
    App, AsyncApp, Bounds, Context, Element, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, GlobalElementId, Hsla, InspectorElementId,
    IntoElement, KeyBinding, LayoutId, MouseButton, ParentElement, Pixels, Render, Styled,
    UTF16Selection, WeakEntity, Window, actions, div, prelude::*, rgb, rgba,
};
use std::ops::Range;
use std::sync::Arc;

// Define actions for special keys
actions!(
    terminal,
    [
        Enter, Backspace, Tab, Escape, Up, Down, Left, Right, Home, End, Delete, PageUp, PageDown,
    ]
);

/// Terminal cell: (character, foreground, background, is_cursor)
type TerminalCell = (char, Hsla, Hsla, bool);

pub struct TerminalView {
    terminal: Option<Arc<Terminal>>,
    focus_handle: FocusHandle,
    preedit_text: String,
    /// Error message if terminal creation failed
    error_message: Option<String>,
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

                // Event-based refresh: only update when terminal events occur
                cx.spawn(
                    async move |this: WeakEntity<TerminalView>, cx: &mut AsyncApp| {
                        while let Ok(_event) = event_rx.recv().await {
                            let should_break = cx.update(|cx| {
                                if let Some(this) = this.upgrade() {
                                    this.update(cx, |_, cx: &mut Context<TerminalView>| {
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

                Self {
                    terminal: Some(terminal),
                    focus_handle,
                    preedit_text: String::new(),
                    error_message: None,
                }
            }
            Err(e) => Self {
                terminal: None,
                focus_handle,
                preedit_text: String::new(),
                error_message: Some(format!("Failed to create terminal: {}", e)),
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
        if let Some(ref terminal) = self.terminal {
            terminal.write(text.as_bytes());
        }
    }

    /// Bind terminal key actions
    pub fn bind_keys(cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("enter", Enter, Some("Terminal")),
            KeyBinding::new("backspace", Backspace, Some("Terminal")),
            KeyBinding::new("tab", Tab, Some("Terminal")),
            KeyBinding::new("escape", Escape, Some("Terminal")),
            KeyBinding::new("up", Up, Some("Terminal")),
            KeyBinding::new("down", Down, Some("Terminal")),
            KeyBinding::new("left", Left, Some("Terminal")),
            KeyBinding::new("right", Right, Some("Terminal")),
            KeyBinding::new("home", Home, Some("Terminal")),
            KeyBinding::new("end", End, Some("Terminal")),
            KeyBinding::new("delete", Delete, Some("Terminal")),
            KeyBinding::new("pageup", PageUp, Some("Terminal")),
            KeyBinding::new("pagedown", PageDown, Some("Terminal")),
        ]);
    }

    // Action handlers - send ANSI escape sequences to terminal
    fn write_to_terminal(&self, data: &[u8]) {
        if let Some(ref terminal) = self.terminal {
            terminal.write(data);
        }
    }

    fn on_enter(&mut self, _: &Enter, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\r");
    }

    fn on_backspace(&mut self, _: &Backspace, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x7f");
    }

    fn on_tab(&mut self, _: &Tab, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\t");
    }

    fn on_escape(&mut self, _: &Escape, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b");
    }

    fn on_up(&mut self, _: &Up, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[A");
    }

    fn on_down(&mut self, _: &Down, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[B");
    }

    fn on_left(&mut self, _: &Left, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[D");
    }

    fn on_right(&mut self, _: &Right, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[C");
    }

    fn on_home(&mut self, _: &Home, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[H");
    }

    fn on_end(&mut self, _: &End, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[F");
    }

    fn on_delete(&mut self, _: &Delete, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[3~");
    }

    fn on_page_up(&mut self, _: &PageUp, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[5~");
    }

    fn on_page_down(&mut self, _: &PageDown, _: &mut Window, _: &mut Context<Self>) {
        self.write_to_terminal(b"\x1b[6~");
    }

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
            NamedColor::BrightRed => theme::ansi::RED,
            NamedColor::BrightGreen => theme::ansi::GREEN,
            NamedColor::BrightYellow => theme::ansi::YELLOW,
            NamedColor::BrightBlue => theme::ansi::BLUE,
            NamedColor::BrightMagenta => theme::ansi::MAGENTA,
            NamedColor::BrightCyan => theme::ansi::CYAN,
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

    fn render_terminal_content(&self, _cx: &mut Context<Self>) -> gpui::AnyElement {
        // Show error message if terminal creation failed
        if let Some(ref error) = self.error_message {
            return div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .size_full()
                .child(div().text_color(Hsla::from(rgb(RED))).child(error.clone()))
                .into_any_element();
        }

        let Some(ref terminal) = self.terminal else {
            return div()
                .flex()
                .items_center()
                .justify_center()
                .size_full()
                .child(
                    div()
                        .text_color(Hsla::from(rgb(TEXT_MUTED)))
                        .child("Terminal not available"),
                )
                .into_any_element();
        };

        let mut rows: Vec<Vec<TerminalCell>> = Vec::new();

        terminal.with_term(|term| {
            let content = term.grid();
            let cols = content.columns();
            let total_lines = content.screen_lines();

            let cursor = term.grid().cursor.point;
            let cursor_line = cursor.line.0;
            let cursor_col = cursor.column.0;

            for line_idx in 0..total_lines {
                let mut row_cells: Vec<TerminalCell> = Vec::new();
                let is_cursor_line = line_idx as i32 == cursor_line;

                for col_idx in 0..cols {
                    let point = Point::new(Line(line_idx as i32), Column(col_idx));
                    let cell = &content[point];

                    let fg = Self::ansi_color_to_hsla(cell.fg);
                    let bg = if cell.bg == AnsiColor::Named(NamedColor::Background) {
                        Hsla::from(rgba(0x00000000))
                    } else {
                        Self::ansi_color_to_hsla(cell.bg)
                    };

                    let is_cursor = is_cursor_line && col_idx == cursor_col;
                    let c = if cell.c == ' ' || cell.c == '\0' {
                        ' '
                    } else {
                        cell.c
                    };

                    row_cells.push((c, fg, bg, is_cursor));
                }

                rows.push(row_cells);
            }
        });

        let preedit = self.preedit_text.clone();

        div()
            .flex()
            .flex_col()
            .font_family(MONOSPACE_FONT)
            .text_sm()
            .children(rows.into_iter().map(|row_cells| {
                div().flex().flex_row().children(row_cells.into_iter().map(
                    |(c, fg, bg, is_cursor)| {
                        let mut cell_div = div().text_color(if is_cursor {
                            Hsla::from(rgb(BG_BASE))
                        } else {
                            fg
                        });

                        if is_cursor {
                            cell_div = cell_div.bg(Hsla::from(rgb(ROSEWATER)));
                        } else if bg.a > 0.0 {
                            cell_div = cell_div.bg(bg);
                        }

                        cell_div.child(c.to_string())
                    },
                ))
            }))
            .when(!preedit.is_empty(), |this| {
                this.child(
                    div()
                        .absolute()
                        .bg(rgb(BG_SURFACE0))
                        .text_color(rgb(YELLOW))
                        .px_2()
                        .py_1()
                        .rounded_sm()
                        .child(format!("IME: {}", preedit)),
                )
            })
            .into_any_element()
    }
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// IME input handler for terminal - implements EntityInputHandler
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

/// Custom element that handles input during paint phase
struct TerminalElement {
    view: Entity<TerminalView>,
    content: gpui::AnyElement,
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = gpui::AnyElement;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some("terminal-input-element".into())
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
        let mut content = std::mem::replace(&mut self.content, gpui::Empty.into_any_element());
        let layout_id = content.request_layout(window, cx);
        (layout_id, content)
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        content: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        content.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        content: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        content.paint(window, cx);

        // Set up input handler during paint phase
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

impl Render for TerminalView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.focus_handle.clone();

        let content = div()
            .id("terminal-view")
            .key_context("Terminal")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(rgb(BG_BASE))
            .text_color(rgb(TEXT))
            .p_2()
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
            .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
                window.focus(&focus_handle, cx);
            })
            .child(self.render_terminal_content(cx))
            .into_any_element();

        TerminalElement {
            view: cx.entity(),
            content,
        }
    }
}
