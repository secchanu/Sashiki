//! Git operations for worktree management
//!
//! All git operations use the git CLI instead of libgit2 for:
//! - Consistent behavior (remove_worktree already used CLI)
//! - Hook support (post-checkout etc.)
//! - Simpler build (no C library dependency)

use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GitError {
    #[error("Git command failed: {0}")]
    Command(String),
    #[error("Git command not found or failed to execute: {0}")]
    Exec(#[from] std::io::Error),
    #[error("Failed to parse git output: {0}")]
    #[allow(dead_code)]
    Parse(String),
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

/// Git config key constants for session template
pub const CONFIG_PRE_CREATE_CMD: &str = "sashiki.template.preCreateCommand";
pub const CONFIG_FILE_COPY: &str = "sashiki.template.fileCopy";
pub const CONFIG_POST_CREATE_CMD: &str = "sashiki.template.postCreateCommand";
pub const CONFIG_WORKING_DIR: &str = "sashiki.template.workingDirectory";

/// Git repository wrapper using CLI commands
pub struct GitRepo {
    /// Working directory of the main worktree
    workdir: PathBuf,
    /// Shared .git directory (commondir equivalent)
    git_dir: PathBuf,
}

/// Run a git command and return stdout on success
fn run_git(workdir: &Path, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(workdir)
        .output()
        .map_err(GitError::Exec)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitError::Command(stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

impl GitRepo {
    /// Open a repository at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        let workdir_str = run_git(path, &["rev-parse", "--show-toplevel"])?;
        let workdir = PathBuf::from(workdir_str.trim());

        let git_dir_str = run_git(path, &["rev-parse", "--git-common-dir"])?;
        let git_dir_raw = PathBuf::from(git_dir_str.trim());
        // --git-common-dir may return a relative path; resolve it
        let git_dir = if git_dir_raw.is_relative() {
            path.join(&git_dir_raw)
                .canonicalize()
                .unwrap_or_else(|_| path.join(&git_dir_raw))
        } else {
            git_dir_raw
        };

        Ok(Self { workdir, git_dir })
    }

    /// Create a GitRepo from known paths (used in async contexts)
    pub fn from_parts(workdir: PathBuf, git_dir: PathBuf) -> Self {
        Self { workdir, git_dir }
    }

    /// Get the main worktree working directory path
    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    /// Get the shared .git directory path
    pub fn git_dir(&self) -> &Path {
        &self.git_dir
    }

    /// List all worktrees using `git worktree list --porcelain`
    pub fn list_worktrees(&self) -> Result<Vec<Worktree>> {
        let output = run_git(&self.workdir, &["worktree", "list", "--porcelain"])?;
        let mut worktrees = Vec::new();

        // Parse porcelain output: blocks separated by empty lines
        // Each block has: worktree <path>, HEAD <hash>, branch refs/heads/<name>, [locked], [bare]
        let mut current_path: Option<PathBuf> = None;
        let mut current_branch: Option<String> = None;
        let mut current_locked = false;
        let mut is_bare = false;

        for line in output.lines() {
            if line.is_empty() {
                // End of block - flush current worktree
                if let Some(path) = current_path.take() {
                    let is_main = worktrees.is_empty();
                    let name = self.worktree_name(&path, is_main);
                    worktrees.push(Worktree {
                        name,
                        path,
                        branch: current_branch.take(),
                        is_main,
                        locked: current_locked,
                    });
                    current_locked = false;
                    is_bare = false;
                }
                continue;
            }

            if let Some(path_str) = line.strip_prefix("worktree ") {
                current_path = Some(PathBuf::from(path_str));
            } else if let Some(branch_ref) = line.strip_prefix("branch refs/heads/") {
                current_branch = Some(branch_ref.to_string());
            } else if line.starts_with("HEAD ") && current_branch.is_none() {
                // Detached HEAD - use short hash
                let hash = line.strip_prefix("HEAD ").unwrap_or("");
                if hash.len() >= 7 {
                    current_branch = Some(hash[..7].to_string());
                }
            } else if line == "bare" {
                is_bare = true;
            } else if line.starts_with("locked") {
                current_locked = true;
            }
        }

        // Flush last block (porcelain output may not end with empty line)
        if let Some(path) = current_path.take() {
            if !is_bare {
                let is_main = worktrees.is_empty();
                let name = self.worktree_name(&path, is_main);
                worktrees.push(Worktree {
                    name,
                    path,
                    branch: current_branch.take(),
                    is_main,
                    locked: current_locked,
                });
            }
        }

        Ok(worktrees)
    }

    /// Determine worktree name
    fn worktree_name(&self, path: &Path, is_main: bool) -> String {
        if is_main {
            return path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("main")
                .to_string();
        }

        // For linked worktrees, check .git/worktrees/<name>/gitdir to find matching name
        let worktrees_dir = self.git_dir.join("worktrees");
        if let Ok(entries) = std::fs::read_dir(&worktrees_dir) {
            for entry in entries.flatten() {
                let gitdir_file = entry.path().join("gitdir");
                if let Ok(content) = std::fs::read_to_string(&gitdir_file) {
                    let referenced = PathBuf::from(content.trim());
                    // gitdir contains path to the .git file in the worktree
                    if let Some(parent) = referenced.parent() {
                        if parent == path {
                            if let Some(name) = entry.file_name().to_str() {
                                return name.to_string();
                            }
                        }
                    }
                }
            }
        }

        // Fallback: use directory name
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    /// Create a new worktree with the specified branch.
    ///
    /// Branch resolution is delegated to `git worktree add`:
    /// 1. If a local branch with the name exists, use it
    /// 2. If a remote branch `origin/{branch}` exists, create local branch from it
    /// 3. Otherwise, create a new branch from HEAD
    ///
    /// Stale worktree entries are automatically pruned before creation.
    pub fn create_worktree(&self, name: &str, branch: &str, path: &Path) -> Result<Worktree> {
        // Prune stale worktree entries
        let _ = run_git(&self.workdir, &["worktree", "prune"]);

        // Clean up orphaned worktree directory in .git/worktrees/<name>
        let git_worktrees_dir = self.git_dir.join("worktrees").join(name);
        if git_worktrees_dir.exists() {
            std::fs::remove_dir_all(&git_worktrees_dir).map_err(|e| {
                GitError::Command(format!(
                    "Failed to remove orphaned worktree directory '{}': {}",
                    git_worktrees_dir.display(),
                    e
                ))
            })?;
        }

        let path_str = path.to_string_lossy();

        // Check if a local branch exists
        let local_exists = run_git(
            &self.workdir,
            &["rev-parse", "--verify", &format!("refs/heads/{}", branch)],
        )
        .is_ok();

        if local_exists {
            // Local branch exists - use it directly
            run_git(&self.workdir, &["worktree", "add", &path_str, branch])?;
        } else {
            // Check if a remote tracking branch exists
            let remote_exists = run_git(
                &self.workdir,
                &[
                    "rev-parse",
                    "--verify",
                    &format!("refs/remotes/origin/{}", branch),
                ],
            )
            .is_ok();

            if remote_exists {
                // Remote branch exists - create local tracking branch
                run_git(
                    &self.workdir,
                    &[
                        "worktree",
                        "add",
                        "-b",
                        branch,
                        &path_str,
                        &format!("origin/{}", branch),
                    ],
                )?;
            } else {
                // Create new branch from HEAD
                run_git(
                    &self.workdir,
                    &["worktree", "add", "-b", branch, &path_str, "HEAD"],
                )?;
            }
        }

        Ok(Worktree {
            name: name.to_string(),
            path: path.to_path_buf(),
            branch: Some(branch.to_string()),
            is_main: false,
            locked: false,
        })
    }

    /// Remove a worktree using git command.
    ///
    /// # Safety
    /// The `name` parameter is passed to the git command. Callers should ensure
    /// the name comes from trusted sources (e.g., our own worktree list) or has
    /// been validated with `validate_branch_name`.
    pub fn remove_worktree(&self, name: &str) -> Result<()> {
        let output = std::process::Command::new("git")
            .args(["worktree", "remove", "--force", name])
            .current_dir(&self.workdir)
            .output()
            .map_err(GitError::Exec)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Ignore "not a working tree" errors
            if !stderr.contains("is not a working tree") {
                return Err(GitError::Command(format!(
                    "git worktree remove failed: {}",
                    stderr.trim()
                )));
            }
        }

        Ok(())
    }

    /// Get list of changed files using `git status --porcelain=v1`
    pub fn get_changed_files(&self) -> Result<Vec<ChangedFile>> {
        let output = run_git(&self.workdir, &["status", "--porcelain=v1"])?;
        let mut files = Vec::new();

        for line in output.lines() {
            if line.len() < 3 {
                continue;
            }

            let index_status = line.as_bytes()[0];
            let wt_status = line.as_bytes()[1];
            let path_str = &line[3..];

            // Handle renamed files: "old -> new"
            let path = if let Some(arrow_pos) = path_str.find(" -> ") {
                PathBuf::from(&path_str[arrow_pos + 4..])
            } else {
                PathBuf::from(path_str)
            };

            let change_type = if matches!(
                (index_status, wt_status),
                (b'A', _) | (_, b'A') | (b'?', b'?')
            ) {
                ChangeType::Added
            } else if matches!((index_status, wt_status), (b'M', _) | (_, b'M')) {
                ChangeType::Modified
            } else if matches!((index_status, wt_status), (b'D', _) | (_, b'D')) {
                ChangeType::Deleted
            } else if matches!((index_status, wt_status), (b'R', _) | (_, b'R')) {
                ChangeType::Renamed
            } else {
                ChangeType::Unknown
            };

            let staged = matches!(index_status, b'A' | b'M' | b'D' | b'R');

            files.push(ChangedFile {
                path,
                change_type,
                staged,
            });
        }

        Ok(files)
    }

    /// Get the worktrees directory path ({project}.worktrees/)
    pub fn worktrees_dir(&self) -> Option<PathBuf> {
        let parent = self.workdir.parent()?;
        let repo_name = self.workdir.file_name()?.to_str()?;
        Some(parent.join(format!("{}.worktrees", repo_name)))
    }

    /// Generate worktree path: {project}.worktrees/{branch}
    pub fn generate_worktree_path(&self, branch: &str) -> Option<PathBuf> {
        let worktrees_dir = self.worktrees_dir()?;
        let safe_branch = branch.replace('/', "-");
        Some(worktrees_dir.join(safe_branch))
    }

    /// Get diff for a specific file using `git diff HEAD`
    pub fn get_file_diff(&self, file_path: &Path) -> Result<String> {
        let relative_path = file_path
            .strip_prefix(&self.workdir)
            .unwrap_or(file_path);
        let rel_str = relative_path.to_string_lossy();

        // Try staged + unstaged diff against HEAD
        match run_git(&self.workdir, &["diff", "HEAD", "--", &rel_str]) {
            Ok(diff) if !diff.is_empty() => Ok(diff),
            _ => {
                // Fallback: unstaged changes only (for initial commits with no HEAD)
                run_git(&self.workdir, &["diff", "--", &rel_str])
                    .or_else(|_| Ok(String::new()))
            }
        }
    }

    /// Get file content from HEAD using `git show HEAD:<path>`
    pub fn get_file_content_from_head(&self, file_path: &Path) -> Result<String> {
        let relative_path = file_path
            .strip_prefix(&self.workdir)
            .unwrap_or(file_path);
        let spec = format!("HEAD:{}", relative_path.to_string_lossy());
        run_git(&self.workdir, &["show", &spec])
    }

    /// Generate diff for added-only file (all lines as +)
    pub fn generate_added_diff(&self, file_path: &Path) -> Result<String> {
        let content = std::fs::read_to_string(file_path)
            .map_err(|e| GitError::Command(e.to_string()))?;

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

    // --- Git config access for session templates ---

    /// Read all values for a multi-valued git config key
    pub fn get_config_values(&self, key: &str) -> Vec<String> {
        match run_git(&self.workdir, &["config", "--get-all", key]) {
            Ok(output) => output
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Read a single git config value
    pub fn get_config_value(&self, key: &str) -> Option<String> {
        run_git(&self.workdir, &["config", "--get", key])
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    /// Set all values for a multi-valued git config key (local scope)
    pub fn set_config_values(&self, key: &str, values: &[String]) -> Result<()> {
        // Remove all existing values first (ignore error if key doesn't exist)
        let _ = run_git(&self.workdir, &["config", "--local", "--unset-all", key]);

        // Add each value
        for value in values {
            run_git(&self.workdir, &["config", "--local", "--add", key, value])?;
        }
        Ok(())
    }

    /// Set a single git config value (local scope)
    pub fn set_config_value(&self, key: &str, value: &str) -> Result<()> {
        run_git(&self.workdir, &["config", "--local", key, value])?;
        Ok(())
    }

    /// Remove a git config key (local scope)
    pub fn remove_config_key(&self, key: &str) -> Result<()> {
        let _ = run_git(&self.workdir, &["config", "--local", "--unset-all", key]);
        Ok(())
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
