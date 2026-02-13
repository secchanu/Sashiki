//! UI components

pub mod dialogs;
pub mod file_list;
pub mod file_tree;
pub mod file_view;
pub mod render;
pub mod sidebar;
pub mod terminal;

pub use file_tree::{ChangeInfo, FileListMode, FileTreeNode, read_dir_shallow};
pub use file_view::{FileView, SendToTerminalEvent};

use crate::theme::*;
use gpui::{IntoElement, ParentElement, Styled, div, rgb};

/// Renders the "main" badge for main worktree indicator
pub fn render_main_badge() -> impl IntoElement {
    div()
        .px_1()
        .bg(rgb(GREEN))
        .text_color(rgb(BG_BASE))
        .text_xs()
        .rounded_sm()
        .child("main")
}

/// Renders the "locked" badge for locked worktree indicator
pub fn render_locked_badge() -> impl IntoElement {
    div()
        .px_1()
        .bg(rgb(YELLOW))
        .text_color(rgb(BG_BASE))
        .text_xs()
        .rounded_sm()
        .child("locked")
}
