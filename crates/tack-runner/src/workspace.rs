//! Deterministic, attempt-scoped worktree reservation and safe cleanup.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use thiserror::Error;

use crate::error::IoKind;
use crate::evidence::{GitEvidence, PublishedBranch};

use super::{
    AttemptId, AttemptLease, RepositorySpec, WorkspaceId, WorkspaceMode, journal::WorkspaceJournal,
};

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

/// A lock another thread poisoned by panicking while it held it: the panic is the cause and was
/// reported where it happened, so this logs that the lock is unusable and answers an `Io` failure.
pub(crate) fn poisoned<T>(_: std::sync::PoisonError<T>) -> WorkspaceError {
    tracing::error!("a runner workspace lock was poisoned by an earlier panic");
    WorkspaceError::Io(IoKind(std::io::ErrorKind::Other))
}

impl From<std::io::Error> for WorkspaceError {
    fn from(failure: std::io::Error) -> Self {
        WorkspaceError::Io(IoKind(failure.kind()))
    }
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
    /// Another attempt holds the lock on this `in_place` folder. A lock left
    /// by a dead process is not detected: out of scope, remove the file.
    #[error("another attempt is working in this folder")]
    Busy,
    /// `HEAD` or the index is not what was captured and verified.
    #[error("the tree changed after it was verified")]
    TreeChanged,
    /// A file operation failed; the kind says how (`NotFound`, `PermissionDenied`, ...) and
    /// nothing else of it, since the operating system's text can name a path.
    #[error("runner workspace operation failed: {0}")]
    Io(IoKind),
}

#[path = "git.rs"]
pub mod git;

/// The only boundary allowed to create a Git worktree. Tests inject a fake;
/// the real implementation does not pretend that an empty directory is a
/// checked-out repository.
#[async_trait]
pub trait WorktreeProvisioner: Send + Sync {
    async fn provision(
        &self,
        workspace: &Workspace,
        repository: &RepositorySpec,
    ) -> Result<(), WorkspaceError>;

    /// An empty directory with a repository and one empty initial commit, in
    /// place of a fetch: the checkout for a request that names no remote.
    /// Refused unless a provisioner opts in.
    async fn provision_scratch(&self, _workspace: &Workspace) -> Result<(), WorkspaceError> {
        Err(WorkspaceError::WorktreeUnavailable)
    }

    /// What provisioning fixed for `workspace`, read before the harness
    /// starts: the commit id the base revision resolved to, and the URL the
    /// branch is pushed to. Either is `None` when this provisioner has none.
    async fn provisioned(&self, _workspace: &Workspace) -> (Option<String>, Option<String>) {
        (None, None)
    }

    /// What the attempt changed in `workspace`, read before the workspace is
    /// deleted, as the difference from `base_commit` (the commit id fixed at
    /// provisioning) to a commit made of the work tree as it stands. That
    /// commit is `GitEvidence::captured_commit`; `commit` is its author and
    /// message. `exclude` names workspace-relative paths that are the
    /// runner's own, not the harness's. `Ok(None)` is a provisioner with no
    /// repository to read.
    async fn capture_evidence(
        &self,
        _workspace: &Workspace,
        _exclude: &[&str],
        _base_commit: Option<&str>,
        _commit: (&str, &str),
    ) -> Result<Option<GitEvidence>, WorkspaceError> {
        Ok(None)
    }

    /// [`Self::capture_evidence`] for a `repository` attempt, which has no
    /// work tree: the difference from `base_commit` to the commit `head` in
    /// the repository, read with git plumbing alone. `Ok(None)` is a
    /// provisioner with no repository to read.
    async fn capture_range(
        &self,
        _workspace: &Workspace,
        _exclude: &[&str],
        _base_commit: &str,
        _head: &str,
    ) -> Result<Option<GitEvidence>, WorkspaceError> {
        Ok(None)
    }

    /// [`Self::publish_branch`] for a `repository` attempt: `branch` is made
    /// in the repository at the commit `head` and, when `push` is set, that
    /// commit is pushed to `remote`.
    async fn publish_range(
        &self,
        _workspace: &Workspace,
        _branch: &str,
        _push: bool,
        _remote: Option<&str>,
        _head: &str,
    ) -> Result<Option<PublishedBranch>, WorkspaceError> {
        Ok(None)
    }

    /// Puts the `captured` commit on `branch` and, when `push` is set, pushes
    /// exactly that commit to `remote`. It refuses
    /// ([`WorkspaceError::TreeChanged`]) unless `HEAD` is still `head`, the
    /// commit it was at when `captured` was made, and the index still holds
    /// `captured`'s tree. A `local_branch` worktree is removed here and its
    /// branch stays in the user's repository. `Ok(None)` is a provisioner
    /// with no repository.
    async fn publish_branch(
        &self,
        _workspace: &Workspace,
        _branch: &str,
        _push: bool,
        _remote: Option<&str>,
        _captured: &str,
        _head: &str,
    ) -> Result<Option<PublishedBranch>, WorkspaceError> {
        Ok(None)
    }
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

    /// The runner's state directory, which the workspaces root sits under.
    pub fn state_dir(&self) -> Result<PathBuf, WorkspaceError> {
        let root = self.ensure_safe_root()?;
        root.parent()
            .map(Path::to_path_buf)
            .ok_or(WorkspaceError::UnsafeRoot)
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
        // `in_place` and `repository`: the workspace IS the user's folder,
        // never under `root`.
        let path = if matches!(
            repository.workspace_mode,
            WorkspaceMode::InPlace | WorkspaceMode::Repository
        ) {
            repository
                .repository_path
                .as_deref()
                .and_then(|folder| folder.canonicalize().ok())
                .filter(|folder| folder.is_dir())
                .ok_or(WorkspaceError::UnsafePath)?
        } else {
            root.join(&key)
        };
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
        if repository.workspace_mode == WorkspaceMode::InPlace {
            self.lock_folder(workspace)?;
            let provisioned = self.provisioner.provision(workspace, repository).await;
            if provisioned.is_err() {
                self.unlock_folder(workspace);
            }
            return provisioned;
        }
        // Nothing is made for `repository`: the provisioner only reads the
        // commit the base revision names.
        if repository.workspace_mode != WorkspaceMode::Repository {
            self.reserve_directory(workspace)?;
        }
        self.provisioner.provision(workspace, repository).await?;
        Ok(())
    }

    /// `<state dir>/locks/<sha256(folder)>`, holding the attempt id.
    fn lock_path(&self, workspace: &Workspace) -> Result<PathBuf, WorkspaceError> {
        let state = self.ensure_safe_root()?;
        let state = state.parent().ok_or(WorkspaceError::UnsafeRoot)?;
        Ok(state.join("locks").join(crate::harness::sha256::sha256_hex(
            workspace.path.to_string_lossy().as_bytes(),
        )))
    }

    fn lock_folder(&self, workspace: &Workspace) -> Result<(), WorkspaceError> {
        let lock = self.lock_path(workspace)?;
        fs::create_dir_all(lock.parent().ok_or(WorkspaceError::UnsafeRoot)?)
            .map_err(WorkspaceError::from)?;
        match OpenOptions::new().write(true).create_new(true).open(&lock) {
            Ok(mut file) => file
                .write_all(workspace.attempt_id.as_str().as_bytes())
                .and_then(|()| file.sync_all())
                .map_err(WorkspaceError::from),
            // The same attempt restarting holds its own lock.
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                match fs::read_to_string(&lock) {
                    Ok(holder) if holder == workspace.attempt_id.as_str() => Ok(()),
                    _ => Err(WorkspaceError::Busy),
                }
            }
            Err(error) => Err(WorkspaceError::from(error)),
        }
    }

    /// Removes the lock only when this attempt holds it.
    fn unlock_folder(&self, workspace: &Workspace) {
        if let Ok(lock) = self.lock_path(workspace)
            && fs::read_to_string(&lock).is_ok_and(|holder| holder == workspace.attempt_id.as_str())
        {
            let _ = fs::remove_file(lock);
        }
    }

    /// [`Self::provision`] for a request with no remote: the same directory
    /// guards, then an empty repository instead of a fetch.
    pub async fn provision_scratch(&self, workspace: &Workspace) -> Result<(), WorkspaceError> {
        self.reserve_directory(workspace)?;
        self.provisioner.provision_scratch(workspace).await?;
        Ok(())
    }

    fn reserve_directory(&self, workspace: &Workspace) -> Result<(), WorkspaceError> {
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
        fs::create_dir_all(&workspace.path).map_err(WorkspaceError::from)?;
        owner_only(&workspace.path).map_err(WorkspaceError::from)?;

        let marker = workspace.path.join(".tack-attempt");
        if created {
            write_marker(&marker, workspace.attempt_id.as_str())?;
        } else {
            let stored = fs::read_to_string(&marker).map_err(|_| WorkspaceError::UnsafePath)?;
            if stored != workspace.attempt_id.as_str() {
                return Err(WorkspaceError::AttemptMismatch);
            }
        }
        Ok(())
    }

    pub async fn provisioned(&self, workspace: &Workspace) -> (Option<String>, Option<String>) {
        self.provisioner.provisioned(workspace).await
    }

    pub async fn capture_evidence(
        &self,
        workspace: &Workspace,
        exclude: &[&str],
        base_commit: Option<&str>,
        commit: (&str, &str),
    ) -> Result<Option<GitEvidence>, WorkspaceError> {
        self.provisioner
            .capture_evidence(workspace, exclude, base_commit, commit)
            .await
    }

    pub async fn capture_range(
        &self,
        workspace: &Workspace,
        exclude: &[&str],
        base_commit: &str,
        head: &str,
    ) -> Result<Option<GitEvidence>, WorkspaceError> {
        self.provisioner
            .capture_range(workspace, exclude, base_commit, head)
            .await
    }

    pub async fn publish_range(
        &self,
        workspace: &Workspace,
        branch: &str,
        push: bool,
        remote: Option<&str>,
        head: &str,
    ) -> Result<Option<PublishedBranch>, WorkspaceError> {
        self.provisioner
            .publish_range(workspace, branch, push, remote, head)
            .await
    }

    pub async fn publish_branch(
        &self,
        workspace: &Workspace,
        branch: &str,
        push: bool,
        remote: Option<&str>,
        captured: &str,
        head: &str,
    ) -> Result<Option<PublishedBranch>, WorkspaceError> {
        self.provisioner
            .publish_branch(workspace, branch, push, remote, captured, head)
            .await
    }

    /// Deletes only a resolved child of this dedicated root. The root itself,
    /// repository roots, symlinks and unknown paths are refused, never guessed.
    pub fn cleanup(&self, workspace: &Workspace) -> Result<CleanupResult, WorkspaceError> {
        // A folder outside the root is an `in_place` folder (or foreign): it
        // is never deleted, and the lock is all that is released.
        if !workspace.path.starts_with(self.ensure_safe_root()?) {
            self.unlock_folder(workspace);
            return Ok(CleanupResult::Refused);
        }
        let Some((_, candidate)) = self.owned_path(workspace)? else {
            return Ok(CleanupResult::Refused);
        };
        fs::remove_dir_all(candidate).map_err(WorkspaceError::from)?;
        Ok(CleanupResult::Deleted)
    }

    /// Moves a workspace whose evidence could not be read into `quarantine/`
    /// next to the workspace root, under the same path validation `cleanup`
    /// applies, so the files survive for an operator. Returns the new path.
    pub fn keep(&self, workspace: &Workspace) -> Result<PathBuf, WorkspaceError> {
        let Some((root, candidate)) = self.owned_path(workspace)? else {
            return Err(WorkspaceError::UnsafePath);
        };
        let quarantine = root
            .parent()
            .ok_or(WorkspaceError::UnsafeRoot)?
            .join("quarantine");
        fs::create_dir_all(&quarantine).map_err(WorkspaceError::from)?;
        let destination = quarantine.join(encode_id(workspace.attempt_id.as_str()));
        if destination.exists() {
            return Err(WorkspaceError::Io(IoKind(
                std::io::ErrorKind::AlreadyExists,
            )));
        }
        fs::rename(candidate, &destination).map_err(WorkspaceError::from)?;
        Ok(destination)
    }

    /// The root and the resolved directory of a workspace this manager owns;
    /// `None` for the root itself, symlinks, foreign paths and a mismatched
    /// marker, never guessed.
    fn owned_path(
        &self,
        workspace: &Workspace,
    ) -> Result<Option<(PathBuf, PathBuf)>, WorkspaceError> {
        let root = self.ensure_safe_root()?;
        // Inspect the requested path before resolving it. Inspecting only the
        // canonical target would make a symlink inside `root` look like an
        // ordinary directory and could delete a different workspace.
        if fs::symlink_metadata(&workspace.path)
            .map_err(|_| WorkspaceError::UnsafePath)?
            .file_type()
            .is_symlink()
        {
            return Ok(None);
        }
        let candidate = workspace
            .path
            .canonicalize()
            .map_err(|_| WorkspaceError::UnsafePath)?;
        if candidate == root || !candidate.starts_with(&root) {
            return Ok(None);
        }
        if candidate != root.join(encode_id(workspace.attempt_id.as_str())) {
            return Ok(None);
        }
        let marker = candidate.join(".tack-attempt");
        let stored = fs::read_to_string(marker).map_err(|_| WorkspaceError::UnsafePath)?;
        if stored != workspace.attempt_id.as_str() {
            return Ok(None);
        }
        Ok(Some((root, candidate)))
    }

    fn ensure_safe_root(&self) -> Result<PathBuf, WorkspaceError> {
        fs::create_dir_all(&self.root).map_err(WorkspaceError::from)?;
        let root = self
            .root
            .canonicalize()
            .map_err(|_| WorkspaceError::UnsafeRoot)?;
        if root.join(".git").exists() {
            return Err(WorkspaceError::UnsafeRoot);
        }
        owner_only(&root).map_err(WorkspaceError::from)?;
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
        .map_err(WorkspaceError::from)?;
    marker
        .write_all(attempt_id.as_bytes())
        .map_err(WorkspaceError::from)?;
    marker.sync_all().map_err(WorkspaceError::from)?;
    owner_only(path).map_err(WorkspaceError::from)
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
#[path = "workspace/tests.rs"]
mod tests;
