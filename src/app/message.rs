//! Application messages

use iced::keyboard::{Key, Modifiers};
use iced::widget::text_editor;
use iced::Size;
use std::path::PathBuf;

/// Application messages
#[derive(Debug, Clone)]
pub enum Message {
    // Session management
    SelectSession(usize),

    // Terminal
    ToggleTerminal,
    TerminalFocus(bool),
    TerminalTick,

    // Repository
    OpenRepositoryDialog,
    CloseDialog,
    DialogPathChanged(String),
    OpenRepository,

    // Worktree
    ShowWorktreeDialog,
    CloseWorktreeDialog,
    WorktreeBranchChanged(String),
    CreateWorktree,
    ShowDeleteConfirm(String),
    ConfirmDelete,
    CancelDelete,

    // File operations
    ShowDiff(PathBuf),
    OpenFile(PathBuf),
    EditFile(PathBuf),
    EditorAction(text_editor::Action),
    SaveFile,
    CancelEdit,
    InsertPath(PathBuf),

    // File list
    ToggleFileSource,
    ToggleFileListMode,
    ToggleDir(PathBuf),

    // Keyboard
    KeyPressed(Key, Modifiers),

    // Window
    WindowResized(Size),
}
