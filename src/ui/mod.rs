//! UI module for Sashiki

mod diff_view;
mod file_tree;
mod markdown_editor;
mod sidebar;
mod terminal_view;
mod text_view;

pub use diff_view::{render_diff_stats, DiffView};
pub use file_tree::{FileListMode, FileSource, FileTree};
pub use markdown_editor::MarkdownEditor;
pub use sidebar::Sidebar;
pub use terminal_view::TerminalView;
pub use text_view::TextView;

use egui::Color32;

#[derive(Clone)]
pub struct Theme {
    pub bg_primary: Color32,
    pub bg_secondary: Color32,
    pub bg_tertiary: Color32,
    pub text_primary: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub accent: Color32,
    pub border: Color32,
    pub diff_add_bg: Color32,
    pub diff_delete_bg: Color32,
    pub diff_add_fg: Color32,
    pub diff_delete_fg: Color32,
    pub line_number: Color32,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            bg_primary: Color32::from_rgb(30, 30, 30),
            bg_secondary: Color32::from_rgb(37, 37, 38),
            bg_tertiary: Color32::from_rgb(45, 45, 46),
            text_primary: Color32::from_rgb(212, 212, 212),
            text_secondary: Color32::from_rgb(180, 180, 180),
            text_muted: Color32::from_rgb(128, 128, 128),
            accent: Color32::from_rgb(0, 122, 204),
            border: Color32::from_rgb(60, 60, 60),
            diff_add_bg: Color32::from_rgba_unmultiplied(35, 134, 54, 60),
            diff_delete_bg: Color32::from_rgba_unmultiplied(218, 54, 51, 60),
            diff_add_fg: Color32::from_rgb(87, 171, 90),
            diff_delete_fg: Color32::from_rgb(248, 81, 73),
            line_number: Color32::from_rgb(96, 96, 96),
        }
    }

    pub fn light() -> Self {
        Self {
            bg_primary: Color32::from_rgb(255, 255, 255),
            bg_secondary: Color32::from_rgb(245, 245, 245),
            bg_tertiary: Color32::from_rgb(235, 235, 235),
            text_primary: Color32::from_rgb(36, 36, 36),
            text_secondary: Color32::from_rgb(64, 64, 64),
            text_muted: Color32::from_rgb(128, 128, 128),
            accent: Color32::from_rgb(0, 102, 204),
            border: Color32::from_rgb(200, 200, 200),
            diff_add_bg: Color32::from_rgba_unmultiplied(35, 134, 54, 40),
            diff_delete_bg: Color32::from_rgba_unmultiplied(218, 54, 51, 40),
            diff_add_fg: Color32::from_rgb(22, 99, 29),
            diff_delete_fg: Color32::from_rgb(179, 29, 40),
            line_number: Color32::from_rgb(150, 150, 150),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SplitDirection {
    #[default]
    Horizontal,
    Vertical,
}

impl SplitDirection {
    pub fn toggle(&mut self) {
        *self = match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        };
    }
}
