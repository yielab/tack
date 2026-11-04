//! What the board asks a runner to do on a person's behalf, outside any attempt: the runner asks
//! for it (`POST /operations/poll`), does it beside the harness it names, and says how it went
//! (`POST /operations/{operation_id}/result`). Both are additive v1 operations; a runner against
//! a board without them gets a refusal and goes on with its work.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::capabilities::TeamReport;
use super::types::{HarnessKind, ProtocolVersion, RunnerId};

/// One thing the board asks of a harness's team.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case")]
pub enum TeamOperation {
    /// Lets a team that stopped on its budget claim tasks again.
    ResumeTeam { harness: HarnessKind, team: String },
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
        team: Option<TeamReport>,
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
