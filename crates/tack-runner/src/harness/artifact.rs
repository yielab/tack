//! Workspace-confined artifact staging.
//!
//! An adapter finishes a run with some output worth keeping (a patch, a log,
//! a generated file) that the `docs/contracts/runner-v1/artifact.request.json`
//! shape (`artifact_id`, `kind`, `name`, `media_type`, `size_bytes`,
//! `sha256`, ...) expects to eventually upload. `PullProtocol` has no
//! artifact-upload method yet (the same known C3 gap as event batching), so
//! this module is the local half only: copy the named file out of the
//! attempt's own workspace into a dedicated, owner-only, attempt-scoped
//! staging directory, with a real checksum computed along the way, ready for
//! whichever future card wires the upload transport.
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
    #[error("artifact staging root is unsafe")]
    UnsafeStagingRoot,
    #[error("artifact source path escapes its declared workspace")]
    WorkspaceEscape,
    #[error("artifact source does not exist or is not a regular file")]
    SourceUnavailable,
    #[error("artifact staging operation failed")]
    Io,
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
            .map_err(|_| ArtifactError::WorkspaceEscape)?;
        let candidate = root.join(source_relative);
        // Inspect the requested path *before* resolving it — exactly as
        // `workspace.rs`'s cleanup does, and for the same reason:
        // `canonicalize` follows symlinks, so checking `is_symlink()` on the
        // already-resolved path would just inspect the final real target and
        // never see that a symlink was involved at all.
        let symlink_check =
            fs::symlink_metadata(&candidate).map_err(|_| ArtifactError::SourceUnavailable)?;
        if symlink_check.file_type().is_symlink() {
            return Err(ArtifactError::SourceUnavailable);
        }
        let resolved = candidate
            .canonicalize()
            .map_err(|_| ArtifactError::SourceUnavailable)?;
        if resolved != root && !resolved.starts_with(&root) {
            return Err(ArtifactError::WorkspaceEscape);
        }
        let metadata = fs::metadata(&resolved).map_err(|_| ArtifactError::SourceUnavailable)?;
        if !metadata.is_file() {
            return Err(ArtifactError::SourceUnavailable);
        }

        let name = source_relative
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(ArtifactError::SourceUnavailable)?
            .to_owned();
        let bytes = fs::read(&resolved).map_err(|_| ArtifactError::Io)?;
        let sha256 = sha256_hex(&bytes);

        let attempt_dir = self.ensure_attempt_dir(attempt_id)?;
        let staged_path = attempt_dir.join(format!("{}-{name}", &sha256[..16]));
        fs::write(&staged_path, &bytes).map_err(|_| ArtifactError::Io)?;
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
        fs::create_dir_all(&self.staging_root).map_err(|_| ArtifactError::UnsafeStagingRoot)?;
        owner_only_dir(&self.staging_root)?;
        // Hex-encoded so an attempt id can never traverse outside its own
        // directory, mirroring the same encoding `journal.rs`/`workspace.rs`
        // already use for the same reason.
        let attempt_dir = self.staging_root.join(encode_id(attempt_id));
        fs::create_dir_all(&attempt_dir).map_err(|_| ArtifactError::Io)?;
        owner_only_dir(&attempt_dir)?;
        Ok(attempt_dir)
    }
}

fn encode_id(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(unix)]
fn owner_only_dir(path: &Path) -> Result<(), ArtifactError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|_| ArtifactError::Io)
}

#[cfg(not(unix))]
fn owner_only_dir(_path: &Path) -> Result<(), ArtifactError> {
    Ok(())
}

#[cfg(unix)]
fn owner_only_file(path: &Path) -> Result<(), ArtifactError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|_| ArtifactError::Io)
}

#[cfg(not(unix))]
fn owner_only_file(_path: &Path) -> Result<(), ArtifactError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "tack-runner-artifact-{label}-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    #[test]
    fn stages_a_file_with_a_correct_checksum_and_size() {
        let workspace = temp_dir("workspace");
        let staging = temp_dir("staging");
        fs::write(workspace.join("changes.patch"), b"diff --git a b\n").expect("write source");

        let stager = ArtifactStager::new(&staging);
        let staged = stager
            .stage_file(
                "attempt-one",
                &workspace,
                Path::new("changes.patch"),
                "patch",
                "text/x-diff",
            )
            .expect("stage file");

        assert_eq!(staged.name, "changes.patch");
        assert_eq!(staged.size_bytes, b"diff --git a b\n".len() as u64);
        assert_eq!(staged.sha256, sha256_hex(b"diff --git a b\n"));
        assert_eq!(
            fs::read(&staged.staged_path).expect("read staged file"),
            b"diff --git a b\n"
        );
        fs::remove_dir_all(workspace).expect("cleanup");
        fs::remove_dir_all(staging).expect("cleanup");
    }

    #[test]
    fn refuses_a_source_outside_its_workspace_root() {
        let workspace = temp_dir("workspace-escape");
        let staging = temp_dir("staging-escape");
        let outside = temp_dir("outside");
        fs::write(outside.join("secret.txt"), b"not part of this workspace").expect("write");

        let stager = ArtifactStager::new(&staging);
        let escaping_relative = Path::new("..")
            .join(
                outside
                    .file_name()
                    .expect("outside dir name")
                    .to_str()
                    .expect("utf8"),
            )
            .join("secret.txt");

        let result = stager.stage_file(
            "attempt-one",
            &workspace,
            &escaping_relative,
            "log",
            "text/plain",
        );
        assert!(matches!(result, Err(ArtifactError::WorkspaceEscape)));
        fs::remove_dir_all(workspace).expect("cleanup");
        fs::remove_dir_all(staging).expect("cleanup");
        fs::remove_dir_all(outside).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_symlinked_source() {
        use std::os::unix::fs::symlink;

        let workspace = temp_dir("workspace-symlink");
        let staging = temp_dir("staging-symlink");
        let outside = temp_dir("outside-symlink");
        fs::write(outside.join("real.txt"), b"outside content").expect("write outside file");
        symlink(outside.join("real.txt"), workspace.join("link.txt")).expect("symlink");

        let stager = ArtifactStager::new(&staging);
        let result = stager.stage_file(
            "attempt-one",
            &workspace,
            Path::new("link.txt"),
            "log",
            "text/plain",
        );
        assert!(matches!(result, Err(ArtifactError::SourceUnavailable)));
        fs::remove_dir_all(workspace).expect("cleanup");
        fs::remove_dir_all(staging).expect("cleanup");
        fs::remove_dir_all(outside).expect("cleanup");
    }

    /// Reinforces "adapters cannot cross-read each other's workspaces" on the
    /// staging side: two attempts staging a same-named file land in
    /// independent directories with independent content, never colliding.
    #[test]
    fn distinct_attempts_get_isolated_staging_directories() {
        let workspace_a = temp_dir("workspace-a");
        let workspace_b = temp_dir("workspace-b");
        let staging = temp_dir("staging-shared");
        fs::write(workspace_a.join("out.txt"), b"attempt-a-content").expect("write a");
        fs::write(workspace_b.join("out.txt"), b"attempt-b-content").expect("write b");

        let stager = ArtifactStager::new(&staging);
        let staged_a = stager
            .stage_file(
                "attempt-a",
                &workspace_a,
                Path::new("out.txt"),
                "log",
                "text/plain",
            )
            .expect("stage a");
        let staged_b = stager
            .stage_file(
                "attempt-b",
                &workspace_b,
                Path::new("out.txt"),
                "log",
                "text/plain",
            )
            .expect("stage b");

        assert_ne!(staged_a.staged_path, staged_b.staged_path);
        assert_eq!(
            fs::read(&staged_a.staged_path).unwrap(),
            b"attempt-a-content"
        );
        assert_eq!(
            fs::read(&staged_b.staged_path).unwrap(),
            b"attempt-b-content"
        );
        fs::remove_dir_all(workspace_a).expect("cleanup");
        fs::remove_dir_all(workspace_b).expect("cleanup");
        fs::remove_dir_all(staging).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn staged_files_and_directories_are_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let workspace = temp_dir("workspace-perms");
        let staging = temp_dir("staging-perms");
        fs::write(workspace.join("out.txt"), b"content").expect("write");

        let stager = ArtifactStager::new(&staging);
        let staged = stager
            .stage_file(
                "attempt-one",
                &workspace,
                Path::new("out.txt"),
                "log",
                "text/plain",
            )
            .expect("stage");

        let file_mode = fs::metadata(&staged.staged_path)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(file_mode & 0o077, 0, "staged file is owner-only");
        let dir_mode = fs::metadata(staged.staged_path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(
            dir_mode & 0o077,
            0,
            "attempt staging directory is owner-only"
        );
        fs::remove_dir_all(workspace).expect("cleanup");
        fs::remove_dir_all(staging).expect("cleanup");
    }
}
