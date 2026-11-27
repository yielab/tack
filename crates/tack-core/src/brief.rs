//! The item brief: what "done" means for an item, in a shape a verifier can
//! check. Pure — no database, no handler. `docs/contracts/brief-v1/` is the
//! wire shape and `example.json` there is pinned by this module's tests.

use std::collections::HashSet;

use validator::ValidationError;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{AcceptanceCriterion, Constraint, Item, ItemBrief, MetricOp, Risk};

/// One saved state of an item's brief. `brief_sha256` is the SHA-256 of the
/// brief-v1 JSON as the runner stages it (`brief.json`); it is empty for a
/// version recorded before hashes were kept.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ItemBriefVersion {
    pub item_id: Uuid,
    pub version: i64,
    pub acceptance: Vec<AcceptanceCriterion>,
    pub constraints: Vec<Constraint>,
    pub definition_of_done: Option<String>,
    pub risk: Option<Risk>,
    pub brief_sha256: String,
    pub created_at: DateTime<Utc>,
}

/// Longest `command.run` a brief accepts, in characters.
const RUN_MAX_CHARS: usize = 2_000;

/// The checks `validator`'s length limits cannot express: criterion ids are
/// unique within the brief and no `command.run` exceeds 2 000 characters.
/// Wired into `UpsertItemBrief::acceptance` as a custom validator.
pub fn validate(acceptance: &[AcceptanceCriterion]) -> Result<(), ValidationError> {
    let mut seen = HashSet::new();
    for criterion in acceptance {
        let id = criterion_id(criterion);
        if !seen.insert(id) {
            return Err(ValidationError::new("duplicate_criterion_id")
                .with_message(format!("criterion id `{id}` is used twice").into()));
        }
        if let AcceptanceCriterion::Command { run, .. } = criterion
            && run.chars().count() > RUN_MAX_CHARS
        {
            return Err(ValidationError::new("run_too_long").with_message(
                format!("criterion `{id}`: run is longer than {RUN_MAX_CHARS} characters").into(),
            ));
        }
    }
    Ok(())
}

/// What a screen says of an item's brief, so none of them reads the rule: whether something says
/// when the item is done, how many of its checks a machine runs, its risk, and, when it cannot be
/// verified yet, why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BriefSummary {
    pub done_when: bool,
    pub checks: u32,
    pub risk: Option<Risk>,
    pub not_ready: Option<String>,
}

/// The reason a task has neither a Done when nor a check a machine runs.
const NOT_READY: &str = "No Done when and no check a machine runs";

/// The summary of `brief`. A task says when it is done with a line a person ticks (a `manual`
/// criterion, the checklist the item page calls Done when) or with a definition of done, the
/// item's or the project's (a run receives the project's when the item has none). A check is a
/// criterion a person does not tick (`manual` is the only kind a machine does not run).
pub fn readiness(brief: Option<&ItemBrief>, project_done: Option<&str>) -> BriefSummary {
    let said = |text: Option<&str>| text.is_some_and(|t| !t.trim().is_empty());
    let ticked = brief.is_some_and(|b| {
        b.acceptance
            .iter()
            .any(|c| matches!(c, AcceptanceCriterion::Manual { .. }))
    });
    let done_when =
        ticked || said(brief.and_then(|b| b.definition_of_done.as_deref())) || said(project_done);
    let checks = brief.map_or(0, |b| {
        b.acceptance
            .iter()
            .filter(|c| !matches!(c, AcceptanceCriterion::Manual { .. }))
            .count() as u32
    });
    BriefSummary {
        done_when,
        checks,
        risk: brief.and_then(|b| b.risk.clone()),
        not_ready: (!done_when && checks == 0).then(|| NOT_READY.to_owned()),
    }
}

/// The brief as Markdown, for a pull-request body or a harness prompt: one
/// heading per section, one line per criterion led by its id, and the
/// `manual` criteria last under "Needs a human".
pub fn render_markdown(item: &Item, brief: &ItemBrief) -> String {
    render_titled(&item.title, brief)
}

/// [`render_markdown`] for a caller that has the title and not the item.
pub fn render_titled(title: &str, brief: &ItemBrief) -> String {
    let mut out = format!("# {title}\n");
    if let Some(risk) = &brief.risk {
        let risk = match risk {
            Risk::Low => "low",
            Risk::Medium => "medium",
            Risk::High => "high",
        };
        out.push_str(&format!("\nRisk: {risk}\n"));
    }
    if let Some(done) = &brief.definition_of_done {
        out.push_str(&format!("\n## Definition of done\n\n{done}\n"));
    }
    let (manual, checked): (Vec<_>, Vec<_>) = brief
        .acceptance
        .iter()
        .partition(|c| matches!(c, AcceptanceCriterion::Manual { .. }));
    push_section(
        &mut out,
        "Acceptance",
        checked.into_iter().map(criterion_line),
    );
    push_section(
        &mut out,
        "Constraints",
        brief.constraints.iter().map(constraint_line),
    );
    push_section(
        &mut out,
        "Needs a human",
        manual.into_iter().map(criterion_line),
    );
    out
}

fn push_section(out: &mut String, heading: &str, lines: impl Iterator<Item = String>) {
    let lines: Vec<String> = lines.collect();
    if lines.is_empty() {
        return;
    }
    out.push_str(&format!("\n## {heading}\n\n"));
    for line in lines {
        out.push_str(&format!("- {line}\n"));
    }
}

fn criterion_id(criterion: &AcceptanceCriterion) -> &str {
    match criterion {
        AcceptanceCriterion::Command { id, .. }
        | AcceptanceCriterion::Test { id, .. }
        | AcceptanceCriterion::Metric { id, .. }
        | AcceptanceCriterion::File { id, .. }
        | AcceptanceCriterion::Absent { id, .. }
        | AcceptanceCriterion::Manual { id, .. } => id,
    }
}

fn criterion_line(criterion: &AcceptanceCriterion) -> String {
    let id = criterion_id(criterion);
    match criterion {
        AcceptanceCriterion::Command {
            title,
            run,
            expect_exit,
            cwd,
            ..
        } => {
            let cwd = cwd
                .as_deref()
                .map(|d| format!(" in `{d}`"))
                .unwrap_or_default();
            format!("`{id}` {title}: `{run}`{cwd} exits {expect_exit}")
        }
        AcceptanceCriterion::Test {
            title,
            name,
            runner,
            ..
        } => {
            let runner = runner
                .as_deref()
                .map(|r| format!(" ({r})"))
                .unwrap_or_default();
            format!("`{id}` {title}: test `{name}` passes{runner}")
        }
        AcceptanceCriterion::Metric {
            title,
            name,
            op,
            threshold,
            unit,
            ..
        } => {
            let op = match op {
                MetricOp::Lte => "≤",
                MetricOp::Gte => "≥",
                MetricOp::Eq => "=",
            };
            let unit = unit.as_deref().unwrap_or_default();
            format!("`{id}` {title}: `{name}` {op} {threshold}{unit}")
        }
        AcceptanceCriterion::File { title, path, .. } => {
            format!("`{id}` {title}: `{path}` exists")
        }
        AcceptanceCriterion::Absent { title, path, .. } => {
            format!("`{id}` {title}: `{path}` does not exist")
        }
        AcceptanceCriterion::Manual { title, text, .. } => format!("`{id}` {title}: {text}"),
    }
}

fn constraint_line(constraint: &Constraint) -> String {
    match constraint {
        Constraint::ForbiddenPath { glob } => format!("do not touch `{glob}`"),
        Constraint::AllowedDependency { name } => format!("may add the dependency `{name}`"),
        Constraint::MaxChangedFiles { n } => format!("change at most {n} files"),
        Constraint::Note { text } => text.clone(),
    }
}

#[cfg(test)]
#[path = "brief/tests.rs"]
mod tests;
