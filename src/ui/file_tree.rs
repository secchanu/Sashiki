//! File tree types and utilities for file list display

use crate::git::ChangeType;
use std::path::{Path, PathBuf};

/// File list display mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileListMode {
    #[default]
    Changes,
    AllFiles,
}

/// Git change information for a file
#[derive(Debug, Clone, Copy)]
pub struct ChangeInfo {
    pub change_type: ChangeType,
    /// Whether the change is staged (for future use in staging UI)
    #[allow(dead_code)]
    pub staged: bool,
}

/// File tree node for tree view
#[derive(Debug, Clone)]
pub struct FileTreeNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub children: Vec<FileTreeNode>,
    pub change_info: Option<ChangeInfo>,
}

impl FileTreeNode {
    /// Create an empty root node
    pub fn new_root() -> Self {
        Self {
            name: String::new(),
            path: PathBuf::new(),
            is_dir: true,
            children: Vec::new(),
            change_info: None,
        }
    }

    /// Build file tree from a list of file paths with change info
    pub fn from_files(files: impl IntoIterator<Item = (PathBuf, Option<ChangeInfo>)>) -> Self {
        let mut root = Self::new_root();
        for (path, change_info) in files {
            root.insert(&path, change_info);
        }
        root.sort();
        root
    }

    /// Insert a path into the tree
    pub fn insert(&mut self, path: &Path, change_info: Option<ChangeInfo>) {
        self.insert_with_full_path(path, path, change_info);
    }

    /// Internal helper that tracks the full path while recursing
    fn insert_with_full_path(&mut self, full_path: &Path, remaining_path: &Path, change_info: Option<ChangeInfo>) {
        let components: Vec<_> = remaining_path.components().collect();

        if components.is_empty() {
            return;
        }

        let first = components[0].as_os_str().to_string_lossy().to_string();
        let remaining: PathBuf = components[1..].iter().collect();

        let child_idx = self.children.iter().position(|c| c.name == first);

        if remaining.as_os_str().is_empty() {
            // Leaf node (file) - use full_path for correct path
            if let Some(idx) = child_idx {
                self.children[idx].change_info = change_info;
            } else {
                self.children.push(FileTreeNode {
                    name: first,
                    path: full_path.to_path_buf(),
                    is_dir: false,
                    children: Vec::new(),
                    change_info,
                });
            }
        } else {
            // Directory node
            let child = if let Some(idx) = child_idx {
                &mut self.children[idx]
            } else {
                let dir_path: PathBuf = full_path
                    .components()
                    .take(full_path.components().count() - remaining.components().count())
                    .collect();
                self.children.push(FileTreeNode {
                    name: first,
                    path: dir_path,
                    is_dir: true,
                    children: Vec::new(),
                    change_info: None,
                });
                self.children.last_mut().unwrap()
            };

            child.insert_with_full_path(full_path, &remaining, change_info);
        }
    }

    /// Sort the tree: directories first, then by name
    pub fn sort(&mut self) {
        self.children.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });

        for child in &mut self.children {
            child.sort();
        }
    }
}

/// Read only immediate children of a directory (for lazy loading tree view)
pub fn read_dir_shallow(path: &Path) -> std::io::Result<Vec<(PathBuf, bool)>> {
    let mut result = Vec::new();

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();

        // Skip hidden files/directories
        if let Some(name) = entry_path.file_name()
            && name.to_string_lossy().starts_with('.') {
                continue;
            }

        let is_dir = entry_path.is_dir();
        result.push((entry_path, is_dir));
    }

    // Sort: directories first, then by name
    result.sort_by(|a, b| match (a.1, b.1) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.0.cmp(&b.0),
    });

    Ok(result)
}
