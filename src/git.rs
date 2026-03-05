//! Git operations for worktree management
//!
//! All git operations use the git CLI instead of libgit2 for:
//! - Consistent behavior (remove_worktree already used CLI)
//! - Hook support (post-checkout etc.)
//! - Simpler build (no C library dependency)

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
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
pub const CONFIG_FILE_SYNC: &str = "sashiki.template.fileSync";
pub const CONFIG_POST_CREATE_CMD: &str = "sashiki.template.postCreateCommand";
pub const CONFIG_WORKING_DIR: &str = "sashiki.template.workingDirectory";

/// Git repository wrapper using CLI commands
#[derive(Clone)]
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

/// Run a git command with stdin input and return stdout on success.
fn run_git_with_input(workdir: &Path, args: &[&str], input: &str) -> Result<String> {
    let mut child = std::process::Command::new("git")
        .args(args)
        .current_dir(workdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(GitError::Exec)?;

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| GitError::Command("Failed to open stdin for git command".to_string()))?;
        stdin
            .write_all(input.as_bytes())
            .map_err(|e| GitError::Command(e.to_string()))?;
    }

    let output = child.wait_with_output().map_err(GitError::Exec)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitError::Command(stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[derive(Debug, Clone)]
struct ParsedDiff {
    preamble: Vec<String>,
    hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone)]
struct DiffHunk {
    header: String,
    lines: Vec<String>,
    new_start: usize,
    new_count: usize,
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
            // Strip trailing '/' that git appends to untracked directories.
            let path = if let Some(arrow_pos) = path_str.find(" -> ") {
                PathBuf::from(path_str[arrow_pos + 4..].trim_end_matches('/'))
            } else {
                PathBuf::from(path_str.trim_end_matches('/'))
            };

            let staged_change = Self::index_status_to_change(index_status);
            let unstaged_change = if index_status == b'?' && wt_status == b'?' {
                Some(ChangeType::Added)
            } else {
                Self::worktree_status_to_change(wt_status)
            };

            if staged_change.is_some() || unstaged_change.is_some() {
                files.push(ChangedFile {
                    path,
                    staged_change,
                    unstaged_change,
                });
            }
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
        let rel_str = self.relative_path_string(file_path);

        // Try staged + unstaged diff against HEAD
        match run_git(&self.workdir, &["diff", "HEAD", "--", &rel_str]) {
            Ok(diff) if !diff.is_empty() => Ok(diff),
            _ => {
                // Fallback: unstaged changes only (for initial commits with no HEAD)
                run_git(&self.workdir, &["diff", "--", &rel_str]).or_else(|_| Ok(String::new()))
            }
        }
    }

    /// Get file content from HEAD using `git show HEAD:<path>`
    pub fn get_file_content_from_head(&self, file_path: &Path) -> Result<String> {
        let spec = format!("HEAD:{}", self.relative_path_string(file_path));
        run_git(&self.workdir, &["show", &spec])
    }

    /// Get file content from index using `git show :<path>`
    pub fn get_file_content_from_index(&self, file_path: &Path) -> Result<String> {
        let spec = format!(":{}", self.relative_path_string(file_path));
        run_git(&self.workdir, &["show", &spec])
    }

    /// Get staged-only diff for a specific file (`git diff --cached`).
    pub fn get_file_diff_staged(&self, file_path: &Path) -> Result<String> {
        let rel = self.relative_path_string(file_path);
        run_git(&self.workdir, &["diff", "--cached", "--", &rel]).or_else(|_| Ok(String::new()))
    }

    /// Get unstaged-only diff for a specific file (`git diff`).
    pub fn get_file_diff_unstaged(&self, file_path: &Path) -> Result<String> {
        let rel = self.relative_path_string(file_path);
        run_git(&self.workdir, &["diff", "--", &rel]).or_else(|_| Ok(String::new()))
    }

    /// Stage a file (`git add -- <path>`).
    pub fn stage_file(&self, file_path: &Path) -> Result<()> {
        let rel = self.relative_path_string(file_path);
        run_git(&self.workdir, &["add", "--", &rel])?;
        Ok(())
    }

    /// Stage the unstaged hunk containing the given (1-based) new-file line.
    pub fn stage_hunk_at_line(&self, file_path: &Path, line: usize) -> Result<()> {
        let diff = self.get_unstaged_file_diff(file_path, Some(3))?;
        let parsed = Self::parse_diff(&diff)?;
        if parsed.hunks.is_empty() {
            return Err(GitError::Command(
                "No unstaged hunks found for this file".to_string(),
            ));
        }

        let selected = parsed
            .hunks
            .iter()
            .enumerate()
            .find(|(_, h)| Self::hunk_contains_line(h, line))
            .map(|(idx, _)| idx)
            .ok_or_else(|| {
                GitError::Command(format!(
                    "No unstaged hunk found at line {}. Select a changed line and retry.",
                    line
                ))
            })?;

        let patch = Self::build_patch(&parsed, &[selected]);
        self.apply_patch_to_index(&patch, false, false)?;
        Ok(())
    }

    /// Stage unstaged changes intersecting the given (1-based) new-file line range.
    pub fn stage_line_range(
        &self,
        file_path: &Path,
        range_start: usize,
        range_end: usize,
    ) -> Result<()> {
        let (start, end) = if range_start <= range_end {
            (range_start, range_end)
        } else {
            (range_end, range_start)
        };

        let diff = self.get_unstaged_file_diff(file_path, Some(0))?;
        let parsed = Self::parse_diff(&diff)?;
        if parsed.hunks.is_empty() {
            return Err(GitError::Command(
                "No unstaged hunks found for this file".to_string(),
            ));
        }

        let selected: Vec<usize> = parsed
            .hunks
            .iter()
            .enumerate()
            .filter(|(_, h)| Self::hunk_intersects_range(h, start, end))
            .map(|(idx, _)| idx)
            .collect();

        if selected.is_empty() {
            return Err(GitError::Command(format!(
                "No unstaged changes intersect selected range {}-{}",
                start, end
            )));
        }

        let patch = Self::build_patch(&parsed, &selected);
        self.apply_patch_to_index(&patch, true, false)?;
        Ok(())
    }

    /// Unstage the staged hunk containing the given (1-based) new-file line.
    pub fn unstage_hunk_at_line(&self, file_path: &Path, line: usize) -> Result<()> {
        let diff = self.get_staged_file_diff(file_path, Some(3))?;
        let parsed = Self::parse_diff(&diff)?;
        if parsed.hunks.is_empty() {
            return Err(GitError::Command(
                "No staged hunks found for this file".to_string(),
            ));
        }

        let selected = parsed
            .hunks
            .iter()
            .enumerate()
            .find(|(_, h)| Self::hunk_contains_line(h, line))
            .map(|(idx, _)| idx)
            .ok_or_else(|| {
                GitError::Command(format!(
                    "No staged hunk found at line {}. Select a staged line and retry.",
                    line
                ))
            })?;

        let patch = Self::build_patch(&parsed, &[selected]);
        self.apply_patch_to_index(&patch, false, true)?;
        Ok(())
    }

    /// Unstage staged changes intersecting the given (1-based) new-file line range.
    pub fn unstage_line_range(
        &self,
        file_path: &Path,
        range_start: usize,
        range_end: usize,
    ) -> Result<()> {
        let (start, end) = if range_start <= range_end {
            (range_start, range_end)
        } else {
            (range_end, range_start)
        };

        let diff = self.get_staged_file_diff(file_path, Some(0))?;
        let parsed = Self::parse_diff(&diff)?;
        if parsed.hunks.is_empty() {
            return Err(GitError::Command(
                "No staged hunks found for this file".to_string(),
            ));
        }

        let selected: Vec<usize> = parsed
            .hunks
            .iter()
            .enumerate()
            .filter(|(_, h)| Self::hunk_intersects_range(h, start, end))
            .map(|(idx, _)| idx)
            .collect();

        if selected.is_empty() {
            return Err(GitError::Command(format!(
                "No staged changes intersect selected range {}-{}",
                start, end
            )));
        }

        let patch = Self::build_patch(&parsed, &selected);
        self.apply_patch_to_index(&patch, true, true)?;
        Ok(())
    }

    /// Discard the unstaged hunk containing the given (1-based) new-file line.
    pub fn discard_hunk_at_line(&self, file_path: &Path, line: usize) -> Result<()> {
        let diff = self.get_unstaged_file_diff(file_path, Some(3))?;
        let parsed = Self::parse_diff(&diff)?;
        if parsed.hunks.is_empty() {
            return Err(GitError::Command(
                "No unstaged hunks found for this file".to_string(),
            ));
        }

        let selected = parsed
            .hunks
            .iter()
            .enumerate()
            .find(|(_, h)| Self::hunk_contains_line(h, line))
            .map(|(idx, _)| idx)
            .ok_or_else(|| {
                GitError::Command(format!(
                    "No unstaged hunk found at line {}. Select a changed line and retry.",
                    line
                ))
            })?;

        let patch = Self::build_patch(&parsed, &[selected]);
        self.apply_patch_to_worktree(&patch, false, true)?;
        Ok(())
    }

    /// Discard unstaged changes intersecting the given (1-based) new-file line range.
    pub fn discard_line_range(
        &self,
        file_path: &Path,
        range_start: usize,
        range_end: usize,
    ) -> Result<()> {
        let (start, end) = if range_start <= range_end {
            (range_start, range_end)
        } else {
            (range_end, range_start)
        };

        let diff = self.get_unstaged_file_diff(file_path, Some(0))?;
        let parsed = Self::parse_diff(&diff)?;
        if parsed.hunks.is_empty() {
            return Err(GitError::Command(
                "No unstaged hunks found for this file".to_string(),
            ));
        }

        let selected: Vec<usize> = parsed
            .hunks
            .iter()
            .enumerate()
            .filter(|(_, h)| Self::hunk_intersects_range(h, start, end))
            .map(|(idx, _)| idx)
            .collect();

        if selected.is_empty() {
            return Err(GitError::Command(format!(
                "No unstaged changes intersect selected range {}-{}",
                start, end
            )));
        }

        let patch = Self::build_patch(&parsed, &selected);
        self.apply_patch_to_worktree(&patch, true, true)?;
        Ok(())
    }

    /// Unstage a file (`git restore --staged -- <path>`).
    /// Falls back to `git rm --cached` for unborn HEAD cases.
    pub fn unstage_file(&self, file_path: &Path) -> Result<()> {
        let rel = self.relative_path_string(file_path);
        match run_git(&self.workdir, &["restore", "--staged", "--", &rel]) {
            Ok(_) => Ok(()),
            Err(primary_err) => {
                let fallback = run_git(&self.workdir, &["rm", "--cached", "--quiet", "--", &rel]);
                match fallback {
                    Ok(_) => Ok(()),
                    Err(_) => Err(primary_err),
                }
            }
        }
    }

    /// Discard unstaged working-tree changes for a tracked file (`git restore -- <path>`).
    pub fn restore_file(&self, file_path: &Path) -> Result<()> {
        let rel = self.relative_path_string(file_path);
        run_git(&self.workdir, &["restore", "--", &rel])?;
        Ok(())
    }

    /// Delete an untracked file from the working tree (`git clean -f -- <path>`).
    pub fn clean_file(&self, file_path: &Path) -> Result<()> {
        let rel = self.relative_path_string(file_path);
        run_git(&self.workdir, &["clean", "-f", "--", &rel])?;
        Ok(())
    }

    /// Stage all changes in the worktree with a single `git add -A` command.
    pub fn stage_all(&self) -> Result<()> {
        run_git(&self.workdir, &["add", "-A"])?;
        Ok(())
    }

    /// Unstage all staged changes.
    /// Falls back to `git rm --cached -r .` for unborn HEAD cases (initial commit).
    pub fn unstage_all(&self) -> Result<()> {
        match run_git(&self.workdir, &["restore", "--staged", "."]) {
            Ok(_) => Ok(()),
            Err(primary_err) => {
                let fallback = run_git(&self.workdir, &["rm", "--cached", "-r", "--quiet", "."]);
                match fallback {
                    Ok(_) => Ok(()),
                    Err(_) => Err(primary_err),
                }
            }
        }
    }

    /// Create a commit with the given message (`git commit -m <message>`).
    pub fn commit(&self, message: &str) -> Result<()> {
        run_git(&self.workdir, &["commit", "-m", message])?;
        Ok(())
    }

    /// List stashes.
    pub fn list_stashes(&self) -> Result<Vec<StashEntry>> {
        let output = run_git(
            &self.workdir,
            &["stash", "list", "--pretty=format:%gd%x00%s"],
        )?;

        let mut entries = Vec::new();
        for line in output.lines() {
            if line.is_empty() {
                continue;
            }
            if let Some((reference, message)) = line.split_once('\0') {
                entries.push(StashEntry {
                    reference: reference.to_string(),
                    message: message.to_string(),
                });
            }
        }
        Ok(entries)
    }

    /// Create a new stash with the specified scope.
    pub fn create_stash(&self, message: Option<&str>, mode: StashMode) -> Result<()> {
        let mut args = vec!["stash", "push"];
        match mode {
            StashMode::Staged => args.push("--staged"),
            StashMode::IncludeUntracked => args.push("--include-untracked"),
            StashMode::All => {}
        }
        if let Some(msg) = message.map(str::trim).filter(|m| !m.is_empty()) {
            args.push("-m");
            args.push(msg);
        }
        run_git(&self.workdir, &args)?;
        Ok(())
    }

    /// List files changed in a stash entry (`git stash show --name-status <ref>`).
    /// Returns `(status, path)` pairs, e.g. `("M", "src/app.rs")`.
    pub fn stash_show_files(&self, reference: &str) -> Result<Vec<(String, String)>> {
        let output = run_git(
            &self.workdir,
            &["stash", "show", "--name-status", reference],
        )?;
        Ok(output
            .lines()
            .filter(|l| !l.is_empty())
            .filter_map(|l| {
                let mut parts = l.splitn(2, '\t');
                let status = parts.next()?.trim().to_string();
                let path = parts.next()?.trim().to_string();
                Some((status, path))
            })
            .collect())
    }

    /// Apply a stash by reference (e.g. `stash@{0}`).
    pub fn apply_stash(&self, reference: &str) -> Result<()> {
        run_git(&self.workdir, &["stash", "apply", reference])?;
        Ok(())
    }

    /// Drop a stash by reference (e.g. `stash@{0}`).
    pub fn drop_stash(&self, reference: &str) -> Result<()> {
        run_git(&self.workdir, &["stash", "drop", reference])?;
        Ok(())
    }

    /// Apply and drop a stash in one step (`git stash pop <ref>`).
    /// On merge conflict, the stash is kept intact so the user can resolve.
    pub fn pop_stash(&self, reference: &str) -> Result<()> {
        run_git(&self.workdir, &["stash", "pop", reference])?;
        Ok(())
    }

    /// Amend the most recent commit.
    /// If `message` is `Some`, replaces the message via stdin (`-F -`) to preserve multi-line bodies.
    /// Otherwise keeps the existing message unchanged.
    pub fn amend_commit(&self, message: Option<&str>) -> Result<()> {
        match message {
            Some(msg) => run_git_with_input(&self.workdir, &["commit", "--amend", "-F", "-"], msg)?,
            None => run_git(&self.workdir, &["commit", "--amend", "--no-edit"])?,
        };
        Ok(())
    }

    /// Get the full message of the most recent commit (`git log -1 --format=%B`).
    pub fn get_last_commit_message(&self) -> Result<String> {
        let output = run_git(&self.workdir, &["log", "-1", "--format=%B"])?;
        Ok(output.trim().to_string())
    }

    /// Undo the last commit with `git reset --soft HEAD~1`.
    /// Changes from the undone commit are left staged.
    pub fn undo_last_commit(&self) -> Result<()> {
        run_git(&self.workdir, &["reset", "--soft", "HEAD~1"])?;
        Ok(())
    }

    /// Discard all unstaged changes: restores tracked files and removes untracked ones.
    /// On unborn HEAD, `restore .` is expected to fail and is silently ignored.
    pub fn discard_all_changes(&self) -> Result<()> {
        // Restore tracked files to HEAD state; ignore error on unborn HEAD
        let _ = run_git(&self.workdir, &["restore", "."]);
        // Remove untracked files and directories (excludes .gitignore entries)
        run_git(&self.workdir, &["clean", "-fd"])?;
        Ok(())
    }

    fn get_unstaged_file_diff(&self, file_path: &Path, unified: Option<usize>) -> Result<String> {
        let rel = self.relative_path_string(file_path);
        if let Some(lines) = unified {
            let unified_opt = format!("--unified={lines}");
            run_git(&self.workdir, &["diff", &unified_opt, "--", &rel])
        } else {
            run_git(&self.workdir, &["diff", "--", &rel])
        }
    }

    fn get_staged_file_diff(&self, file_path: &Path, unified: Option<usize>) -> Result<String> {
        let rel = self.relative_path_string(file_path);
        if let Some(lines) = unified {
            let unified_opt = format!("--unified={lines}");
            run_git(
                &self.workdir,
                &["diff", "--cached", &unified_opt, "--", &rel],
            )
        } else {
            run_git(&self.workdir, &["diff", "--cached", "--", &rel])
        }
    }

    fn apply_patch_to_index(&self, patch: &str, unidiff_zero: bool, reverse: bool) -> Result<()> {
        let mut args = vec!["apply", "--cached", "--recount", "--whitespace=nowarn"];
        if unidiff_zero {
            args.push("--unidiff-zero");
        }
        if reverse {
            args.push("-R");
        }
        args.push("-");
        run_git_with_input(&self.workdir, &args, patch)?;
        Ok(())
    }

    fn apply_patch_to_worktree(
        &self,
        patch: &str,
        unidiff_zero: bool,
        reverse: bool,
    ) -> Result<()> {
        let mut args = vec!["apply", "--recount", "--whitespace=nowarn"];
        if unidiff_zero {
            args.push("--unidiff-zero");
        }
        if reverse {
            args.push("-R");
        }
        args.push("-");
        run_git_with_input(&self.workdir, &args, patch)?;
        Ok(())
    }

    fn parse_diff(diff: &str) -> Result<ParsedDiff> {
        let mut preamble = Vec::new();
        let mut hunks = Vec::new();
        let mut current: Option<DiffHunk> = None;

        for line in diff.lines() {
            if line.starts_with("@@") {
                if let Some(h) = current.take() {
                    hunks.push(h);
                }
                let (new_start, new_count) = Self::parse_hunk_header_new_range(line)
                    .ok_or_else(|| GitError::Parse(format!("Invalid hunk header: {}", line)))?;
                current = Some(DiffHunk {
                    header: line.to_string(),
                    lines: Vec::new(),
                    new_start,
                    new_count,
                });
            } else if let Some(ref mut h) = current {
                h.lines.push(line.to_string());
            } else {
                preamble.push(line.to_string());
            }
        }

        if let Some(h) = current {
            hunks.push(h);
        }

        Ok(ParsedDiff { preamble, hunks })
    }

    fn parse_hunk_header_new_range(header: &str) -> Option<(usize, usize)> {
        let mut parts = header.split_whitespace();
        if parts.next()? != "@@" {
            return None;
        }
        let _old = parts.next()?;
        let new_part = parts.next()?;
        let new_spec = new_part.strip_prefix('+')?;

        let (start, count) = if let Some((s, c)) = new_spec.split_once(',') {
            (s.parse().ok()?, c.parse().ok()?)
        } else {
            (new_spec.parse().ok()?, 1)
        };
        Some((start, count))
    }

    fn hunk_contains_line(hunk: &DiffHunk, line: usize) -> bool {
        if hunk.new_count == 0 {
            return line == hunk.new_start;
        }
        let end = hunk
            .new_start
            .saturating_add(hunk.new_count.saturating_sub(1));
        line >= hunk.new_start && line <= end
    }

    fn hunk_intersects_range(hunk: &DiffHunk, start: usize, end: usize) -> bool {
        let hunk_start = hunk.new_start;
        let hunk_end = if hunk.new_count == 0 {
            hunk.new_start
        } else {
            hunk.new_start
                .saturating_add(hunk.new_count.saturating_sub(1))
        };
        hunk_start <= end && hunk_end >= start
    }

    fn build_patch(parsed: &ParsedDiff, selected_hunks: &[usize]) -> String {
        let mut patch = String::new();
        for line in &parsed.preamble {
            patch.push_str(line);
            patch.push('\n');
        }
        for idx in selected_hunks {
            if let Some(hunk) = parsed.hunks.get(*idx) {
                patch.push_str(&hunk.header);
                patch.push('\n');
                for line in &hunk.lines {
                    patch.push_str(line);
                    patch.push('\n');
                }
            }
        }
        patch
    }

    fn index_status_to_change(status: u8) -> Option<ChangeType> {
        match status {
            b'A' => Some(ChangeType::Added),
            b'M' | b'C' | b'T' | b'U' => Some(ChangeType::Modified),
            b'D' => Some(ChangeType::Deleted),
            b'R' => Some(ChangeType::Renamed),
            b' ' | b'?' | b'!' => None,
            _ => Some(ChangeType::Unknown),
        }
    }

    fn worktree_status_to_change(status: u8) -> Option<ChangeType> {
        match status {
            b'?' | b'A' => Some(ChangeType::Added),
            b'M' | b'C' | b'T' | b'U' => Some(ChangeType::Modified),
            b'D' => Some(ChangeType::Deleted),
            b'R' => Some(ChangeType::Renamed),
            b' ' | b'!' => None,
            _ => Some(ChangeType::Unknown),
        }
    }

    fn relative_path_string(&self, file_path: &Path) -> String {
        file_path
            .strip_prefix(&self.workdir)
            .unwrap_or(file_path)
            .to_string_lossy()
            .to_string()
    }

    /// Generate diff for added-only file (all lines as +)
    pub fn generate_added_diff(&self, file_path: &Path) -> Result<String> {
        let content =
            std::fs::read_to_string(file_path).map_err(|e| GitError::Command(e.to_string()))?;

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
    pub staged_change: Option<ChangeType>,
    pub unstaged_change: Option<ChangeType>,
}

impl ChangedFile {
    pub fn has_staged_changes(&self) -> bool {
        self.staged_change.is_some()
    }

    pub fn has_unstaged_changes(&self) -> bool {
        self.unstaged_change.is_some()
    }
}

#[derive(Debug, Clone)]
pub struct StashEntry {
    pub reference: String,
    pub message: String,
}

/// Scope of changes to include when pushing a stash.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum StashMode {
    /// Staged + unstaged tracked changes (default `git stash push`).
    #[default]
    All,
    /// Only staged changes (`--staged`).
    Staged,
    /// Staged + unstaged + untracked files (`--include-untracked`).
    IncludeUntracked,
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
