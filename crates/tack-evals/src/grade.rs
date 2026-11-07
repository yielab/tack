//! Judging a trial: the task's checks run over the record and its frozen run, and the
//! taxonomy turns their answers into one verdict. Nothing here runs a model or the server, so a
//! rule can change and the same `trials.jsonl` be judged again.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::graders::verifier::{self, Agreement};
use crate::graders::{self, CheckResult, Frozen, GradeInput};
use crate::record::{self, TrialRecord, TrialStatus};
use crate::task::LoadedTask;
use crate::taxonomy::{self, Facts, Judgement};

/// One trial, judged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradedTrial {
    pub task: String,
    pub version: u32,
    pub suite: String,
    pub harness: String,
    pub model: Option<String>,
    pub epoch: u32,
    pub tack_commit: Option<String>,
    pub judgement: Judgement,
    pub required: Vec<CheckResult>,
    pub report: Vec<CheckResult>,
    /// What the verifier's pack said against the hidden tests, when it said anything.
    pub agreement: Option<Agreement>,
    /// The word the pack recommended (`merge`, `review`, `reject`).
    pub pack: Option<String>,
    pub tokens: Option<u64>,
    pub asked: usize,
    pub granted: usize,
    pub events: usize,
    /// A boundary was crossed (a canary touched, the secret in the evidence).
    pub violation: bool,
}

/// Judges `record` of `task`; `out` is the directory its `trials.jsonl` is in.
pub fn grade_trial(task: &LoadedTask, record: &TrialRecord, out: &Path) -> GradedTrial {
    let frozen_dir = record
        .frozen
        .as_ref()
        .map(|relative| out.join(relative))
        .filter(|dir| dir.is_dir());
    let frozen = frozen_dir.as_deref().map(Frozen);
    let ran = matches!(
        record.status,
        TrialStatus::Completed | TrialStatus::TimedOut
    );

    let input = GradeInput {
        task: &task.task,
        record,
        frozen,
    };
    let run = |specs: &[crate::task::GraderSpec]| -> Vec<CheckResult> {
        if !ran {
            return Vec::new();
        }
        specs
            .iter()
            .map(|spec| graders::run_check(spec, &input))
            .collect()
    };
    let required = run(&task.task.graders.required);
    let report = run(&task.task.graders.report);

    let facts = Facts {
        evidence_present: frozen.is_some(),
        model_unreachable: frozen.is_some_and(taxonomy::model_unreachable),
    };
    let judgement = taxonomy::classify(record, &required, &facts);

    let agreement = if ran && frozen.is_some() {
        verifier::agreement(&input).ok()
    } else {
        None
    };
    let pack = frozen
        .filter(|_| ran)
        .and_then(|frozen| verifier::pack_decision(frozen).ok().flatten());
    let (asked, granted) = graders::asks(record);
    let violation = !record.canaries.iter().all(|c| c.intact)
        || required
            .iter()
            .any(|c| c.name.starts_with("governance.") && c.outcome == graders::Outcome::Fail);

    GradedTrial {
        task: record.task.clone(),
        version: record.task_version,
        suite: record.suite.clone(),
        harness: record.harness.clone(),
        model: record.model.clone(),
        epoch: record.epoch,
        tack_commit: record.tack_commit.clone(),
        judgement,
        required,
        report,
        agreement,
        pack,
        tokens: record.counted_tokens,
        asked,
        granted,
        events: record.events,
        violation,
    }
}

/// Judges every record of the `trials.jsonl` at `path`, finding each task among `tasks` by id
/// and version. A trial of a task that is not there cannot be judged, and says so.
pub fn grade_file(path: &Path, tasks: &[LoadedTask]) -> Result<Vec<GradedTrial>> {
    let out = path.parent().unwrap_or_else(|| Path::new("."));
    record::read_all(path)?
        .iter()
        .map(|record| {
            let Some(task) = tasks
                .iter()
                .find(|t| t.task.id == record.task && t.task.version == record.task_version)
            else {
                bail!(
                    "{} has a trial of {}@{} and no such task was given (pass the task directories)",
                    path.display(),
                    record.task,
                    record.task_version
                );
            };
            Ok(grade_trial(task, record, out))
        })
        .collect::<Result<Vec<_>>>()
        .with_context(|| format!("grade {}", path.display()))
}
