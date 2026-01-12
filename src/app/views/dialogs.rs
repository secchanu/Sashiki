//! Dialog views

use crate::app::message::Message;
use crate::theme::Palette;
use iced::widget::{button, column, container, row, text, text_input};
use iced::{Element, Length};

/// Open repository dialog
pub fn view_open_dialog<'a>(
    base: Element<'a, Message>,
    dialog_path: &str,
    error_message: Option<&String>,
    palette: &Palette,
) -> Element<'a, Message> {
    let bg = palette.bg_secondary;
    let border_color = palette.border;
    let text_primary = palette.text_primary;
    let error_color = palette.diff_delete_fg;

    let dialog = container(
        column![
            text("Open Repository").size(16).color(text_primary),
            text_input("Path to repository...", dialog_path)
                .on_input(Message::DialogPathChanged)
                .on_submit(Message::OpenRepository)
                .padding(8)
                .width(Length::Fixed(400.0)),
            if let Some(error) = error_message {
                text(error.clone()).size(11).color(error_color)
            } else {
                text("")
            },
            row![
                button("Open")
                    .on_press(Message::OpenRepository)
                    .padding([4, 12]),
                button("Cancel")
                    .on_press(Message::CloseDialog)
                    .padding([4, 12]),
            ]
            .spacing(8)
        ]
        .spacing(12)
        .padding(16),
    )
    .style(move |_theme| container::Style {
        background: Some(bg.into()),
        border: iced::Border {
            color: border_color,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    });

    iced::widget::stack![base, container(dialog).center(Length::Fill)].into()
}

/// Create worktree dialog
pub fn view_worktree_dialog<'a>(
    base: Element<'a, Message>,
    worktree_branch: &str,
    error_message: Option<&String>,
    palette: &Palette,
) -> Element<'a, Message> {
    let bg = palette.bg_secondary;
    let border_color = palette.border;
    let text_primary = palette.text_primary;
    let text_muted = palette.text_muted;
    let error_color = palette.diff_delete_fg;

    let dialog = container(
        column![
            text("Create Worktree").size(16).color(text_primary),
            text("Enter branch name for the new worktree.")
                .size(11)
                .color(text_muted),
            text_input("Branch name...", worktree_branch)
                .on_input(Message::WorktreeBranchChanged)
                .on_submit(Message::CreateWorktree)
                .padding(8)
                .width(Length::Fixed(300.0)),
            if let Some(error) = error_message {
                text(error.clone()).size(11).color(error_color)
            } else {
                text("")
            },
            row![
                button("Create")
                    .on_press(Message::CreateWorktree)
                    .padding([4, 12]),
                button("Cancel")
                    .on_press(Message::CloseWorktreeDialog)
                    .padding([4, 12]),
            ]
            .spacing(8)
        ]
        .spacing(12)
        .padding(16),
    )
    .style(move |_theme| container::Style {
        background: Some(bg.into()),
        border: iced::Border {
            color: border_color,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    });

    iced::widget::stack![base, container(dialog).center(Length::Fill)].into()
}

/// Delete confirmation dialog
pub fn view_delete_confirm_dialog<'a>(
    base: Element<'a, Message>,
    worktree_name: &str,
    error_message: Option<&String>,
    palette: &Palette,
) -> Element<'a, Message> {
    let bg = palette.bg_secondary;
    let border_color = palette.border;
    let text_primary = palette.text_primary;
    let text_secondary = palette.text_secondary;
    let error_color = palette.diff_delete_fg;

    let mut content = column![
        text("セッションを削除")
            .size(16)
            .color(text_primary),
        text(format!(
            "セッション「{}」を削除しますか？\nこの操作は取り消せません。",
            worktree_name
        ))
        .size(12)
        .color(text_secondary),
    ]
    .spacing(12);

    if let Some(error) = error_message {
        content = content.push(text(error.clone()).size(11).color(error_color));
    }

    let delete_color = palette.diff_delete_fg;
    content = content.push(
        row![
            button(text("削除").color(delete_color))
                .on_press(Message::ConfirmDelete)
                .padding([4, 12])
                .style(move |_theme, _status| button::Style {
                    background: Some(delete_color.scale_alpha(0.2).into()),
                    ..Default::default()
                }),
            button("キャンセル")
                .on_press(Message::CancelDelete)
                .padding([4, 12]),
        ]
        .spacing(8),
    );

    let dialog = container(content.padding(16)).style(move |_theme| container::Style {
        background: Some(bg.into()),
        border: iced::Border {
            color: border_color,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..Default::default()
    });

    iced::widget::stack![base, container(dialog).center(Length::Fill)].into()
}
