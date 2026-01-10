//! Action module for click-to-terminal and other interactions
//!
//! This module defines the actions that can be triggered by clicking
//! on elements in the UI (file paths, line numbers, selected text, etc.)

use std::path::PathBuf;

/// Actions that can be triggered by user interaction
#[derive(Debug, Clone)]
pub enum Action {
    /// Insert text into the active terminal (without executing)
    InsertToTerminal(String),

    /// Execute command in the active terminal (insert + enter)
    ExecuteInTerminal(String),

    /// Show diff for a file
    ShowDiff(PathBuf),

    /// Copy text to clipboard
    CopyToClipboard(String),

    /// No action
    None,
}

impl Action {
    /// Create an insert action for a file path
    pub fn insert_path(path: impl Into<String>) -> Self {
        Action::InsertToTerminal(path.into())
    }

    /// Create an insert action for file:line format
    pub fn insert_location(path: &str, line: usize) -> Self {
        Action::InsertToTerminal(format!("{}:{}", path, line))
    }

    /// Check if this action requires terminal
    pub fn needs_terminal(&self) -> bool {
        matches!(
            self,
            Action::InsertToTerminal(_) | Action::ExecuteInTerminal(_)
        )
    }
}

/// Formats for inserting paths into terminal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PathFormat {
    /// Absolute path
    #[default]
    Absolute,
    /// Relative to worktree root
    Relative,
    /// Just filename
    FileName,
    /// file:line format
    WithLine,
}

impl PathFormat {
    pub fn format_path(&self, path: &PathBuf, base: Option<&PathBuf>, line: Option<usize>) -> String {
        let path_str = match self {
            PathFormat::Absolute => path.display().to_string(),
            PathFormat::Relative => {
                if let Some(base) = base {
                    path.strip_prefix(base)
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| path.display().to_string())
                } else {
                    path.display().to_string()
                }
            }
            PathFormat::FileName => path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string()),
            PathFormat::WithLine => {
                let base_path = if let Some(base) = base {
                    path.strip_prefix(base)
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|_| path.display().to_string())
                } else {
                    path.display().to_string()
                };
                if let Some(line) = line {
                    format!("{}:{}", base_path, line)
                } else {
                    base_path
                }
            }
        };

        // Quote path if it contains spaces
        if path_str.contains(' ') {
            format!("\"{}\"", path_str)
        } else {
            path_str
        }
    }
}

/// Action queue for batching actions
#[derive(Default)]
pub struct ActionQueue {
    pending: Vec<Action>,
}

impl ActionQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Push an action to the queue
    pub fn push(&mut self, action: Action) {
        if !matches!(action, Action::None) {
            self.pending.push(action);
        }
    }

    /// Take all pending actions
    pub fn take(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.pending)
    }

    /// Check if queue has pending actions
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_creation() {
        let insert = Action::insert_path("/path/to/file");
        assert!(matches!(insert, Action::InsertToTerminal(_)));

        let location = Action::insert_location("file.rs", 42);
        if let Action::InsertToTerminal(s) = location {
            assert_eq!(s, "file.rs:42");
        } else {
            panic!("Expected InsertToTerminal");
        }
    }

    #[test]
    fn test_action_needs_terminal() {
        assert!(Action::InsertToTerminal("test".into()).needs_terminal());
        assert!(Action::ExecuteInTerminal("test".into()).needs_terminal());
        assert!(!Action::CopyToClipboard("test".into()).needs_terminal());
        assert!(!Action::ShowDiff(PathBuf::new()).needs_terminal());
    }

    #[test]
    fn test_path_format_absolute() {
        let format = PathFormat::Absolute;
        let path = PathBuf::from("/home/user/project/src/main.rs");
        let result = format.format_path(&path, None, None);
        assert_eq!(result, "/home/user/project/src/main.rs");
    }

    #[test]
    fn test_path_format_relative() {
        let format = PathFormat::Relative;
        let path = PathBuf::from("/home/user/project/src/main.rs");
        let base = PathBuf::from("/home/user/project");
        let result = format.format_path(&path, Some(&base), None);
        assert_eq!(result, "src/main.rs");
    }

    #[test]
    fn test_path_format_filename() {
        let format = PathFormat::FileName;
        let path = PathBuf::from("/home/user/project/src/main.rs");
        let result = format.format_path(&path, None, None);
        assert_eq!(result, "main.rs");
    }

    #[test]
    fn test_path_format_with_line() {
        let format = PathFormat::WithLine;
        let path = PathBuf::from("/home/user/project/src/main.rs");
        let base = PathBuf::from("/home/user/project");
        let result = format.format_path(&path, Some(&base), Some(42));
        assert_eq!(result, "src/main.rs:42");
    }

    #[test]
    fn test_path_format_with_spaces() {
        let format = PathFormat::Absolute;
        let path = PathBuf::from("/home/user/my project/src/main.rs");
        let result = format.format_path(&path, None, None);
        assert_eq!(result, "\"/home/user/my project/src/main.rs\"");
    }

    #[test]
    fn test_action_queue() {
        let mut queue = ActionQueue::new();
        assert!(!queue.has_pending());

        queue.push(Action::InsertToTerminal("test".into()));
        queue.push(Action::None); // Should be ignored
        queue.push(Action::CopyToClipboard("copy".into()));

        assert!(queue.has_pending());

        let actions = queue.take();
        assert_eq!(actions.len(), 2);
        assert!(!queue.has_pending());
    }
}
