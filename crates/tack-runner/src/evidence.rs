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

pub mod checked_files;

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
    /// The commit `base_revision` resolved to in the checkout.
    pub base_commit: String,
    pub head_commit: String,
    /// The commit the work tree amounts to: `head_commit` when nothing is
    /// staged on top of it, otherwise a commit of the staged tree whose parent
    /// it is. The patch is `base_commit..captured_commit`, the verifier
    /// reads it, and publishing pushes it. Empty without a repository to
    /// commit to.
    pub captured_commit: String,
    /// The staged diff against `HEAD` is non-empty: the harness left work
    /// that it did not commit.
    pub worktree_dirty: bool,
    pub files: Vec<FileChange>,
    pub patch: Vec<u8>,
    pub truncated: bool,
    /// `files.json` of a folder without git: the changed entries as
    /// `{path, op, size, mtime, sha256}`. `Some` means there is no patch.
    pub snapshot: Option<Vec<u8>>,
}

/// The branch the runner published for an attempt: the value of
/// `evidence.json.branch` and of the completion report's `git` entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedBranch {
    pub branch: String,
    pub head_commit: String,
    pub pushed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchManifest {
    pub sha256: String,
    pub size_bytes: u64,
    pub truncated: bool,
}

/// `evidence.json`. `brief` is the request's brief, or `null` when the item had
/// none; `branch` is `null` unless the runner pushed a branch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttemptEvidence {
    pub v: String,
    pub attempt_id: String,
    pub harness_kind: String,
    pub captured: bool,
    pub reason: Option<String>,
    pub base_commit: String,
    /// The `base_revision` the request named, as asked.
    #[serde(default)]
    pub base_revision_requested: String,
    pub head_commit: Option<String>,
    pub worktree_dirty: Option<bool>,
    pub files: Vec<FileChange>,
    pub patch: Option<PatchManifest>,
    pub brief: Option<serde_json::Value>,
    pub branch: Option<serde_json::Value>,
    pub terminal_reason: serde_json::Value,
    pub usage: serde_json::Value,
}

/// The file the Planner profile writes at the workspace root.
pub const PLAN_FILE: &str = "tack-plan.json";
/// Written beside `src/` in the scratch directory (never staged) when
/// [`PLAN_FILE`] exists but does not parse; the engine reads it into the
/// `attempt.plan_invalid` event.
pub const PLAN_INVALID_FILE: &str = "plan-invalid.txt";
/// Written beside `src/` in the scratch directory (never staged) when the
/// capture made a commit of the work: two lines, the commit that was verified
/// and the `HEAD` it was made on. The verifier and the publish read it.
pub const CAPTURED_COMMIT_FILE: &str = "captured-commit.txt";

/// `tack-plan.json`, the shape pinned by `docs/contracts/plan-v1/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanV1 {
    pub v: String,
    pub subtasks: Vec<PlanSubtask>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanSubtask {
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub acceptance: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimate: Option<String>,
}

/// The steps of a run that recorded them (`terminal_reason.steps`), staged as one artifact.
pub const TRAJECTORY_FILE: &str = "trajectory.json";
/// Why `captured` is false when git could not read the workspace.
pub const EVIDENCE_REASON_GIT_UNREADABLE: &str = "git could not read the workspace";
/// Why `captured` is false when the provisioner has no repository to read.
pub const EVIDENCE_REASON_NO_REPOSITORY: &str = "this worktree provisioner reads no repository";

/// Captures and stages the evidence of one attempt. Returns the scratch
/// directory holding the staged files (the caller removes it once they are
/// uploaded) and one JSON object per staged artifact, in the shape
/// `terminal_reason.artifact` already has.
///
/// `base_commit` is the commit id fixed at provisioning, and `commit` the
/// author and message of the commit the capture makes of the work. With a
/// `head`, the workspace is a repository with no work tree: the evidence is
/// the difference from `base_commit` to that commit, `commit` is not used, and
/// the plan file is not looked for.
#[allow(clippy::too_many_arguments)]
pub async fn capture<P: WorktreeProvisioner>(
    workspaces: &WorkspaceManager<P>,
    workspace: &Workspace,
    harness_kind: String,
    terminal_reason: serde_json::Value,
    usage: serde_json::Value,
    brief: Option<&serde_json::Value>,
    base_commit: Option<&str>,
    commit: (&str, &str),
    head: Option<&str>,
) -> (PathBuf, Vec<serde_json::Value>) {
    let attempt_id = workspace.attempt_id.as_str();
    // An install the runner prepared is not the agent's work, whether or not the
    // project ignores it.
    let mut exclude = vec![RUNNER_DIR, PLAN_FILE];
    if head.is_none() {
        exclude.extend(crate::prepare::prepared_folders(&workspace.path));
    }
    let captured = match (head, base_commit) {
        (Some(head), Some(base_commit)) => {
            workspaces
                .capture_range(workspace, &exclude, base_commit, head)
                .await
        }
        // A range with no base was never recorded: nothing to read.
        (Some(_), None) => Err(crate::client::workspace::WorkspaceError::Git),
        (None, _) => {
            workspaces
                .capture_evidence(workspace, &exclude, base_commit, commit)
                .await
        }
    };
    let (git, reason) = match captured {
        Ok(Some(git)) => (Some(git), None),
        Ok(None) => (None, Some(EVIDENCE_REASON_NO_REPOSITORY)),
        Err(_) => (None, Some(EVIDENCE_REASON_GIT_UNREADABLE)),
    };
    let evidence = AttemptEvidence {
        v: "1".to_owned(),
        attempt_id: attempt_id.to_owned(),
        harness_kind,
        captured: git.is_some(),
        reason: reason.map(str::to_owned),
        base_commit: git.as_ref().map_or_else(
            || workspace.base_revision.clone(),
            |git| git.base_commit.clone(),
        ),
        base_revision_requested: workspace.base_revision.clone(),
        head_commit: git
            .as_ref()
            .filter(|git| git.snapshot.is_none())
            .map(|git| git.head_commit.clone()),
        worktree_dirty: git.as_ref().map(|git| git.worktree_dirty),
        files: git
            .as_ref()
            .map(|git| git.files.clone())
            .unwrap_or_default(),
        patch: git
            .as_ref()
            .filter(|git| git.snapshot.is_none())
            .map(|git| PatchManifest {
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
    if let Some(git) = git.as_ref().filter(|git| !git.captured_commit.is_empty()) {
        let _ = fs::write(
            scratch.join(CAPTURED_COMMIT_FILE),
            format!("{}\n{}\n", git.captured_commit, git.head_commit),
        );
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
        if let Some(snapshot) = &git.snapshot {
            stage("files.json", "snapshot", "application/json", snapshot);
        } else {
            stage("changes.patch", "patch", "text/x-diff", &git.patch);
            let files = serde_json::to_vec(&git.files).unwrap_or_default();
            stage("files.json", "files", "application/json", &files);
        }
    }
    if let Some(brief) = brief {
        let bytes = serde_json::to_vec_pretty(brief).unwrap_or_default();
        stage("brief.json", "brief", "application/json", &bytes);
    }
    // What each step of the run was made of, as the harness recorded it: the verifier is given
    // this file's address and digest as the attempt's trajectory.
    if let Some(steps) = evidence.terminal_reason["steps"]
        .as_array()
        .filter(|steps| !steps.is_empty())
    {
        let bytes = serde_json::to_vec_pretty(steps).unwrap_or_default();
        stage(TRAJECTORY_FILE, "trajectory", "application/json", &bytes);
    }
    // Read after capture, from the workspace that is about to be deleted. A
    // missing file is not a failure.
    let plan = head
        .is_none()
        .then(|| fs::read(workspace.path.join(PLAN_FILE)).ok())
        .flatten();
    if let Some(bytes) = plan {
        let parsed = serde_json::from_slice::<PlanV1>(&bytes)
            .map_err(|error| error.to_string())
            .and_then(|plan| {
                if plan.v == "1" {
                    Ok(())
                } else {
                    Err(format!("unsupported plan version {:?}", plan.v))
                }
            });
        match parsed {
            Ok(()) => stage(PLAN_FILE, "plan", "application/vnd.tack.plan+json", &bytes),
            Err(reason) => {
                let _ = fs::write(scratch.join(PLAN_INVALID_FILE), reason);
            }
        }
    }
    let manifest = serde_json::to_vec_pretty(&evidence).unwrap_or_default();
    stage("evidence.json", "evidence", "application/json", &manifest);
    (scratch, staged)
}

#[cfg(test)]
#[path = "evidence/tests.rs"]
mod tests;
