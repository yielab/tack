//! Workspace-confined artifact staging.
//!
//! An adapter finishes a run with some output worth keeping (a patch, a log,
//! a generated file) that the `docs/contracts/runner-v1/artifact.request.json`
//! shape (`artifact_id`, `kind`, `name`, `media_type`, `size_bytes`,
//! `sha256`, ...) expects to eventually upload. `PullProtocol` has no
//! artifact-upload method yet (the same gap as event batching), so
//! this module is the local half only: copy the named file out of the
//! attempt's own workspace into a dedicated, owner-only, attempt-scoped
//! staging directory, with a real checksum computed along the way, ready for
//! whenever the upload transport is wired.
//!
//! Confinement mirrors `workspace.rs`'s cleanup guard for the same reason:
//! canonicalize before comparing, so a symlink or a `..` component cannot
//! make a file outside the workspace look like it belongs to it. This is the
//! artifact-side instance of "adapters cannot cross-read each other's
//! workspaces" — an adapter can only ever stage a file that is really inside
//! the workspace it was handed for *this* attempt.

use std::{
    fs,
    path::{Path, PathBuf},
};

use thiserror::Error;

use crate::error::IoKind;

use super::sha256::sha256_hex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedArtifact {
    pub kind: String,
    pub name: String,
    pub media_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub staged_path: PathBuf,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ArtifactError {
    /// Carries the kind of the failure only: the operating system's own text can name the path.
    #[error("artifact staging root is unsafe: {0}")]
    UnsafeStagingRoot(IoKind),
    #[error("artifact source path escapes its declared workspace")]
    WorkspaceEscape,
    #[error("artifact workspace could not be checked: {0}")]
    WorkspaceUnavailable(IoKind),
    #[error("artifact source does not exist or is not a regular file")]
    SourceUnavailable,
    #[error("artifact source could not be inspected: {0}")]
    SourceInspect(IoKind),
    #[error("artifact staging operation failed: {0}")]
    Io(IoKind),
}

/// Roots every staged artifact under `staging_root/<attempt_id>/...`, one
/// owner-only directory per attempt.
pub struct ArtifactStager {
    staging_root: PathBuf,
}

impl ArtifactStager {
    pub fn new(staging_root: impl Into<PathBuf>) -> Self {
        Self {
            staging_root: staging_root.into(),
        }
    }

    /// Stages `source_relative` (resolved against `workspace_root`) for
    /// `attempt_id`. Computes a real SHA-256 (see `sha256.rs` for why this is
    /// hand-rolled rather than a new dependency) and byte size directly from
    /// the bytes actually copied, never a value trusted from the harness.
    pub fn stage_file(
        &self,
        attempt_id: &str,
        workspace_root: &Path,
        source_relative: &Path,
        kind: &str,
        media_type: &str,
    ) -> Result<StagedArtifact, ArtifactError> {
        let root = workspace_root
            .canonicalize()
            .map_err(|error| ArtifactError::WorkspaceUnavailable(IoKind(error.kind())))?;
        let candidate = root.join(source_relative);
        // Inspect the requested path *before* resolving it — exactly as
        // `workspace.rs`'s cleanup does, and for the same reason:
        // `canonicalize` follows symlinks, so checking `is_symlink()` on the
        // already-resolved path would just inspect the final real target and
        // never see that a symlink was involved at all.
        let symlink_check = fs::symlink_metadata(&candidate)
            .map_err(|error| ArtifactError::SourceInspect(IoKind(error.kind())))?;
        if symlink_check.file_type().is_symlink() {
            return Err(ArtifactError::SourceUnavailable);
        }
        let resolved = candidate
            .canonicalize()
            .map_err(|error| ArtifactError::SourceInspect(IoKind(error.kind())))?;
        if resolved != root && !resolved.starts_with(&root) {
            return Err(ArtifactError::WorkspaceEscape);
        }
        let metadata = fs::metadata(&resolved)
            .map_err(|error| ArtifactError::SourceInspect(IoKind(error.kind())))?;
        if !metadata.is_file() {
            return Err(ArtifactError::SourceUnavailable);
        }

        let name = source_relative
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(ArtifactError::SourceUnavailable)?
            .to_owned();
        let bytes = fs::read(&resolved).map_err(|error| ArtifactError::Io(IoKind(error.kind())))?;
        let sha256 = sha256_hex(&bytes);

        let attempt_dir = self.ensure_attempt_dir(attempt_id)?;
        let staged_path = attempt_dir.join(format!("{}-{name}", &sha256[..16]));
        fs::write(&staged_path, &bytes).map_err(|error| ArtifactError::Io(IoKind(error.kind())))?;
        owner_only_file(&staged_path)?;

        Ok(StagedArtifact {
            kind: kind.to_owned(),
            name,
            media_type: media_type.to_owned(),
            size_bytes: bytes.len() as u64,
            sha256,
            staged_path,
        })
    }

    fn ensure_attempt_dir(&self, attempt_id: &str) -> Result<PathBuf, ArtifactError> {
        fs::create_dir_all(&self.staging_root)
            .map_err(|error| ArtifactError::UnsafeStagingRoot(IoKind(error.kind())))?;
        owner_only_dir(&self.staging_root)?;
        // Hex-encoded so an attempt id can never traverse outside its own
        // directory, mirroring the same encoding `journal.rs`/`workspace.rs`
        // already use for the same reason.
        let attempt_dir = self.staging_root.join(encode_id(attempt_id));
        fs::create_dir_all(&attempt_dir)
            .map_err(|error| ArtifactError::Io(IoKind(error.kind())))?;
        owner_only_dir(&attempt_dir)?;
        Ok(attempt_dir)
    }
}

pub(super) fn encode_id(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(unix)]
fn owner_only_dir(path: &Path) -> Result<(), ArtifactError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| ArtifactError::Io(IoKind(error.kind())))
}

#[cfg(not(unix))]
fn owner_only_dir(_path: &Path) -> Result<(), ArtifactError> {
    Ok(())
}

#[cfg(unix)]
fn owner_only_file(path: &Path) -> Result<(), ArtifactError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| ArtifactError::Io(IoKind(error.kind())))
}

#[cfg(not(unix))]
fn owner_only_file(_path: &Path) -> Result<(), ArtifactError> {
    Ok(())
}

#[cfg(test)]
#[path = "artifact/tests.rs"]
mod tests;
