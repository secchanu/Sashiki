//! File tree component for displaying files
//!
//! Supports Git view (changed files) and File view (all files).
//! Both views support flat list and tree display modes.

use crate::git::{FileStatus, FileStatusType};
use crate::ui::Theme;
use egui::{self, Ui};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Source of files to display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileSource {
    /// Show only git changed files
    #[default]
    Git,
    /// Show all files in the worktree
    All,
}

impl FileSource {
    pub fn toggle(&mut self) {
        *self = match self {
            FileSource::Git => FileSource::All,
            FileSource::All => FileSource::Git,
        };
    }

    pub fn label(&self) -> &'static str {
        match self {
            FileSource::Git => "Git",
            FileSource::All => "Files",
        }
    }
}

/// View mode for file list
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileListMode {
    /// Flat list of files
    #[default]
    Flat,
    /// Tree view grouped by directory
    Tree,
}

impl FileListMode {
    pub fn toggle(&mut self) {
        *self = match self {
            FileListMode::Flat => FileListMode::Tree,
            FileListMode::Tree => FileListMode::Flat,
        };
    }

    pub fn icon(&self) -> &'static str {
        match self {
            FileListMode::Flat => "≡",
            FileListMode::Tree => "⊞",
        }
    }
}

/// A file entry (either from git status or filesystem)
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub status: Option<FileStatusType>,
    pub is_directory: bool,
}

impl FileEntry {
    pub fn from_git_status(status: &FileStatus) -> Self {
        Self {
            path: status.path.clone(),
            status: Some(status.status),
            is_directory: false,
        }
    }

    pub fn is_markdown(&self) -> bool {
        self.path
            .extension()
            .map(|ext| ext.eq_ignore_ascii_case("md"))
            .unwrap_or(false)
    }
}

/// File tree state
pub struct FileTree {
    /// Source of files (Git or All)
    pub source: FileSource,
    /// Display mode (Flat or Tree)
    pub mode: FileListMode,
    /// Expanded directories (for tree mode)
    expanded_dirs: HashSet<PathBuf>,
    /// Cached file entries for filesystem view
    cached_entries: Vec<FileEntry>,
    /// Path for which entries are cached
    cached_path: Option<PathBuf>,
}

impl FileTree {
    pub fn new() -> Self {
        Self {
            source: FileSource::Git,
            mode: FileListMode::Tree,
            expanded_dirs: HashSet::new(),
            cached_entries: Vec::new(),
            cached_path: None,
        }
    }

    /// Toggle a directory's expanded state
    pub fn toggle_dir(&mut self, dir: &Path) {
        if self.expanded_dirs.contains(dir) {
            self.expanded_dirs.remove(dir);
        } else {
            self.expanded_dirs.insert(dir.to_path_buf());
        }
    }

    /// Check if a directory is expanded
    pub fn is_expanded(&self, dir: &Path) -> bool {
        self.expanded_dirs.contains(dir)
    }

    /// Clear cached entries
    pub fn invalidate_cache(&mut self) {
        self.cached_entries.clear();
        self.cached_path = None;
    }

    /// Load all files from a directory
    pub fn load_files(&mut self, root: &Path, git_files: &[FileStatus]) {
        if self.cached_path.as_deref() == Some(root) && !self.cached_entries.is_empty() {
            return;
        }

        self.cached_entries.clear();

        // Create a set of git-tracked changed files for status lookup
        let git_status_map: HashMap<PathBuf, FileStatusType> = git_files
            .iter()
            .map(|f| (f.path.clone(), f.status))
            .collect();

        // Walk the directory tree
        self.walk_directory(root, root, &git_status_map);

        self.cached_path = Some(root.to_path_buf());
    }

    fn walk_directory(
        &mut self,
        root: &Path,
        dir: &Path,
        git_status: &HashMap<PathBuf, FileStatusType>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };

        let mut items: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        items.sort_by_key(|e| e.file_name());

        for entry in items {
            let path = entry.path();
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();

            // Skip hidden files and common ignore patterns
            if file_name.starts_with('.')
                || file_name == "node_modules"
                || file_name == "target"
                || file_name == "__pycache__"
                || file_name == "venv"
                || file_name == ".git"
            {
                continue;
            }

            let is_dir = path.is_dir();
            let rel_path = path.strip_prefix(root).unwrap_or(&path).to_path_buf();

            // Get git status if available
            let status = git_status.get(&rel_path).copied();

            self.cached_entries.push(FileEntry {
                path: rel_path,
                status,
                is_directory: is_dir,
            });

            // Recurse into directories (but limit depth for performance)
            if is_dir {
                let depth = path.strip_prefix(root).map(|p| p.components().count()).unwrap_or(0);
                if depth < 10 {
                    self.walk_directory(root, &path, git_status);
                }
            }
        }
    }

    /// Get entries to display based on current source
    pub fn get_entries(&self, git_files: &[FileStatus]) -> Vec<FileEntry> {
        match self.source {
            FileSource::Git => git_files.iter().map(FileEntry::from_git_status).collect(),
            FileSource::All => self.cached_entries.clone(),
        }
    }

    /// Show the file tree
    pub fn show(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        entries: &[FileEntry],
    ) -> FileTreeResponse {
        let mut response = FileTreeResponse::default();

        // Files mode always uses tree view (flat is impractical for many files)
        let effective_mode = match self.source {
            FileSource::All => FileListMode::Tree,
            FileSource::Git => self.mode,
        };

        match effective_mode {
            FileListMode::Flat => {
                self.show_flat(ui, theme, entries, &mut response);
            }
            FileListMode::Tree => {
                self.show_tree(ui, theme, entries, &mut response);
            }
        }

        response
    }

    fn show_flat(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        entries: &[FileEntry],
        response: &mut FileTreeResponse,
    ) {
        // Filter to only show files (not directories) in flat mode
        let files: Vec<_> = entries.iter().filter(|e| !e.is_directory).collect();

        for entry in files.iter().take(200) {
            self.render_file_item(ui, theme, entry, 0, response);
        }
    }

    fn show_tree(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        entries: &[FileEntry],
        response: &mut FileTreeResponse,
    ) {
        // Build tree structure
        let tree = build_tree(entries);

        // Render tree
        self.render_tree_node(ui, theme, &tree, &PathBuf::new(), 0, response);
    }

    fn render_tree_node(
        &mut self,
        ui: &mut Ui,
        theme: &Theme,
        tree: &TreeNode,
        current_path: &PathBuf,
        depth: usize,
        response: &mut FileTreeResponse,
    ) {
        // Sort directories first, then files
        let mut dirs: Vec<_> = tree.children.keys().collect();
        dirs.sort();

        let mut file_items: Vec<_> = tree.files.iter().collect();
        file_items.sort_by_key(|f| &f.path);

        // Render directories
        for dir_name in dirs {
            let dir_path = if current_path.as_os_str().is_empty() {
                PathBuf::from(dir_name)
            } else {
                current_path.join(dir_name)
            };

            let is_expanded = self.is_expanded(&dir_path);
            let child = &tree.children[dir_name];
            let file_count = count_files(child);

            // Directory item
            let item_response = ui.allocate_response(
                egui::vec2(ui.available_width(), 20.0),
                egui::Sense::click(),
            );

            // Draw background on hover
            if item_response.hovered() {
                ui.painter().rect_filled(
                    item_response.rect,
                    0.0,
                    theme.bg_tertiary,
                );
            }

            let rect = item_response.rect;
            let indent = depth as f32 * 12.0 + 8.0;
            let text_pos = rect.min + egui::vec2(indent, 2.0);

            // Expand/collapse icon
            let icon = if is_expanded { "▼" } else { "▶" };
            ui.painter().text(
                text_pos,
                egui::Align2::LEFT_TOP,
                icon,
                egui::FontId::proportional(10.0),
                theme.text_muted,
            );

            // Directory name
            ui.painter().text(
                text_pos + egui::vec2(14.0, 0.0),
                egui::Align2::LEFT_TOP,
                dir_name,
                egui::FontId::proportional(11.0),
                theme.text_primary,
            );

            // File count
            ui.painter().text(
                text_pos + egui::vec2(14.0 + dir_name.len() as f32 * 7.0 + 8.0, 0.0),
                egui::Align2::LEFT_TOP,
                &format!("({})", file_count),
                egui::FontId::proportional(10.0),
                theme.text_muted,
            );

            if item_response.clicked() {
                self.toggle_dir(&dir_path);
            }

            // Render children if expanded
            if is_expanded {
                self.render_tree_node(ui, theme, child, &dir_path, depth + 1, response);
            }
        }

        // Render files at this level
        for entry in file_items {
            self.render_file_item(ui, theme, entry, depth, response);
        }
    }

    fn render_file_item(
        &self,
        ui: &mut Ui,
        theme: &Theme,
        entry: &FileEntry,
        depth: usize,
        response: &mut FileTreeResponse,
    ) {
        let (status_char, status_color) = match entry.status {
            Some(FileStatusType::New) => ("+", theme.diff_add_fg),
            Some(FileStatusType::Modified) => ("~", theme.accent),
            Some(FileStatusType::Deleted) => ("-", theme.diff_delete_fg),
            Some(FileStatusType::Renamed) => ("R", theme.accent),
            Some(FileStatusType::Untracked) => ("?", theme.text_muted),
            None => (" ", theme.text_muted),
        };

        let display_name = if self.mode == FileListMode::Tree {
            entry.path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| entry.path.display().to_string())
        } else {
            entry.path.display().to_string()
        };

        // File item
        let item_response = ui.allocate_response(
            egui::vec2(ui.available_width(), 20.0),
            egui::Sense::click(),
        );

        // Draw background on hover
        if item_response.hovered() {
            ui.painter().rect_filled(
                item_response.rect,
                0.0,
                theme.bg_tertiary,
            );
        }

        let rect = item_response.rect;
        let indent = depth as f32 * 12.0 + 8.0;
        let text_pos = rect.min + egui::vec2(indent, 2.0);

        // Status indicator
        ui.painter().text(
            text_pos,
            egui::Align2::LEFT_TOP,
            status_char,
            egui::FontId::monospace(11.0),
            status_color,
        );

        // File name
        ui.painter().text(
            text_pos + egui::vec2(14.0, 0.0),
            egui::Align2::LEFT_TOP,
            &display_name,
            egui::FontId::monospace(11.0),
            if item_response.hovered() { theme.text_primary } else { theme.text_secondary },
        );

        // Handle clicks - always show diff for git files, open for others
        if item_response.clicked() {
            if entry.status.is_some() {
                // Git changed file (including markdown) - show diff
                response.show_diff = Some(entry.path.clone());
            } else {
                // Regular file - open for viewing
                response.open_file = Some(entry.path.clone());
            }
        }

        // Right-click: always insert path to terminal (consistent behavior)
        if item_response.secondary_clicked() {
            response.insert_to_terminal = Some(entry.path.clone());
        }

        let hint = if entry.status.is_some() {
            "Click: view diff | Right-click: insert to terminal"
        } else {
            "Click: view file | Right-click: insert to terminal"
        };
        item_response.on_hover_text(hint);
    }
}

impl Default for FileTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Response from file tree interaction
#[derive(Default)]
pub struct FileTreeResponse {
    pub show_diff: Option<PathBuf>,
    pub open_file: Option<PathBuf>,
    pub insert_to_terminal: Option<PathBuf>,
}

/// Tree node for directory structure
struct TreeNode {
    children: HashMap<String, TreeNode>,
    files: Vec<FileEntry>,
}

impl TreeNode {
    fn new() -> Self {
        Self {
            children: HashMap::new(),
            files: Vec::new(),
        }
    }
}

/// Build a tree structure from a list of file entries
fn build_tree(entries: &[FileEntry]) -> TreeNode {
    let mut root = TreeNode::new();

    for entry in entries {
        if entry.is_directory {
            continue; // Skip directories in tree building
        }

        let components: Vec<_> = entry.path.components().collect();

        if components.is_empty() {
            continue;
        }

        if components.len() == 1 {
            // File at root level
            root.files.push(entry.clone());
        } else {
            // File in a subdirectory
            let mut current = &mut root;

            for (i, component) in components.iter().enumerate() {
                let name = component.as_os_str().to_string_lossy().to_string();

                if i == components.len() - 1 {
                    // This is the file
                    current.files.push(entry.clone());
                } else {
                    // This is a directory
                    current = current.children.entry(name).or_insert_with(TreeNode::new);
                }
            }
        }
    }

    root
}

/// Count total files in a tree node
fn count_files(node: &TreeNode) -> usize {
    let mut count = node.files.len();
    for child in node.children.values() {
        count += count_files(child);
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_file(path: &str, status: Option<FileStatusType>) -> FileEntry {
        FileEntry {
            path: PathBuf::from(path),
            status,
            is_directory: false,
        }
    }

    #[test]
    fn test_file_source_toggle() {
        let mut source = FileSource::Git;
        assert_eq!(source.label(), "Git");

        source.toggle();
        assert_eq!(source, FileSource::All);
        assert_eq!(source.label(), "Files");

        source.toggle();
        assert_eq!(source, FileSource::Git);
    }

    #[test]
    fn test_file_list_mode_toggle() {
        let mut mode = FileListMode::Flat;

        mode.toggle();
        assert_eq!(mode, FileListMode::Tree);

        mode.toggle();
        assert_eq!(mode, FileListMode::Flat);
    }

    #[test]
    fn test_file_tree_expand_collapse() {
        let mut tree = FileTree::new();
        let dir = PathBuf::from("src/ui");

        assert!(!tree.is_expanded(&dir));

        tree.toggle_dir(&dir);
        assert!(tree.is_expanded(&dir));

        tree.toggle_dir(&dir);
        assert!(!tree.is_expanded(&dir));
    }

    #[test]
    fn test_build_tree() {
        let entries = vec![
            make_file("README.md", Some(FileStatusType::Modified)),
            make_file("src/main.rs", Some(FileStatusType::Modified)),
            make_file("src/lib.rs", Some(FileStatusType::New)),
            make_file("src/ui/mod.rs", Some(FileStatusType::Modified)),
        ];

        let tree = build_tree(&entries);

        // Root should have README.md and src directory
        assert_eq!(tree.files.len(), 1);
        assert_eq!(tree.children.len(), 1);
        assert!(tree.children.contains_key("src"));
    }

    #[test]
    fn test_file_entry_is_markdown() {
        let md = make_file("README.md", None);
        let rs = make_file("main.rs", None);

        assert!(md.is_markdown());
        assert!(!rs.is_markdown());
    }
}
