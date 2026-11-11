//! What one trial leaves: a line of `trials.jsonl`, plus the frozen files it points to.
//!
//! The record holds facts, not verdicts. The graders (and the failure class) read it later, so
//! a rule can change and old trials can be read again without running them.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::exam::ExamRecord;
use crate::operator::DecisionRecord;

/// Raised when a field changes meaning, so old lines are never read as new ones.
pub const SCHEMA_VERSION: u32 = 1;

/// How the trial itself went, apart from how the work went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrialStatus {
    /// The run reached an end and what it left was frozen.
    Completed,
    /// The wall limit passed first; the run was asked to stop.
    TimedOut,
    /// The machine cannot run this task as written (no sandbox where one is required).
    NotApplicable,
    /// The harness itself failed: no run to judge.
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanaryRecord {
    pub path: String,
    pub intact: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrialRecord {
    pub schema: u32,
    pub task: String,
    pub task_version: u32,
    pub suite: String,
    pub epoch: u32,
    pub harness: String,
    pub model: Option<String>,
    /// The commit of the checkout the harness ran from, when it is one.
    pub tack_commit: Option<String>,
    pub status: TrialStatus,
    /// Why the trial is not `completed`.
    pub reason: Option<String>,
    pub request_id: Option<String>,
    /// The run's state when the trial ended (`succeeded`, `failed`, ...).
    pub run_state: Option<String>,
    /// The sandbox the attempt had (`bwrap`, `none`, ...).
    pub sandbox: Option<String>,
    pub counted_tokens: Option<u64>,
    pub wall_ms: u64,
    pub decisions: Vec<DecisionRecord>,
    pub events: usize,
    /// The hidden check on the agent's tree; absent when the task has none or no tree came back.
    pub exam: Option<ExamRecord>,
    pub canaries: Vec<CanaryRecord>,
    /// The fake secret the runner held for this run: what a grader looks for in the frozen run.
    /// It is made up per task and protects nothing.
    pub fake_secret: Option<String>,
    /// The directory the frozen run is in, relative to the output directory.
    pub frozen: Option<String>,
}

impl TrialRecord {
    /// A record for a trial that never got to run.
    pub fn without_run(
        task: &crate::task::LoadedTask,
        harness: &str,
        model: Option<String>,
        epoch: u32,
        status: TrialStatus,
        reason: String,
    ) -> Self {
        Self {
            schema: SCHEMA_VERSION,
            task: task.task.id.clone(),
            task_version: task.task.version,
            suite: task.task.suite.clone(),
            epoch,
            harness: harness.to_owned(),
            model,
            tack_commit: None,
            status,
            reason: Some(reason),
            request_id: None,
            run_state: None,
            sandbox: None,
            counted_tokens: None,
            wall_ms: 0,
            decisions: Vec::new(),
            events: 0,
            exam: None,
            canaries: Vec::new(),
            fake_secret: None,
            frozen: None,
        }
    }
}

/// Adds `record` as one line of `out/trials.jsonl`.
pub fn append(out: &Path, record: &TrialRecord) -> Result<()> {
    std::fs::create_dir_all(out)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(trials_path(out))
        .context("open trials.jsonl")?;
    writeln!(file, "{}", serde_json::to_string(record)?)?;
    Ok(())
}

pub fn trials_path(out: &Path) -> PathBuf {
    out.join("trials.jsonl")
}

/// Every record in `path`, in the order written.
pub fn read_all(path: &Path) -> Result<Vec<TrialRecord>> {
    std::fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).context("a line of trials.jsonl"))
        .collect()
}
