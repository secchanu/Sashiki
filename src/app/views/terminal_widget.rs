//! Custom terminal widget with IME support

use crate::app::message::Message;
use crate::theme::Palette;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad, Renderer as AdvancedRenderer};
use iced::advanced::widget::{self, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::advanced::input_method::{InputMethod, Purpose};
use iced::advanced::text::{Paragraph, Renderer as TextRenderer};
use iced::mouse::{self, Cursor};
use iced::{Element, Event, Length, Point, Rectangle, Size};
use unicode_width::UnicodeWidthChar;

// Terminal rendering constants
const FONT_SIZE: f32 = 13.0;
const LINE_HEIGHT: f32 = FONT_SIZE * 1.2;
const PADDING: f32 = 8.0;

/// Terminal widget state
#[derive(Debug)]
pub struct TerminalWidgetState {
    /// Cached character width for monospace font
    char_width: f32,
}

impl Default for TerminalWidgetState {
    fn default() -> Self {
        Self {
            char_width: FONT_SIZE * 0.6, // Initial estimate, will be measured
        }
    }
}

/// Custom terminal widget that supports IME input
pub struct TerminalWidget<'a> {
    content: String,
    focused: bool,
    palette: &'a Palette,
    /// Cursor position from terminal state (row, col)
    cursor_pos: (usize, usize),
    /// IME preedit text (composition in progress)
    preedit_text: &'a str,
}

impl<'a> TerminalWidget<'a> {
    pub fn new(content: String, focused: bool, palette: &'a Palette, cursor_pos: (usize, usize), preedit_text: &'a str) -> Self {
        Self {
            content,
            focused,
            palette,
            cursor_pos,
            preedit_text,
        }
    }
}

impl<'a> Widget<Message, iced::Theme, iced::Renderer> for TerminalWidget<'a> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<TerminalWidgetState>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(TerminalWidgetState::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        // Measure character width using the monospace font
        let state = tree.state.downcast_mut::<TerminalWidgetState>();

        // Create a paragraph with a single character to measure its width
        let paragraph = <iced::Renderer as TextRenderer>::Paragraph::with_text(iced::advanced::text::Text {
            content: "M",
            bounds: Size::new(f32::INFINITY, f32::INFINITY),
            size: iced::Pixels(FONT_SIZE),
            line_height: iced::advanced::text::LineHeight::Absolute(iced::Pixels(LINE_HEIGHT)),
            font: iced::Font::MONOSPACE,
            align_x: iced::alignment::Horizontal::Left.into(),
            align_y: iced::alignment::Vertical::Top.into(),
            shaping: iced::advanced::text::Shaping::Advanced,
            wrapping: iced::advanced::text::Wrapping::None,
        });

        state.char_width = paragraph.min_width();

        layout::Node::new(limits.max())
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<TerminalWidgetState>();
        let bounds = layout.bounds();

        // Handle mouse click for focus
        if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event {
            if cursor.is_over(bounds) {
                shell.publish(Message::TerminalFocus(true));
            }
        }

        // Request IME when focused
        if self.focused {
            let (cursor_row, cursor_col) = self.cursor_pos;
            let char_width = state.char_width;

            let cursor_x = bounds.x + PADDING + (cursor_col as f32 * char_width);
            let cursor_y = bounds.y + PADDING + (cursor_row as f32 * LINE_HEIGHT);

            let cursor_position = Point::new(cursor_x, cursor_y);
            shell.request_input_method::<String>(&InputMethod::Enabled {
                cursor: Rectangle::new(cursor_position, Size::new(1.0, FONT_SIZE)),
                purpose: Purpose::Normal,
                preedit: None,
            });
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<TerminalWidgetState>();
        let bounds = layout.bounds();
        let char_width = state.char_width;

        // Draw background
        renderer.fill_quad(
            Quad {
                bounds,
                border: iced::Border {
                    color: if self.focused {
                        self.palette.accent
                    } else {
                        self.palette.border
                    },
                    width: if self.focused { 2.0 } else { 1.0 },
                    radius: 0.0.into(),
                },
                shadow: Default::default(),
                snap: true,
            },
            self.palette.bg_secondary,
        );

        // Draw text content
        let text = iced::advanced::Text {
            content: self.content.clone(),
            bounds: Size::new(bounds.width - PADDING * 2.0, bounds.height - PADDING * 2.0),
            size: iced::Pixels(FONT_SIZE),
            line_height: iced::advanced::text::LineHeight::Absolute(iced::Pixels(LINE_HEIGHT)),
            font: iced::Font::MONOSPACE,
            align_x: iced::alignment::Horizontal::Left.into(),
            align_y: iced::alignment::Vertical::Top.into(),
            shaping: iced::advanced::text::Shaping::Advanced,
            wrapping: iced::advanced::text::Wrapping::None,
        };

        renderer.fill_text(
            text,
            Point::new(bounds.x + PADDING, bounds.y + PADDING),
            self.palette.text_primary,
            bounds,
        );

        // Draw cursor and preedit text at actual position
        if self.focused {
            let (cursor_row, cursor_col) = self.cursor_pos;
            let cursor_x = bounds.x + PADDING + (cursor_col as f32 * char_width);
            let cursor_y = bounds.y + PADDING + (cursor_row as f32 * LINE_HEIGHT);

            // Draw preedit text (IME composition) if present
            if !self.preedit_text.is_empty() {
                // Calculate preedit width considering full-width characters
                let preedit_width: f32 = self.preedit_text
                    .chars()
                    .map(|c| c.width().unwrap_or(1) as f32 * char_width)
                    .sum();
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle::new(
                            Point::new(cursor_x, cursor_y),
                            Size::new(preedit_width, FONT_SIZE),
                        ),
                        border: iced::Border {
                            color: self.palette.accent,
                            width: 1.0,
                            radius: 0.0.into(),
                        },
                        shadow: Default::default(),
                        snap: true,
                    },
                    self.palette.bg_tertiary,
                );

                // Draw preedit text
                let preedit = iced::advanced::Text {
                    content: self.preedit_text.to_string(),
                    bounds: Size::new(preedit_width, FONT_SIZE),
                    size: iced::Pixels(FONT_SIZE),
                    line_height: iced::advanced::text::LineHeight::Absolute(iced::Pixels(LINE_HEIGHT)),
                    font: iced::Font::MONOSPACE,
                    align_x: iced::alignment::Horizontal::Left.into(),
                    align_y: iced::alignment::Vertical::Top.into(),
                    shaping: iced::advanced::text::Shaping::Advanced,
                    wrapping: iced::advanced::text::Wrapping::None,
                };

                renderer.fill_text(
                    preedit,
                    Point::new(cursor_x, cursor_y),
                    self.palette.text_primary,
                    bounds,
                );
            } else {
                // Draw cursor block when no preedit
                renderer.fill_quad(
                    Quad {
                        bounds: Rectangle::new(
                            Point::new(cursor_x, cursor_y),
                            Size::new(char_width, FONT_SIZE),
                        ),
                        border: iced::Border::default(),
                        shadow: Default::default(),
                        snap: true,
                    },
                    self.palette.accent,
                );
            }
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }
}

impl<'a> From<TerminalWidget<'a>> for Element<'a, Message> {
    fn from(widget: TerminalWidget<'a>) -> Self {
        Element::new(widget)
    }
}

/// Helper function to create a terminal widget
pub fn terminal_widget<'a>(
    content: String,
    focused: bool,
    palette: &'a Palette,
    cursor_pos: (usize, usize),
    preedit_text: &'a str,
) -> TerminalWidget<'a> {
    TerminalWidget::new(content, focused, palette, cursor_pos, preedit_text)
}
