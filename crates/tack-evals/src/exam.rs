//! The exam: the hidden checks run on a tree the agent can no longer reach.
//!
//! The agent's work comes back as a patch. It is applied to a fresh export of the seed
//! revision, the pristine `hidden/` is put over whatever the agent left under that name, and
//! the check runs there. The agent never sees the exam, and anything it wrote into `hidden/`
//! is replaced before the exam starts.
//!
//! The same exam runs with no agent at all over a task's reference solution
//! ([`check_solution`]): a task whose own solution fails it is broken.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::task::LoadedTask;
use crate::world::{copy_dir, copy_dir_except};

/// How long a hidden check may run.
pub const EXAM_TIMEOUT: Duration = Duration::from_secs(300);
const OUTPUT_TAIL_CHARS: usize = 4_000;

/// What the hidden check said.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExamRecord {
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    /// The end of the check's output: where a failure explains itself.
    pub output_tail: String,
}

/// Writes the tree of `revision` in `repo` into `dest`.
pub fn export_revision(repo: &Path, revision: &str, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    let mut archive = Command::new("git")
        .args(["archive", "--format=tar", revision])
        .current_dir(repo)
        .stdout(Stdio::piped())
        .spawn()
        .context("run git archive")?;
    let unpack = Command::new("tar")
        .args(["-x", "-C"])
        .arg(dest)
        .stdin(archive.stdout.take().context("piped archive")?)
        .status()
        .context("run tar")?;
    if !archive.wait()?.success() || !unpack.success() {
        bail!("could not export {revision} of the seed repository");
    }
    Ok(())
}

/// Applies a unified diff to `tree`.
pub fn apply_patch(tree: &Path, patch: &[u8]) -> Result<()> {
    let mut apply = Command::new("git")
        .args(["apply", "--whitespace=nowarn", "-"])
        .current_dir(tree)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("run git apply")?;
    apply
        .stdin
        .take()
        .context("piped stdin")?
        .write_all(patch)
        .context("feed the patch")?;
    let output = apply.wait_with_output()?;
    if !output.status.success() {
        bail!(
            "the patch does not apply: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// Puts the pristine `hidden` over `tree/hidden` and runs `command` in `tree`.
pub fn run_exam(tree: &Path, hidden: &Path, command: &str, home: &Path) -> Result<ExamRecord> {
    let slot = tree.join("hidden");
    if slot.exists() {
        // A file, a link or a directory: whatever the agent made, it goes.
        if slot.is_dir() && !slot.is_symlink() {
            std::fs::remove_dir_all(&slot)?;
        } else {
            std::fs::remove_file(&slot)?;
        }
    }
    copy_dir(hidden, &slot).context("put the exam in place")?;

    std::fs::create_dir_all(home)?;
    let log = tempfile::NamedTempFile::new().context("exam output file")?;
    let mut child = Command::new("sh")
        .args(["-c", command])
        .current_dir(tree)
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("HOME", home)
        .stdin(Stdio::null())
        .stdout(log.reopen()?)
        .stderr(log.reopen()?)
        .spawn()
        .context("run the hidden check")?;

    let started = Instant::now();
    let (status, timed_out) = loop {
        if let Some(status) = child.try_wait()? {
            break (Some(status), false);
        }
        if started.elapsed() > EXAM_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            break (None, true);
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let output = String::from_utf8_lossy(&std::fs::read(log.path())?).into_owned();
    let skip = output.chars().count().saturating_sub(OUTPUT_TAIL_CHARS);
    Ok(ExamRecord {
        passed: status.is_some_and(|s| s.success()),
        exit_code: status.and_then(|s| s.code()),
        timed_out,
        output_tail: output.chars().skip(skip).collect(),
    })
}

/// What the reference solution of a task did to its own exam.
#[derive(Debug, Clone, PartialEq)]
pub enum SolutionVerdict {
    Passed,
    /// The task is broken; the reason says how.
    Broken(String),
    /// Nothing to check: the task has no hidden check.
    Skipped(String),
}

/// Applies `solution/` over `repo/` in a clean directory and runs the hidden check there.
///
/// A `.patch` or `.diff` file at the top of `solution/` is applied (in name order); everything
/// else is copied over the tree as it is.
pub fn check_solution(task: &LoadedTask) -> Result<SolutionVerdict> {
    let Some(command) = task.hidden_command() else {
        return Ok(SolutionVerdict::Skipped(
            "no outcome.hidden_tests check to run".to_owned(),
        ));
    };
    let solution = task.solution_dir();
    if !solution.is_dir() {
        return Ok(SolutionVerdict::Broken("solution/ is missing".to_owned()));
    }
    let scratch = tempfile::tempdir().context("scratch directory")?;
    let tree = scratch.path().join("tree");
    copy_dir(&task.repo_dir(), &tree)?;

    let is_patch = |path: &Path| {
        path.parent() == Some(solution.as_path())
            && path
                .extension()
                .is_some_and(|e| e == "patch" || e == "diff")
    };
    copy_dir_except(&solution, &tree, &is_patch)?;
    let mut patches: Vec<_> = std::fs::read_dir(&solution)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| is_patch(path))
        .collect();
    patches.sort();
    for patch in patches {
        if let Err(failure) = apply_patch(&tree, &std::fs::read(&patch)?) {
            return Ok(SolutionVerdict::Broken(format!(
                "{}: {failure}",
                patch
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("patch")
            )));
        }
    }

    let exam = run_exam(
        &tree,
        &task.hidden_dir(),
        command,
        &scratch.path().join("home"),
    )?;
    Ok(if exam.passed {
        SolutionVerdict::Passed
    } else {
        SolutionVerdict::Broken(format!(
            "the hidden check failed (exit {:?}{}): {}",
            exam.exit_code,
            if exam.timed_out { ", timed out" } else { "" },
            exam.output_tail.trim()
        ))
    })
}
