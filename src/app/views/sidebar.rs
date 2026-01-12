//! Sidebar view component

use super::file_list::{view_all_files, view_git_files};
use crate::app::message::Message;
use crate::git::FileStatus;
use crate::session::{SessionManager, SessionStatus};
use crate::theme::{button_style, Palette};
use iced::widget::{button, column, container, row, text, Space};
use iced::{Color, Element, Length};
use std::collections::HashSet;
use std::path::PathBuf;

/// Source of files to display in sidebar
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileSource {
    #[default]
    Git,
    All,
}

impl FileSource {
    pub fn toggle(&mut self) {
        *self = match self {
            FileSource::Git => FileSource::All,
            FileSource::All => FileSource::Git,
        };
    }

    pub fn label(&self) -> &'static str {
        match self {
            FileSource::Git => "Git",
            FileSource::All => "Files",
        }
    }
}

/// View mode for file list
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileListMode {
    #[default]
    Flat,
    Tree,
}

impl FileListMode {
    pub fn toggle(&mut self) {
        *self = match self {
            FileListMode::Flat => FileListMode::Tree,
            FileListMode::Tree => FileListMode::Flat,
        };
    }

    pub fn icon(&self) -> &'static str {
        match self {
            FileListMode::Flat => "≡",
            FileListMode::Tree => "⊞",
        }
    }
}

/// Sidebar state for rendering
pub struct SidebarState<'a> {
    pub sessions: &'a SessionManager,
    pub file_source: FileSource,
    pub file_list_mode: FileListMode,
    pub git_files: &'a [FileStatus],
    pub all_files: &'a [PathBuf],
    pub expanded_dirs: &'a HashSet<PathBuf>,
    pub palette: &'a Palette,
    pub width: f32,
}

/// Render the sidebar
pub fn view_sidebar(state: SidebarState<'_>) -> Element<'_, Message> {
    let mut sidebar = column![]
        .spacing(4)
        .padding(8)
        .width(Length::Fixed(state.width));

    // Sessions header
    let header = row![
        text("Sessions")
            .size(12)
            .color(state.palette.text_secondary),
        Space::new().width(Length::Fill),
        button("+")
            .on_press(Message::ShowWorktreeDialog)
            .padding([2, 6])
    ];
    sidebar = sidebar.push(header);

    // Session list
    let active_idx = state.sessions.active_index();
    for (idx, session) in state.sessions.sessions().iter().enumerate() {
        let is_active = idx == active_idx;
        let color = if is_active {
            state.palette.text_primary
        } else {
            state.palette.text_secondary
        };

        let status_color = match session.status {
            SessionStatus::Idle => state.palette.text_muted,
            SessionStatus::Running => state.palette.accent,
            SessionStatus::Completed => state.palette.diff_add_fg,
            SessionStatus::Error => state.palette.diff_delete_fg,
        };

        let session_name = session.display_name().to_string();
        let worktree_name = session.worktree.name.clone();
        let is_main = session.worktree.is_main;

        let session_label = row![
            text(session.status.symbol())
                .size(12)
                .color(status_color),
            text(session_name).size(12).color(color),
        ]
        .spacing(4);

        let select_btn = button(session_label)
            .on_press(Message::SelectSession(idx))
            .width(Length::Fill)
            .padding([4, 8])
            .style(move |_theme, _status| button::Style {
                background: if is_active {
                    Some(Color::from_rgba8(0x32, 0x32, 0x4a, 0.5).into())
                } else {
                    None
                },
                text_color: color,
                ..Default::default()
            });

        let delete_color = state.palette.diff_delete_fg;
        let session_row: Element<Message> = if !is_main {
            row![
                select_btn,
                button(text("x").size(10).color(state.palette.text_muted))
                    .on_press(Message::ShowDeleteConfirm(worktree_name))
                    .padding([4, 6])
                    .style(move |_theme, _status| button::Style {
                        text_color: delete_color,
                        ..Default::default()
                    })
            ]
            .spacing(2)
            .into()
        } else {
            select_btn.into()
        };

        sidebar = sidebar.push(session_row);
    }

    // Files header with toggle buttons
    let files_header = row![
        button(
            text(state.file_source.label())
                .size(10)
                .color(state.palette.text_primary)
        )
        .on_press(Message::ToggleFileSource)
        .padding([2, 6])
        .style(button_style),
        button(
            text(state.file_list_mode.icon())
                .size(10)
                .color(state.palette.text_primary)
        )
        .on_press(Message::ToggleFileListMode)
        .padding([2, 6])
        .style(button_style),
    ]
    .spacing(4);
    sidebar = sidebar.push(files_header);

    // File list based on source and mode
    let is_tree = state.file_list_mode == FileListMode::Tree;
    let files_list = match state.file_source {
        FileSource::Git => view_git_files(
            &state.git_files,
            is_tree,
            state.expanded_dirs,
            state.palette,
        ),
        FileSource::All => view_all_files(
            &state.all_files,
            is_tree,
            state.expanded_dirs,
            state.palette,
        ),
    };
    sidebar = sidebar.push(files_list);

    let bg = state.palette.bg_secondary;
    container(sidebar)
        .style(move |_theme| container::Style {
            background: Some(bg.into()),
            ..Default::default()
        })
        .height(Length::Fill)
        .into()
}
