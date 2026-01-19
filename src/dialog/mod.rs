//! Dialog components for worktree management

/// Active dialog state
#[derive(Default)]
pub enum ActiveDialog {
    #[default]
    None,
    CreateWorktree,
    DeleteConfirm {
        target_index: usize,
    },
    Deleting,
    Error {
        message: String,
    },
}
