//! The real [`WorktreeProvisioner`]: a private, attempt-scoped git checkout.
//!
//! # Why a private clone and not `git worktree add`
//!
//! The trait is named `WorktreeProvisioner` because the *product* requirement
//! is an isolated working tree per attempt, not because git's `worktree`
//! subcommand must be the mechanism. Two facts rule that subcommand out here:
//!
//! 1. `git worktree add` keeps administrative state (`worktrees/<name>`, a
//!    lock file and a `gitdir` pointer) inside one shared repository. Two
//!    attempts provisioning at once contend on that repository's index lock,
//!    and a runner killed mid-add leaves a registered-but-absent worktree that
//!    a later attempt inherits — exactly the two failure modes a private
//!    clone avoids. Recovering from it needs `git worktree prune` against shared
//!    mutable state that another live attempt may be using at that moment.
//! 2. `git worktree add` refuses a target directory that already exists and is
//!    not empty. Every attempt directory already carries the `.tack-attempt`
//!    marker that [`super::WorkspaceManager`] writes *before* provisioning, so
//!    the target is never empty by construction.
//!
//! `git init` in place has neither problem: it accepts a non-empty directory,
//! every attempt owns 100% of its own repository state, and cleanup is a plain
//! recursive delete of a directory this runner stamped — which
//! [`super::WorkspaceManager::cleanup`] already implements and guards.
//!
//! # Restart safety
//!
//! Provisioning is not atomic — a checkout is thousands of files. A runner
//! killed halfway leaves a directory that *looks* like a checkout but is not
//! one. The completion sentinel [`CHECKOUT_MARKER`] is written (and fsynced)
//! only after `checkout` returns, and it records the exact resolved commit. On
//! a restart the provisioner either finds a sentinel that agrees with the live
//! repository — and reuses the checkout — or it discards everything under the
//! attempt directory and provisions again. A half-made checkout is therefore
//! never inherited, by this attempt or any later one.
//!
//! # What never reaches a log
//!
//! A remote URL can embed credentials (`https://user:token@host/repo.git`) and
//! a query string. Git echoes the remote back in most of its error messages,
//! so raw git output is treated as tainted: it is scrubbed through
//! [`SecretMaterial`] (seeded with the remote, its userinfo and its password)
//! and [`redact_query`] before it can reach a tracing field, and the typed
//! errors this module returns carry no remote, path or git text at all.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use async_trait::async_trait;
use tokio::process::Command;

use super::{Workspace, WorkspaceError, WorktreeProvisioner};
use crate::{
    client::RepositorySpec,
    harness::redact::{SecretMaterial, redact_query},
};

/// Written only after a checkout completed; contains the resolved commit.
pub const CHECKOUT_MARKER: &str = ".tack-checkout";
/// Written by `WorkspaceManager` before provisioning; must survive a purge.
const ATTEMPT_MARKER: &str = ".tack-attempt";

/// Default wall-clock ceiling for one git invocation. Cloning a large
/// repository over a slow link is legitimately slow, so this is generous; its
/// job is to turn a hung `git` (an auth prompt, a black-holed TCP connection)
/// into a typed failure instead of an attempt that never reports anything.
pub const DEFAULT_GIT_TIMEOUT: Duration = Duration::from_secs(600);

/// Provisions each attempt its own git checkout using the local `git` binary.
#[derive(Debug, Clone)]
pub struct GitWorktreeProvisioner {
    program: PathBuf,
    timeout: Duration,
}

impl Default for GitWorktreeProvisioner {
    fn default() -> Self {
        Self::new("git", DEFAULT_GIT_TIMEOUT)
    }
}

impl GitWorktreeProvisioner {
    pub fn new(program: impl Into<PathBuf>, timeout: Duration) -> Self {
        Self {
            program: program.into(),
            timeout,
        }
    }

    /// One bounded `git` invocation inside `directory`.
    ///
    /// The child inherits the operator's ambient git configuration on purpose:
    /// the runner-v1 contract has no channel for repository credentials, so a
    /// runner-local `~/.gitconfig`, credential helper or SSH agent is the only
    /// way a private remote can ever work. What it must *not* inherit is
    /// repository-selecting state (`GIT_DIR` and friends): a runner started
    /// from inside a git repository, or under a git hook, would otherwise
    /// silently operate on that repository instead of the attempt's.
    async fn git(
        &self,
        directory: &Path,
        args: &[&str],
        secrets: &SecretMaterial,
    ) -> Result<GitOutput, WorkspaceError> {
        let mut command = Command::new(&self.program);
        command
            .current_dir(directory)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_COMMON_DIR")
            .env_remove("GIT_OBJECT_DIRECTORY")
            .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
            .env_remove("GIT_CEILING_DIRECTORIES")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let child = command.spawn().map_err(|error| {
            // `NotFound` at spawn has two causes — the program is not on
            // `PATH`, or the working directory no longer exists — and telling
            // an operator "git is not installed" when the attempt directory
            // vanished underneath the runner would send them to the wrong
            // place entirely.
            match (error.kind(), directory.is_dir()) {
                (std::io::ErrorKind::NotFound, true) => WorkspaceError::GitUnavailable,
                (std::io::ErrorKind::NotFound, false) => WorkspaceError::UnsafePath,
                _ => WorkspaceError::Io,
            }
        })?;
        // `kill_on_drop` turns the timeout into a real kill: dropping the
        // future drops the child, which sends SIGKILL. A hung `git` therefore
        // cannot outlive the attempt that spawned it.
        let output = match tokio::time::timeout(self.timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(_)) => return Err(WorkspaceError::Io),
            Err(_) => return Err(WorkspaceError::GitTimeout),
        };
        let result = GitOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        };
        if !result.success {
            // Only the subcommand name and scrubbed stderr — never the full
            // argument list, which carries the remote URL verbatim.
            tracing::debug!(
                subcommand = args.first().copied().unwrap_or("git"),
                detail = %result.redacted_stderr(secrets),
                "git command failed"
            );
        }
        Ok(result)
    }

    async fn git_ok(
        &self,
        directory: &Path,
        args: &[&str],
        secrets: &SecretMaterial,
    ) -> Result<GitOutput, WorkspaceError> {
        let output = self.git(directory, args, secrets).await?;
        if output.success {
            Ok(output)
        } else {
            Err(WorkspaceError::Git)
        }
    }

    /// True when this directory already holds a completed checkout of exactly
    /// this revision. Three independent facts must agree — the sentinel, a
    /// live `.git`, and the commit `HEAD` actually points at — because any one
    /// of them alone can survive a kill that invalidated the others.
    async fn already_provisioned(
        &self,
        path: &Path,
        requested: &str,
        secrets: &SecretMaterial,
    ) -> bool {
        let Ok(recorded) = fs::read_to_string(path.join(CHECKOUT_MARKER)) else {
            return false;
        };
        let recorded = recorded.trim().to_owned();
        if recorded.is_empty() || !path.join(".git").exists() {
            return false;
        }
        let Ok(head) = self
            .git(path, &["rev-parse", "--verify", "HEAD"], secrets)
            .await
        else {
            return false;
        };
        if !head.success || head.stdout != recorded {
            return false;
        }
        // A sentinel from a *different* requested revision must not be reused:
        // the same attempt directory is only ever re-provisioned for the same
        // attempt, but a caller could still hand a changed `base_revision`.
        match self
            .git(
                path,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("{requested}^{{commit}}"),
                ],
                secrets,
            )
            .await
        {
            Ok(resolved) if resolved.success => resolved.stdout == recorded,
            _ => false,
        }
    }

    /// Removes every entry under the attempt directory except the attempt
    /// marker, which identifies the directory this runner is allowed to touch
    /// and must therefore outlive the purge.
    ///
    /// The caller has already proven `path` is a non-symlink directory holding
    /// a marker that matches this attempt; nothing outside it is reachable,
    /// because entries are removed by direct `read_dir` handle, never by a
    /// path assembled from untrusted input.
    fn purge_partial_checkout(path: &Path) -> Result<(), WorkspaceError> {
        for entry in fs::read_dir(path).map_err(|_| WorkspaceError::Io)? {
            let entry = entry.map_err(|_| WorkspaceError::Io)?;
            if entry.file_name() == ATTEMPT_MARKER {
                continue;
            }
            let file_type = entry.file_type().map_err(|_| WorkspaceError::Io)?;
            let outcome = if file_type.is_dir() {
                fs::remove_dir_all(entry.path())
            } else {
                // A symlink is removed as a link; `remove_dir_all` would be
                // refused on it anyway, and neither call follows it.
                fs::remove_file(entry.path())
            };
            outcome.map_err(|_| WorkspaceError::Io)?;
        }
        Ok(())
    }

    /// Fetches `revision` as cheaply as the remote allows, then leaves the
    /// working tree detached at the exact commit. Returns the resolved commit.
    async fn fetch_and_checkout(
        &self,
        path: &Path,
        remote: &str,
        revision: &str,
        secrets: &SecretMaterial,
    ) -> Result<String, WorkspaceError> {
        self.git_ok(path, &["init", "--quiet"], secrets).await?;
        // `set-url` covers the re-provision case where `origin` already exists
        // from a purged-but-not-quite attempt; `add` covers the fresh case.
        if self
            .git_ok(path, &["remote", "add", "origin", remote], secrets)
            .await
            .is_err()
        {
            self.git_ok(path, &["remote", "set-url", "origin", remote], secrets)
                .await?;
        }

        // Fetching the single requested commit is by far the cheapest path,
        // but it only works when the server allows it (`uploadpack.allowAny*`;
        // most forges do, a plain HTTP dumb remote does not) and when the
        // revision is a commit id rather than a branch name. Its failure is
        // expected and is not an error — it falls back to a full fetch.
        let shallow = self
            .git(
                path,
                &["fetch", "--no-tags", "--depth", "1", "origin", revision],
                secrets,
            )
            .await?;
        let resolved = if shallow.success {
            let head = self
                .git_ok(path, &["rev-parse", "--verify", "FETCH_HEAD"], secrets)
                .await?;
            head.stdout
        } else {
            self.git_ok(path, &["fetch", "--no-tags", "origin"], secrets)
                .await
                .map_err(|error| match error {
                    // A failed full fetch after a failed narrow fetch is the
                    // "cannot reach or read this remote" case, reported as
                    // itself rather than as a generic git failure.
                    WorkspaceError::Git => WorkspaceError::RepositoryUnreachable,
                    other => other,
                })?;
            self.resolve_revision(path, revision, secrets).await?
        };

        self.git_ok(
            path,
            &[
                "-c",
                "advice.detachedHead=false",
                "checkout",
                "--detach",
                &resolved,
            ],
            secrets,
        )
        .await?;

        // The requested revision governs, not what was fetched. If the caller
        // named a full commit id, the checked-out commit must be that exact
        // commit — otherwise the attempt would run against code nobody asked
        // for while reporting the requested `base_revision` to the server.
        let head = self
            .git_ok(path, &["rev-parse", "--verify", "HEAD"], secrets)
            .await?
            .stdout;
        if head != resolved || (is_full_commit_id(revision) && !head.eq_ignore_ascii_case(revision))
        {
            return Err(WorkspaceError::RevisionUnavailable);
        }
        Ok(head)
    }

    /// Maps a requested revision onto a commit that now exists locally.
    /// Ordered deliberately: an exact object id first, then a remote-tracking
    /// branch, then a tag. `origin/<name>` is tried before a bare `<name>`
    /// because after `git init` a bare branch name resolves to nothing, while
    /// a *local* name colliding with a fetched one cannot exist yet.
    async fn resolve_revision(
        &self,
        path: &Path,
        revision: &str,
        secrets: &SecretMaterial,
    ) -> Result<String, WorkspaceError> {
        for candidate in [
            revision.to_owned(),
            format!("origin/{revision}"),
            format!("refs/tags/{revision}"),
        ] {
            let output = self
                .git(
                    path,
                    &[
                        "rev-parse",
                        "--verify",
                        "--quiet",
                        &format!("{candidate}^{{commit}}"),
                    ],
                    secrets,
                )
                .await?;
            if output.success && !output.stdout.is_empty() {
                return Ok(output.stdout);
            }
        }
        Err(WorkspaceError::RevisionUnavailable)
    }
}

#[async_trait]
impl WorktreeProvisioner for GitWorktreeProvisioner {
    async fn provision(
        &self,
        workspace: &Workspace,
        repository: &RepositorySpec,
    ) -> Result<(), WorkspaceError> {
        let path = workspace.path.as_path();
        // Independent of `WorkspaceManager`'s own guard on purpose: this impl
        // deletes files, so it re-proves for itself that the directory is a
        // real directory this runner stamped for this exact attempt.
        let metadata = fs::symlink_metadata(path).map_err(|_| WorkspaceError::UnsafePath)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(WorkspaceError::UnsafePath);
        }
        let marker = fs::read_to_string(path.join(ATTEMPT_MARKER))
            .map_err(|_| WorkspaceError::UnsafePath)?;
        if marker != workspace.attempt_id.as_str() {
            return Err(WorkspaceError::AttemptMismatch);
        }

        let secrets = remote_secrets(&repository.remote);
        if self
            .already_provisioned(path, &repository.base_revision, &secrets)
            .await
        {
            tracing::debug!(
                attempt_id = workspace.attempt_id.as_str(),
                workspace_id = workspace.id.as_str(),
                "reusing the existing attempt checkout"
            );
            return Ok(());
        }
        // Either nothing was provisioned yet, or a previous provision was
        // interrupted. Both are discarded rather than repaired: a partial
        // checkout has no trustworthy state to repair from.
        Self::purge_partial_checkout(path)?;

        let result = self
            .fetch_and_checkout(
                path,
                &repository.remote,
                &repository.base_revision,
                &secrets,
            )
            .await;
        let resolved = match result {
            Ok(resolved) => resolved,
            Err(error) => {
                // Leave nothing that a later provision could mistake for a
                // usable checkout. The sentinel was never written, so this is
                // belt-and-braces; it also keeps a failed attempt's directory
                // small instead of holding a half-fetched object store.
                let _ = Self::purge_partial_checkout(path);
                tracing::warn!(
                    attempt_id = workspace.attempt_id.as_str(),
                    workspace_id = workspace.id.as_str(),
                    failure = %error,
                    "attempt checkout failed"
                );
                return Err(error);
            }
        };

        write_checkout_marker(&path.join(CHECKOUT_MARKER), &resolved)?;
        tracing::info!(
            attempt_id = workspace.attempt_id.as_str(),
            workspace_id = workspace.id.as_str(),
            "attempt checkout ready"
        );
        Ok(())
    }
}

struct GitOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

impl GitOutput {
    /// The only sanctioned way to surface git's own text. Both scrubbing
    /// passes are applied: the exact remote (and its userinfo/password, which
    /// git may print on its own) is replaced wholesale, and any surviving
    /// URL-shaped query string is dropped.
    fn redacted_stderr(&self, secrets: &SecretMaterial) -> String {
        secrets
            .scrub(&self.stderr)
            .split_whitespace()
            .map(redact_query)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Every value that must never survive into a log line for this remote.
fn remote_secrets(remote: &str) -> SecretMaterial {
    let mut material = SecretMaterial::new();
    material.register(remote);
    for secret in url_secrets(remote) {
        material.register(secret);
    }
    material
}

/// Extracts the userinfo of a URL — `user`, `password` and `user:password` —
/// so each can be scrubbed even when git prints only one of them.
fn url_secrets(remote: &str) -> Vec<String> {
    let Some(rest) = remote.split_once("://").map(|(_, rest)| rest) else {
        return Vec::new();
    };
    let Some((userinfo, _)) = rest.split_once('@') else {
        return Vec::new();
    };
    let mut secrets = vec![userinfo.to_owned()];
    if let Some((user, password)) = userinfo.split_once(':') {
        secrets.push(user.to_owned());
        secrets.push(password.to_owned());
    }
    secrets.retain(|secret| !secret.is_empty());
    secrets
}

fn is_full_commit_id(revision: &str) -> bool {
    revision.len() == 40
        && revision
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

fn write_checkout_marker(path: &Path, commit: &str) -> Result<(), WorkspaceError> {
    let mut marker = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|_| WorkspaceError::Io)?;
    marker
        .write_all(commit.as_bytes())
        .map_err(|_| WorkspaceError::Io)?;
    // Durable before it is trusted: an unsynced sentinel after a power loss
    // would claim a checkout that the filesystem never finished writing.
    marker.sync_all().map_err(|_| WorkspaceError::Io)?;
    super::owner_only(path).map_err(|_| WorkspaceError::Io)
}

#[cfg(test)]
mod tests {
    use std::{
        process::Command as SyncCommand,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;
    use crate::client::{AttemptId, WorkspaceId};

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    /// Resolves `git` to an absolute path instead of relying on `PATH`.
    ///
    /// Not paranoia: `harness::claude_code`'s discovery test overwrites the
    /// process-wide `PATH` for the duration of its assertion, and the Rust test
    /// harness runs tests on many threads, so any concurrently-running test that
    /// resolves a bare program name can observe that empty `PATH` and fail with a
    /// spurious "binary not found". This was found as a one-in-many-runs failure
    /// of `a_checkout_of_a_different_revision_is_never_reused` during
    /// `cargo test --workspace`.
    fn git_program() -> PathBuf {
        for candidate in [
            "/usr/bin/git",
            "/bin/git",
            "/usr/local/bin/git",
            "/opt/homebrew/bin/git",
        ] {
            if Path::new(candidate).is_file() {
                return PathBuf::from(candidate);
            }
        }
        PathBuf::from("git")
    }

    fn temp_dir(label: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "tack-runner-git-{label}-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temporary directory");
        path
    }

    fn run_git(directory: &Path, args: &[&str]) -> String {
        let output = SyncCommand::new(git_program())
            .current_dir(directory)
            .args(args)
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    /// A real git repository with two commits — never a fake. The card's
    /// acceptance is explicit that this must be proven against real git.
    struct SourceRepository {
        path: PathBuf,
        first_commit: String,
        second_commit: String,
    }

    impl SourceRepository {
        fn create() -> Self {
            let path = temp_dir("source");
            run_git(&path, &["-c", "init.defaultBranch=main", "init", "--quiet"]);
            run_git(&path, &["config", "user.email", "runner@example.invalid"]);
            run_git(&path, &["config", "user.name", "Tack Runner Test"]);
            fs::write(path.join("README.md"), "first\n").expect("write");
            run_git(&path, &["add", "."]);
            run_git(&path, &["commit", "--quiet", "-m", "first"]);
            let first_commit = run_git(&path, &["rev-parse", "HEAD"]);
            fs::write(path.join("README.md"), "second\n").expect("write");
            fs::write(path.join("added.txt"), "only in the second commit\n").expect("write");
            run_git(&path, &["add", "."]);
            run_git(&path, &["commit", "--quiet", "-m", "second"]);
            let second_commit = run_git(&path, &["rev-parse", "HEAD"]);
            Self {
                path,
                first_commit,
                second_commit,
            }
        }

        fn spec(&self, revision: &str) -> RepositorySpec {
            RepositorySpec {
                remote: self.path.to_string_lossy().into_owned(),
                base_revision: revision.to_owned(),
            }
        }
    }

    /// Builds the attempt directory exactly as `WorkspaceManager::provision`
    /// leaves it before it calls the provisioner: an owner-only directory
    /// holding the attempt marker and nothing else.
    fn attempt_workspace(root: &Path, attempt_id: &str, revision: &str) -> Workspace {
        let path = root.join(attempt_id);
        fs::create_dir_all(&path).expect("attempt directory");
        super::super::owner_only(&path).expect("owner-only attempt directory");
        fs::write(path.join(ATTEMPT_MARKER), attempt_id).expect("attempt marker");
        Workspace {
            attempt_id: AttemptId::new(attempt_id),
            id: WorkspaceId::new(format!("ws_{attempt_id}")),
            path,
            base_revision: revision.to_owned(),
        }
    }

    fn head_of(workspace: &Workspace) -> String {
        run_git(&workspace.path, &["rev-parse", "HEAD"])
    }

    // -----------------------------------------------------------------
    // The capability itself: a claimed attempt gets a real checkout.
    // -----------------------------------------------------------------

    #[tokio::test]
    async fn an_attempt_receives_a_real_checkout_of_the_requested_commit() {
        let source = SourceRepository::create();
        let root = temp_dir("root");
        let workspace = attempt_workspace(&root, "attempt-one", &source.first_commit);

        GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
            .provision(&workspace, &source.spec(&source.first_commit))
            .await
            .expect("the attempt is checked out");

        // The working tree is the requested commit, not merely "some clone".
        assert_eq!(head_of(&workspace), source.first_commit);
        assert_eq!(
            fs::read_to_string(workspace.path.join("README.md")).expect("tracked file"),
            "first\n"
        );
        assert!(
            !workspace.path.join("added.txt").exists(),
            "a file added by a later commit must not be present"
        );
        // Detached: no branch is checked out, so nothing the harness does can
        // move a ref the next attempt would inherit.
        assert_eq!(
            run_git(&workspace.path, &["rev-parse", "--abbrev-ref", "HEAD"]),
            "HEAD",
            "the checkout must be detached: nothing the harness does may move a branch"
        );
        assert_eq!(
            fs::read_to_string(workspace.path.join(CHECKOUT_MARKER)).expect("sentinel"),
            source.first_commit
        );
        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source.path).expect("cleanup");
    }

    #[tokio::test]
    async fn a_branch_name_resolves_through_the_remote_tracking_ref() {
        let source = SourceRepository::create();
        let root = temp_dir("root");
        let workspace = attempt_workspace(&root, "attempt-branch", "main");

        GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
            .provision(&workspace, &source.spec("main"))
            .await
            .expect("a branch name is a valid base revision");

        assert_eq!(head_of(&workspace), source.second_commit);
        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source.path).expect("cleanup");
    }

    #[tokio::test]
    async fn two_concurrent_attempts_cannot_see_each_others_files() {
        let source = SourceRepository::create();
        let root = temp_dir("root");
        let first = attempt_workspace(&root, "attempt-one", &source.first_commit);
        let second = attempt_workspace(&root, "attempt-two", &source.second_commit);
        let provisioner = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT);

        let first_spec = source.spec(&source.first_commit);
        let second_spec = source.spec(&source.second_commit);
        let (one, two) = tokio::join!(
            provisioner.provision(&first, &first_spec),
            provisioner.provision(&second, &second_spec),
        );
        one.expect("first attempt");
        two.expect("second attempt");

        assert_ne!(first.path, second.path);
        assert_eq!(head_of(&first), source.first_commit);
        assert_eq!(head_of(&second), source.second_commit);

        // A file one attempt's harness writes is invisible to the other, and
        // neither repository's git state is shared: the second attempt's
        // `added.txt` exists only there.
        fs::write(first.path.join("scratch.txt"), "work in progress").expect("harness write");
        assert!(!second.path.join("scratch.txt").exists());
        assert!(second.path.join("added.txt").exists());
        assert!(!first.path.join("added.txt").exists());
        assert_ne!(
            fs::canonicalize(first.path.join(".git")).expect("first git dir"),
            fs::canonicalize(second.path.join(".git")).expect("second git dir"),
        );
        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source.path).expect("cleanup");
    }

    #[tokio::test]
    async fn the_attempt_checkout_stays_owner_only() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let source = SourceRepository::create();
            let root = temp_dir("root");
            let workspace = attempt_workspace(&root, "attempt-one", &source.first_commit);

            GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
                .provision(&workspace, &source.spec(&source.first_commit))
                .await
                .expect("checkout");

            let mode = fs::metadata(&workspace.path)
                .expect("attempt directory")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(
                mode, 0o700,
                "provisioning must not widen the attempt directory"
            );
            let sentinel = fs::metadata(workspace.path.join(CHECKOUT_MARKER))
                .expect("sentinel")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(sentinel, 0o600);
            fs::remove_dir_all(root).expect("cleanup");
            fs::remove_dir_all(source.path).expect("cleanup");
        }
    }

    // -----------------------------------------------------------------
    // Crash safety: no half-made checkout is ever inherited.
    // -----------------------------------------------------------------

    /// A genuine kill mid-provision: a one-millisecond budget makes the timeout
    /// fire while `git` is still working, and `kill_on_drop` SIGKILLs it — the
    /// same state a killed runner leaves behind. The restart must then produce
    /// a correct checkout rather than inheriting the wreckage.
    #[tokio::test]
    async fn a_runner_killed_mid_provision_leaves_nothing_a_restart_inherits() {
        let source = SourceRepository::create();
        let root = temp_dir("root");
        let workspace = attempt_workspace(&root, "attempt-one", &source.first_commit);

        let killed = GitWorktreeProvisioner::new(git_program(), Duration::from_micros(1))
            .provision(&workspace, &source.spec(&source.first_commit))
            .await;
        assert!(
            killed.is_err(),
            "the interrupted provision must not report success"
        );
        assert!(
            !workspace.path.join(CHECKOUT_MARKER).exists(),
            "an interrupted provision must never leave a completion sentinel"
        );
        assert!(
            workspace.path.join(ATTEMPT_MARKER).exists(),
            "the attempt marker identifies the directory and must survive"
        );

        GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
            .provision(&workspace, &source.spec(&source.first_commit))
            .await
            .expect("the restart provisions a usable checkout");
        assert_eq!(head_of(&workspace), source.first_commit);
        assert_eq!(
            fs::read_to_string(workspace.path.join("README.md")).expect("tracked file"),
            "first\n"
        );
        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source.path).expect("cleanup");
    }

    #[tokio::test]
    async fn a_partial_checkout_without_a_sentinel_is_discarded_not_reused() {
        let source = SourceRepository::create();
        let root = temp_dir("root");
        let workspace = attempt_workspace(&root, "attempt-one", &source.first_commit);
        // The debris a kill can leave: a half-written git directory, a stray
        // file, and a nested directory — but no sentinel.
        fs::create_dir_all(workspace.path.join(".git/objects")).expect("partial git dir");
        fs::write(workspace.path.join(".git/HEAD"), "garbage").expect("partial HEAD");
        fs::write(workspace.path.join("stale.txt"), "left by a dead attempt").expect("stale file");

        GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
            .provision(&workspace, &source.spec(&source.first_commit))
            .await
            .expect("a partial checkout is replaced, not repaired");

        assert_eq!(head_of(&workspace), source.first_commit);
        assert!(
            !workspace.path.join("stale.txt").exists(),
            "debris from an interrupted attempt must not survive into the new checkout"
        );
        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source.path).expect("cleanup");
    }

    #[tokio::test]
    async fn a_sentinel_that_disagrees_with_head_is_not_trusted() {
        let source = SourceRepository::create();
        let root = temp_dir("root");
        let workspace = attempt_workspace(&root, "attempt-one", &source.first_commit);
        let provisioner = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT);
        provisioner
            .provision(&workspace, &source.spec(&source.first_commit))
            .await
            .expect("first checkout");

        // A sentinel naming a commit the working tree is not on is the exact
        // shape of a torn write. Re-provisioning must rebuild rather than
        // believe the file.
        fs::write(workspace.path.join(CHECKOUT_MARKER), &source.second_commit).expect("tear");
        fs::write(workspace.path.join("stale.txt"), "from the torn state").expect("stale");
        provisioner
            .provision(&workspace, &source.spec(&source.first_commit))
            .await
            .expect("rebuild");

        assert_eq!(head_of(&workspace), source.first_commit);
        assert!(!workspace.path.join("stale.txt").exists());
        assert_eq!(
            fs::read_to_string(workspace.path.join(CHECKOUT_MARKER)).expect("sentinel"),
            source.first_commit
        );
        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source.path).expect("cleanup");
    }

    #[tokio::test]
    async fn a_completed_checkout_is_reused_on_restart_instead_of_refetched() {
        let source = SourceRepository::create();
        let root = temp_dir("root");
        let workspace = attempt_workspace(&root, "attempt-one", &source.first_commit);
        let provisioner = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT);
        provisioner
            .provision(&workspace, &source.spec(&source.first_commit))
            .await
            .expect("first checkout");
        // Work in progress from before the restart. Reuse is asserted by its
        // survival, not by timing: a re-provision would purge it.
        fs::write(workspace.path.join("in-progress.txt"), "harness output").expect("write");

        provisioner
            .provision(&workspace, &source.spec(&source.first_commit))
            .await
            .expect("restart reuses the existing checkout");

        assert!(workspace.path.join("in-progress.txt").exists());
        assert_eq!(head_of(&workspace), source.first_commit);
        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source.path).expect("cleanup");
    }

    #[tokio::test]
    async fn a_checkout_of_a_different_revision_is_never_reused() {
        let source = SourceRepository::create();
        let root = temp_dir("root");
        let workspace = attempt_workspace(&root, "attempt-one", &source.first_commit);
        let provisioner = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT);
        provisioner
            .provision(&workspace, &source.spec(&source.first_commit))
            .await
            .expect("first checkout");

        provisioner
            .provision(&workspace, &source.spec(&source.second_commit))
            .await
            .expect("second revision");

        assert_eq!(head_of(&workspace), source.second_commit);
        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source.path).expect("cleanup");
    }

    // -----------------------------------------------------------------
    // Typed failures. Unsupported is typed; nothing is faked as success.
    // -----------------------------------------------------------------

    #[tokio::test]
    async fn a_revision_that_does_not_exist_is_typed_and_writes_no_sentinel() {
        let source = SourceRepository::create();
        let root = temp_dir("root");
        let missing = "0123456789abcdef0123456789abcdef01234567";
        let workspace = attempt_workspace(&root, "attempt-one", missing);

        let error = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
            .provision(&workspace, &source.spec(missing))
            .await
            .expect_err("a missing revision cannot succeed");

        assert_eq!(error, WorkspaceError::RevisionUnavailable);
        assert!(!workspace.path.join(CHECKOUT_MARKER).exists());
        assert!(
            !workspace.path.join(".git").exists(),
            "a failed provision leaves no repository a later attempt could mistake for a checkout"
        );
        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source.path).expect("cleanup");
    }

    #[tokio::test]
    async fn an_unreachable_repository_is_typed_as_unreachable() {
        let root = temp_dir("root");
        let workspace = attempt_workspace(&root, "attempt-one", "main");
        let repository = RepositorySpec {
            remote: root
                .join("no-such-repository")
                .to_string_lossy()
                .into_owned(),
            base_revision: "main".into(),
        };

        let error = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
            .provision(&workspace, &repository)
            .await
            .expect_err("an absent remote cannot succeed");

        assert_eq!(error, WorkspaceError::RepositoryUnreachable);
        assert!(!workspace.path.join(CHECKOUT_MARKER).exists());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[tokio::test]
    async fn a_missing_git_binary_is_typed_not_a_generic_io_failure() {
        let source = SourceRepository::create();
        let root = temp_dir("root");
        let workspace = attempt_workspace(&root, "attempt-one", &source.first_commit);

        let error =
            GitWorktreeProvisioner::new("tack-runner-no-such-git-binary", Duration::from_secs(5))
                .provision(&workspace, &source.spec(&source.first_commit))
                .await
                .expect_err("a missing git binary cannot succeed");

        assert_eq!(error, WorkspaceError::GitUnavailable);
        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source.path).expect("cleanup");
    }

    /// Writes an executable stand-in and waits until it is actually
    /// executable. The wait is not superstition: another test thread forking
    /// while this file is open for writing inherits that descriptor, and the
    /// exec then fails with `ETXTBSY`. Probing until a trivial invocation
    /// succeeds removes a race that otherwise makes this test flaky under
    /// parallel load — which is how it was found.
    #[cfg(unix)]
    fn stand_in_program(directory: &Path, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let program = directory.join(name);
        fs::write(
            &program,
            format!("#!/bin/sh\n[ \"$1\" = \"--probe\" ] && exit 0\n{body}\n"),
        )
        .expect("stand-in program");
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).expect("executable");
        for _ in 0..200 {
            match SyncCommand::new(&program).arg("--probe").status() {
                Ok(status) if status.success() => return program,
                _ => std::thread::sleep(Duration::from_millis(10)),
            }
        }
        panic!("stand-in program never became executable");
    }

    /// Deterministic proof of the timeout, independent of how fast real git
    /// is: a stand-in `git` that never returns. Without `kill_on_drop` this
    /// test would hang instead of failing.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_hanging_git_is_killed_and_reported_as_a_timeout() {
        let root = temp_dir("root");
        let bin = temp_dir("bin");
        let program = stand_in_program(&bin, "git", "sleep 30");
        let workspace = attempt_workspace(&root, "attempt-one", "main");
        let repository = RepositorySpec {
            remote: "https://example.invalid/repository.git".into(),
            base_revision: "main".into(),
        };

        let started = std::time::Instant::now();
        let error = GitWorktreeProvisioner::new(&program, Duration::from_millis(200))
            .provision(&workspace, &repository)
            .await
            .expect_err("a hanging git cannot succeed");

        assert_eq!(error, WorkspaceError::GitTimeout);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the timeout must bound the call, not wait for the child"
        );
        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(bin).expect("cleanup");
    }

    // -----------------------------------------------------------------
    // Refusals. The provisioner deletes files, so it re-proves ownership.
    // -----------------------------------------------------------------

    #[tokio::test]
    async fn a_directory_marked_for_another_attempt_is_refused_and_untouched() {
        let source = SourceRepository::create();
        let root = temp_dir("root");
        let workspace = attempt_workspace(&root, "attempt-one", &source.first_commit);
        fs::write(workspace.path.join(ATTEMPT_MARKER), "a-different-attempt").expect("marker");
        fs::write(
            workspace.path.join("evidence.txt"),
            "belongs to another attempt",
        )
        .expect("evidence");

        let error = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
            .provision(&workspace, &source.spec(&source.first_commit))
            .await
            .expect_err("another attempt's directory is refused");

        assert_eq!(error, WorkspaceError::AttemptMismatch);
        // The claim under test is "writes nothing", so absence is asserted
        // directly: the refusal must not have deleted the other attempt's work.
        assert_eq!(
            fs::read_to_string(workspace.path.join("evidence.txt")).expect("evidence survives"),
            "belongs to another attempt"
        );
        assert_eq!(
            fs::read_dir(&workspace.path).expect("read").count(),
            2,
            "no entry was created or removed by the refusal"
        );
        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source.path).expect("cleanup");
    }

    #[tokio::test]
    async fn a_directory_with_no_marker_is_refused_and_untouched() {
        let source = SourceRepository::create();
        let root = temp_dir("root");
        let path = root.join("unmarked");
        fs::create_dir_all(&path).expect("directory");
        fs::write(path.join("evidence.txt"), "not ours").expect("evidence");
        let workspace = Workspace {
            attempt_id: AttemptId::new("attempt-one"),
            id: WorkspaceId::new("ws_unmarked"),
            path: path.clone(),
            base_revision: source.first_commit.clone(),
        };

        let error = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
            .provision(&workspace, &source.spec(&source.first_commit))
            .await
            .expect_err("an unmarked directory is refused");

        assert_eq!(error, WorkspaceError::UnsafePath);
        assert!(path.join("evidence.txt").exists());
        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source.path).expect("cleanup");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_symlinked_attempt_path_is_refused_before_anything_is_written() {
        use std::os::unix::fs::symlink;

        let source = SourceRepository::create();
        let root = temp_dir("root");
        let victim = root.join("victim");
        fs::create_dir_all(&victim).expect("victim");
        fs::write(victim.join(ATTEMPT_MARKER), "attempt-one").expect("marker");
        fs::write(victim.join("important.txt"), "do not delete").expect("important");
        let link = root.join("attempt-one");
        symlink(&victim, &link).expect("symlink");
        let workspace = Workspace {
            attempt_id: AttemptId::new("attempt-one"),
            id: WorkspaceId::new("ws_link"),
            path: link,
            base_revision: source.first_commit.clone(),
        };

        let error = GitWorktreeProvisioner::new(git_program(), DEFAULT_GIT_TIMEOUT)
            .provision(&workspace, &source.spec(&source.first_commit))
            .await
            .expect_err("a symlinked attempt path is refused");

        assert_eq!(error, WorkspaceError::UnsafePath);
        assert!(victim.join("important.txt").exists());
        fs::remove_dir_all(root).expect("cleanup");
        fs::remove_dir_all(source.path).expect("cleanup");
    }

    // -----------------------------------------------------------------
    // Rule 12: a credential embedded in the remote never reaches a log.
    // -----------------------------------------------------------------

    // Capturing what actually reached a log line needs a *global* subscriber,
    // not a scoped one: `tracing` caches per-callsite interest process-wide,
    // so a callsite first evaluated by another test (with no subscriber
    // installed) is cached as "never interested" and a later thread-local
    // subscriber never sees it. That is exactly how this assertion silently
    // passed on nothing when the suite ran in parallel. Installing the
    // subscriber globally rebuilds the interest cache; a thread-local buffer
    // then keeps this test's captured output separate from every other
    // test's, so the assertion stays about this call and nothing else.
    thread_local! {
        static CAPTURED: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    struct ThreadLocalCapture;

    impl std::io::Write for ThreadLocalCapture {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            CAPTURED.with(|captured| captured.borrow_mut().extend_from_slice(buffer));
            Ok(buffer.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl tracing_subscriber::fmt::MakeWriter<'_> for ThreadLocalCapture {
        type Writer = Self;

        fn make_writer(&self) -> Self::Writer {
            ThreadLocalCapture
        }
    }

    fn install_log_capture() {
        static INSTALLED: std::sync::Once = std::sync::Once::new();
        INSTALLED.call_once(|| {
            tracing_subscriber::fmt()
                .with_writer(ThreadLocalCapture)
                .with_max_level(tracing::Level::DEBUG)
                .with_ansi(false)
                .init();
        });
        CAPTURED.with(|captured| captured.borrow_mut().clear());
    }

    #[tokio::test]
    async fn a_credential_in_the_remote_url_never_reaches_a_log_line() {
        const PASSWORD: &str = "canary-git-password-9f3a";
        let root = temp_dir("root");
        let workspace = attempt_workspace(&root, "attempt-one", "main");
        let remote =
            format!("https://tack-user:{PASSWORD}@127.0.0.1:1/org/repo.git?token={PASSWORD}");
        let repository = RepositorySpec {
            remote: remote.clone(),
            base_revision: "main".into(),
        };

        install_log_capture();
        let error = GitWorktreeProvisioner::new(git_program(), Duration::from_secs(20))
            .provision(&workspace, &repository)
            .await
            .expect_err("an unreachable remote cannot succeed");

        assert_eq!(error, WorkspaceError::RepositoryUnreachable);
        let captured = CAPTURED
            .with(|captured| String::from_utf8(captured.borrow().clone()))
            .expect("utf-8");
        assert!(
            captured.contains("git command failed") && captured.contains("attempt checkout failed"),
            "the test is only load-bearing if git's failure was actually logged: {captured:?}"
        );
        assert!(
            !captured.contains(PASSWORD),
            "a credential embedded in the remote reached a log line: {captured}"
        );
        assert!(
            !captured.contains("tack-user"),
            "the remote's user reached a log line: {captured}"
        );
        assert!(
            !format!("{error}").contains(PASSWORD),
            "the typed error carries credential material"
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn redacted_output_strips_userinfo_and_query_strings() {
        let remote = "https://user:secret-token@example.invalid/org/repo.git?key=secret-token";
        let output = GitOutput {
            success: false,
            stdout: String::new(),
            stderr: format!(
                "fatal: could not read from {remote}\nfatal: authentication failed for 'https://user:secret-token@example.invalid/org/repo.git'"
            ),
        };

        let redacted = output.redacted_stderr(&remote_secrets(remote));

        assert!(!redacted.contains("secret-token"));
        assert!(!redacted.contains("user:secret-token"));
        assert!(!redacted.contains("?key="));
        assert!(redacted.contains("fatal"), "the diagnosis itself survives");
    }

    #[test]
    fn a_remote_without_userinfo_yields_no_spurious_secrets() {
        assert!(url_secrets("https://example.invalid/org/repo.git").is_empty());
        assert!(url_secrets("/var/lib/repositories/repo.git").is_empty());
        assert_eq!(
            url_secrets("https://user:pass@example.invalid/repo.git"),
            vec!["user:pass".to_owned(), "user".to_owned(), "pass".to_owned()]
        );
    }

    #[test]
    fn only_a_full_commit_id_is_treated_as_one() {
        assert!(is_full_commit_id(
            "0123456789abcdef0123456789abcdef01234567"
        ));
        assert!(!is_full_commit_id("main"));
        assert!(!is_full_commit_id("0123456"));
        assert!(!is_full_commit_id(
            "z123456789abcdef0123456789abcdef01234567"
        ));
    }
}
