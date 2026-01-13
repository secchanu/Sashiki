//! View components for the application

mod dialogs;
mod file_list;
mod main_content;
mod sidebar;
mod status_bar;
mod terminal;
mod terminal_widget;

pub use dialogs::{view_delete_confirm_dialog, view_open_dialog, view_worktree_dialog};
pub use main_content::{view_changed_files, view_diff, view_editor, view_file, view_welcome, ViewState};
pub use sidebar::{view_sidebar, FileListMode, FileSource, SidebarState};
pub use status_bar::view_status_bar;
pub use terminal::view_terminal;
