//! Terminal view component

use crate::app::message::Message;
use crate::session::WorktreeSession;
use crate::theme::Palette;
use iced::widget::{button, container, scrollable, text};
use iced::{Element, Length};

/// Render the terminal view
pub fn view_terminal<'a>(
    session: Option<&WorktreeSession>,
    focused: bool,
    height: f32,
    palette: &Palette,
) -> Element<'a, Message> {
    let border_color = if focused {
        palette.accent
    } else {
        palette.border
    };
    let bg = palette.bg_secondary;

    let terminal_content: Element<Message> = if let Some(session) = session {
        let output = session.terminal.get_visible_text();
        let cursor_indicator = if focused { "▊" } else { "" };
        let display_text = format!("{}{}", output, cursor_indicator);

        let terminal_text = scrollable(
            container(text(display_text).size(13).color(palette.text_primary)).width(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill);

        button(terminal_text)
            .on_press(Message::TerminalFocus(true))
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(0)
            .style(|_theme, _status| button::Style {
                background: None,
                ..Default::default()
            })
            .into()
    } else {
        text("No terminal").color(palette.text_muted).into()
    };

    container(terminal_content)
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .padding(8)
        .style(move |_theme| container::Style {
            background: Some(bg.into()),
            border: iced::Border {
                color: border_color,
                width: if focused { 2.0 } else { 1.0 },
                radius: 0.0.into(),
            },
            ..Default::default()
        })
        .into()
}
