//! What the board asks a runner to do on a person's behalf, outside any attempt: the runner asks
//! for it (`POST /operations/poll`), does it beside the harness it names, and says how it went
//! (`POST /operations/{operation_id}/result`). Both are additive v1 operations; a runner against
//! a board without them gets a refusal and goes on with its work.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::capabilities::TeamReport;
use super::types::{HarnessKind, ProtocolVersion, RunnerId};

/// What a person decides about a memory entry that waits for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MemoryDecision {
    Approve,
    Refuse,
}

/// One thing the board asks of a harness's team.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case")]
pub enum TeamOperation {
    /// Lets a team that stopped on its budget claim tasks again.
    ResumeTeam { harness: HarnessKind, team: String },
    /// Approves or refuses one memory entry that waits for a person. A refusal carries the reason a
    /// person gave. The harness answers with the team's report afterwards.
    DecideMemory {
        harness: HarnessKind,
        team: String,
        entry: String,
        decision: MemoryDecision,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Makes the team of a project's repository from its shape and applies the team's folder
    /// (`.docket/teams/<team>`) inside the project's checkout on the runner's computer. A folder
    /// that would run commands there is refused with a review until the call carries `accept`, the
    /// code a person gave after reading it.
    ApplyTeam {
        harness: HarnessKind,
        team: String,
        shape: String,
        /// The project's checkout on the runner's computer.
        directory: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        accept: Option<String>,
    },
}

/// An operation with the identity its result is sent back under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerOperation {
    pub operation_id: String,
    #[serde(flatten)]
    pub operation: TeamOperation,
}

/// `POST /operations/poll`: what is waiting for this runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationPollRequest {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationPollResponse {
    pub protocol_version: ProtocolVersion,
    /// Each is handed out once; empty when nothing waits.
    pub operations: Vec<RunnerOperation>,
}

/// How an operation ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum OperationOutcome {
    /// It was done. `team` is the team's report as the harness gave it afterwards, when it did.
    Done {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        team: Option<Box<TeamReport>>,
    },
    /// The harness will run commands on the computer and wants a person to read them first. The
    /// same operation is sent again carrying `code` as `accept` once a person agrees.
    NeedsReview {
        code: String,
        /// What it would run, one line each, as the harness listed it.
        commands: Vec<String>,
    },
    /// The runner did not do it, with the sentence a person reads.
    Refused { reason: String },
}

/// `POST /operations/{operation_id}/result`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationResultRequest {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
    pub operation_id: String,
    pub outcome: OperationOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationResultResponse {
    pub protocol_version: ProtocolVersion,
    pub operation_id: String,
    pub accepted_at: DateTime<Utc>,
}
