//! Bounded, cancellable child-process supervision.
//!
//! This is the seam a concrete [`super::HarnessAdapter`] (`engine::HarnessAdapter`)
//! implementation composes inside `validate`/`start`/`cancel`/`wait`: it owns
//! spawning the harness CLI, capturing its stdout/stderr under a hard memory
//! bound, enforcing a timeout, and killing the *entire* descendant process
//! tree on cancellation — not just the direct child, which a plain
//! `Child::kill()` would leave behind. A long-running harness must never
//! buffer an entire run in memory, and a cancelled attempt must not leave
//! orphaned descendants running.
//!
//! ## Process-group cancellation
//!
//! On Unix, [`ProcessSpec::spawn`] places the child in a **new** process
//! group whose id equals the child's own pid (`process_group(0)`, mirroring
//! `setsid`-style detachment from the runner's own group). Any descendant the
//! child spawns without calling `setpgid` itself inherits that same group.
//! Cancellation sends the signal to the *group* (`kill(-pgid, sig)`), which
//! is why it also reaches grandchildren. This needs exactly one raw `libc`
//! symbol (`kill(2)`); see the module-level note on why that is declared via
//! a bare `extern "C"` block instead of adding the `libc` crate as a
//! dependency.
//!
//! Non-Unix targets fall back to killing only the direct child
//! (`tokio::process::Child::kill`), matching the same best-effort pattern
//! already used for non-Unix permissions in `workspace.rs`/`journal.rs`; this
//! is a documented limitation, not a silent gap.

use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use thiserror::Error;
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
    time,
};

use super::redact::{RedactedEnv, SecretMaterial};

/// What to launch and where. `working_directory` must be the attempt's own
/// workspace path (or a descendant of it) — [`ProcessSpec::spawn`] refuses to
/// start a process whose working directory escapes `workspace_root`, which is
/// the structural half of "adapters cannot cross-read each other's
/// workspaces" (the empirical half is that each attempt's process only ever
/// receives its own workspace path as `current_dir`, so a relative read never
/// resolves into a sibling attempt's files).
#[derive(Clone)]
pub struct ProcessSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// The child's *complete* environment. Spawning always starts from a
    /// cleared environment (never the runner's own) — see the module docs on
    /// why silently inheriting the host environment would itself be a rule-12
    /// leak. The adapter must include everything the harness needs (`PATH`
    /// included, if it shells out).
    pub env: BTreeMap<String, String>,
    /// Piped to the child's stdin and closed. Prefer this over `args` for a
    /// prompt body or any other large/sensitive payload: `args` becomes the
    /// process's argv, world-readable via `ps`/`/proc/<pid>/cmdline` on a
    /// shared host, while stdin is not.
    pub stdin: Option<Vec<u8>>,
    pub working_directory: PathBuf,
    pub workspace_root: PathBuf,
}

impl std::fmt::Debug for ProcessSpec {
    /// Same rationale as `EnrollmentCredential`/`RunnerCredential`: this type
    /// is trivially reachable from a `?` early return or a panic message, so
    /// its `Debug` must be safe to print unconditionally rather than relying
    /// on every call site to remember not to.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProcessSpec")
            .field("program", &self.program)
            .field("args", &"[REDACTED]")
            .field("env", &RedactedEnv(&self.env))
            .field("stdin", &self.stdin.as_ref().map(|_| "[REDACTED]"))
            .field("working_directory", &self.working_directory)
            .field("workspace_root", &self.workspace_root)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessLimits {
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
    pub timeout: Duration,
    /// Grace period between SIGTERM and SIGKILL on cancellation/timeout.
    pub termination_grace: Duration,
}

impl ProcessLimits {
    pub const fn new(max_stdout_bytes: usize, max_stderr_bytes: usize, timeout: Duration) -> Self {
        Self {
            max_stdout_bytes,
            max_stderr_bytes,
            timeout,
            termination_grace: Duration::from_secs(5),
        }
    }
}

/// One captured stream, bounded to a hard byte cap. Truncation is a field,
/// never a silently shortened string: a caller that only checks `.text` and
/// ignores `.truncated` still gets a correct (merely incomplete) value, but
/// nothing can *represent* "this is the whole stream" when it is not.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CapturedOutput {
    pub text: String,
    pub truncated: bool,
    pub bytes_dropped: u64,
    pub total_bytes_seen: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessExit {
    Exited(i32),
    #[cfg(unix)]
    Signaled(i32),
    /// The process did not exit within `ProcessLimits::timeout` and was
    /// killed (its whole group, on Unix).
    TimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessResult {
    pub exit: ProcessExit,
    pub stdout: CapturedOutput,
    pub stderr: CapturedOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOutcome {
    /// The group acknowledged SIGTERM within the grace period.
    Stopped,
    /// The group did not stop from SIGTERM alone and was sent SIGKILL.
    Killed,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProcessError {
    #[error("process working directory escapes its declared workspace root")]
    WorkspaceEscape,
    #[error("process could not be spawned")]
    Spawn,
    #[error("process io failed")]
    Io,
    #[error("process signal delivery failed")]
    Signal,
}

/// A spawned child under group-aware supervision. Constructed only via
/// [`ProcessSpec::spawn`].
pub struct SupervisedProcess {
    child: Child,
    pid: u32,
}

impl ProcessSpec {
    /// Validates workspace confinement, then spawns the child in its own
    /// process group (Unix). Confinement is checked with the same
    /// canonicalize-then-`starts_with` pattern `workspace.rs` uses for
    /// cleanup, for the same reason: a symlink or `..` component must be
    /// resolved before comparison, not after.
    pub async fn spawn(&self) -> Result<SupervisedProcess, ProcessError> {
        let root = self
            .workspace_root
            .canonicalize()
            .map_err(|_| ProcessError::WorkspaceEscape)?;
        let working_directory = self
            .working_directory
            .canonicalize()
            .map_err(|_| ProcessError::WorkspaceEscape)?;
        if working_directory != root && !working_directory.starts_with(&root) {
            return Err(ProcessError::WorkspaceEscape);
        }

        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .current_dir(&working_directory)
            .env_clear()
            .envs(&self.env)
            .kill_on_drop(true)
            .stdin(if self.stdin.is_some() {
                std::process::Stdio::piped()
            } else {
                std::process::Stdio::null()
            })
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        #[cfg(unix)]
        {
            // A new group whose id equals the child's own pid, independent
            // of the runner process's group.
            command.process_group(0);
        }

        let mut child = command.spawn().map_err(|_| ProcessError::Spawn)?;
        let pid = child.id().ok_or(ProcessError::Spawn)?;

        if let Some(input) = &self.stdin {
            use tokio::io::AsyncWriteExt;
            if let Some(mut stdin) = child.stdin.take() {
                // Best-effort: a harness that exits before reading stdin
                // (e.g. the `failure` fake-binary mode) makes this a broken
                // pipe, which is not itself a spawn failure — the exit/wait
                // path below is the authoritative outcome.
                let _ = stdin.write_all(input).await;
                drop(stdin);
            }
        }

        Ok(SupervisedProcess { child, pid })
    }
}

impl SupervisedProcess {
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Runs the child to completion (or timeout), capturing stdout/stderr
    /// under `limits`' byte caps and scrubbing `secrets` from whatever text
    /// is retained. Reader tasks keep draining each pipe even past the cap
    /// (dropping bytes instead of storing them) so a chatty child can never
    /// deadlock writing into a full OS pipe buffer while this future is
    /// waiting on something else.
    pub async fn wait_with_capture(
        mut self,
        limits: &ProcessLimits,
        secrets: &SecretMaterial,
    ) -> Result<ProcessResult, ProcessError> {
        let mut stdout_pipe = self.child.stdout.take().ok_or(ProcessError::Io)?;
        let mut stderr_pipe = self.child.stderr.take().ok_or(ProcessError::Io)?;
        let stdout_cap = limits.max_stdout_bytes;
        let stderr_cap = limits.max_stderr_bytes;

        let stdout_task =
            tokio::spawn(async move { capture_bounded(&mut stdout_pipe, stdout_cap).await });
        let stderr_task =
            tokio::spawn(async move { capture_bounded(&mut stderr_pipe, stderr_cap).await });

        let exit = match time::timeout(limits.timeout, self.child.wait()).await {
            Ok(Ok(status)) => status_to_exit(status),
            Ok(Err(_)) => return Err(ProcessError::Io),
            Err(_) => {
                kill_tree(self.pid, &mut self.child, limits.termination_grace).await?;
                ProcessExit::TimedOut
            }
        };

        let stdout_raw = stdout_task.await.map_err(|_| ProcessError::Io)?;
        let stderr_raw = stderr_task.await.map_err(|_| ProcessError::Io)?;
        Ok(ProcessResult {
            exit,
            stdout: finalize_capture(stdout_raw, secrets),
            stderr: finalize_capture(stderr_raw, secrets),
        })
    }

    /// Requests cancellation: SIGTERM to the whole group, then SIGKILL after
    /// `grace` if the group has not stopped. This kills descendants the
    /// child itself spawned, not only the direct child — see the module
    /// docs. Consumes `self` because a cancelled process must never be
    /// waited on again by the caller; [`Self::wait_with_capture`] already
    /// reaps it as part of killing the tree.
    pub async fn cancel(mut self, grace: Duration) -> Result<CancelOutcome, ProcessError> {
        kill_tree(self.pid, &mut self.child, grace).await
    }
}

async fn kill_tree(
    pid: u32,
    child: &mut Child,
    grace: Duration,
) -> Result<CancelOutcome, ProcessError> {
    #[cfg(unix)]
    {
        unix::signal_group(pid, unix::SIGTERM).map_err(|_| ProcessError::Signal)?;
        if time::timeout(grace, child.wait()).await.is_ok() {
            return Ok(CancelOutcome::Stopped);
        }
        unix::signal_group(pid, unix::SIGKILL).map_err(|_| ProcessError::Signal)?;
        let _ = time::timeout(Duration::from_secs(5), child.wait()).await;
        Ok(CancelOutcome::Killed)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        let _ = grace;
        // Documented limitation: no portable process-group primitive here,
        // so only the direct child is targeted.
        let _ = child.start_kill();
        let _ = child.wait().await;
        Ok(CancelOutcome::Killed)
    }
}

/// Reads `pipe` to EOF, retaining at most `cap` bytes but always continuing
/// to drain past the cap so the writer end never blocks on a full pipe.
async fn capture_bounded(pipe: &mut (impl tokio::io::AsyncRead + Unpin), cap: usize) -> RawCapture {
    let mut buffer = Vec::with_capacity(cap.min(64 * 1024));
    let mut total: u64 = 0;
    let mut chunk = [0u8; 32 * 1024];
    loop {
        match pipe.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => {
                total += n as u64;
                if buffer.len() < cap {
                    let room = cap - buffer.len();
                    buffer.extend_from_slice(&chunk[..n.min(room)]);
                }
                // Bytes beyond `cap` are intentionally never stored.
            }
            Err(_) => break,
        }
    }
    RawCapture { buffer, total }
}

struct RawCapture {
    buffer: Vec<u8>,
    total: u64,
}

fn finalize_capture(raw: RawCapture, secrets: &SecretMaterial) -> CapturedOutput {
    let truncated = raw.total > raw.buffer.len() as u64;
    let bytes_dropped = raw.total - raw.buffer.len() as u64;
    let text = secrets.scrub(&String::from_utf8_lossy(&raw.buffer));
    CapturedOutput {
        text,
        truncated,
        bytes_dropped,
        total_bytes_seen: raw.total,
    }
}

fn status_to_exit(status: std::process::ExitStatus) -> ProcessExit {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return ProcessExit::Signaled(signal);
        }
    }
    ProcessExit::Exited(status.code().unwrap_or(-1))
}

/// Returns whether a process with this pid currently exists, via `kill(pid,
/// 0)` (sends no signal; only checks deliverability). Used by tests to prove
/// a cancelled descendant is actually gone rather than merely unresponsive.
#[cfg(unix)]
pub fn process_alive(pid: u32) -> bool {
    unix::process_alive(pid)
}

#[cfg(unix)]
mod unix {
    use std::io;

    // A bare `extern "C"` block avoids adding the `libc` crate as a new
    // `tack-runner` dependency for the one POSIX syscall this module needs. `libc` is already resolved
    // transitively in `Cargo.lock` (tokio depends on it), but Cargo does not
    // let a crate call into a dependency it has not declared directly, so
    // that transitive presence cannot be relied on here. `kill(2)`'s
    // signature is part of the stable, decades-old POSIX ABI linked into
    // every Unix Rust binary already (via the platform's libc), so this
    // declaration is not itself a new dependency in any practical sense.
    unsafe extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }

    pub const SIGTERM: i32 = 15;
    pub const SIGKILL: i32 = 9;

    /// POSIX ESRCH ("no such process"). Stable at 3 across Linux, macOS and
    /// the BSDs, the only platforms this `#[cfg(unix)]` module targets.
    const ESRCH: i32 = 3;

    /// Sends `sig` to the process **group** led by `pid` (a negative pid
    /// argument to `kill(2)` targets the whole group). `pid` must be a group
    /// leader this runner itself spawned with `process_group(0)` — never an
    /// arbitrary or externally supplied pid.
    pub fn signal_group(pid: u32, sig: i32) -> io::Result<()> {
        let result = unsafe { kill(-(pid as i32), sig) };
        if result == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(ESRCH) {
            // The whole group is already gone: cancelling an
            // already-terminated attempt is idempotent, not an error.
            return Ok(());
        }
        Err(error)
    }

    pub fn process_alive(pid: u32) -> bool {
        unsafe { kill(pid as i32, 0) == 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::fixtures::fake_harness_command;
    use std::path::Path;

    /// A scratch directory that removes itself, and everything written under
    /// it, when the returned guard drops — including when an assertion panics
    /// first.
    fn temp_workspace(label: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(label)
            .tempdir()
            .expect("temporary directory")
    }

    fn spec(workspace: &Path, env: BTreeMap<String, String>) -> ProcessSpec {
        let (program, args) = fake_harness_command();
        ProcessSpec {
            program,
            args,
            env,
            stdin: None,
            working_directory: workspace.to_path_buf(),
            workspace_root: workspace.to_path_buf(),
        }
    }

    fn env_with_mode(mode: &str) -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        env.insert("TACK_FAKE_HARNESS_MODE".to_owned(), mode.to_owned());
        env
    }

    fn generous_limits() -> ProcessLimits {
        ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(10))
    }

    #[tokio::test]
    async fn success_mode_exits_cleanly_and_captures_stdout() {
        let workspace_dir = temp_workspace("success");
        let workspace = workspace_dir.path();
        let process = spec(workspace, env_with_mode("success"))
            .spawn()
            .await
            .expect("spawn");
        let result = process
            .wait_with_capture(&generous_limits(), &SecretMaterial::new())
            .await
            .expect("wait");

        assert_eq!(result.exit, ProcessExit::Exited(0));
        assert!(result.stdout.text.contains("fake-harness-ok"));
        assert!(!result.stdout.truncated);
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    #[tokio::test]
    async fn failure_mode_reports_configured_exit_code() {
        let workspace_dir = temp_workspace("failure");
        let workspace = workspace_dir.path();
        let mut env = env_with_mode("failure");
        env.insert("TACK_FAKE_HARNESS_EXIT_CODE".to_owned(), "17".to_owned());
        let process = spec(workspace, env).spawn().await.expect("spawn");
        let result = process
            .wait_with_capture(&generous_limits(), &SecretMaterial::new())
            .await
            .expect("wait");

        assert_eq!(result.exit, ProcessExit::Exited(17));
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    /// Acceptance: "adapters cannot cross-read each other's workspaces" —
    /// structural half. A spec whose working directory is not inside its
    /// declared workspace root is refused before anything is spawned.
    #[tokio::test]
    async fn spawn_refuses_a_working_directory_outside_its_workspace_root() {
        let workspace_dir = temp_workspace("escape-root");
        let workspace = workspace_dir.path();
        let sibling_dir = temp_workspace("escape-sibling");
        let sibling = sibling_dir.path();
        let mut escaping = spec(workspace, env_with_mode("success"));
        escaping.working_directory = sibling.to_path_buf();

        assert!(matches!(
            escaping.spawn().await,
            Err(ProcessError::WorkspaceEscape)
        ));
        std::fs::remove_dir_all(workspace).expect("cleanup");
        std::fs::remove_dir_all(sibling).expect("cleanup");
    }

    /// Acceptance: "adapters cannot cross-read each other's workspaces" —
    /// empirical half, for the non-adversarial case this layer actually
    /// defends against: two attempts get two real, distinct workspace
    /// directories, each with its own file at the *same relative name*. A
    /// process spawned into workspace A and told to read `./canary.txt`
    /// must see A's own content and never B's, proving workspace assignment
    /// is never accidentally shared or aliased across attempts. (A harness
    /// that deliberately path-traverses via `../` once running is a
    /// different, OS-sandboxing problem — chroot/namespaces/landlock — not
    /// attempted here.
    /// The structural half above, refusing a working directory outside its
    /// declared root before spawn, is what actually stops a *misconfigured*
    /// adapter from pointing at the wrong workspace in the first place.)
    #[tokio::test]
    async fn each_workspace_confined_process_only_ever_sees_its_own_canary_file() {
        let workspace_a_dir = temp_workspace("isolation-a");
        let workspace_a = workspace_a_dir.path();
        let workspace_b_dir = temp_workspace("isolation-b");
        let workspace_b = workspace_b_dir.path();
        std::fs::write(workspace_a.join("canary.txt"), "workspace-a-secret")
            .expect("write workspace a canary");
        std::fs::write(workspace_b.join("canary.txt"), "workspace-b-secret")
            .expect("write workspace b canary");

        let mut env_a = env_with_mode("read_relative");
        env_a.insert(
            "TACK_FAKE_HARNESS_READ_PATH".to_owned(),
            "canary.txt".to_owned(),
        );
        let read_a = spec(workspace_a, env_a)
            .spawn()
            .await
            .expect("spawn")
            .wait_with_capture(&generous_limits(), &SecretMaterial::new())
            .await
            .expect("wait");
        assert_eq!(read_a.exit, ProcessExit::Exited(0));
        assert!(read_a.stdout.text.contains("workspace-a-secret"));
        assert!(!read_a.stdout.text.contains("workspace-b-secret"));

        let mut env_b = env_with_mode("read_relative");
        env_b.insert(
            "TACK_FAKE_HARNESS_READ_PATH".to_owned(),
            "canary.txt".to_owned(),
        );
        let read_b = spec(workspace_b, env_b)
            .spawn()
            .await
            .expect("spawn")
            .wait_with_capture(&generous_limits(), &SecretMaterial::new())
            .await
            .expect("wait");
        assert_eq!(read_b.exit, ProcessExit::Exited(0));
        assert!(read_b.stdout.text.contains("workspace-b-secret"));
        assert!(!read_b.stdout.text.contains("workspace-a-secret"));

        std::fs::remove_dir_all(workspace_a).expect("cleanup");
        std::fs::remove_dir_all(workspace_b).expect("cleanup");
    }

    /// Acceptance: high-volume output stays memory-bounded. Drives 8 MiB of
    /// real stdout through a 64 KiB cap and asserts the captured buffer never
    /// exceeds the cap while the full byte count is still known and the
    /// process still exits cleanly (proving the drain-past-cap loop does not
    /// deadlock the child on a full pipe).
    #[tokio::test]
    async fn high_volume_output_is_memory_bounded_and_explicitly_truncated() {
        let workspace_dir = temp_workspace("high-volume");
        let workspace = workspace_dir.path();
        const VOLUME_BYTES: usize = 8 * 1024 * 1024;
        const CAP: usize = 64 * 1024;
        let mut env = env_with_mode("high_volume");
        env.insert(
            "TACK_FAKE_HARNESS_VOLUME_BYTES".to_owned(),
            VOLUME_BYTES.to_string(),
        );
        let limits = ProcessLimits::new(CAP, CAP, Duration::from_secs(30));
        let result = spec(workspace, env)
            .spawn()
            .await
            .expect("spawn")
            .wait_with_capture(&limits, &SecretMaterial::new())
            .await
            .expect("wait");

        assert_eq!(
            result.exit,
            ProcessExit::Exited(0),
            "child was not deadlocked"
        );
        assert!(
            result.stdout.text.len() <= CAP,
            "captured buffer must never exceed the configured cap"
        );
        assert!(result.stdout.truncated);
        assert_eq!(result.stdout.total_bytes_seen, VOLUME_BYTES as u64);
        assert_eq!(
            result.stdout.bytes_dropped,
            VOLUME_BYTES as u64 - result.stdout.text.len() as u64
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    /// Acceptance: cancel kills descendants. The fake binary spawns a
    /// grandchild `sleep` in the background (inheriting the same process
    /// group, since it does not call `setpgid` itself) and writes its pid to
    /// a file before waiting on it. Cancelling the direct child must also
    /// reap that grandchild, not merely the shell that spawned it.
    #[tokio::test]
    async fn cancel_kills_the_whole_descendant_tree_not_only_the_direct_child() {
        let workspace_dir = temp_workspace("cancel-tree");
        let workspace = workspace_dir.path();
        let pidfile = workspace.join("grandchild.pid");
        let mut env = env_with_mode("spawn_child");
        env.insert(
            "TACK_FAKE_HARNESS_PIDFILE".to_owned(),
            pidfile.to_str().expect("utf8 pidfile path").to_owned(),
        );
        env.insert(
            "TACK_FAKE_HARNESS_SLEEP_SECONDS".to_owned(),
            "3600".to_owned(),
        );
        let process = spec(workspace, env).spawn().await.expect("spawn");
        let direct_child_pid = process.pid();

        let grandchild_pid = wait_for_pidfile(&pidfile).await;
        assert!(
            process_alive(grandchild_pid),
            "grandchild must be observed running before cancellation"
        );

        let outcome = process
            .cancel(Duration::from_secs(2))
            .await
            .expect("cancel");
        assert!(matches!(
            outcome,
            CancelOutcome::Stopped | CancelOutcome::Killed
        ));

        // Give the kernel a brief moment to finish reaping; poll instead of a
        // single fixed sleep so this is not a hidden pacing dependency.
        assert!(
            wait_until_dead(grandchild_pid, Duration::from_secs(5)).await,
            "grandchild must be gone after cancelling its parent"
        );
        assert!(
            !process_alive(direct_child_pid)
                || wait_until_dead(direct_child_pid, Duration::from_secs(5)).await,
            "direct child must also be gone"
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    /// Acceptance: timeouts. A process that runs longer than the configured
    /// timeout is killed and reported as `TimedOut` rather than hanging the
    /// caller or being reported as any other terminal shape.
    #[tokio::test]
    async fn a_process_exceeding_its_timeout_is_killed_and_reported_as_timed_out() {
        let workspace_dir = temp_workspace("timeout");
        let workspace = workspace_dir.path();
        let mut env = env_with_mode("hang");
        env.insert(
            "TACK_FAKE_HARNESS_SLEEP_SECONDS".to_owned(),
            "3600".to_owned(),
        );
        let limits = ProcessLimits::new(4096, 4096, Duration::from_millis(50));
        let result = spec(workspace, env)
            .spawn()
            .await
            .expect("spawn")
            .wait_with_capture(&limits, &SecretMaterial::new())
            .await
            .expect("wait");

        assert_eq!(result.exit, ProcessExit::TimedOut);
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    /// Acceptance: secret canaries are absent from logs and events. A canary
    /// value is placed in the child's environment and stdin, and the fake
    /// binary is asked to actively echo it back on both stdout and stderr —
    /// simulating a worst-case leaky harness. The captured output must still
    /// not contain it once `SecretMaterial` has been told about it, and the
    /// `ProcessSpec`'s own `Debug` output must never contain it either.
    #[tokio::test]
    async fn secret_canaries_never_survive_into_captured_output_or_spec_debug() {
        let workspace_dir = temp_workspace("canary");
        let workspace = workspace_dir.path();
        const CANARY_ENV: &str = "tack-test-canary-env-73f1";
        const CANARY_STDIN: &str = "tack-test-canary-stdin-91ab";
        let mut env = env_with_mode("echo_canary");
        env.insert("TACK_TEST_SECRET".to_owned(), CANARY_ENV.to_owned());
        env.insert(
            "TACK_FAKE_HARNESS_ECHO_ENV_KEYS".to_owned(),
            "TACK_TEST_SECRET".to_owned(),
        );

        let mut process_spec = spec(workspace, env);
        process_spec.stdin = Some(CANARY_STDIN.as_bytes().to_vec());

        let debug_output = format!("{process_spec:?}");
        assert!(!debug_output.contains(CANARY_ENV));
        assert!(!debug_output.contains(CANARY_STDIN));

        let mut secrets = SecretMaterial::new();
        secrets.register(CANARY_ENV).register(CANARY_STDIN);

        let result = process_spec
            .spawn()
            .await
            .expect("spawn")
            .wait_with_capture(&generous_limits(), &secrets)
            .await
            .expect("wait");

        assert!(
            result.stdout.text.contains("[REDACTED]"),
            "the fake harness must actually have echoed something for this test to be meaningful"
        );
        assert!(!result.stdout.text.contains(CANARY_ENV));
        assert!(!result.stdout.text.contains(CANARY_STDIN));
        assert!(!result.stderr.text.contains(CANARY_ENV));
        assert!(!result.stderr.text.contains(CANARY_STDIN));
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    /// Not itself one of the five acceptance bullets, but every mode
    /// documented at the top of `fake_harness.sh` (`version`,
    /// `unknown_version`, `malformed`) is exercised here at least once, so a
    /// regression in the shared fixture is caught here rather than
    /// discovered later by whichever adapter reaches for it first.
    #[tokio::test]
    async fn every_documented_fixture_mode_behaves_as_documented() {
        let limits = generous_limits();

        let workspace_dir = temp_workspace("mode-version");
        let workspace = workspace_dir.path();
        let mut env = env_with_mode("version");
        env.insert("TACK_FAKE_HARNESS_VERSION".to_owned(), "9.9.9".to_owned());
        let result = spec(workspace, env)
            .spawn()
            .await
            .expect("spawn")
            .wait_with_capture(&limits, &SecretMaterial::new())
            .await
            .expect("wait");
        assert_eq!(result.exit, ProcessExit::Exited(0));
        assert!(result.stdout.text.contains("9.9.9"));
        std::fs::remove_dir_all(workspace).expect("cleanup");

        let workspace_dir = temp_workspace("mode-unknown-version");
        let workspace = workspace_dir.path();
        let result = spec(workspace, env_with_mode("unknown_version"))
            .spawn()
            .await
            .expect("spawn")
            .wait_with_capture(&limits, &SecretMaterial::new())
            .await
            .expect("wait");
        assert_eq!(result.exit, ProcessExit::Exited(0));
        assert!(result.stdout.text.contains("999.999.999"));
        std::fs::remove_dir_all(workspace).expect("cleanup");

        let workspace_dir = temp_workspace("mode-malformed");
        let workspace = workspace_dir.path();
        let result = spec(workspace, env_with_mode("malformed"))
            .spawn()
            .await
            .expect("spawn")
            .wait_with_capture(&limits, &SecretMaterial::new())
            .await
            .expect("wait");
        assert_eq!(result.exit, ProcessExit::Exited(0));
        assert!(
            serde_json::from_str::<serde_json::Value>(&result.stdout.text).is_err(),
            "malformed mode must actually produce unparseable output"
        );
        std::fs::remove_dir_all(workspace).expect("cleanup");
    }

    async fn wait_for_pidfile(path: &Path) -> u32 {
        for _ in 0..200 {
            if let Ok(contents) = std::fs::read_to_string(path)
                && let Ok(pid) = contents.trim().parse::<u32>()
            {
                return pid;
            }
            time::sleep(Duration::from_millis(25)).await;
        }
        panic!("grandchild pidfile was never written: {}", path.display());
    }

    async fn wait_until_dead(pid: u32, budget: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + budget;
        while tokio::time::Instant::now() < deadline {
            if !process_alive(pid) {
                return true;
            }
            time::sleep(Duration::from_millis(25)).await;
        }
        !process_alive(pid)
    }
}
