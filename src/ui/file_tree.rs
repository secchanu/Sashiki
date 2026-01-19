//! File tree types and utilities for file list display

use crate::git::ChangeType;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};

/// Compare two items with directory-first ordering, then by name
pub fn dir_first_cmp<T: Ord>(is_dir_a: bool, is_dir_b: bool, name_a: &T, name_b: &T) -> Ordering {
    match (is_dir_a, is_dir_b) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => name_a.cmp(name_b),
    }
}

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
    fn insert_with_full_path(
        &mut self,
        full_path: &Path,
        remaining_path: &Path,
        change_info: Option<ChangeInfo>,
    ) {
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
                self.children.last_mut().expect("just pushed an element")
            };

            child.insert_with_full_path(full_path, &remaining, change_info);
        }
    }

    /// Sort the tree: directories first, then by name
    pub fn sort(&mut self) {
        self.children
            .sort_by(|a, b| dir_first_cmp(a.is_dir, b.is_dir, &a.name, &b.name));
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
            && name.to_string_lossy().starts_with('.')
        {
            continue;
        }

        let is_dir = entry_path.is_dir();
        result.push((entry_path, is_dir));
    }

    result.sort_by(|a, b| dir_first_cmp(a.1, b.1, &a.0, &b.0));
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dir_first_cmp_both_dirs() {
        assert_eq!(dir_first_cmp(true, true, &"a", &"b"), Ordering::Less);
        assert_eq!(dir_first_cmp(true, true, &"b", &"a"), Ordering::Greater);
        assert_eq!(dir_first_cmp(true, true, &"a", &"a"), Ordering::Equal);
    }

    #[test]
    fn test_dir_first_cmp_both_files() {
        assert_eq!(dir_first_cmp(false, false, &"a", &"b"), Ordering::Less);
        assert_eq!(dir_first_cmp(false, false, &"b", &"a"), Ordering::Greater);
        assert_eq!(dir_first_cmp(false, false, &"a", &"a"), Ordering::Equal);
    }

    #[test]
    fn test_dir_first_cmp_dir_before_file() {
        // Directory should come before file regardless of name
        assert_eq!(dir_first_cmp(true, false, &"z", &"a"), Ordering::Less);
        assert_eq!(dir_first_cmp(false, true, &"a", &"z"), Ordering::Greater);
    }

    #[test]
    fn test_file_tree_node_from_empty() {
        let tree = FileTreeNode::from_files(std::iter::empty());
        assert!(tree.children.is_empty());
        assert!(tree.is_dir);
    }

    #[test]
    fn test_file_tree_node_single_file() {
        let files = vec![(PathBuf::from("test.txt"), None)];
        let tree = FileTreeNode::from_files(files);

        assert_eq!(tree.children.len(), 1);
        assert_eq!(tree.children[0].name, "test.txt");
        assert_eq!(tree.children[0].path, PathBuf::from("test.txt"));
        assert!(!tree.children[0].is_dir);
    }

    #[test]
    fn test_file_tree_node_nested_file() {
        let files = vec![(PathBuf::from("src/main.rs"), None)];
        let tree = FileTreeNode::from_files(files);

        assert_eq!(tree.children.len(), 1);
        let src = &tree.children[0];
        assert_eq!(src.name, "src");
        assert!(src.is_dir);

        assert_eq!(src.children.len(), 1);
        let main_rs = &src.children[0];
        assert_eq!(main_rs.name, "main.rs");
        assert_eq!(main_rs.path, PathBuf::from("src/main.rs"));
        assert!(!main_rs.is_dir);
    }

    #[test]
    fn test_file_tree_node_multiple_files_sorted() {
        let files = vec![
            (PathBuf::from("z.txt"), None),
            (PathBuf::from("a.txt"), None),
            (PathBuf::from("dir/file.txt"), None),
        ];
        let tree = FileTreeNode::from_files(files);

        // Directory should come first, then files sorted alphabetically
        assert_eq!(tree.children.len(), 3);
        assert_eq!(tree.children[0].name, "dir"); // dir first
        assert!(tree.children[0].is_dir);
        assert_eq!(tree.children[1].name, "a.txt"); // then a.txt
        assert_eq!(tree.children[2].name, "z.txt"); // then z.txt
    }

    #[test]
    fn test_file_tree_node_with_change_info() {
        let change_info = ChangeInfo {
            change_type: ChangeType::Modified,
            staged: false,
        };
        let files = vec![(PathBuf::from("modified.txt"), Some(change_info))];
        let tree = FileTreeNode::from_files(files);

        assert_eq!(tree.children.len(), 1);
        let file = &tree.children[0];
        assert!(file.change_info.is_some());
        let info = file.change_info.unwrap();
        assert_eq!(info.change_type, ChangeType::Modified);
        assert!(!info.staged);
    }

    #[test]
    fn test_file_tree_node_deep_nesting() {
        let files = vec![(PathBuf::from("a/b/c/d/file.txt"), None)];
        let tree = FileTreeNode::from_files(files);

        let a = &tree.children[0];
        assert_eq!(a.name, "a");
        assert!(a.is_dir);

        let b = &a.children[0];
        assert_eq!(b.name, "b");
        assert!(b.is_dir);

        let c = &b.children[0];
        assert_eq!(c.name, "c");
        assert!(c.is_dir);

        let d = &c.children[0];
        assert_eq!(d.name, "d");
        assert!(d.is_dir);

        let file = &d.children[0];
        assert_eq!(file.name, "file.txt");
        assert!(!file.is_dir);
        assert_eq!(file.path, PathBuf::from("a/b/c/d/file.txt"));
    }
}
