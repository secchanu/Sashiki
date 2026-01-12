//! Status bar view component

use crate::app::message::Message;
use crate::session::SessionManager;
use crate::theme::Palette;
use iced::widget::{container, text, Row};
use iced::{Element, Length};

/// Render the status bar
pub fn view_status_bar<'a>(sessions: &SessionManager, palette: &Palette) -> Element<'a, Message> {
    let statuses: Vec<_> = sessions
        .sessions()
        .iter()
        .map(|s| (s.display_name().to_string(), s.status))
        .collect();

    let active_idx = sessions.active_index();
    let bg = palette.bg_tertiary;

    let status_items = Row::with_children(
        statuses
            .iter()
            .enumerate()
            .map(|(idx, (name, status))| {
                let is_active = idx == active_idx;
                let color = if is_active {
                    palette.accent
                } else {
                    palette.text_muted
                };
                text(format!("{} {}", status.symbol(), name))
                    .size(11)
                    .color(color)
                    .into()
            })
            .collect::<Vec<_>>(),
    )
    .spacing(16);

    container(status_items)
        .width(Length::Fill)
        .height(Length::Fixed(28.0))
        .padding([4, 8])
        .style(move |_theme| container::Style {
            background: Some(bg.into()),
            ..Default::default()
        })
        .into()
}
