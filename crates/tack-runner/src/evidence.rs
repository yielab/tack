//! What an attempt changed, captured in its workspace before the workspace is
//! deleted, for every harness kind.
//!
//! The workspace is a throwaway clone, so nothing the harness did to it
//! survives cleanup unless it is read out first. [`capture`] reads the diff
//! against the base revision through [`WorktreeProvisioner::capture_evidence`]
//! and stages up to four files from a scratch directory outside the workspace,
//! through [`ArtifactStager`], for the engine's ordinary manifest-then-content
//! upload: `changes.patch` (kind `patch`), `files.json` (`files`), `brief.json`
//! (`brief`, only when the request carried one) and `evidence.json`
//! (`evidence`, the shape pinned by `docs/contracts/evidence-v1/`). A capture that cannot happen still yields
//! `evidence.json`, with `captured: false` and a reason, and never changes the
//! attempt's outcome.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    client::workspace::{Workspace, WorkspaceManager, WorktreeProvisioner},
    harness::{artifact::ArtifactStager, sha256::sha256_hex},
};

/// Largest patch staged, in bytes. A longer diff is cut here and says so in
/// the manifest (`patch.truncated`) rather than silently.
pub const PATCH_CAP_BYTES: usize = 8 * 1024 * 1024;

/// The workspace-relative directory the runner writes its own run log into.
const RUNNER_DIR: &str = ".tack-runner";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileOp {
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub op: FileOp,
}

/// What git reported for one workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitEvidence {
    pub head_commit: String,
    /// The staged diff against `HEAD` is non-empty: the harness left work
    /// that it did not commit.
    pub worktree_dirty: bool,
    pub files: Vec<FileChange>,
    pub patch: Vec<u8>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchManifest {
    pub sha256: String,
    pub size_bytes: u64,
    pub truncated: bool,
}

/// `evidence.json`. `brief` is the request's brief, or `null` when the item had
/// none; `branch` is written by a later task and is `null` here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttemptEvidence {
    pub v: String,
    pub attempt_id: String,
    pub harness_kind: String,
    pub captured: bool,
    pub reason: Option<String>,
    pub base_commit: String,
    pub head_commit: Option<String>,
    pub worktree_dirty: Option<bool>,
    pub files: Vec<FileChange>,
    pub patch: Option<PatchManifest>,
    pub brief: Option<serde_json::Value>,
    pub branch: Option<serde_json::Value>,
    pub terminal_reason: serde_json::Value,
    pub usage: serde_json::Value,
}

/// Captures and stages the evidence of one attempt. Returns the scratch
/// directory holding the staged files (the caller removes it once they are
/// uploaded) and one JSON object per staged artifact, in the shape
/// `terminal_reason.artifact` already has.
pub async fn capture<P: WorktreeProvisioner>(
    workspaces: &WorkspaceManager<P>,
    workspace: &Workspace,
    harness_kind: String,
    terminal_reason: serde_json::Value,
    usage: serde_json::Value,
    brief: Option<&serde_json::Value>,
) -> (PathBuf, Vec<serde_json::Value>) {
    let attempt_id = workspace.attempt_id.as_str();
    let (git, reason) = match workspaces.capture_evidence(workspace, &[RUNNER_DIR]).await {
        Ok(Some(git)) => (Some(git), None),
        Ok(None) => (None, Some("this worktree provisioner reads no repository")),
        Err(_) => (None, Some("git could not read the workspace")),
    };
    let evidence = AttemptEvidence {
        v: "1".to_owned(),
        attempt_id: attempt_id.to_owned(),
        harness_kind,
        captured: git.is_some(),
        reason: reason.map(str::to_owned),
        base_commit: workspace.base_revision.clone(),
        head_commit: git.as_ref().map(|git| git.head_commit.clone()),
        worktree_dirty: git.as_ref().map(|git| git.worktree_dirty),
        files: git
            .as_ref()
            .map(|git| git.files.clone())
            .unwrap_or_default(),
        patch: git.as_ref().map(|git| PatchManifest {
            sha256: sha256_hex(&git.patch),
            size_bytes: git.patch.len() as u64,
            truncated: git.truncated,
        }),
        brief: brief.cloned(),
        branch: None,
        terminal_reason,
        usage,
    };

    // The scratch must live outside the workspace, which is about to be deleted.
    #[allow(clippy::disallowed_methods)]
    let scratch = std::env::temp_dir().join(format!(
        "tack-evidence-{}-{}",
        std::process::id(),
        attempt_id
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ));
    let _ = fs::remove_dir_all(&scratch);
    let source = scratch.join("src");
    let mut staged = Vec::new();
    if fs::create_dir_all(&source).is_err() {
        return (scratch, staged);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&scratch, fs::Permissions::from_mode(0o700));
    }
    let stager = ArtifactStager::new(scratch.join("staged"));
    let mut stage = |name: &str, kind: &str, media_type: &str, bytes: &[u8]| {
        if fs::write(source.join(name), bytes).is_err() {
            return;
        }
        match stager.stage_file(attempt_id, &source, Path::new(name), kind, media_type) {
            Ok(artifact) => staged.push(serde_json::json!({
                "kind": artifact.kind,
                "name": artifact.name,
                "media_type": artifact.media_type,
                "size_bytes": artifact.size_bytes,
                "sha256": artifact.sha256,
                "staged_path": artifact.staged_path.display().to_string(),
            })),
            Err(error) => tracing::warn!(?error, name, "evidence could not be staged"),
        }
    };
    if let Some(git) = &git {
        stage("changes.patch", "patch", "text/x-diff", &git.patch);
        let files = serde_json::to_vec(&git.files).unwrap_or_default();
        stage("files.json", "files", "application/json", &files);
    }
    if let Some(brief) = brief {
        let bytes = serde_json::to_vec_pretty(brief).unwrap_or_default();
        stage("brief.json", "brief", "application/json", &bytes);
    }
    let manifest = serde_json::to_vec_pretty(&evidence).unwrap_or_default();
    stage("evidence.json", "evidence", "application/json", &manifest);
    (scratch, staged)
}

#[cfg(test)]
#[path = "evidence/tests.rs"]
mod tests;
