//! Git and Worktree management module
//!
//! Handles Git repository operations and worktree management.

use git2::{Repository, StatusOptions};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GitError {
    #[error("Git operation failed: {0}")]
    Git2Error(#[from] git2::Error),
    #[error("Invalid worktree: {0}")]
    InvalidWorktree(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Branch not found: {0}")]
    BranchNotFound(String),
    #[error("Worktree already exists: {0}")]
    WorktreeExists(String),
}

#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub name: String,
    pub path: PathBuf,
    pub branch: Option<String>,
    pub is_main: bool,
}

impl WorktreeInfo {
    pub fn display_name(&self) -> &str {
        if self.is_main {
            "main"
        } else {
            &self.name
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileStatus {
    pub path: PathBuf,
    pub status: FileStatusType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatusType {
    New,
    Modified,
    Deleted,
    Renamed,
    Untracked,
}

pub struct GitManager {
    repo_path: PathBuf,
}

impl GitManager {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, GitError> {
        let path = path.as_ref().to_path_buf();
        let repo = Repository::discover(&path)?;
        let repo_path = repo
            .workdir()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| repo.path().to_path_buf());
        Ok(Self { repo_path })
    }

    fn open_repo(&self) -> Result<Repository, GitError> {
        Repository::open(&self.repo_path).map_err(GitError::from)
    }

    pub fn list_worktrees(&self) -> Result<Vec<WorktreeInfo>, GitError> {
        let repo = self.open_repo()?;
        let mut worktrees = Vec::new();

        // Add main worktree
        let main_branch = repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(String::from));
        worktrees.push(WorktreeInfo {
            name: "main".to_string(),
            path: self.repo_path.clone(),
            branch: main_branch,
            is_main: true,
        });

        // Add linked worktrees
        let wt_names = repo.worktrees()?;
        for wt_name in wt_names.iter().flatten() {
            if let Ok(wt) = repo.find_worktree(wt_name) {
                let wt_path = wt.path().to_path_buf();
                let branch = Self::get_worktree_branch(&wt_path);
                worktrees.push(WorktreeInfo {
                    name: wt_name.to_string(),
                    path: wt_path,
                    branch,
                    is_main: false,
                });
            }
        }

        Ok(worktrees)
    }

    fn get_worktree_branch(wt_path: &Path) -> Option<String> {
        let repo = Repository::open(wt_path).ok()?;
        let head = repo.head().ok()?;
        head.shorthand().map(String::from)
    }

    pub fn get_changed_files(&self, worktree_path: &Path) -> Result<Vec<FileStatus>, GitError> {
        let repo = Repository::open(worktree_path)?;
        let mut opts = StatusOptions::new();
        opts.include_untracked(true)
            .recurse_untracked_dirs(true)
            .include_ignored(false);

        let statuses = repo.statuses(Some(&mut opts))?;
        let mut files = Vec::new();

        for entry in statuses.iter() {
            let status = entry.status();
            let path = entry
                .path()
                .map(|p| PathBuf::from(p))
                .unwrap_or_default();

            let status_type = if status.is_wt_new() || status.is_index_new() {
                Some(FileStatusType::New)
            } else if status.is_wt_modified() || status.is_index_modified() {
                Some(FileStatusType::Modified)
            } else if status.is_wt_deleted() || status.is_index_deleted() {
                Some(FileStatusType::Deleted)
            } else if status.is_wt_renamed() || status.is_index_renamed() {
                Some(FileStatusType::Renamed)
            } else if status.is_ignored() {
                None
            } else {
                Some(FileStatusType::Untracked)
            };

            if let Some(st) = status_type {
                files.push(FileStatus { path, status: st });
            }
        }

        Ok(files)
    }

    pub fn get_file_content_at_head(
        &self,
        worktree_path: &Path,
        file_path: &Path,
    ) -> Result<String, GitError> {
        let repo = Repository::open(worktree_path)?;
        let head = repo.head()?;
        let tree = head.peel_to_tree()?;

        let relative_path = file_path
            .strip_prefix(worktree_path)
            .unwrap_or(file_path);

        let entry = tree.get_path(relative_path)?;
        let blob = repo.find_blob(entry.id())?;

        String::from_utf8(blob.content().to_vec())
            .map_err(|_| GitError::InvalidWorktree("File is not valid UTF-8".to_string()))
    }

    /// Get the worktrees directory path (e.g., "repo.worktrees/")
    pub fn worktrees_dir(&self) -> PathBuf {
        let repo_name = self
            .repo_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("repo");
        let parent = self.repo_path.parent().unwrap_or(&self.repo_path);
        parent.join(format!("{}.worktrees", repo_name))
    }

    /// Check if a branch exists locally
    pub fn branch_exists_local(&self, branch_name: &str) -> Result<bool, GitError> {
        let repo = self.open_repo()?;
        let exists = repo
            .find_branch(branch_name, git2::BranchType::Local)
            .is_ok();
        Ok(exists)
    }

    /// Check if a branch exists on remote
    pub fn branch_exists_remote(&self, branch_name: &str) -> Result<bool, GitError> {
        let repo = self.open_repo()?;
        // Check all remotes for the branch
        let remotes = repo.remotes()?;
        for remote_name in remotes.iter().flatten() {
            let remote_branch = format!("{}/{}", remote_name, branch_name);
            if repo
                .find_branch(&remote_branch, git2::BranchType::Remote)
                .is_ok()
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Create a new worktree
    ///
    /// - If branch exists locally, use it
    /// - If branch exists on remote, create tracking branch
    /// - Otherwise, create new branch from HEAD
    ///
    /// Worktree path: `{repo_parent}/{repo_name}.worktrees/{branch_name}/`
    pub fn create_worktree(&self, branch_name: &str) -> Result<WorktreeInfo, GitError> {
        let repo = self.open_repo()?;

        // Determine worktree path
        let worktrees_dir = self.worktrees_dir();
        let worktree_path = worktrees_dir.join(branch_name);

        // Check if worktree already exists
        if worktree_path.exists() {
            return Err(GitError::WorktreeExists(branch_name.to_string()));
        }

        // Create worktrees directory if it doesn't exist
        std::fs::create_dir_all(&worktrees_dir)?;

        let local_exists = self.branch_exists_local(branch_name)?;
        let remote_exists = self.branch_exists_remote(branch_name)?;

        if local_exists {
            // Use existing local branch
            let branch = repo.find_branch(branch_name, git2::BranchType::Local)?;
            let reference = branch.into_reference();
            repo.worktree(
                branch_name,
                &worktree_path,
                Some(git2::WorktreeAddOptions::new().reference(Some(&reference))),
            )?;
        } else if remote_exists {
            // Create local branch tracking remote
            // First find the remote branch
            let remotes = repo.remotes()?;
            let mut remote_ref = None;
            for remote_name in remotes.iter().flatten() {
                let remote_branch_name = format!("{}/{}", remote_name, branch_name);
                if let Ok(branch) =
                    repo.find_branch(&remote_branch_name, git2::BranchType::Remote)
                {
                    remote_ref = Some(branch.into_reference());
                    break;
                }
            }

            if let Some(reference) = remote_ref {
                let commit = reference.peel_to_commit()?;
                // Create local branch
                repo.branch(branch_name, &commit, false)?;
                let local_branch = repo.find_branch(branch_name, git2::BranchType::Local)?;
                let local_ref = local_branch.into_reference();
                repo.worktree(
                    branch_name,
                    &worktree_path,
                    Some(git2::WorktreeAddOptions::new().reference(Some(&local_ref))),
                )?;
            }
        } else {
            // Create new branch from HEAD
            let head = repo.head()?;
            let commit = head.peel_to_commit()?;
            repo.branch(branch_name, &commit, false)?;
            let branch = repo.find_branch(branch_name, git2::BranchType::Local)?;
            let reference = branch.into_reference();
            repo.worktree(
                branch_name,
                &worktree_path,
                Some(git2::WorktreeAddOptions::new().reference(Some(&reference))),
            )?;
        }

        tracing::info!(
            "Created worktree '{}' at {:?}",
            branch_name,
            worktree_path
        );

        Ok(WorktreeInfo {
            name: branch_name.to_string(),
            path: worktree_path,
            branch: Some(branch_name.to_string()),
            is_main: false,
        })
    }

    /// Remove a worktree
    ///
    /// This removes the worktree directory and prunes it from git
    pub fn remove_worktree(&self, worktree_name: &str) -> Result<(), GitError> {
        let repo = self.open_repo()?;

        // Find the worktree
        let worktree = repo.find_worktree(worktree_name).map_err(|_| {
            GitError::InvalidWorktree(format!("Worktree '{}' not found", worktree_name))
        })?;

        let worktree_path = worktree.path().to_path_buf();

        // Check if worktree is locked
        if let Ok(git2::WorktreeLockStatus::Locked(_)) = worktree.is_locked() {
            return Err(GitError::InvalidWorktree(format!(
                "Worktree '{}' is locked",
                worktree_name
            )));
        }

        // Remove the worktree directory
        if worktree_path.exists() {
            std::fs::remove_dir_all(&worktree_path)?;
        }

        // Prune worktree from git
        worktree.prune(Some(
            git2::WorktreePruneOptions::new()
                .valid(true)
                .working_tree(true),
        ))?;

        tracing::info!("Removed worktree '{}' at {:?}", worktree_name, worktree_path);

        Ok(())
    }

    /// Get repository path
    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();

        // Create initial commit
        let sig = repo.signature().unwrap_or_else(|_| {
            git2::Signature::now("Test", "test@test.com").unwrap()
        });
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .unwrap();

        dir
    }

    #[test]
    fn test_open_repository() {
        let dir = setup_test_repo();
        let manager = GitManager::open(dir.path());
        assert!(manager.is_ok());
    }

    #[test]
    fn test_list_worktrees() {
        let dir = setup_test_repo();
        let manager = GitManager::open(dir.path()).unwrap();
        let worktrees = manager.list_worktrees().unwrap();

        assert!(!worktrees.is_empty());
        assert!(worktrees[0].is_main);
    }

    #[test]
    fn test_get_changed_files_empty() {
        let dir = setup_test_repo();
        let manager = GitManager::open(dir.path()).unwrap();
        let files = manager.get_changed_files(dir.path()).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_get_changed_files_with_new_file() {
        let dir = setup_test_repo();

        // Create a new file
        std::fs::write(dir.path().join("new_file.txt"), "content").unwrap();

        let manager = GitManager::open(dir.path()).unwrap();
        let files = manager.get_changed_files(dir.path()).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, PathBuf::from("new_file.txt"));
    }

    #[test]
    fn test_worktree_info_display_name() {
        let main_wt = WorktreeInfo {
            name: "main".to_string(),
            path: PathBuf::from("/tmp/repo"),
            branch: Some("master".to_string()),
            is_main: true,
        };
        assert_eq!(main_wt.display_name(), "main");

        let feature_wt = WorktreeInfo {
            name: "feature-x".to_string(),
            path: PathBuf::from("/tmp/repo-feature-x"),
            branch: Some("feature-x".to_string()),
            is_main: false,
        };
        assert_eq!(feature_wt.display_name(), "feature-x");
    }
}
