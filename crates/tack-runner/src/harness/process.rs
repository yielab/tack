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

use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    time::Duration,
};

use thiserror::Error;
use tokio::{
    io::AsyncReadExt,
    process::{Child, Command},
    sync::mpsc,
    time,
};

use super::redact::{RedactedEnv, SecretMaterial};
use crate::client::{
    DecisionAnswer,
    engine::{Question, StreamSignal},
};

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
    /// After `stdin` is written, keep the pipe open instead of closing it —
    /// for a CLI that keeps reading (and keeps the process alive) past its
    /// first message. [`SupervisedProcess::wait_with_capture_and_questions`]
    /// is the only caller that later writes to it and closes it.
    pub keep_stdin_open: bool,
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
                if self.keep_stdin_open {
                    // Handed back so an interactive wait can keep writing to
                    // it and close it once the conversation is over; every
                    // other caller drops it here, closing the pipe now.
                    child.stdin = Some(stdin);
                } else {
                    drop(stdin);
                }
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

    /// The interactive counterpart of [`Self::wait_with_capture`], for a
    /// child spawned with `keep_stdin_open`. Reads stdout line by line into
    /// the same bounded head/tail capture, offering each line to `signal`;
    /// a [`StreamSignal::Question`] goes out on `questions_tx` and this loop
    /// blocks on `answers_rx` for the reply, which `answer` turns into the
    /// bytes written back to the child's stdin. A `StreamSignal::Finished`,
    /// a dropped answer channel, or EOF all close stdin and end the loop —
    /// the child's own exit (or the timeout) is what this method actually
    /// waits on. `signal`/`answer` stay generic closures rather than a
    /// `HarnessGrammar` reference so this module keeps no per-CLI knowledge.
    pub async fn wait_with_capture_and_questions(
        mut self,
        limits: &ProcessLimits,
        secrets: &SecretMaterial,
        mut signal: impl FnMut(&str) -> Option<StreamSignal> + Send,
        mut answer: impl FnMut(&Question, &DecisionAnswer) -> Vec<u8> + Send,
        questions_tx: mpsc::Sender<Question>,
        mut answers_rx: mpsc::Receiver<DecisionAnswer>,
    ) -> Result<ProcessResult, ProcessError> {
        let mut stdin = Some(self.child.stdin.take().ok_or(ProcessError::Io)?);
        let stdout_pipe = self.child.stdout.take().ok_or(ProcessError::Io)?;
        let mut stderr_pipe = self.child.stderr.take().ok_or(ProcessError::Io)?;
        let stderr_cap = limits.max_stderr_bytes;
        let stderr_task =
            tokio::spawn(async move { capture_bounded(&mut stderr_pipe, stderr_cap).await });

        let stdout_cap = limits.max_stdout_bytes;
        let read_and_drive = async move {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            let mut accumulator = BoundedAccumulator::new(stdout_cap);
            let mut reader = BufReader::new(stdout_pipe);
            let mut raw_line = Vec::new();
            loop {
                raw_line.clear();
                let Ok(n) = reader.read_until(b'\n', &mut raw_line).await else {
                    break;
                };
                if n == 0 {
                    break;
                }
                accumulator.push(&raw_line);
                let text = String::from_utf8_lossy(&raw_line);
                let text = text.trim_end_matches(['\n', '\r']);
                match signal(text) {
                    Some(StreamSignal::Question(mut question)) => {
                        // The same scrub captured output gets, applied before
                        // the question ever leaves this loop — a tool's
                        // command or path argument is ordinary content, not
                        // secret, but is scrubbed anyway in case it happens
                        // to carry one.
                        question.prompt = secrets.scrub(&question.prompt);
                        let mut metadata = serde_json::Value::Object(question.metadata);
                        secrets.scrub_json(&mut metadata);
                        question.metadata = match metadata {
                            serde_json::Value::Object(map) => map,
                            _ => serde_json::Map::new(),
                        };
                        // Nobody heard the question, or heard it and went
                        // away without answering: reply with the question's
                        // own deny option rather than leave the child
                        // blocked on stdin until the run's own timeout.
                        let resolved = if questions_tx.send(question.clone()).await.is_err() {
                            deny_option(&question)
                        } else {
                            answers_rx
                                .recv()
                                .await
                                .unwrap_or_else(|| deny_option(&question))
                        };
                        let bytes = answer(&question, &resolved);
                        let Some(pipe) = stdin.as_mut() else { break };
                        if pipe.write_all(&bytes).await.is_err()
                            || pipe.write_all(b"\n").await.is_err()
                        {
                            break;
                        }
                    }
                    // Closing stdin is what lets a CLI that keeps listening
                    // exit; its remaining output is still drained to EOF so
                    // it can never block on a full pipe.
                    Some(StreamSignal::Finished) => stdin = None,
                    None => {}
                }
            }
            drop(stdin);
            accumulator.finish()
        };
        tokio::pin!(read_and_drive);

        let deadline = time::Instant::now() + limits.timeout;
        let mut stdout_raw: Option<RawCapture> = None;
        let exit = {
            let child_wait = self.child.wait();
            tokio::pin!(child_wait);
            loop {
                tokio::select! {
                    biased;
                    status = &mut child_wait => {
                        let status = status.map_err(|_| ProcessError::Io)?;
                        break status_to_exit(status);
                    }
                    accumulator = &mut read_and_drive, if stdout_raw.is_none() => {
                        stdout_raw = Some(accumulator);
                    }
                    () = time::sleep_until(deadline) => {
                        break ProcessExit::TimedOut;
                    }
                }
            }
        };
        if exit == ProcessExit::TimedOut {
            kill_tree(self.pid, &mut self.child, limits.termination_grace).await?;
        }
        if stdout_raw.is_none() {
            stdout_raw = Some((&mut read_and_drive).await);
        }

        let stderr_raw = stderr_task.await.map_err(|_| ProcessError::Io)?;
        Ok(ProcessResult {
            exit,
            stdout: finalize_capture(
                stdout_raw.unwrap_or_else(|| RawCapture {
                    head: Vec::new(),
                    tail: VecDeque::new(),
                    total: 0,
                }),
                secrets,
            ),
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

/// The head/tail bookkeeping both a whole-stream capture
/// ([`capture_bounded`]) and the interactive line loop
/// ([`SupervisedProcess::wait_with_capture_and_questions`]) drive: the first
/// `cap / 2` bytes seen, kept once and never touched again, and the last
/// `cap - cap / 2` bytes, a ring that drops from the front as new bytes
/// arrive. One accumulator, fed from either a raw read or a decoded line, so
/// the two capture paths can never disagree about what "bounded" means.
struct BoundedAccumulator {
    head: Vec<u8>,
    head_cap: usize,
    tail: VecDeque<u8>,
    tail_cap: usize,
    total: u64,
}

impl BoundedAccumulator {
    fn new(cap: usize) -> Self {
        let head_cap = cap / 2;
        let tail_cap = cap - head_cap;
        Self {
            head: Vec::with_capacity(head_cap.min(64 * 1024)),
            head_cap,
            tail: VecDeque::with_capacity(tail_cap.min(64 * 1024)),
            tail_cap,
            total: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        self.total += bytes.len() as u64;
        let mut rest = bytes;
        if self.head.len() < self.head_cap {
            let room = self.head_cap - self.head.len();
            let take = room.min(rest.len());
            self.head.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
        }
        self.tail.extend(rest.iter().copied());
        while self.tail.len() > self.tail_cap {
            self.tail.pop_front();
        }
    }

    fn finish(self) -> RawCapture {
        RawCapture {
            head: self.head,
            tail: self.tail,
            total: self.total,
        }
    }
}

/// Reads `pipe` to EOF into a [`BoundedAccumulator`], always continuing to
/// drain past the cap so the writer end never blocks on a full pipe. A
/// stream that never exceeds `cap` fills `head` then `tail` without either
/// ever dropping anything, so the two concatenate back into the whole,
/// contiguous stream.
async fn capture_bounded(pipe: &mut (impl tokio::io::AsyncRead + Unpin), cap: usize) -> RawCapture {
    let mut accumulator = BoundedAccumulator::new(cap);
    let mut chunk = [0u8; 32 * 1024];
    loop {
        match pipe.read(&mut chunk).await {
            Ok(0) => break,
            Ok(n) => accumulator.push(&chunk[..n]),
            Err(_) => break,
        }
    }
    accumulator.finish()
}

struct RawCapture {
    head: Vec<u8>,
    tail: VecDeque<u8>,
    total: u64,
}

/// The answer nobody gave: by convention every grammar orders a question's
/// options with its safe default last (`decision.create.request.json`'s own
/// example: `allow_once` then `deny`), so this is always that one. The
/// engine falls back to the same convention on expiry, a failed create or a
/// failed poll — this is the same rule applied where a run's own interactive
/// loop finds nobody ever heard the question at all.
fn deny_option(question: &Question) -> DecisionAnswer {
    match question.options.last() {
        Some(option) => DecisionAnswer {
            option_id: Some(option.option_id.clone()),
            text: None,
        },
        None => DecisionAnswer {
            option_id: None,
            text: None,
        },
    }
}

/// Byte-identical to the raw capture when nothing was dropped (`head` and
/// `tail` are then a contiguous, gapless split of the same stream, so they
/// are rejoined as raw bytes before the one lossy UTF-8 decode — decoding
/// each half separately could split a multi-byte character right at the
/// join and render it differently than decoding the whole thing at once).
/// Once something is dropped, `head` and `tail` are no longer adjacent, so
/// they decode separately and join around one marker line naming the gap.
fn finalize_capture(raw: RawCapture, secrets: &SecretMaterial) -> CapturedOutput {
    let kept = raw.head.len() as u64 + raw.tail.len() as u64;
    let truncated = raw.total > kept;
    let bytes_dropped = raw.total - kept;
    let text = if truncated {
        let head_text = secrets.scrub(&String::from_utf8_lossy(&raw.head));
        let tail_bytes: Vec<u8> = raw.tail.into_iter().collect();
        let tail_text = secrets.scrub(&String::from_utf8_lossy(&tail_bytes));
        format!("{head_text}\n[\u{2026} {bytes_dropped} bytes dropped \u{2026}]\n{tail_text}")
    } else {
        let mut whole = raw.head;
        whole.extend(raw.tail);
        secrets.scrub(&String::from_utf8_lossy(&whole))
    };
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
#[path = "process/tests.rs"]
pub(crate) mod tests;
