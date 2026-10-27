//! The four exchanges a runner repeats for every attempt: claim, heartbeat, event batch and
//! completion. The board and the runner both serialize and deserialize these structs, so the
//! shape of `docs/contracts/runner-v1/{claim,heartbeat,event-batch,completion}.*.json` is
//! written once, here, and checked against those fixtures by the contract tests.
//!
//! None of them denies unknown fields: `protocol.json` says an unknown optional field is
//! ignored (and kept when a value is round-tripped), so a later additive field never turns an
//! older peer's message into a rejection.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::types::{
    ActualExecution, AttemptId, AttemptSnapshot, Checkpoint, ClaimRequestId, CompletionId,
    ExecutionRequestSnapshot, ExecutionState, FencingToken, ProtocolVersion, RecoveryJournalState,
    RunnerId, Usage,
};

/// `POST /claim`: a runner with free capacity asks for work, waiting up to `wait_ms` for some.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimRequest {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
    pub claim_request_id: ClaimRequestId,
    pub available_capacity: u32,
    pub wait_ms: u64,
}

/// The lease a claim grants: who holds the attempt, under which fence, until when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimLease {
    pub attempt_id: AttemptId,
    pub runner_id: RunnerId,
    pub fencing_token: FencingToken,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// A claim that found work: the lease, the immutable request and the attempt it opened.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClaimGranted {
    pub protocol_version: ProtocolVersion,
    pub claim_request_id: ClaimRequestId,
    pub lease: ClaimLease,
    pub request: ExecutionRequestSnapshot,
    pub attempt: AttemptSnapshot,
}

/// A claim that found nothing eligible. `lease` is always `null`; it is kept so the message
/// has the field `claim.no-work.response.json` fixes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimIdle {
    pub protocol_version: ProtocolVersion,
    pub claim_request_id: ClaimRequestId,
    pub lease: (),
    pub retry_after_ms: u64,
    pub reason: String,
}

/// The answer to a claim: work, or the instruction to ask again later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ClaimResponse {
    Work(Box<ClaimGranted>),
    Idle(ClaimIdle),
}

/// One attempt a runner reports as still running locally. `state` is the runner's own view,
/// used to reconcile; it never authorizes a transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveAttempt {
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub state: ExecutionState,
    pub journal_state: RecoveryJournalState,
    pub last_event_checkpoint: Option<Checkpoint>,
}

/// `POST /heartbeat`: the runner's liveness, its free capacity and every lease it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
    pub heartbeat_id: String,
    pub sent_at: DateTime<Utc>,
    pub available_capacity: u32,
    pub active_attempts: Vec<ActiveAttempt>,
}

/// What a heartbeat did to one lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseResult {
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub lease_expires_at: DateTime<Utc>,
    pub cancellation_requested: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    pub protocol_version: ProtocolVersion,
    pub heartbeat_id: String,
    pub accepted_at: DateTime<Utc>,
    pub lease_results: Vec<LeaseResult>,
}

/// One event of a batch. `payload` is whatever the source emitted for that `kind`, so it stays
/// a JSON value; the board stores it verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventRecord {
    pub event_id: String,
    pub sequence: i64,
    pub occurred_at: DateTime<Utc>,
    pub source: String,
    pub kind: String,
    pub payload: serde_json::Value,
}

/// `POST /attempts/{id}/events`: events between two checkpoints of one attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventBatchRequest {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub previous_checkpoint: Option<Checkpoint>,
    pub checkpoint: Checkpoint,
    pub events: Vec<EventRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventBatchResponse {
    pub protocol_version: ProtocolVersion,
    pub attempt_id: AttemptId,
    pub accepted_event_ids: Vec<String>,
    pub duplicate_event_ids: Vec<String>,
    pub committed_checkpoint: Checkpoint,
}

/// `POST /attempts/{id}/completion`: how an attempt ended, what it ran on and what it used.
///
/// `terminal_reason` is an object the harness adapter fills (`code`, `message`, and whatever
/// else that harness can say, such as the staged artifacts); the board stores and shows it
/// without branching on its keys, so it stays a JSON value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub completion_id: CompletionId,
    pub terminal_state: ExecutionState,
    pub terminal_reason: serde_json::Value,
    pub actual_execution: ActualExecution,
    pub usage: Usage,
    pub final_event_checkpoint: Option<Checkpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub protocol_version: ProtocolVersion,
    pub attempt_id: AttemptId,
    pub completion_id: CompletionId,
    pub state: ExecutionState,
    pub replayed: bool,
    pub committed_at: DateTime<Utc>,
}
