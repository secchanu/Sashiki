//! Terminal view component

use super::terminal_widget::terminal_widget;
use crate::app::message::Message;
use crate::session::WorktreeSession;
use crate::theme::Palette;
use iced::widget::{container, text};
use iced::{Element, Length};

/// Render the terminal view
pub fn view_terminal<'a>(
    session: Option<&'a WorktreeSession>,
    focused: bool,
    height: f32,
    palette: &'a Palette,
    preedit_text: &'a str,
) -> Element<'a, Message> {
    if let Some(session) = session {
        let output = session.terminal.get_visible_text();
        let cursor_pos = session.terminal.get_cursor_position();

        container(terminal_widget(output, focused, palette, cursor_pos, preedit_text))
            .width(Length::Fill)
            .height(Length::Fixed(height))
            .into()
    } else {
        container(text("No terminal").color(palette.text_muted))
            .width(Length::Fill)
            .height(Length::Fixed(height))
            .padding(8)
            .style(move |_theme| container::Style {
                background: Some(palette.bg_secondary.into()),
                border: iced::Border {
                    color: palette.border,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            })
            .into()
    }
}
