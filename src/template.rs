//! Session template configuration
//!
//! Defines what happens when a new worktree/session is created:
//! - Pre-create commands (run in the main worktree before creation)
//! - File copies (glob patterns copied from main worktree to new)
//! - Post-create commands (run in the new worktree after creation)
//! - Working directory (relative to worktree root)
//!
//! Configuration is stored in git config under `[sashiki "template"]`.

use crate::git::{self, GitRepo};
use std::path::{Path, PathBuf};

/// Session template configuration loaded from git config
#[derive(Debug, Clone, Default)]
pub struct TemplateConfig {
    /// Commands to run before worktree creation (in main worktree)
    pub pre_create_commands: Vec<String>,
    /// Glob patterns for files to copy from main worktree
    pub file_copies: Vec<String>,
    /// Commands to run after worktree creation (in new worktree)
    pub post_create_commands: Vec<String>,
    /// Working directory relative to worktree root (for terminal and post-create commands)
    pub working_directory: Option<String>,
}

impl TemplateConfig {
    /// Load template config from git config
    pub fn load(repo: &GitRepo) -> Self {
        Self {
            pre_create_commands: repo.get_config_values(git::CONFIG_PRE_CREATE_CMD),
            file_copies: repo.get_config_values(git::CONFIG_FILE_COPY),
            post_create_commands: repo.get_config_values(git::CONFIG_POST_CREATE_CMD),
            working_directory: repo.get_config_value(git::CONFIG_WORKING_DIR),
        }
    }

    /// Save template config to local git config
    pub fn save(&self, repo: &GitRepo) -> git::Result<()> {
        repo.set_config_values(git::CONFIG_PRE_CREATE_CMD, &self.pre_create_commands)?;
        repo.set_config_values(git::CONFIG_FILE_COPY, &self.file_copies)?;
        repo.set_config_values(git::CONFIG_POST_CREATE_CMD, &self.post_create_commands)?;

        if let Some(ref dir) = self.working_directory {
            if !dir.is_empty() {
                repo.set_config_value(git::CONFIG_WORKING_DIR, dir)?;
            } else {
                repo.remove_config_key(git::CONFIG_WORKING_DIR)?;
            }
        } else {
            repo.remove_config_key(git::CONFIG_WORKING_DIR)?;
        }

        Ok(())
    }

    /// Check if template has any configured actions
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.pre_create_commands.is_empty()
            && self.file_copies.is_empty()
            && self.post_create_commands.is_empty()
    }

    /// Resolve the effective working directory for a worktree
    pub fn resolve_working_directory(&self, worktree_path: &Path) -> PathBuf {
        match &self.working_directory {
            Some(dir) if !dir.is_empty() => worktree_path.join(dir),
            _ => worktree_path.to_path_buf(),
        }
    }

    /// Build the list of creation steps for progress display
    pub fn creation_steps(&self) -> Vec<String> {
        let mut steps = Vec::new();

        for cmd in &self.pre_create_commands {
            steps.push(cmd.clone());
        }

        steps.push("Creating worktree".to_string());

        if !self.file_copies.is_empty() {
            steps.push("Copying files".to_string());
        }

        for cmd in &self.post_create_commands {
            steps.push(cmd.clone());
        }

        steps
    }

    /// Copy files matching glob patterns from source to destination worktree
    pub fn copy_files(&self, source_root: &Path, dest_root: &Path) -> Vec<FileCopyResult> {
        let mut results = Vec::new();

        for pattern in &self.file_copies {
            let full_pattern = source_root.join(pattern).to_string_lossy().to_string();

            match glob::glob(&full_pattern) {
                Ok(paths) => {
                    let mut matched = false;
                    for entry in paths {
                        match entry {
                            Ok(src_path) if src_path.is_file() => {
                                matched = true;
                                let result = copy_single_file(source_root, dest_root, &src_path);
                                results.push(result);
                            }
                            Ok(_) => {} // skip directories
                            Err(e) => {
                                results.push(FileCopyResult {
                                    path: pattern.clone(),
                                    success: false,
                                    error: Some(format!("Glob error: {}", e)),
                                });
                            }
                        }
                    }
                    if !matched {
                        // Pattern matched no files - not an error, just skip
                    }
                }
                Err(e) => {
                    results.push(FileCopyResult {
                        path: pattern.clone(),
                        success: false,
                        error: Some(format!("Invalid pattern '{}': {}", pattern, e)),
                    });
                }
            }
        }

        results
    }
}

/// Result of a single file copy operation
#[derive(Debug, Clone)]
pub struct FileCopyResult {
    pub path: String,
    pub success: bool,
    pub error: Option<String>,
}

/// Copy a single file from source worktree to destination worktree
fn copy_single_file(source_root: &Path, dest_root: &Path, src_path: &Path) -> FileCopyResult {
    let relative = match src_path.strip_prefix(source_root) {
        Ok(r) => r,
        Err(_) => {
            return FileCopyResult {
                path: src_path.to_string_lossy().to_string(),
                success: false,
                error: Some("Failed to determine relative path".to_string()),
            };
        }
    };

    let dest_path = dest_root.join(relative);
    let rel_str = relative.to_string_lossy().to_string();

    // Don't overwrite existing files
    if dest_path.exists() {
        return FileCopyResult {
            path: rel_str,
            success: true,
            error: None,
        };
    }

    // Create parent directories
    if let Some(parent) = dest_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return FileCopyResult {
                path: rel_str,
                success: false,
                error: Some(format!("Failed to create directory: {}", e)),
            };
        }
    }

    match std::fs::copy(src_path, &dest_path) {
        Ok(_) => FileCopyResult {
            path: rel_str,
            success: true,
            error: None,
        },
        Err(e) => FileCopyResult {
            path: rel_str,
            success: false,
            error: Some(format!("Copy failed: {}", e)),
        },
    }
}

/// Run a shell command synchronously in the given working directory
pub fn run_shell_command(cmd: &str, workdir: &Path) -> std::result::Result<(), String> {
    #[cfg(unix)]
    let output = std::process::Command::new("sh")
        .args(["-c", cmd])
        .current_dir(workdir)
        .output();

    #[cfg(windows)]
    let output = std::process::Command::new("cmd")
        .args(["/C", cmd])
        .current_dir(workdir)
        .output();

    match output {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let msg = if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                format!("Command exited with status: {}", o.status)
            };
            Err(msg)
        }
        Err(e) => Err(e.to_string()),
    }
}
