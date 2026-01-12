//! Main content area views (welcome, file, editor, diff, changed files)

use crate::app::message::Message;
use crate::diff::{to_side_by_side, DiffLineType, DiffResult};
use crate::git::FileStatus;
use crate::theme::{accent_button_style, button_style, Palette};
use iced::widget::{
    button, column, container, row, scrollable, text, text_editor, Column, Space,
};
use iced::{Element, Length};
use std::path::{Path, PathBuf};

/// View state for main content area
#[derive(Debug, Clone, Default)]
pub enum ViewState {
    #[default]
    Welcome,
    ChangedFiles,
    File { path: PathBuf, content: String },
    Editor { path: PathBuf },
    Diff { path: PathBuf },
}

/// Welcome screen
pub fn view_welcome<'a>(palette: &Palette, has_repo: bool) -> Element<'a, Message> {
    let title = text("Sashiki").size(28).color(palette.text_muted);

    let subtitle = text("Lightweight cockpit for AI agents")
        .size(14)
        .color(palette.text_muted);

    let hint = if !has_repo {
        text("Press Ctrl+O to open a repository")
            .size(12)
            .color(palette.text_secondary)
    } else {
        text("")
    };

    column![title, subtitle, hint]
        .spacing(8)
        .align_x(iced::Alignment::Center)
        .into()
}

/// Changed files list
pub fn view_changed_files<'a>(files: &[FileStatus], palette: &Palette) -> Element<'a, Message> {
    if files.is_empty() {
        return text("No changes detected")
            .size(16)
            .color(palette.text_muted)
            .into();
    }

    let header = text(format!("{} changed files", files.len()))
        .size(14)
        .color(palette.text_secondary);

    let files_list = Column::with_children(
        files
            .iter()
            .map(|file| {
                let status_char = match file.status {
                    crate::git::FileStatusType::New => "+",
                    crate::git::FileStatusType::Modified => "~",
                    crate::git::FileStatusType::Deleted => "-",
                    crate::git::FileStatusType::Renamed => "R",
                    crate::git::FileStatusType::Untracked => "?",
                };
                let color = match file.status {
                    crate::git::FileStatusType::New => palette.diff_add_fg,
                    crate::git::FileStatusType::Modified => palette.accent,
                    crate::git::FileStatusType::Deleted => palette.diff_delete_fg,
                    _ => palette.text_secondary,
                };
                let path = file.path.clone();

                row![
                    text(status_char).size(12).color(color),
                    button(
                        text(file.path.display().to_string())
                            .size(12)
                            .color(palette.text_primary)
                    )
                    .on_press(Message::ShowDiff(path))
                    .padding([2, 4])
                    .style(|_theme, _status| button::Style::default()),
                ]
                .spacing(8)
                .into()
            })
            .collect::<Vec<_>>(),
    )
    .spacing(4);

    column![
        header,
        scrollable(files_list)
            .width(Length::Fill)
            .height(Length::Fill)
    ]
    .spacing(8)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// File view (read-only)
pub fn view_file<'a>(
    path: &Path,
    content: &'a str,
    palette: &'a Palette,
) -> Element<'a, Message> {
    let path_clone = path.to_path_buf();
    let header = row![
        text(path.display().to_string())
            .size(12)
            .color(palette.text_secondary),
        Space::new().width(Length::Fill),
        button(text("Edit").size(11).color(palette.text_primary))
            .on_press(Message::EditFile(path_clone.clone()))
            .padding([2, 8])
            .style(button_style),
        button(text("→").size(11).color(palette.text_primary))
            .on_press(Message::InsertPath(path_clone))
            .padding([2, 8])
            .style(button_style),
    ]
    .spacing(8);

    let content_view = scrollable(text(content).size(13).color(palette.text_primary))
        .width(Length::Fill)
        .height(Length::Fill);

    column![header, content_view]
        .spacing(8)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Editor view
pub fn view_editor<'a>(
    path: &Path,
    content: &'a text_editor::Content,
    modified: bool,
    error_message: Option<&String>,
    palette: &'a Palette,
) -> Element<'a, Message> {
    let modified_indicator = if modified { "*" } else { "" };
    let path_clone = path.to_path_buf();

    let header = row![
        text(format!("{}{}", path.display(), modified_indicator))
            .size(12)
            .color(palette.text_secondary),
        Space::new().width(Length::Fill),
        button(text("Save").size(11))
            .on_press(Message::SaveFile)
            .padding([2, 8])
            .style(accent_button_style),
        button(text("Cancel").size(11).color(palette.text_primary))
            .on_press(Message::CancelEdit)
            .padding([2, 8])
            .style(button_style),
        button(text("→").size(11).color(palette.text_primary))
            .on_press(Message::InsertPath(path_clone))
            .padding([2, 8])
            .style(button_style),
    ]
    .spacing(8);

    let error_row: Element<Message> = if let Some(error) = error_message {
        text(error.clone())
            .size(11)
            .color(palette.diff_delete_fg)
            .into()
    } else {
        Space::new().height(0).into()
    };

    let editor = text_editor(content)
        .on_action(Message::EditorAction)
        .height(Length::Fill);

    column![header, error_row, editor].spacing(8).into()
}

/// Diff view
pub fn view_diff<'a>(
    path: &Path,
    diff_result: Option<&DiffResult>,
    palette: &'a Palette,
) -> Element<'a, Message> {
    let path_clone = path.to_path_buf();
    let insert_path = path.to_path_buf();

    let stats_text: Element<Message> = if let Some(result) = diff_result {
        text(format!(
            "+{} -{} ~{}",
            result.stats.additions, result.stats.deletions, result.stats.unchanged
        ))
        .size(11)
        .color(palette.text_muted)
        .into()
    } else {
        Space::new().width(0).into()
    };

    let header = row![
        text(format!("Diff: {}", path.display()))
            .size(12)
            .color(palette.text_secondary),
        Space::new().width(Length::Fill),
        stats_text,
        button(text("Edit").size(11).color(palette.text_primary))
            .on_press(Message::EditFile(path_clone.clone()))
            .padding([2, 8])
            .style(button_style),
        button(text("→").size(11).color(palette.text_primary))
            .on_press(Message::InsertPath(insert_path))
            .padding([2, 8])
            .style(button_style),
    ]
    .spacing(8);

    let diff_content: Element<Message> = if let Some(result) = diff_result {
        let side_by_side = to_side_by_side(result);

        let left_lines = Column::with_children(
            side_by_side
                .iter()
                .map(|line| {
                    let line_num = line
                        .left_line_num
                        .map(|n| format!("{:4}", n))
                        .unwrap_or_else(|| "    ".to_string());
                    let content = line
                        .left_content
                        .as_ref()
                        .map(|s| s.trim_end().to_string())
                        .unwrap_or_default();
                    let (bg_color, fg_color) = match line.left_type {
                        DiffLineType::Delete => (palette.diff_delete_bg, palette.diff_delete_fg),
                        _ => (palette.bg_primary, palette.text_primary),
                    };
                    container(
                        row![
                            text(line_num).size(11).color(palette.text_muted),
                            text(content).size(12).color(fg_color),
                        ]
                        .spacing(8),
                    )
                    .width(Length::Fill)
                    .style(move |_theme| container::Style {
                        background: Some(bg_color.into()),
                        ..Default::default()
                    })
                    .into()
                })
                .collect::<Vec<_>>(),
        )
        .spacing(0);

        let right_lines = Column::with_children(
            side_by_side
                .iter()
                .map(|line| {
                    let line_num = line
                        .right_line_num
                        .map(|n| format!("{:4}", n))
                        .unwrap_or_else(|| "    ".to_string());
                    let content = line
                        .right_content
                        .as_ref()
                        .map(|s| s.trim_end().to_string())
                        .unwrap_or_default();
                    let (bg_color, fg_color) = match line.right_type {
                        DiffLineType::Insert => (palette.diff_add_bg, palette.diff_add_fg),
                        _ => (palette.bg_primary, palette.text_primary),
                    };
                    container(
                        row![
                            text(line_num).size(11).color(palette.text_muted),
                            text(content).size(12).color(fg_color),
                        ]
                        .spacing(8),
                    )
                    .width(Length::Fill)
                    .style(move |_theme| container::Style {
                        background: Some(bg_color.into()),
                        ..Default::default()
                    })
                    .into()
                })
                .collect::<Vec<_>>(),
        )
        .spacing(0);

        let bg_secondary = palette.bg_secondary;
        let border_color = palette.border;

        let left_pane = container(scrollable(left_lines).height(Length::Fill))
            .width(Length::FillPortion(1))
            .style(move |_theme| container::Style {
                background: Some(bg_secondary.into()),
                border: iced::Border {
                    color: border_color,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            });

        let right_pane = container(scrollable(right_lines).height(Length::Fill))
            .width(Length::FillPortion(1))
            .style(move |_theme| container::Style {
                background: Some(bg_secondary.into()),
                border: iced::Border {
                    color: border_color,
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            });

        row![left_pane, right_pane]
            .spacing(2)
            .height(Length::Fill)
            .into()
    } else {
        text("No diff available")
            .color(palette.text_muted)
            .into()
    };

    column![header, diff_content].spacing(8).into()
}
