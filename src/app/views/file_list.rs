//! File list views (flat and tree modes)

use crate::app::message::Message;
use crate::git::{FileStatus, FileStatusType};
use crate::theme::Palette;
use iced::widget::{button, mouse_area, row, scrollable, text, Column};
use iced::{Color, Element, Length};
use std::collections::{BTreeSet, HashSet};
use std::path::PathBuf;

/// File item for display in the list
pub struct FileItem {
    pub path: PathBuf,
    pub color: Color,
    pub on_click: Message,
    pub on_insert: Message,
}

/// Build file items for git changed files
fn file_items_from_git(files: &[FileStatus], palette: &Palette) -> Vec<FileItem> {
    files
        .iter()
        .map(|file| {
            let color = match file.status {
                FileStatusType::New => palette.diff_add_fg,
                FileStatusType::Modified => palette.accent,
                FileStatusType::Deleted => palette.diff_delete_fg,
                _ => palette.text_secondary,
            };
            FileItem {
                path: file.path.clone(),
                color,
                on_click: Message::ShowDiff(file.path.clone()),
                on_insert: Message::InsertPath(file.path.clone()),
            }
        })
        .collect()
}

/// Build file items for all files
fn file_items_from_paths(files: &[PathBuf], palette: &Palette) -> Vec<FileItem> {
    files
        .iter()
        .map(|path| FileItem {
            path: path.clone(),
            color: palette.text_secondary,
            on_click: Message::OpenFile(path.clone()),
            on_insert: Message::InsertPath(path.clone()),
        })
        .collect()
}

/// Render a single file item row
fn view_file_item<'a>(item: &FileItem, palette: &Palette) -> Element<'a, Message> {
    let path_display = item.path.display().to_string();
    let on_click = item.on_click.clone();
    let on_insert = item.on_insert.clone();
    let on_right_click = item.on_insert.clone();
    let color = item.color;
    let muted = palette.text_muted;

    mouse_area(
        row![
            button(text(path_display).size(11).color(color))
                .on_press(on_click)
                .width(Length::Fill)
                .padding([2, 4])
                .style(|_theme, _status| button::Style::default()),
            button(text("→").size(9).color(muted))
                .on_press(on_insert)
                .padding([2, 4])
                .style(|_theme, _status| button::Style::default()),
        ]
        .spacing(2),
    )
    .on_right_press(on_right_click)
    .into()
}

/// Render file list in flat mode
fn view_flat<'a>(items: &[FileItem], palette: &Palette) -> Element<'a, Message> {
    scrollable(
        Column::with_children(
            items
                .iter()
                .map(|item| view_file_item(item, palette))
                .collect::<Vec<_>>(),
        )
        .spacing(2)
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// Build tree items recursively
fn build_tree_items<'a>(
    items: &mut Vec<Element<'a, Message>>,
    file_items: &[FileItem],
    current_dir: &PathBuf,
    depth: usize,
    expanded_dirs: &HashSet<PathBuf>,
    palette: &Palette,
) {
    let mut dirs_at_level: BTreeSet<PathBuf> = BTreeSet::new();
    let mut files_at_level: Vec<&FileItem> = Vec::new();

    for item in file_items {
        let relative = if current_dir.as_os_str().is_empty() {
            Some(item.path.clone())
        } else {
            item.path
                .strip_prefix(current_dir)
                .ok()
                .map(|p| p.to_path_buf())
        };

        if let Some(rel) = relative {
            let components: Vec<_> = rel.components().collect();
            if components.len() == 1 {
                files_at_level.push(item);
            } else if !components.is_empty() {
                if let Some(first) = components.first() {
                    let subdir = if current_dir.as_os_str().is_empty() {
                        PathBuf::from(first.as_os_str())
                    } else {
                        current_dir.join(first.as_os_str())
                    };
                    dirs_at_level.insert(subdir);
                }
            }
        }
    }

    let indent_str = "  ".repeat(depth);

    // Render directories first
    for dir in &dirs_at_level {
        let is_expanded = expanded_dirs.contains(dir);
        let icon = if is_expanded { "▼" } else { "▶" };
        let dir_name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let dir_path = dir.clone();

        items.push(
            button(
                row![
                    text(format!("{}{}", indent_str, icon))
                        .size(10)
                        .color(palette.text_muted),
                    text(dir_name).size(11).color(palette.text_secondary),
                ]
                .spacing(4),
            )
            .on_press(Message::ToggleDir(dir_path.clone()))
            .width(Length::Fill)
            .padding([2, 4])
            .style(|_theme, _status| button::Style::default())
            .into(),
        );

        if is_expanded {
            build_tree_items(items, file_items, &dir_path, depth + 1, expanded_dirs, palette);
        }
    }

    // Render files
    for item in files_at_level {
        let file_name = item
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let label = format!("{}  {}", indent_str, file_name);
        let on_click = item.on_click.clone();
        let on_insert = item.on_insert.clone();
        let on_right_click = item.on_insert.clone();
        let color = item.color;
        let muted = palette.text_muted;

        items.push(
            mouse_area(
                row![
                    button(text(label).size(11).color(color))
                        .on_press(on_click)
                        .width(Length::Fill)
                        .padding([2, 4])
                        .style(|_theme, _status| button::Style::default()),
                    button(text("→").size(9).color(muted))
                        .on_press(on_insert)
                        .padding([2, 4])
                        .style(|_theme, _status| button::Style::default()),
                ]
                .spacing(2),
            )
            .on_right_press(on_right_click)
            .into(),
        );
    }
}

/// Render file list in tree mode
fn view_tree<'a>(
    items: &[FileItem],
    expanded_dirs: &HashSet<PathBuf>,
    palette: &Palette,
) -> Element<'a, Message> {
    let mut elements: Vec<Element<Message>> = Vec::new();
    build_tree_items(
        &mut elements,
        items,
        &PathBuf::new(),
        0,
        expanded_dirs,
        palette,
    );

    scrollable(Column::with_children(elements).spacing(2).width(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// View git changed files
pub fn view_git_files<'a>(
    files: &[FileStatus],
    tree_mode: bool,
    expanded_dirs: &HashSet<PathBuf>,
    palette: &Palette,
) -> Element<'a, Message> {
    let items = file_items_from_git(files, palette);

    if tree_mode {
        view_tree(&items, expanded_dirs, palette)
    } else {
        view_flat(&items, palette)
    }
}

/// View all files
pub fn view_all_files<'a>(
    files: &[PathBuf],
    tree_mode: bool,
    expanded_dirs: &HashSet<PathBuf>,
    palette: &Palette,
) -> Element<'a, Message> {
    let items = file_items_from_paths(files, palette);

    if tree_mode {
        view_tree(&items, expanded_dirs, palette)
    } else {
        view_flat(&items, palette)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_items_from_git() {
        let palette = Palette::dark();
        let files = vec![
            FileStatus {
                path: PathBuf::from("new.rs"),
                status: FileStatusType::New,
            },
            FileStatus {
                path: PathBuf::from("mod.rs"),
                status: FileStatusType::Modified,
            },
        ];

        let items = file_items_from_git(&files, &palette);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].color, palette.diff_add_fg);
        assert_eq!(items[1].color, palette.accent);
    }

    #[test]
    fn test_file_items_from_paths() {
        let palette = Palette::dark();
        let files = vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")];

        let items = file_items_from_paths(&files, &palette);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].color, palette.text_secondary);
    }
}
