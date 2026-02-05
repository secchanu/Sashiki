//! Dialog components for worktree management

/// Active dialog state
#[derive(Default)]
pub enum ActiveDialog {
    #[default]
    None,
    CreateWorktree,
    /// Worktree creation in progress with step-by-step progress
    Creating {
        branch: String,
        steps: Vec<String>,
        current_step: usize,
    },
    DeleteConfirm {
        target_index: usize,
    },
    Deleting,
    /// Template settings dialog
    TemplateSettings,
    Error {
        message: String,
    },
}
