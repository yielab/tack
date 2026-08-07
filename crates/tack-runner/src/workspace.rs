//! Deterministic, attempt-scoped worktree reservation and safe cleanup.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use thiserror::Error;

use super::{AttemptId, AttemptLease, RepositorySpec, WorkspaceId, journal::WorkspaceJournal};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub attempt_id: AttemptId,
    pub id: WorkspaceId,
    pub path: PathBuf,
    pub base_revision: String,
}

impl Workspace {
    pub fn journal(&self) -> WorkspaceJournal {
        WorkspaceJournal {
            workspace_id: self.id.clone(),
            path: self.path.clone(),
            base_revision: self.base_revision.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupResult {
    Deleted,
    Refused,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WorkspaceError {
    #[error("runner workspace root is unsafe")]
    UnsafeRoot,
    #[error("runner workspace path is unresolved or unsafe")]
    UnsafePath,
    #[error("runner workspace marker does not match its attempt")]
    AttemptMismatch,
    #[error("worktree provisioning is not configured")]
    WorktreeUnavailable,
    #[error("runner workspace operation failed")]
    Io,
}

/// The only boundary allowed to create a Git worktree. Tests inject a fake;
/// C3 does not pretend that an empty directory is a checked-out repository.
#[async_trait]
pub trait WorktreeProvisioner: Send + Sync {
    async fn provision(
        &self,
        workspace: &Workspace,
        repository: &RepositorySpec,
    ) -> Result<(), WorkspaceError>;
}

#[derive(Debug, Default)]
pub struct UnavailableWorktreeProvisioner;

#[async_trait]
impl WorktreeProvisioner for UnavailableWorktreeProvisioner {
    async fn provision(
        &self,
        _workspace: &Workspace,
        _repository: &RepositorySpec,
    ) -> Result<(), WorkspaceError> {
        Err(WorkspaceError::WorktreeUnavailable)
    }
}

pub struct WorkspaceManager<P> {
    root: PathBuf,
    provisioner: P,
}

impl<P> WorkspaceManager<P>
where
    P: WorktreeProvisioner,
{
    pub fn new(root: impl Into<PathBuf>, provisioner: P) -> Self {
        Self {
            root: root.into(),
            provisioner,
        }
    }

    pub async fn prepare(
        &self,
        lease: &AttemptLease,
        repository: &RepositorySpec,
    ) -> Result<Workspace, WorkspaceError> {
        let workspace = self.plan(lease, repository)?;
        self.provision(&workspace, repository).await?;
        Ok(workspace)
    }

    /// Computes a deterministic isolated location without provisioning a
    /// worktree. Engine code journals this intent before calling `provision`.
    pub fn plan(
        &self,
        lease: &AttemptLease,
        repository: &RepositorySpec,
    ) -> Result<Workspace, WorkspaceError> {
        let root = self.ensure_safe_root()?;
        let key = encode_id(lease.attempt_id.as_str());
        let path = root.join(&key);
        Ok(Workspace {
            attempt_id: lease.attempt_id.clone(),
            id: WorkspaceId::new(format!("ws_{key}")),
            path,
            base_revision: repository.base_revision.clone(),
        })
    }

    /// Performs the first repository-side effect only after a journal intent
    /// is durable. An existing matching marker is a restart of the same
    /// attempt, never reuse by a different attempt.
    pub async fn provision(
        &self,
        workspace: &Workspace,
        repository: &RepositorySpec,
    ) -> Result<(), WorkspaceError> {
        let created = !workspace.path.exists();
        fs::create_dir_all(&workspace.path).map_err(|_| WorkspaceError::Io)?;
        owner_only(&workspace.path).map_err(|_| WorkspaceError::Io)?;

        let marker = workspace.path.join(".tack-attempt");
        if created {
            write_marker(&marker, workspace.attempt_id.as_str())?;
        } else {
            let stored = fs::read_to_string(&marker).map_err(|_| WorkspaceError::UnsafePath)?;
            if stored != workspace.attempt_id.as_str() {
                return Err(WorkspaceError::AttemptMismatch);
            }
        }
        self.provisioner.provision(workspace, repository).await?;
        Ok(())
    }

    /// Deletes only a resolved child of this dedicated root. The root itself,
    /// repository roots, symlinks and unknown paths are refused, never guessed.
    pub fn cleanup(&self, workspace: &Path) -> Result<CleanupResult, WorkspaceError> {
        let root = self.ensure_safe_root()?;
        // Inspect the requested path before resolving it. Inspecting only the
        // canonical target would make a symlink inside `root` look like an
        // ordinary directory and could delete a different workspace.
        if fs::symlink_metadata(workspace)
            .map_err(|_| WorkspaceError::UnsafePath)?
            .file_type()
            .is_symlink()
        {
            return Ok(CleanupResult::Refused);
        }
        let candidate = workspace
            .canonicalize()
            .map_err(|_| WorkspaceError::UnsafePath)?;
        if candidate == root || !candidate.starts_with(&root) {
            return Ok(CleanupResult::Refused);
        }
        fs::remove_dir_all(candidate).map_err(|_| WorkspaceError::Io)?;
        Ok(CleanupResult::Deleted)
    }

    fn ensure_safe_root(&self) -> Result<PathBuf, WorkspaceError> {
        fs::create_dir_all(&self.root).map_err(|_| WorkspaceError::Io)?;
        let root = self
            .root
            .canonicalize()
            .map_err(|_| WorkspaceError::UnsafeRoot)?;
        if root.join(".git").exists() {
            return Err(WorkspaceError::UnsafeRoot);
        }
        owner_only(&root).map_err(|_| WorkspaceError::Io)?;
        Ok(root)
    }
}

fn encode_id(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn write_marker(path: &Path, attempt_id: &str) -> Result<(), WorkspaceError> {
    let mut marker = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| WorkspaceError::Io)?;
    marker
        .write_all(attempt_id.as_bytes())
        .map_err(|_| WorkspaceError::Io)?;
    marker.sync_all().map_err(|_| WorkspaceError::Io)?;
    owner_only(path).map_err(|_| WorkspaceError::Io)
}

#[cfg(unix)]
fn owner_only(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if path.is_file() { 0o600 } else { 0o700 }),
    )
}

#[cfg(not(unix))]
fn owner_only(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;
    use crate::client::{AttemptId, AttemptState, FencingToken, RunnerId, Timestamp};

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    #[derive(Default)]
    struct FakeProvisioner;

    #[async_trait]
    impl WorktreeProvisioner for FakeProvisioner {
        async fn provision(
            &self,
            _workspace: &Workspace,
            _repository: &RepositorySpec,
        ) -> Result<(), WorkspaceError> {
            Ok(())
        }
    }

    fn root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "tack-runner-workspace-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::SeqCst)
        ))
    }

    fn lease(id: &str) -> AttemptLease {
        AttemptLease {
            attempt_id: AttemptId::new(id),
            runner_id: RunnerId::new("runner"),
            fencing_token: FencingToken(1),
            attempt_number: 1,
            state: AttemptState::Leased,
            issued_at: Timestamp::new("2026-08-06T12:20:00Z"),
            expires_at: Timestamp::new("2026-08-06T12:21:00Z"),
        }
    }

    fn repository() -> RepositorySpec {
        RepositorySpec {
            remote: "https://example.invalid/repository.git".into(),
            base_revision: "base".into(),
        }
    }

    #[tokio::test]
    async fn attempts_receive_distinct_deterministic_workspaces() {
        let root = root();
        let manager = WorkspaceManager::new(&root, FakeProvisioner);
        let first = manager
            .prepare(&lease("attempt-one"), &repository())
            .await
            .expect("first");
        let second = manager
            .prepare(&lease("attempt-two"), &repository())
            .await
            .expect("second");
        let repeated = manager
            .prepare(&lease("attempt-one"), &repository())
            .await
            .expect("repeat");

        assert_ne!(first.path, second.path);
        assert_eq!(first.path, repeated.path);
        fs::remove_dir_all(root).expect("remove temporary workspace root");
    }

    #[tokio::test]
    async fn cleanup_refuses_root_and_unresolved_paths() {
        let root = root();
        let manager = WorkspaceManager::new(&root, FakeProvisioner);
        let workspace = manager
            .prepare(&lease("attempt-one"), &repository())
            .await
            .expect("workspace");

        assert_eq!(
            manager.cleanup(&root).expect("refuse root"),
            CleanupResult::Refused
        );
        assert!(matches!(
            manager.cleanup(&root.join("not-created")),
            Err(WorkspaceError::UnsafePath)
        ));
        assert_eq!(
            manager.cleanup(&workspace.path).expect("cleanup workspace"),
            CleanupResult::Deleted
        );
        fs::remove_dir_all(root).expect("remove temporary workspace root");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cleanup_refuses_a_symlink_before_resolving_its_target() {
        use std::os::unix::fs::symlink;

        let root = root();
        let manager = WorkspaceManager::new(&root, FakeProvisioner);
        let workspace = manager
            .prepare(&lease("attempt-one"), &repository())
            .await
            .expect("workspace");
        let link = root.join("link-to-workspace");
        symlink(&workspace.path, &link).expect("create symlink");

        assert_eq!(
            manager.cleanup(&link).expect("refuse symlink"),
            CleanupResult::Refused
        );
        assert!(workspace.path.exists(), "symlink target is preserved");
        fs::remove_dir_all(root).expect("remove temporary workspace root");
    }
}
