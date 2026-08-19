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
    // Every git failure below is deliberately distinguishable: an operator
    // debugging a stuck attempt needs to know whether git is missing, the
    // remote is unreachable, the revision does not exist, or git hung — and
    // none of these messages may carry a remote URL, a path or git's own text
    // (see `git::GitOutput::redacted_stderr`).
    #[error("the git binary is not available to this runner")]
    GitUnavailable,
    #[error("a git command failed while provisioning the attempt checkout")]
    Git,
    #[error("a git command exceeded the provisioning timeout")]
    GitTimeout,
    #[error("the attempt repository could not be reached")]
    RepositoryUnreachable,
    #[error("the requested base revision does not exist in the attempt repository")]
    RevisionUnavailable,
    #[error("runner workspace operation failed")]
    Io,
}

#[path = "git.rs"]
pub mod git;

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
        let root = self.ensure_safe_root()?;
        if workspace.path != root.join(encode_id(workspace.attempt_id.as_str())) {
            return Err(WorkspaceError::UnsafePath);
        }
        let created = match fs::symlink_metadata(&workspace.path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(WorkspaceError::UnsafePath);
            }
            Ok(_) => false,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            Err(_) => return Err(WorkspaceError::UnsafePath),
        };
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
    pub fn cleanup(&self, workspace: &Workspace) -> Result<CleanupResult, WorkspaceError> {
        let root = self.ensure_safe_root()?;
        // Inspect the requested path before resolving it. Inspecting only the
        // canonical target would make a symlink inside `root` look like an
        // ordinary directory and could delete a different workspace.
        if fs::symlink_metadata(&workspace.path)
            .map_err(|_| WorkspaceError::UnsafePath)?
            .file_type()
            .is_symlink()
        {
            return Ok(CleanupResult::Refused);
        }
        let candidate = workspace
            .path
            .canonicalize()
            .map_err(|_| WorkspaceError::UnsafePath)?;
        if candidate == root || !candidate.starts_with(&root) {
            return Ok(CleanupResult::Refused);
        }
        if candidate != root.join(encode_id(workspace.attempt_id.as_str())) {
            return Ok(CleanupResult::Refused);
        }
        let marker = candidate.join(".tack-attempt");
        let stored = fs::read_to_string(marker).map_err(|_| WorkspaceError::UnsafePath)?;
        if stored != workspace.attempt_id.as_str() {
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
        let root_workspace = Workspace {
            attempt_id: workspace.attempt_id.clone(),
            id: workspace.id.clone(),
            path: root.clone(),
            base_revision: workspace.base_revision.clone(),
        };
        let unresolved = Workspace {
            attempt_id: workspace.attempt_id.clone(),
            id: workspace.id.clone(),
            path: root.join("not-created"),
            base_revision: workspace.base_revision.clone(),
        };

        assert_eq!(
            manager.cleanup(&root_workspace).expect("refuse root"),
            CleanupResult::Refused
        );
        assert!(matches!(
            manager.cleanup(&unresolved),
            Err(WorkspaceError::UnsafePath)
        ));
        assert_eq!(
            manager.cleanup(&workspace).expect("cleanup workspace"),
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
        let linked_workspace = Workspace {
            path: link,
            ..workspace.clone()
        };

        assert_eq!(
            manager.cleanup(&linked_workspace).expect("refuse symlink"),
            CleanupResult::Refused
        );
        assert!(workspace.path.exists(), "symlink target is preserved");
        fs::remove_dir_all(root).expect("remove temporary workspace root");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn provision_rejects_an_existing_attempt_path_symlink() {
        use std::os::unix::fs::symlink;

        let root = root();
        let manager = WorkspaceManager::new(&root, FakeProvisioner);
        let planned = manager
            .plan(&lease("attempt-one"), &repository())
            .expect("plan");
        fs::create_dir_all(root.join("outside")).expect("outside");
        symlink(root.join("outside"), &planned.path).expect("attempt path symlink");

        assert!(matches!(
            manager.provision(&planned, &repository()).await,
            Err(WorkspaceError::UnsafePath)
        ));
        fs::remove_dir_all(root).expect("remove temporary workspace root");
    }

    #[tokio::test]
    async fn cleanup_refuses_a_marker_that_does_not_match_the_workspace_identity() {
        let root = root();
        let manager = WorkspaceManager::new(&root, FakeProvisioner);
        let workspace = manager
            .prepare(&lease("attempt-one"), &repository())
            .await
            .expect("workspace");
        fs::write(workspace.path.join(".tack-attempt"), "other-attempt").expect("alter marker");

        assert_eq!(
            manager.cleanup(&workspace).expect("refuse marker"),
            CleanupResult::Refused
        );
        assert!(workspace.path.exists());
        fs::remove_dir_all(root).expect("remove temporary workspace root");
    }

    /// `ensure_safe_root` has two distinct refusals: "candidate equals root"
    /// (covered above) and "this root is itself a real repository", detected
    /// by a `.git` entry directly under it. Both `plan` and `cleanup` share
    /// this guard; a workspace root that has been pointed at an actual
    /// checkout must never be planned into or cleaned up, because cleanup
    /// deletes directories.
    #[tokio::test]
    async fn cleanup_refuses_a_git_repository_root() {
        let root = root();
        fs::create_dir_all(root.join(".git")).expect("simulate a real git repository");
        let canary = root.join("canary-attempt");
        fs::create_dir_all(&canary).expect("canary attempt directory");
        fs::write(canary.join(".tack-attempt"), "attempt-one").expect("canary marker");
        fs::write(canary.join("work.txt"), "do not delete").expect("canary file");

        let manager = WorkspaceManager::new(&root, FakeProvisioner);
        let workspace = Workspace {
            attempt_id: AttemptId::new("attempt-one"),
            id: WorkspaceId::new("ws_canary"),
            path: canary.clone(),
            base_revision: "base".into(),
        };

        assert!(matches!(
            manager.cleanup(&workspace),
            Err(WorkspaceError::UnsafeRoot)
        ));
        // The guard is shared: `plan` must refuse the same root too, not only
        // `cleanup`.
        assert!(matches!(
            manager.plan(&lease("attempt-two"), &repository()),
            Err(WorkspaceError::UnsafeRoot)
        ));
        assert!(
            canary.exists(),
            "candidate directory survives a refused cleanup"
        );
        assert!(canary.join(".tack-attempt").exists());
        assert!(canary.join("work.txt").exists());
        fs::remove_dir_all(root).expect("remove temporary workspace root");
    }

    /// `cleanup` resolves the candidate with `canonicalize` and refuses
    /// anything outside `root`. A literal `..` component is one way an
    /// (accidentally or maliciously) constructed `Workspace` could try to
    /// point outside the dedicated root without the final path component
    /// itself being a symlink.
    #[tokio::test]
    async fn cleanup_refuses_a_dot_dot_traversal_outside_the_root() {
        let root = root();
        let victim = self::root();
        fs::create_dir_all(&root).expect("workspace root");
        fs::create_dir_all(&victim).expect("sibling victim directory");
        fs::write(victim.join("important.txt"), "do not delete").expect("victim file");

        let manager = WorkspaceManager::new(&root, FakeProvisioner);
        let escaping_path = root
            .join("..")
            .join(victim.file_name().expect("victim directory name"));
        let workspace = Workspace {
            attempt_id: AttemptId::new("attempt-one"),
            id: WorkspaceId::new("ws_dotdot"),
            path: escaping_path,
            base_revision: "base".into(),
        };

        assert_eq!(
            manager.cleanup(&workspace).expect("refuse dot-dot escape"),
            CleanupResult::Refused
        );
        assert!(
            victim.exists(),
            "sibling directory reached via .. survives a refused cleanup"
        );
        assert!(victim.join("important.txt").exists());
        fs::remove_dir_all(&root).expect("remove temporary workspace root");
        fs::remove_dir_all(&victim).expect("remove temporary victim directory");
    }

    /// The existing symlink test proves a symlink *as the final path
    /// component* is refused before it is ever resolved. This proves the
    /// complementary case: an intermediate path component that is a symlink
    /// pointing outside `root`, where the final component itself is an
    /// ordinary directory. `symlink_metadata` on the full path only inspects
    /// the last component, so this can only be caught by the
    /// `canonicalize` + `starts_with(root)` check, not the symlink check.
    #[cfg(unix)]
    #[tokio::test]
    async fn cleanup_refuses_traversal_through_a_symlinked_intermediate_directory() {
        use std::os::unix::fs::symlink;

        let root = root();
        let outside = self::root();
        fs::create_dir_all(&root).expect("workspace root");
        let victim = outside.join("victim");
        fs::create_dir_all(&victim).expect("victim directory");
        fs::write(victim.join("important.txt"), "do not delete").expect("victim file");

        let manager = WorkspaceManager::new(&root, FakeProvisioner);
        let escape_link = root.join("escape");
        symlink(&outside, &escape_link).expect("create escaping symlink");

        let workspace = Workspace {
            attempt_id: AttemptId::new("attempt-one"),
            id: WorkspaceId::new("ws_escape"),
            path: escape_link.join("victim"),
            base_revision: "base".into(),
        };

        assert_eq!(
            manager
                .cleanup(&workspace)
                .expect("refuse symlinked escape"),
            CleanupResult::Refused
        );
        assert!(
            victim.exists(),
            "victim directory outside root survives a refused cleanup"
        );
        assert!(victim.join("important.txt").exists());
        fs::remove_dir_all(&root).expect("remove temporary workspace root");
        fs::remove_dir_all(&outside).expect("remove temporary outside root");
    }
}
