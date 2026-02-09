use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Local;

use crate::error::SyncroError;

#[derive(Debug, Clone)]
pub struct RepoStatus {
    pub path: PathBuf,
    pub branch: String,
    pub modified: usize,
    pub untracked: usize,
    pub deleted: usize,
    pub unpushed_commits: Vec<String>,
    pub has_remote: bool,
    pub branch_on_remote: bool,
    pub error: Option<String>,
}

impl RepoStatus {
    pub fn is_clean(&self) -> bool {
        self.modified == 0
            && self.untracked == 0
            && self.deleted == 0
            && self.unpushed_commits.is_empty()
    }

    pub fn has_changes(&self) -> bool {
        self.modified > 0 || self.untracked > 0 || self.deleted > 0
    }

    pub fn summary(&self) -> String {
        if self.error.is_some() {
            return "error".into();
        }
        if self.is_clean() {
            return "clean".into();
        }

        let mut parts = Vec::new();
        if self.modified > 0 {
            parts.push(format!("{} modified", self.modified));
        }
        if self.untracked > 0 {
            parts.push(format!("{} untracked", self.untracked));
        }
        if self.deleted > 0 {
            parts.push(format!("{} deleted", self.deleted));
        }
        if !self.unpushed_commits.is_empty() {
            parts.push(format!("{} unpushed", self.unpushed_commits.len()));
        }
        parts.join(", ")
    }

    pub fn display_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.display().to_string())
    }
}

fn run_git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(["-C", &repo.display().to_string()])
        .args(args)
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn repo_status(path: &Path) -> RepoStatus {
    let mut status = RepoStatus {
        path: path.to_path_buf(),
        branch: String::new(),
        modified: 0,
        untracked: 0,
        deleted: 0,
        unpushed_commits: Vec::new(),
        has_remote: false,
        branch_on_remote: false,
        error: None,
    };

    // Get current branch
    match run_git(path, &["rev-parse", "--abbrev-ref", "HEAD"]) {
        Ok(branch) => status.branch = branch,
        Err(e) => {
            status.error = Some(e);
            return status;
        }
    }

    // Parse porcelain status
    match run_git(path, &["status", "--porcelain"]) {
        Ok(output) => {
            for line in output.lines() {
                if line.len() < 2 {
                    continue;
                }
                let code = &line[..2];
                match code {
                    s if s.contains('?') => status.untracked += 1,
                    s if s.contains('D') => status.deleted += 1,
                    _ => status.modified += 1,
                }
            }
        }
        Err(e) => {
            status.error = Some(e);
            return status;
        }
    }

    // Check for remote
    status.has_remote = run_git(path, &["remote"])
        .map(|r| !r.is_empty())
        .unwrap_or(false);

    if status.has_remote {
        // Check unpushed commits
        let upstream_ref = format!("@{{u}}..HEAD");
        if let Ok(output) = run_git(path, &["log", &upstream_ref, "--oneline"]) {
            if !output.is_empty() {
                status.unpushed_commits = output.lines().map(String::from).collect();
            }
        }

        // Check if branch exists on remote
        let remote_branch = format!("origin/{}", status.branch);
        status.branch_on_remote = run_git(path, &["rev-parse", "--verify", &remote_branch])
            .is_ok();
    }

    status
}

pub struct SyncResult {
    pub path: PathBuf,
    pub committed: bool,
    pub pushed: bool,
    pub error: Option<String>,
}

pub fn sync_repo(path: &Path) -> Result<SyncResult, SyncroError> {
    let repo_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());

    let mut result = SyncResult {
        path: path.to_path_buf(),
        committed: false,
        pushed: false,
        error: None,
    };

    // Stage all changes
    if let Err(e) = run_git(path, &["add", "-A"]) {
        return Err(SyncroError::Git {
            repo: repo_name,
            message: format!("git add failed: {e}"),
        });
    }

    // Commit
    let date = Local::now().format("%Y-%m-%d").to_string();
    let msg = format!("syncro update {date}");
    match run_git(path, &["commit", "-m", &msg]) {
        Ok(_) => result.committed = true,
        Err(e) => {
            if !e.contains("nothing to commit") {
                return Err(SyncroError::Git {
                    repo: repo_name,
                    message: format!("git commit failed: {e}"),
                });
            }
        }
    }

    // Push if remote exists
    let has_remote = run_git(path, &["remote"])
        .map(|r| !r.is_empty())
        .unwrap_or(false);

    if has_remote {
        // Try normal push first, fall back to --set-upstream
        match run_git(path, &["push"]) {
            Ok(_) => result.pushed = true,
            Err(_) => {
                let branch = run_git(path, &["rev-parse", "--abbrev-ref", "HEAD"]).map_err(
                    |e| SyncroError::Git {
                        repo: repo_name.clone(),
                        message: e,
                    },
                )?;
                match run_git(path, &["push", "--set-upstream", "origin", &branch]) {
                    Ok(_) => result.pushed = true,
                    Err(e) => {
                        result.error = Some(format!("push failed: {e}"));
                    }
                }
            }
        }
    }

    Ok(result)
}
