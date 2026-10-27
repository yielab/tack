//! Every request and response of the runner protocol: enrollment, refresh, claim, heartbeat,
//! accept and start, event batch, decision create and poll, artifact manifest, cancellation
//! observation and completion (the recovery observation lives in `types`). The board and the
//! runner both serialize and deserialize these structs, so the shape of
//! `docs/contracts/runner-v1/*.json` is written once, here, and checked against those fixtures
//! by the contract tests.
//!
//! None of them denies unknown fields: `protocol.json` says an unknown optional field is
//! ignored (and kept when a value is round-tripped), so a later additive field never turns an
//! older peer's message into a rejection.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::capabilities::EmbeddedCapabilitySnapshot;
use super::types::{
    ActualExecution, AttemptId, AttemptSnapshot, CancellationRequestId, Checkpoint, ClaimRequestId,
    CompletionId, ExecutionRequestSnapshot, ExecutionState, FencingToken, ProtocolVersion,
    RecoveryJournalState, RunnerId, Usage, WorkspaceId,
};

/// `POST /enroll`: a runner redeems the single-use enrollment token and reports what it can do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnrollmentRequest {
    pub protocol_version: ProtocolVersion,
    pub enrollment_token: String,
    pub runner_name: String,
    pub runner_version: String,
    pub capabilities: EmbeddedCapabilitySnapshot,
}

/// The runner's identity and its credential, shown once, with the cadence the board expects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnrollmentResponse {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
    pub runner_credential: String,
    pub credential_expires_at: DateTime<Utc>,
    pub heartbeat_interval_seconds: u64,
    pub lease_duration_seconds: u64,
    pub server_time: DateTime<Utc>,
}

/// `POST /refresh`: the runner's current capabilities, and optionally a new credential.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefreshRequest {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
    pub runner_name: String,
    pub runner_version: String,
    #[serde(default)]
    pub rotate_credential: bool,
    pub capabilities: EmbeddedCapabilitySnapshot,
}

/// `runner_credential` is `null` unless the request asked for a rotation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshResponse {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
    pub runner_credential: Option<String>,
    pub credential_expires_at: DateTime<Utc>,
    pub accepted_at: DateTime<Utc>,
}

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

/// `POST /attempts/{id}/accept` (to `preparing`) and `POST /attempts/{id}/start` (to
/// `running`): the workspace the attempt works in and, once a process runs, which one.
/// `process_id` is required by `start` and absent from `accept`; the names (never the values)
/// of the variables the request set and the sandbox are what the board shows about a start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptTransitionRequest {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub workspace_id: WorkspaceId,
    pub base_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub environment_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttemptTransitionResponse {
    pub protocol_version: ProtocolVersion,
    pub attempt_id: AttemptId,
    pub state: ExecutionState,
    pub replayed: bool,
    pub committed_at: DateTime<Utc>,
}

/// One choice a decision offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionOption {
    pub option_id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risks: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_tokens: Option<u64>,
}

/// The option the harness would pick; `option_id` must name one of the request's options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRecommendation {
    pub option_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_refs: Option<Vec<String>>,
}

/// `POST /attempts/{id}/decisions`: a question the attempt needs a person to answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionCreateRequest {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub decision_id: String,
    pub kind: String,
    pub prompt: String,
    pub options: Vec<DecisionOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<DecisionRecommendation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// What the harness says about the question; the board shows it and never branches on it.
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionCreateResponse {
    pub protocol_version: ProtocolVersion,
    pub decision_id: String,
    pub state: String,
    pub created_at: DateTime<Utc>,
}

/// `POST /attempts/{id}/decisions/poll`: the decisions that changed after `after`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionPollRequest {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub after: DateTime<Utc>,
}

/// What a person answered: an option, free text, or both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionAnswer {
    #[serde(default)]
    pub option_id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
}

/// One decision of a poll. `resolved_by` is whoever the board recorded (`kind`, `subject_id`);
/// it is shown, never branched on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionUpdate {
    pub decision_id: String,
    pub state: String,
    pub answer: Option<DecisionAnswer>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub resolved_by: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionPollResponse {
    pub protocol_version: ProtocolVersion,
    pub decisions: Vec<DecisionUpdate>,
    pub next_after: DateTime<Utc>,
}

/// One artifact an attempt announces; its bytes follow in a `PUT` to the grant's path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactManifestEntry {
    pub artifact_id: String,
    pub kind: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    pub size_bytes: u64,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_disposition: Option<String>,
    /// What the producer says about the artifact (such as its base revision); stored verbatim.
    #[serde(default)]
    pub metadata: Map<String, Value>,
}

/// `POST /attempts/{id}/artifacts`: the manifest of what the attempt produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactManifestRequest {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub artifacts: Vec<ArtifactManifestEntry>,
}

/// Where and until when an accepted artifact's bytes may be uploaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactUpload {
    pub method: String,
    pub path: String,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactGrant {
    pub artifact_id: String,
    pub state: String,
    pub upload: ArtifactUpload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactManifestResponse {
    pub protocol_version: ProtocolVersion,
    pub attempt_id: AttemptId,
    pub artifacts: Vec<ArtifactGrant>,
}

/// `PUT /attempts/{id}/artifacts/{artifact_id}/content`: the board has the bytes and they are the
/// ones the manifest announced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactContentResponse {
    pub protocol_version: ProtocolVersion,
    pub attempt_id: AttemptId,
    pub artifact_id: String,
    pub state: String,
    pub size_bytes: u64,
    pub sha256: String,
}

/// What a runner saw after the board asked an attempt to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelObservation {
    ProcessStopped,
    AlreadyTerminal,
    Ambiguous,
}

/// `POST /attempts/{id}/cancellation-observation`. The adapter owns `observed_at` and
/// `details` (non-secret evidence); the board stores both as given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancellationObservationRequest {
    pub protocol_version: ProtocolVersion,
    pub runner_id: RunnerId,
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub cancellation_request_id: CancellationRequestId,
    pub observation: CancelObservation,
    pub observed_at: DateTime<Utc>,
    pub details: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancellationObservationResponse {
    pub protocol_version: ProtocolVersion,
    pub attempt_id: AttemptId,
    pub cancellation_request_id: CancellationRequestId,
    pub state: ExecutionState,
    pub replayed: bool,
    pub committed_at: DateTime<Utc>,
}

#[cfg(test)]
mod content_response_tests {
    use super::*;

    /// The body the board answers an artifact upload with: the keys a runner and the API
    /// reference name, no more.
    #[test]
    fn an_artifact_content_response_is_the_documented_body() {
        let response = ArtifactContentResponse {
            protocol_version: ProtocolVersion::v1(),
            attempt_id: AttemptId::new("att_1"),
            artifact_id: "art_1".into(),
            state: "content_verified".into(),
            size_bytes: 12,
            sha256: "a".repeat(64),
        };
        let value = serde_json::to_value(&response).unwrap();
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "artifact_id",
                "attempt_id",
                "protocol_version",
                "sha256",
                "size_bytes",
                "state"
            ]
        );
        assert_eq!(value["protocol_version"], 1);
        let back: ArtifactContentResponse = serde_json::from_value(value).unwrap();
        assert_eq!(back, response);
    }
}
