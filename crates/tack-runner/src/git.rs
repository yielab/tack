//! The real [`WorktreeProvisioner`]: a private, attempt-scoped git checkout.
//!
//! `git init` in place, not `git worktree add`: `worktree add` keeps administrative
//! state (a lock file, a `gitdir` pointer) inside one shared repository, so two
//! attempts provisioning at once would contend on that repository's index lock, and a
//! runner killed mid-add would leave a registered-but-absent worktree a later attempt
//! inherits. It also refuses a non-empty target directory, and every attempt
//! directory already carries the `.tack-attempt` marker
//! [`super::WorkspaceManager`] writes before provisioning. A private clone has neither
//! problem: every attempt owns 100% of its own repository state, and cleanup is a
//! plain recursive delete ([`super::WorkspaceManager::cleanup`]).
//!
//! Provisioning is not atomic — a checkout is thousands of files. The completion
//! sentinel [`CHECKOUT_MARKER`] is written (and fsynced) only after `checkout`
//! returns, recording the exact resolved commit. On restart the provisioner either
//! finds a sentinel that agrees with the live repository and reuses the checkout, or
//! discards everything under the attempt directory and provisions again — a
//! half-made checkout is never inherited.
//!
//! A remote URL can embed credentials and a query string, and git echoes the remote
//! back in most of its error messages. Raw git output is therefore treated as
//! tainted: scrubbed through [`SecretMaterial`] (seeded with the remote, its userinfo
//! and its password) and [`redact_query`] before it can reach a tracing field; the
//! typed errors this module returns carry no remote, path or git text at all.

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
use crate::evidence::{FileChange, FileOp, GitEvidence, PATCH_CAP_BYTES, PublishedBranch};
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

    async fn provision_scratch(&self, workspace: &Workspace) -> Result<(), WorkspaceError> {
        let path = workspace.path.as_path();
        let metadata = fs::symlink_metadata(path).map_err(|_| WorkspaceError::UnsafePath)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(WorkspaceError::UnsafePath);
        }
        let marker = fs::read_to_string(path.join(ATTEMPT_MARKER))
            .map_err(|_| WorkspaceError::UnsafePath)?;
        if marker != workspace.attempt_id.as_str() {
            return Err(WorkspaceError::AttemptMismatch);
        }
        let secrets = SecretMaterial::new();
        if path.join(CHECKOUT_MARKER).exists() && path.join(".git").exists() {
            return Ok(());
        }
        Self::purge_partial_checkout(path)?;
        self.git_ok(path, &["init", "--quiet"], &secrets).await?;
        self.git_ok(
            path,
            &[
                "-c",
                "user.name=tack",
                "-c",
                "user.email=tack@localhost",
                "commit",
                "--quiet",
                "--allow-empty",
                "--no-gpg-sign",
                "-m",
                "scratch",
            ],
            &secrets,
        )
        .await?;
        let head = self
            .git_ok(path, &["rev-parse", "--verify", "HEAD"], &secrets)
            .await?
            .stdout;
        write_checkout_marker(&path.join(CHECKOUT_MARKER), &head)
    }

    async fn capture_evidence(
        &self,
        workspace: &Workspace,
        exclude: &[&str],
    ) -> Result<Option<GitEvidence>, WorkspaceError> {
        let path = workspace.path.as_path();
        let secrets = SecretMaterial::new();
        // The clone is disposable, and staging is how untracked files and
        // deletions enter one diff. The runner's own markers are never the
        // harness's work, whatever the caller excludes.
        let mut stage = vec![
            "add".to_owned(),
            "-A".to_owned(),
            "--".to_owned(),
            ".".to_owned(),
        ];
        for name in exclude
            .iter()
            .copied()
            .chain([ATTEMPT_MARKER, CHECKOUT_MARKER])
        {
            stage.push(format!(":(exclude){name}"));
        }
        let stage: Vec<&str> = stage.iter().map(String::as_str).collect();
        self.git_ok(path, &stage, &secrets).await?;

        // The checkout was fetched by name into a fresh repository, where a
        // bare branch name resolves to nothing: diff against the commit it is.
        let base_commit = self
            .resolve_revision(path, &workspace.base_revision, &secrets)
            .await?;
        let base = base_commit.as_str();
        let head_commit = self
            .git_ok(path, &["rev-parse", "--verify", "HEAD"], &secrets)
            .await?
            .stdout;
        let dirty = self
            .git_ok(path, &["diff", "--cached", "--name-only", "HEAD"], &secrets)
            .await?;
        // `stdout` is lossy and trimmed, and a patch must reach the server
        // byte for byte, so git writes it to a file inside `.git` (never part
        // of the diff, deleted with the workspace) and it is read raw.
        let patch_file = path.join(".git").join("tack-evidence.patch");
        self.git_ok(
            path,
            &[
                "diff",
                "--cached",
                "--binary",
                "--no-color",
                "--no-ext-diff",
                "--output=.git/tack-evidence.patch",
                base,
            ],
            &secrets,
        )
        .await?;
        let mut patch = fs::read(&patch_file).map_err(|_| WorkspaceError::Io)?;
        let _ = fs::remove_file(&patch_file);
        let truncated = patch.len() > PATCH_CAP_BYTES;
        patch.truncate(PATCH_CAP_BYTES);
        let listing = self
            .git_ok(
                path,
                &["diff", "--cached", "--name-status", "-z", base],
                &secrets,
            )
            .await?;
        Ok(Some(GitEvidence {
            base_commit,
            head_commit,
            worktree_dirty: !dirty.stdout.is_empty(),
            files: parse_name_status(listing.stdout.as_bytes()),
            patch,
            truncated,
        }))
    }

    async fn publish_branch(
        &self,
        workspace: &Workspace,
        branch: &str,
        author: &str,
        message: &str,
    ) -> Result<Option<PublishedBranch>, WorkspaceError> {
        let path = workspace.path.as_path();
        // The remote may embed credentials and git echoes it in its errors.
        let remote = self
            .git_ok(
                path,
                &["remote", "get-url", "origin"],
                &SecretMaterial::new(),
            )
            .await?
            .stdout;
        let secrets = remote_secrets(&remote);
        let (name, email) = match author.split_once('<') {
            Some((name, rest)) => (name.trim(), rest.trim_end_matches('>').trim()),
            None => (author.trim(), ""),
        };
        let (user_name, user_email) = (format!("user.name={name}"), format!("user.email={email}"));
        // The harness wrote into this repository, so nothing it left in
        // `.git/hooks` may run on the operator's side of the push.
        let hooks = "core.hooksPath=/dev/null";

        self.git_ok(path, &["checkout", "-b", branch], &secrets)
            .await?;
        let staged = self
            .git(path, &["diff", "--cached", "--quiet", "HEAD"], &secrets)
            .await?;
        if !staged.success {
            self.git_ok(
                path,
                &[
                    "-c",
                    &user_name,
                    "-c",
                    &user_email,
                    "-c",
                    hooks,
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "--quiet",
                    "--no-verify",
                    "-m",
                    message,
                ],
                &secrets,
            )
            .await?;
        }
        let head_commit = self
            .git_ok(path, &["rev-parse", "--verify", "HEAD"], &secrets)
            .await?
            .stdout;
        self.git_ok(
            path,
            &["-c", hooks, "push", "--no-verify", "origin", branch],
            &secrets,
        )
        .await?;
        Ok(Some(PublishedBranch {
            branch: branch.to_owned(),
            head_commit,
            pushed: true,
        }))
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

/// Parses `git diff --name-status -z`: `M\0path\0`, and `R100\0old\0new\0`
/// for a rename or copy, which is reported under its new path.
fn parse_name_status(raw: &[u8]) -> Vec<FileChange> {
    let text = String::from_utf8_lossy(raw);
    let mut fields = text.split('\0').filter(|field| !field.is_empty());
    let mut files = Vec::new();
    while let Some(status) = fields.next() {
        let renamed = status.starts_with('R') || status.starts_with('C');
        let Some(first) = fields.next() else { break };
        let path = if renamed {
            fields.next().unwrap_or(first)
        } else {
            first
        };
        let op = match status.chars().next() {
            Some('A' | 'C') => FileOp::Added,
            Some('D') => FileOp::Deleted,
            Some('R') => FileOp::Renamed,
            _ => FileOp::Modified,
        };
        files.push(FileChange {
            path: path.to_owned(),
            op,
        });
    }
    files
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
#[path = "git/tests.rs"]
mod tests;
