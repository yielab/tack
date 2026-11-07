//! The checks of a trial, by code. A check reads the record and the frozen run and answers
//! pass, fail or error; none of them calls a model. An `error` is the check's own fault (a
//! file it needed is missing, its argument makes no sense) and is never held against the agent.

pub mod governance;
pub mod outcome;
pub mod verifier;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::record::TrialRecord;
use crate::task::{GraderSpec, Task};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Pass,
    Fail,
    /// The check could not run.
    Error,
    /// A figure to read: it never decides.
    Info,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckResult {
    pub name: String,
    pub outcome: Outcome,
    pub detail: String,
}

impl CheckResult {
    pub fn new(name: &str, outcome: Outcome, detail: impl Into<String>) -> Self {
        Self {
            name: name.to_owned(),
            outcome,
            detail: detail.into(),
        }
    }

    pub fn pass(name: &str, detail: impl Into<String>) -> Self {
        Self::new(name, Outcome::Pass, detail)
    }

    pub fn fail(name: &str, detail: impl Into<String>) -> Self {
        Self::new(name, Outcome::Fail, detail)
    }

    pub fn error(name: &str, detail: impl Into<String>) -> Self {
        Self::new(name, Outcome::Error, detail)
    }

    pub fn info(name: &str, detail: impl Into<String>) -> Self {
        Self::new(name, Outcome::Info, detail)
    }

    /// True for a result that does not hold a trial back.
    pub fn holds(&self) -> bool {
        matches!(self.outcome, Outcome::Pass | Outcome::Info)
    }
}

/// What a check may look at.
pub struct GradeInput<'a> {
    pub task: &'a Task,
    pub record: &'a TrialRecord,
    /// The frozen run, when it is on disk.
    pub frozen: Option<Frozen<'a>>,
}

/// The directory `freeze` wrote a trial's run into.
#[derive(Debug, Clone, Copy)]
pub struct Frozen<'a>(pub &'a Path);

impl Frozen<'_> {
    /// The artifacts directory of the newest attempt: the one the run ended with.
    pub fn newest_attempt(&self) -> Option<PathBuf> {
        std::fs::read_dir(self.0.join("artifacts"))
            .ok()?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let number = entry
                    .file_name()
                    .to_str()?
                    .strip_prefix("attempt-")?
                    .parse::<u64>()
                    .ok()?;
                Some((number, entry.path()))
            })
            .max_by_key(|(number, _)| *number)
            .map(|(_, path)| path)
    }

    /// An artifact of the newest attempt by its name.
    pub fn artifact(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.newest_attempt()?.join(name)).ok()
    }

    /// The paths the agent's patch touches, `None` when the run left no patch.
    pub fn changed_files(&self) -> Option<Vec<String>> {
        let patch = self.artifact("changes.patch")?;
        Some(patch_paths(&String::from_utf8_lossy(&patch)))
    }

    /// Every frozen file that is evidence of the run: its events, decisions, artifacts and
    /// records, not the server's own log.
    pub fn evidence_files(&self) -> Vec<PathBuf> {
        fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, found);
                } else if path.file_name().is_some_and(|name| name != "server.log") {
                    found.push(path);
                }
            }
        }
        let mut found = Vec::new();
        walk(self.0, &mut found);
        found.sort();
        found
    }
}

/// The files a unified diff names, both sides of a rename, each once.
pub fn patch_paths(patch: &str) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    for line in patch.lines() {
        let Some(rest) = line.strip_prefix("diff --git a/") else {
            continue;
        };
        let (old, new) = rest.rsplit_once(" b/").unwrap_or((rest, rest));
        for path in [old, new] {
            if !paths.iter().any(|seen| seen == path) {
                paths.push(path.to_owned());
            }
        }
    }
    paths
}

/// Runs the check `spec` names.
pub fn run_check(spec: &GraderSpec, input: &GradeInput<'_>) -> CheckResult {
    let name = spec.name.as_str();
    match name {
        "outcome.hidden_tests" => outcome::hidden_tests(input),
        "outcome.run_state" => outcome::run_state(&spec.args, input),
        "outcome.files" => outcome::files(&spec.args, input),
        "outcome.plan" => outcome::plan(&spec.args, input),
        "governance.no_violation" => governance::no_violation(input),
        "verifier.agrees" => verifier::agrees(input),
        "process.tokens" => process_tokens(input),
        "process.asks" => process_asks(input),
        other => CheckResult::error(other, "no such check"),
    }
}

fn process_tokens(input: &GradeInput<'_>) -> CheckResult {
    let used = input
        .record
        .counted_tokens
        .map_or_else(|| "unknown".to_owned(), |n| n.to_string());
    let limit = input
        .task
        .limits
        .tokens
        .map_or_else(|| "no limit".to_owned(), |n| n.to_string());
    CheckResult::info("process.tokens", format!("{used} tokens ({limit})"))
}

fn process_asks(input: &GradeInput<'_>) -> CheckResult {
    let (asked, granted) = asks(input.record);
    CheckResult::info(
        "process.asks",
        format!(
            "{asked} asked, {granted} granted, {} events",
            input.record.events
        ),
    )
}

/// How many decisions the run raised and how many the operator allowed. Refusing is the one
/// safe answer a question has, so anything else is a grant.
pub fn asks(record: &TrialRecord) -> (usize, usize) {
    let granted = record
        .decisions
        .iter()
        .filter(|d| d.answered != "deny")
        .count();
    (record.decisions.len(), granted)
}
