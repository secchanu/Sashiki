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
    pub locked: bool,
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
    ///
    /// This method correctly handles being called from either the main worktree
    /// or a linked worktree by using commondir() to find the shared .git directory.
    pub fn list_worktrees(&self) -> Result<Vec<Worktree>> {
        let mut worktrees = Vec::new();

        // Use commondir() to get the shared .git directory
        // This works correctly whether we're in the main worktree or a linked worktree
        let common_dir = self.repo.commondir();
        let worktrees_dir = common_dir.join("worktrees");

        // Determine main worktree path
        // For non-bare repos: commondir is .git, so parent is the main worktree
        // For bare repos: there is no main worktree
        let main_worktree_path = if !self.repo.is_bare() {
            common_dir.parent().map(|p| p.to_path_buf())
        } else {
            None
        };

        // Add main worktree (only if this is not a bare repository)
        if let Some(main_path) = &main_worktree_path {
            // Get branch for main worktree from .git/HEAD
            let branch = self.get_worktree_branch_from_head(common_dir);

            worktrees.push(Worktree {
                name: main_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("main")
                    .to_string(),
                path: main_path.clone(),
                branch,
                is_main: true,
                locked: false, // Main worktree cannot be locked
            });
        }

        // List linked worktrees
        if let Ok(wt_names) = self.repo.worktrees() {
            for name in wt_names.iter().flatten() {
                if let Ok(wt) = self.repo.find_worktree(name) {
                    // wt.path() returns the actual worktree directory path
                    let wt_path = wt.path().to_path_buf();

                    // Get branch from .git/worktrees/<name>/HEAD
                    let wt_git_dir = worktrees_dir.join(name);
                    let branch = self.get_worktree_branch_from_head(&wt_git_dir);

                    // Check if worktree is locked
                    let locked = wt_git_dir.join("locked").exists();

                    worktrees.push(Worktree {
                        name: name.to_string(),
                        path: wt_path,
                        branch,
                        is_main: false,
                        locked,
                    });
                }
            }
        }

        Ok(worktrees)
    }

    /// Create a new worktree with the specified branch.
    ///
    /// Branch resolution order:
    /// 1. If a local branch with the name exists, use it
    /// 2. If a remote branch `origin/{branch}` exists, create local branch from it
    /// 3. Otherwise, create a new branch from HEAD
    ///
    /// Stale worktree entries with the same name are automatically pruned.
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
                ))
                .into());
            }
        }

        // Clean up orphaned worktree directory in .git/worktrees/<name>
        // This handles cases where find_worktree fails but directory remnants exist
        let git_worktrees_dir = self.repo.path().join("worktrees").join(name);
        if git_worktrees_dir.exists() {
            std::fs::remove_dir_all(&git_worktrees_dir).map_err(|e| {
                git2::Error::from_str(&format!(
                    "Failed to remove orphaned worktree directory '{}': {}",
                    git_worktrees_dir.display(),
                    e
                ))
            })?;
        }

        // Check if a local branch with the specified name exists
        if let Ok(local_branch) = self.repo.find_branch(branch, git2::BranchType::Local) {
            // Local branch exists - create worktree with reference to it
            let reference = local_branch.into_reference();
            let mut opts = git2::WorktreeAddOptions::new();
            opts.reference(Some(&reference));
            self.repo.worktree(name, path, Some(&opts))?;
        } else if let Ok(remote_ref) = self
            .repo
            .find_reference(&format!("refs/remotes/origin/{}", branch))
        {
            // Remote branch exists - create local branch from it, then create worktree
            let commit = remote_ref.peel_to_commit()?;
            let local_branch = self.repo.branch(branch, &commit, false)?;
            let reference = local_branch.into_reference();
            let mut opts = git2::WorktreeAddOptions::new();
            opts.reference(Some(&reference));
            self.repo.worktree(name, path, Some(&opts))?;
        } else {
            // No existing branch - create new branch from HEAD
            let head = self.repo.head()?;
            let commit = head.peel_to_commit()?;
            let new_branch = self.repo.branch(branch, &commit, false)?;
            let reference = new_branch.into_reference();
            let mut opts = git2::WorktreeAddOptions::new();
            opts.reference(Some(&reference));
            self.repo.worktree(name, path, Some(&opts))?;
        }

        Ok(Worktree {
            name: name.to_string(),
            path: path.to_path_buf(),
            branch: Some(branch.to_string()),
            is_main: false,
            locked: false,
        })
    }

    /// Remove a worktree using git command (more reliable than libgit2 on Windows).
    ///
    /// # Safety
    /// The `name` parameter is passed to the git command. Callers should ensure
    /// the name comes from trusted sources (e.g., our own worktree list) or has
    /// been validated with `validate_branch_name`.
    pub fn remove_worktree(&self, name: &str) -> Result<()> {
        // Use git command directly to avoid libgit2 XDG issues on Windows
        // Use commondir() to get the correct working directory regardless of
        // whether we're in the main worktree or a linked worktree
        let common_dir = self.repo.commondir();
        let workdir = if self.repo.is_bare() {
            common_dir.to_path_buf()
        } else {
            common_dir
                .parent()
                .map(|p| p.to_path_buf())
                .ok_or_else(|| git2::Error::from_str("No working directory"))?
        };

        let output = std::process::Command::new("git")
            .args(["worktree", "remove", "--force", name])
            .current_dir(&workdir)
            .output()
            .map_err(|e| git2::Error::from_str(&format!("Failed to run git command: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Ignore "not a working tree" errors
            if !stderr.contains("is not a working tree") {
                return Err(git2::Error::from_str(&format!(
                    "git worktree remove failed: {}",
                    stderr.trim()
                ))
                .into());
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
    ///
    /// Uses commondir() to correctly determine the main worktree location
    /// regardless of whether this is called from main or linked worktree.
    pub fn worktrees_dir(&self) -> Option<PathBuf> {
        if self.repo.is_bare() {
            return None;
        }
        // commondir() returns .git, so parent is the main worktree
        let main_worktree = self.repo.commondir().parent()?;
        let parent = main_worktree.parent()?;
        let repo_name = main_worktree.file_name()?.to_str()?;
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
        let workdir = self
            .repo
            .workdir()
            .ok_or_else(|| git2::Error::from_str("No working directory"))?;

        // Get relative path from repo root
        let relative_path = file_path.strip_prefix(workdir).unwrap_or(file_path);

        let mut diff_opts = git2::DiffOptions::new();
        diff_opts.pathspec(relative_path);

        // Get diff between HEAD and working directory
        let head = self.repo.head()?.peel_to_tree()?;
        let diff = self
            .repo
            .diff_tree_to_workdir_with_index(Some(&head), Some(&mut diff_opts))?;

        let mut diff_text = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let prefix = match line.origin() {
                '+' => "+",
                '-' => "-",
                ' ' => " ",
                'F' => "",    // File header - no prefix needed
                'H' => "@@ ", // Hunk header
                _ => "",
            };

            if let Ok(content) = std::str::from_utf8(line.content()) {
                if !prefix.is_empty() || line.origin() == 'F' {
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
        let workdir = self
            .repo
            .workdir()
            .ok_or_else(|| git2::Error::from_str("No working directory"))?;

        let relative_path = file_path.strip_prefix(workdir).unwrap_or(file_path);

        let head = self.repo.head()?.peel_to_tree()?;
        let entry = head.get_path(relative_path)?;
        let blob = self.repo.find_blob(entry.id())?;

        String::from_utf8(blob.content().to_vec())
            .map_err(|e| git2::Error::from_str(&format!("Invalid UTF-8 content: {}", e)).into())
    }

    /// Generate diff for added-only file (all lines as +)
    pub fn generate_added_diff(&self, file_path: &Path) -> Result<String> {
        let content = std::fs::read_to_string(file_path)
            .map_err(|e| git2::Error::from_str(&e.to_string()))?;

        let file_name = file_path
            .file_name()
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

        let file_name = file_path
            .file_name()
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

/// Validate a branch name according to Git rules
pub fn validate_branch_name(name: &str) -> std::result::Result<(), &'static str> {
    if name.is_empty() {
        return Err("Branch name cannot be empty");
    }
    if name.starts_with('/') || name.ends_with('/') {
        return Err("Branch name cannot start or end with /");
    }
    if name.starts_with('.') || name.ends_with('.') {
        return Err("Branch name cannot start or end with .");
    }
    if name.starts_with('-') {
        return Err("Branch name cannot start with -");
    }
    if name.contains("..") {
        return Err("Branch name cannot contain ..");
    }
    if name.contains("//") {
        return Err("Branch name cannot contain //");
    }
    if name.ends_with(".lock") {
        return Err("Branch name cannot end with .lock");
    }
    // Git forbidden characters
    const FORBIDDEN_CHARS: &[char] = &[' ', '~', '^', ':', '?', '*', '[', '\\', '\x7f'];
    for c in name.chars() {
        if c.is_control() || FORBIDDEN_CHARS.contains(&c) {
            return Err("Branch name contains invalid character");
        }
    }
    if name.contains("@{") {
        return Err("Branch name cannot contain @{");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_branch_name_valid() {
        assert!(validate_branch_name("feature/test").is_ok());
        assert!(validate_branch_name("bugfix-123").is_ok());
        assert!(validate_branch_name("release-v1.0.0").is_ok());
        assert!(validate_branch_name("my_branch").is_ok());
        assert!(validate_branch_name("a").is_ok());
    }

    #[test]
    fn test_validate_branch_name_empty() {
        assert_eq!(validate_branch_name(""), Err("Branch name cannot be empty"));
    }

    #[test]
    fn test_validate_branch_name_slash_rules() {
        assert_eq!(
            validate_branch_name("/test"),
            Err("Branch name cannot start or end with /")
        );
        assert_eq!(
            validate_branch_name("test/"),
            Err("Branch name cannot start or end with /")
        );
        assert_eq!(
            validate_branch_name("test//foo"),
            Err("Branch name cannot contain //")
        );
    }

    #[test]
    fn test_validate_branch_name_dot_rules() {
        assert_eq!(
            validate_branch_name(".test"),
            Err("Branch name cannot start or end with .")
        );
        assert_eq!(
            validate_branch_name("test."),
            Err("Branch name cannot start or end with .")
        );
        assert_eq!(
            validate_branch_name("test..foo"),
            Err("Branch name cannot contain ..")
        );
        assert_eq!(
            validate_branch_name("test.lock"),
            Err("Branch name cannot end with .lock")
        );
    }

    #[test]
    fn test_validate_branch_name_dash_rules() {
        assert_eq!(
            validate_branch_name("-test"),
            Err("Branch name cannot start with -")
        );
        // ending with dash is allowed
        assert!(validate_branch_name("test-").is_ok());
    }

    #[test]
    fn test_validate_branch_name_forbidden_chars() {
        assert_eq!(
            validate_branch_name("test branch"),
            Err("Branch name contains invalid character")
        );
        assert_eq!(
            validate_branch_name("test~branch"),
            Err("Branch name contains invalid character")
        );
        assert_eq!(
            validate_branch_name("test^branch"),
            Err("Branch name contains invalid character")
        );
        assert_eq!(
            validate_branch_name("test:branch"),
            Err("Branch name contains invalid character")
        );
        assert_eq!(
            validate_branch_name("test?branch"),
            Err("Branch name contains invalid character")
        );
        assert_eq!(
            validate_branch_name("test*branch"),
            Err("Branch name contains invalid character")
        );
        assert_eq!(
            validate_branch_name("test[branch"),
            Err("Branch name contains invalid character")
        );
        assert_eq!(
            validate_branch_name("test\\branch"),
            Err("Branch name contains invalid character")
        );
    }

    #[test]
    fn test_validate_branch_name_reflog_syntax() {
        assert_eq!(
            validate_branch_name("test@{1}"),
            Err("Branch name cannot contain @{")
        );
    }
}
