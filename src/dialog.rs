//! Dialog components for worktree management

/// Active dialog state
#[derive(Default)]
pub enum ActiveDialog {
    #[default]
    None,
    CreateWorktree,
    Commit,
    Stash,
    /// Worktree creation in progress with step-by-step progress
    Creating {
        branch: String,
        steps: Vec<String>,
        current_step: usize,
    },
    DeleteConfirm {
        target_index: usize,
    },
    DiscardFileConfirm {
        path: std::path::PathBuf,
        change_type: crate::git::ChangeType,
    },
    DiscardAllConfirm,
    /// No staged changes; asking user whether to stage everything and commit.
    SmartCommitConfirm,
    /// Confirm before running `git reset --soft HEAD~1`.
    UndoCommitConfirm,
    /// Confirm before amending the last commit.
    AmendCommitConfirm,
    /// Confirm before discarding an unstaged hunk/line range (pending data in app state).
    DiscardHunkConfirm,
    /// Confirm before applying a stash entry.
    StashApplyConfirm { reference: String },
    /// Confirm before popping a stash entry (apply + drop).
    StashPopConfirm { reference: String },
    /// Confirm before dropping a stash entry.
    StashDropConfirm { reference: String },
    Deleting,
    /// Template settings dialog
    TemplateSettings,
    Error {
        message: String,
    },
}
