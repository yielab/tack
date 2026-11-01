//! What a team is doing, in Tack's own words: the tasks it holds, the step each one is in and the
//! steps it already finished (ADR 0034 decision 5). Filled by a harness's adapter from what its
//! agent reports; no field may carry a harness's own key name, and a field the harness did not say
//! is absent, which is not the same as "none".

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The work one team holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamActivity {
    pub team: String,
    pub tasks: Vec<ActivityTask>,
}

/// One task a team holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ActivityTask {
    /// The task's name in the harness, which is all a person can find it by there.
    #[serde(rename = "ref")]
    pub task_ref: String,
    /// The item the board started this task for; absent for work the board did not start (one a
    /// person, a routine or another channel gave the team directly).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_board: Option<String>,
    pub state: ActivityState,
    /// The step the task is inside now. Absent when it is inside none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_step: Option<CurrentStep>,
    /// What a waiting task waits for. Absent unless `state` is `waiting`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waits_on: Option<WaitsOn>,
    /// The steps that already ended, oldest first.
    #[serde(default)]
    pub finished_steps: Vec<FinishedStep>,
    /// When the harness last changed the task. Absent when it did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed_at: Option<DateTime<Utc>>,
}

/// Where a task is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ActivityState {
    /// Accepted, not started.
    Queued,
    Running,
    /// Stopped until a person answers (see `waits_on`).
    Waiting,
    /// Stopped and not able to go on without outside help.
    Blocked,
    Done,
    Failed,
    Cancelled,
}

impl ActivityState {
    /// The states of a task that will not change again without someone acting on it.
    pub const OVER: [Self; 3] = [Self::Done, Self::Failed, Self::Cancelled];

    /// Whether the task will not change again without someone acting on it.
    pub fn over(self) -> bool {
        Self::OVER.contains(&self)
    }

    /// The word the state has on the wire and in the board's store.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// What a waiting task waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum WaitsOn {
    /// A person's approval of something the team is about to do.
    Approval,
    /// A person's answer to a question.
    Answer,
}

/// The step a task is inside.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CurrentStep {
    pub name: String,
    /// Who does the step, in the harness's own word for it, shown as data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// The model the step was planned on, which is not always the one that served it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// When the step began.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<DateTime<Utc>>,
    /// How many times the task went back to an earlier step before this one; `0` on a first pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle: Option<u32>,
}

/// A step that ended, with what it said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FinishedStep {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Whether the step ended well. Absent when the harness did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
    /// What a reviewing step decided, in the harness's own word, shown as data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    /// What the step said it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// What the step took for granted.
    #[serde(default)]
    pub assumptions: Vec<String>,
    /// What the step left to the next one or to a person.
    #[serde(default)]
    pub questions: Vec<String>,
    /// The tokens the step used, cache reads not counted. Absent when not measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
}
