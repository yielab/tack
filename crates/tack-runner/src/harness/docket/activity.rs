//! What each team is doing, read from docket's task list and trace (ADR 0034 decision 5): every
//! task a team holds, whoever started it, with the step it is in and the steps it finished.
//!
//! docket offers two features that make this cheap, and each is read only when `GET /health`
//! lists it. `taskCursor`: `GET /tasks/<pod>?updatedSince=` answers only the tasks changed since
//! the last answer. `taskCurrent`: a running task carries `current`, the step it is inside. A
//! docket without them is read the long way: the whole list each time, compared with what was
//! sent last, and the step derived from the trace (the newest `hop_started` with no
//! `hop_finished`).

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::Value;
use tack_protocol::execution::{
    ActivityState, ActivityTask, CurrentStep, FinishedStep, TeamActivity, WaitsOn,
};

use crate::harness::redact::SecretMaterial;

use super::{
    client::Client,
    wire::{HealthAnswer, PodList, TaskHop, Tokens, TracePage},
};

/// How long a finished task stays worth reporting; the board keeps the same window.
const WINDOW: Duration = Duration::days(14);

/// The board starts a task with `externalRef` `<item id>/attempt-<n>`; any other label, or none,
/// is work the board did not start.
const ATTEMPT_SEPARATOR: &str = "/attempt-";

/// The features of docket this reader uses.
#[derive(Clone, Copy, Default)]
struct Features {
    /// A running task carries `current`.
    current: bool,
    /// The task list takes `updatedSince`.
    cursor: bool,
}

impl Features {
    async fn of(client: &Client) -> Self {
        let listed = match client.health().await {
            Ok(reply) if reply.status == 200 => reply.read::<HealthAnswer>().ok(),
            _ => None,
        };
        let has = |name: &str| {
            listed
                .as_ref()
                .is_some_and(|health| health.features.iter().any(|feature| feature == name))
        };
        Self {
            current: has("taskCurrent"),
            cursor: has("taskCursor"),
        }
    }
}

/// Where Tack is in one team's tasks.
#[derive(Default)]
pub(super) struct TeamCursor {
    /// `updatedSince` for the next read of the list.
    tasks: Option<String>,
    /// The trace cursor, for a docket that does not send `current`.
    trace: String,
    /// The step each task is inside, by task, as the trace says.
    open: HashMap<String, CurrentStep>,
    /// Each task as it was last reported, so only what changed is sent again.
    sent: HashMap<String, ActivityTask>,
}

/// The tasks of every team that changed since the last call.
pub(super) async fn read(
    client: &Client,
    cursors: &mut HashMap<String, TeamCursor>,
    now: DateTime<Utc>,
) -> Vec<TeamActivity> {
    // These tasks belong to no run, so the only secret known here is the token docket is read with.
    let secrets = client.secrets();
    let features = Features::of(client).await;
    let teams = match client.pods().await {
        Ok(reply) if reply.status == 200 => reply
            .read::<PodList>()
            .map(|list| list.pods)
            .unwrap_or_default(),
        _ => return Vec::new(),
    };
    let mut report = Vec::new();
    for team in teams {
        let cursor = cursors.entry(team.name.clone()).or_default();
        match changed(client, &team.name, cursor, features, now).await {
            Ok(mut tasks) if !tasks.is_empty() => {
                for step in tasks.iter_mut().flat_map(|task| &mut task.finished_steps) {
                    scrub(step, &secrets);
                }
                report.push(TeamActivity {
                    team: team.name,
                    tasks,
                });
            }
            Ok(_) => {}
            Err(why) => {
                tracing::warn!(team = team.name, %why, "a team's activity could not be read")
            }
        }
    }
    report
}

/// The words a finished step carries, scrubbed as a local agent's output is.
fn scrub(step: &mut FinishedStep, secrets: &SecretMaterial) {
    for text in [&mut step.verdict, &mut step.summary].into_iter().flatten() {
        *text = secrets.scrub(text);
    }
    for text in step.assumptions.iter_mut().chain(&mut step.questions) {
        *text = secrets.scrub(text);
    }
}

/// A task as `GET /tasks/<pod>` sends it: the keys Tack reads.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListedTask {
    id: String,
    status: String,
    external_ref: Option<String>,
    created: Option<DateTime<Utc>>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    /// Only on a running task, and only from a docket with `taskCurrent`.
    current: Option<ListedCurrent>,
    #[serde(default)]
    hops: Vec<ListedHop>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListedCurrent {
    step_id: String,
    role: Option<String>,
    model: Option<String>,
    started_at: Option<DateTime<Utc>>,
    cycle: Option<u32>,
}

/// A hop of the record, with the keys beyond the ones the run's own reading uses.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListedHop {
    #[serde(flatten)]
    hop: TaskHop,
    step_id: Option<String>,
    usage: Option<Tokens>,
    trace: Option<HopTrace>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HopTrace {
    last_ts: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct TaskPage {
    tasks: Vec<Value>,
    next: Option<String>,
}

/// The tasks of one team that changed since the last read.
async fn changed(
    client: &Client,
    team: &str,
    cursor: &mut TeamCursor,
    features: Features,
    now: DateTime<Utc>,
) -> Result<Vec<ActivityTask>, String> {
    let since = features
        .cursor
        .then(|| cursor.tasks.clone().unwrap_or_else(|| "0".to_owned()));
    let reply = client.tasks(team, since.as_deref()).await?;
    if reply.status != 200 {
        return Err(reply.error());
    }
    let page: TaskPage = reply.read()?;
    if features.cursor
        && let Some(next) = page.next
    {
        cursor.tasks = Some(next);
    }
    if !features.current {
        follow_trace(client, team, cursor).await;
    }
    let mut fresh: Vec<ActivityTask> = page
        .tasks
        .into_iter()
        .filter_map(|raw| match serde_json::from_value::<ListedTask>(raw) {
            Ok(task) => Some(task),
            Err(why) => {
                tracing::warn!(%why, "a task of the list could not be read");
                None
            }
        })
        .filter_map(|task| activity_task(task, features, &cursor.open, now))
        .collect();
    if !features.current {
        // A running task the list did not return still moved on if its step did.
        let listed: Vec<String> = fresh.iter().map(|task| task.task_ref.clone()).collect();
        for task in cursor.sent.values() {
            if task.state == ActivityState::Running && !listed.contains(&task.task_ref) {
                let mut task = task.clone();
                task.current_step = cursor.open.get(&task.task_ref).cloned();
                fresh.push(task);
            }
        }
    }
    fresh.retain(|task| cursor.sent.get(&task.task_ref) != Some(task));
    for task in &fresh {
        cursor.sent.insert(task.task_ref.clone(), task.clone());
    }
    Ok(fresh)
}

/// Reads the trace past the cursor and keeps, per task, the step it is inside.
async fn follow_trace(client: &Client, team: &str, cursor: &mut TeamCursor) {
    let Ok(reply) = client.traces(team, &cursor.trace).await else {
        return;
    };
    if reply.status != 200 {
        return;
    }
    let Ok(page) = reply.read::<TracePage>() else {
        return;
    };
    for line in &page.events {
        let Ok(line) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let payload = &line["payload"];
        let Some(task) = payload["task"].as_str() else {
            continue;
        };
        match line["event_type"].as_str() {
            Some("hop_started") => {
                cursor.open.insert(
                    task.to_owned(),
                    CurrentStep {
                        name: payload["step"]
                            .as_str()
                            .or_else(|| payload["role"].as_str())
                            .unwrap_or_default()
                            .to_owned(),
                        role: payload["role"].as_str().map(str::to_owned),
                        model: payload["model"].as_str().map(str::to_owned),
                        since: line["ts"].as_str().and_then(|ts| ts.parse().ok()),
                        cycle: payload["cycle"]
                            .as_u64()
                            .and_then(|cycle| u32::try_from(cycle).ok()),
                    },
                );
            }
            Some("hop_finished") => {
                cursor.open.remove(task);
            }
            _ => {}
        }
    }
    if let Some(next) = page.next.filter(|next| !next.is_empty()) {
        cursor.trace = next;
    }
}

/// A listed task in Tack's words, or `None` when it is too old to report or has a status Tack has
/// no word for.
fn activity_task(
    task: ListedTask,
    features: Features,
    open: &HashMap<String, CurrentStep>,
    now: DateTime<Utc>,
) -> Option<ActivityTask> {
    let (state, waits_on) = match task.status.as_str() {
        "pending" => (ActivityState::Queued, None),
        "running" => (ActivityState::Running, None),
        "waiting_approval" => (ActivityState::Waiting, Some(WaitsOn::Approval)),
        "waiting_input" => (ActivityState::Waiting, Some(WaitsOn::Answer)),
        "blocked" => (ActivityState::Blocked, None),
        "done" => (ActivityState::Done, None),
        "failed" => (ActivityState::Failed, None),
        "cancelled" => (ActivityState::Cancelled, None),
        other => {
            tracing::warn!(
                task = task.id,
                status = other,
                "a task has a status Tack has no word for"
            );
            return None;
        }
    };
    let changed_at = task.completed_at.or(task.started_at).or(task.created);
    if state.over() && changed_at.is_some_and(|at| now - at > WINDOW) {
        return None;
    }
    let current_step = if state != ActivityState::Running {
        None
    } else if features.current {
        task.current.map(|current| CurrentStep {
            name: current.step_id,
            role: current.role,
            model: current.model,
            since: current.started_at,
            cycle: current.cycle,
        })
    } else {
        open.get(&task.id).cloned()
    };
    Some(ActivityTask {
        on_board: task.external_ref.as_deref().and_then(on_board),
        task_ref: task.id,
        state,
        current_step,
        waits_on,
        finished_steps: task.hops.into_iter().map(finished_step).collect(),
        changed_at,
    })
}

/// The item a task was started for, when its label is the one the board writes.
fn on_board(external_ref: &str) -> Option<String> {
    let (item, attempt) = external_ref.split_once(ATTEMPT_SEPARATOR)?;
    (uuid::Uuid::parse_str(item).is_ok() && attempt.parse::<u32>().is_ok()).then(|| item.to_owned())
}

fn finished_step(listed: ListedHop) -> FinishedStep {
    let text = |value: Option<String>| value.filter(|value| !value.is_empty());
    let artifact = listed.hop.artifact.unwrap_or_default();
    FinishedStep {
        name: listed
            .step_id
            .or_else(|| listed.hop.role.clone())
            .unwrap_or_default(),
        role: text(listed.hop.role),
        model: text(listed.hop.model),
        ok: listed.hop.ok,
        verdict: text(artifact.verdict).or_else(|| text(listed.hop.verdict)),
        summary: text(artifact.summary),
        assumptions: artifact.assumptions,
        questions: artifact.questions,
        // docket's own rule: what the cache served is not counted.
        tokens: listed.usage.and_then(|usage| {
            let output = usage.output?;
            Some(usage.input?.saturating_sub(usage.cached.unwrap_or(0)) + output)
        }),
        finished_at: listed.trace.and_then(|trace| trace.last_ts),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_boards_own_label_names_an_item() {
        let item = "7f0f6c8e-5a4e-4b1c-9d0a-0c3a2f1b9e11";
        assert_eq!(
            on_board(&format!("{item}/attempt-3")),
            Some(item.to_owned())
        );
        for label in ["", "capture-item/attempt-3", &format!("{item}/retry"), item] {
            assert_eq!(on_board(label), None, "{label}");
        }
    }
}
