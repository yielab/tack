//! Looking at a folder and making one, for the screens that point an agent at code. These run
//! `git` in this process, so they only make sense when the embedded runner (same machine) is
//! configured, and only for absolute paths.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// What was found at a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderFacts {
    pub exists: bool,
    pub is_dir: bool,
    pub is_git: bool,
    pub branch: Option<String>,
    pub remote_url: Option<String>,
    pub dirty_files: usize,
    /// The install command the checked-out commit's lockfile implies, when it has one.
    pub install: Option<DetectedInstall>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedInstall {
    pub command: String,
    pub lockfile: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FolderError {
    #[error("The embedded runner is not configured")]
    Unavailable,
    #[error("path must be absolute")]
    NotAbsolute,
    #[error("The path exists and is not an empty directory")]
    NotEmpty,
    #[error("Could not create the directory")]
    CouldNotCreate,
    #[error("git init failed")]
    GitInitFailed,
}

/// The path to act on: refused when no embedded runner is configured, then when it is not
/// absolute.
pub fn folder_path(runner_configured: bool, raw: &str) -> Result<PathBuf, FolderError> {
    if !runner_configured {
        return Err(FolderError::Unavailable);
    }
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(FolderError::NotAbsolute);
    }
    Ok(path)
}

/// Runs `git` in `dir`; `Some(stdout)` only on a zero exit within the timeout.
async fn run_git(dir: &Path, args: &[&str]) -> Option<String> {
    let mut command = tokio::process::Command::new("git");
    command
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(10), command.output())
        .await
        .ok()?
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// What is at `path`; writes nothing.
pub async fn check_folder(path: &Path) -> FolderFacts {
    let exists = tokio::fs::try_exists(path).await.unwrap_or(false);
    let is_dir = exists && tokio::fs::metadata(path).await.is_ok_and(|m| m.is_dir());
    let is_git = is_dir
        && run_git(path, &["rev-parse", "--is-inside-work-tree"])
            .await
            .is_some_and(|out| out == "true");
    let mut facts = FolderFacts {
        exists,
        is_dir,
        is_git,
        branch: None,
        remote_url: None,
        dirty_files: 0,
        install: None,
    };
    if is_git {
        facts.branch = run_git(path, &["rev-parse", "--abbrev-ref", "HEAD"]).await;
        facts.remote_url = run_git(path, &["remote", "get-url", "origin"]).await;
        facts.dirty_files = run_git(path, &["status", "--porcelain"])
            .await
            .map_or(0, |out| out.lines().count());
        let root_files = run_git(path, &["ls-tree", "--name-only", "HEAD"])
            .await
            .unwrap_or_default();
        let root_files: Vec<&str> = root_files.lines().collect();
        facts.install = tack_protocol::prepare::detect(&root_files).map(|found| DetectedInstall {
            command: found.command.to_owned(),
            lockfile: found.lockfile.to_owned(),
        });
    }
    facts
}

/// Creates `path` and runs `git init`. An existing non-empty directory (or a file) is refused.
pub async fn init_folder(path: &Path) -> Result<(), FolderError> {
    if tokio::fs::try_exists(path).await.unwrap_or(false) {
        let empty_dir = match tokio::fs::read_dir(path).await {
            Ok(mut entries) => entries.next_entry().await.is_ok_and(|e| e.is_none()),
            Err(_) => false,
        };
        if !empty_dir {
            return Err(FolderError::NotEmpty);
        }
    }
    tokio::fs::create_dir_all(path)
        .await
        .map_err(|_| FolderError::CouldNotCreate)?;
    run_git(path, &["init"])
        .await
        .map(drop)
        .ok_or(FolderError::GitInitFailed)
}
