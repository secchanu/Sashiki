//! Git operations for worktree management

use git2::{Repository, StatusOptions};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GitError {
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),
}

pub type Result<T> = std::result::Result<T, GitError>;

/// Represents a git worktree
#[derive(Debug, Clone)]
pub struct Worktree {
    pub name: String,
    pub path: PathBuf,
    pub branch: Option<String>,
    pub is_main: bool,
}

/// Git repository wrapper
pub struct GitRepo {
    repo: Repository,
}

impl GitRepo {
    /// Open a repository at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let repo = Repository::discover(path)?;
        Ok(Self { repo })
    }

    /// List all worktrees
    pub fn list_worktrees(&self) -> Result<Vec<Worktree>> {
        let mut worktrees = Vec::new();

        // Add main worktree
        if let Some(workdir) = self.repo.workdir() {
            let branch = self.get_current_branch();
            worktrees.push(Worktree {
                name: workdir
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("main")
                    .to_string(),
                path: workdir.to_path_buf(),
                branch,
                is_main: true,
            });
        }

        // List linked worktrees
        // git2's wt.path() returns the actual worktree path, not .git/worktrees/<name>
        // We need to read the HEAD file from .git/worktrees/<name>/ to get branch info
        if let Some(git_dir) = self.repo.path().parent() {
            let worktrees_dir = if self.repo.path().ends_with(".git") {
                self.repo.path().join("worktrees")
            } else {
                // Bare repo or worktree's .git file
                git_dir.join("worktrees")
            };

            if let Ok(wt_names) = self.repo.worktrees() {
                for name in wt_names.iter().flatten() {
                    if let Ok(wt) = self.repo.find_worktree(name) {
                        // wt.path() returns the actual worktree directory path
                        let wt_path = wt.path().to_path_buf();

                        // Get branch from .git/worktrees/<name>/HEAD
                        let wt_git_dir = worktrees_dir.join(name);
                        let branch = self.get_worktree_branch_from_head(&wt_git_dir);

                        worktrees.push(Worktree {
                            name: name.to_string(),
                            path: wt_path,
                            branch,
                            is_main: false,
                        });
                    }
                }
            }
        }

        Ok(worktrees)
    }

    /// Create a new worktree with the specified branch
    /// If the branch doesn't exist locally, it will try to create from remote or create new
    pub fn create_worktree(&self, name: &str, branch: &str, path: &Path) -> Result<Worktree> {
        // Clean up stale worktree entry if it exists
        if let Ok(existing_wt) = self.repo.find_worktree(name) {
            // Check if prunable: working_tree=false (must be missing), valid=false (can be invalid), locked=false (must be unlocked)
            if existing_wt.is_prunable(Some(
                git2::WorktreePruneOptions::new()
                    .working_tree(false)
                    .valid(false)
                    .locked(false),
            ))? {
                // Prune: working_tree=true (remove dir if exists), valid=false (allow invalid)
                existing_wt.prune(Some(
                    git2::WorktreePruneOptions::new()
                        .working_tree(true)
                        .valid(false),
                ))?;
            } else {
                return Err(git2::Error::from_str(&format!(
                    "Worktree '{}' already exists and is not prunable",
                    name
                )).into());
            }
        }

        // Clean up orphaned worktree directory in .git/worktrees/<name>
        // This handles cases where find_worktree fails but directory remnants exist
        let git_worktrees_dir = self.repo.path().join("worktrees").join(name);
        if git_worktrees_dir.exists() {
            std::fs::remove_dir_all(&git_worktrees_dir)
                .map_err(|e| git2::Error::from_str(&format!(
                    "Failed to remove orphaned worktree directory '{}': {}",
                    git_worktrees_dir.display(), e
                )))?;
        }

        // Note: worktree name (name) is used as both the git worktree name and the branch name
        // This is because libgit2 creates a branch with the worktree name when no reference is specified
        // The 'branch' parameter is kept for compatibility but should match 'name' for new worktrees

        // Check if a branch with the worktree name already exists
        if self.repo.find_branch(name, git2::BranchType::Local).is_ok() {
            // Branch exists - use checkout_existing option
            let mut opts = git2::WorktreeAddOptions::new();
            opts.checkout_existing(true);
            self.repo.worktree(name, path, Some(&opts))?;
        } else {
            // No local branch - let libgit2 create new branch from HEAD
            // libgit2 creates a branch with the worktree name automatically
            self.repo.worktree(name, path, None)?;
        }

        Ok(Worktree {
            name: name.to_string(),
            path: path.to_path_buf(),
            branch: Some(branch.to_string()),
            is_main: false,
        })
    }

    /// Remove a worktree using git command (more reliable than libgit2 on Windows)
    pub fn remove_worktree(&self, name: &str) -> Result<()> {
        // Use git command directly to avoid libgit2 XDG issues on Windows
        let workdir = self.repo.workdir().ok_or_else(|| {
            git2::Error::from_str("No working directory")
        })?;

        let output = std::process::Command::new("git")
            .args(["worktree", "remove", "--force", name])
            .current_dir(workdir)
            .output()
            .map_err(|e| git2::Error::from_str(&format!("Failed to run git command: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Ignore "not a working tree" errors
            if !stderr.contains("is not a working tree") {
                return Err(git2::Error::from_str(&format!(
                    "git worktree remove failed: {}",
                    stderr.trim()
                )).into());
            }
        }

        Ok(())
    }

    /// Get list of changed files
    pub fn get_changed_files(&self) -> Result<Vec<ChangedFile>> {
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);

        let statuses = self.repo.statuses(Some(&mut opts))?;
        let mut files = Vec::new();

        for entry in statuses.iter() {
            if let Some(path) = entry.path() {
                let status = entry.status();
                let change_type = if status.is_index_new() || status.is_wt_new() {
                    ChangeType::Added
                } else if status.is_index_modified() || status.is_wt_modified() {
                    ChangeType::Modified
                } else if status.is_index_deleted() || status.is_wt_deleted() {
                    ChangeType::Deleted
                } else if status.is_index_renamed() || status.is_wt_renamed() {
                    ChangeType::Renamed
                } else {
                    ChangeType::Unknown
                };

                files.push(ChangedFile {
                    path: PathBuf::from(path),
                    change_type,
                    staged: status.is_index_new()
                        || status.is_index_modified()
                        || status.is_index_deleted()
                        || status.is_index_renamed(),
                });
            }
        }

        Ok(files)
    }

    fn get_current_branch(&self) -> Option<String> {
        self.repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(|s| s.to_string()))
    }

    fn get_worktree_branch_from_head(&self, wt_git_path: &Path) -> Option<String> {
        // Read HEAD file from worktree's git directory
        let head_file = wt_git_path.join("HEAD");
        if let Ok(content) = std::fs::read_to_string(&head_file) {
            let content = content.trim();
            // HEAD contains "ref: refs/heads/<branch>" or a commit hash
            if let Some(branch) = content.strip_prefix("ref: refs/heads/") {
                return Some(branch.to_string());
            }
            // Detached HEAD - return short hash
            if content.len() >= 7 {
                return Some(content[..7].to_string());
            }
        }
        None
    }

    /// Get the worktrees directory path ({project}.worktrees/)
    pub fn worktrees_dir(&self) -> Option<PathBuf> {
        let workdir = self.repo.workdir()?;
        let parent = workdir.parent()?;
        let repo_name = workdir.file_name()?.to_str()?;
        Some(parent.join(format!("{}.worktrees", repo_name)))
    }

    /// Generate worktree path: {project}.worktrees/{branch}
    pub fn generate_worktree_path(&self, branch: &str) -> Option<PathBuf> {
        let worktrees_dir = self.worktrees_dir()?;
        let safe_branch = branch.replace('/', "-");
        Some(worktrees_dir.join(safe_branch))
    }

    /// Get diff for a specific file
    pub fn get_file_diff(&self, file_path: &Path) -> Result<String> {
        let workdir = self.repo.workdir().ok_or_else(|| {
            git2::Error::from_str("No working directory")
        })?;

        // Get relative path from repo root
        let relative_path = file_path.strip_prefix(workdir)
            .unwrap_or(file_path);

        let mut diff_opts = git2::DiffOptions::new();
        diff_opts.pathspec(relative_path);

        // Get diff between HEAD and working directory
        let head = self.repo.head()?.peel_to_tree()?;
        let diff = self.repo.diff_tree_to_workdir_with_index(
            Some(&head),
            Some(&mut diff_opts),
        )?;

        let mut diff_text = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let prefix = match line.origin() {
                '+' => "+",
                '-' => "-",
                ' ' => " ",
                'F' => "", // File header
                'H' => "@@ ", // Hunk header
                _ => "",
            };

            if let Ok(content) = std::str::from_utf8(line.content()) {
                if line.origin() == 'H' {
                    diff_text.push_str("@@ ");
                } else if !prefix.is_empty() || line.origin() == 'F' {
                    diff_text.push_str(prefix);
                }
                diff_text.push_str(content);
            }
            true
        })?;

        Ok(diff_text)
    }

    /// Get file content from HEAD (for deleted files)
    pub fn get_file_content_from_head(&self, file_path: &Path) -> Result<String> {
        let workdir = self.repo.workdir().ok_or_else(|| {
            git2::Error::from_str("No working directory")
        })?;

        let relative_path = file_path.strip_prefix(workdir)
            .unwrap_or(file_path);

        let head = self.repo.head()?.peel_to_tree()?;
        let entry = head.get_path(relative_path)?;
        let blob = self.repo.find_blob(entry.id())?;

        String::from_utf8(blob.content().to_vec())
            .map_err(|_| git2::Error::from_str("Invalid UTF-8 content").into())
    }

    /// Generate diff for added-only file (all lines as +)
    pub fn generate_added_diff(&self, file_path: &Path) -> Result<String> {
        let content = std::fs::read_to_string(file_path)
            .map_err(|e| git2::Error::from_str(&e.to_string()))?;

        let file_name = file_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");

        let lines: Vec<&str> = content.lines().collect();
        let line_count = lines.len();

        let mut diff = String::new();
        diff.push_str("--- /dev/null\n");
        diff.push_str(&format!("+++ b/{}\n", file_name));
        diff.push_str(&format!("@@ -0,0 +1,{} @@\n", line_count));

        for line in lines {
            diff.push('+');
            diff.push_str(line);
            diff.push('\n');
        }

        Ok(diff)
    }

    /// Generate diff for deleted-only file (all lines as -)
    pub fn generate_deleted_diff(&self, file_path: &Path) -> Result<String> {
        let content = self.get_file_content_from_head(file_path)?;

        let file_name = file_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");

        let lines: Vec<&str> = content.lines().collect();
        let line_count = lines.len();

        let mut diff = String::new();
        diff.push_str(&format!("--- a/{}\n", file_name));
        diff.push_str("+++ /dev/null\n");
        diff.push_str(&format!("@@ -1,{} +0,0 @@\n", line_count));

        for line in lines {
            diff.push('-');
            diff.push_str(line);
            diff.push('\n');
        }

        Ok(diff)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
    Unknown,
}


#[derive(Debug, Clone)]
pub struct ChangedFile {
    pub path: PathBuf,
    pub change_type: ChangeType,
    pub staged: bool,
}
