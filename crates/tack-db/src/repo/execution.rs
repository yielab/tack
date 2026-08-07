//! Durable neutral execution queue.  This module intentionally uses opaque text
//! identifiers: runner-protocol ids are not UUIDs and callers must not parse
//! their example prefixes.  All runner-owned writes are fenced in SQL.

use chrono::{DateTime, Duration, Utc};
use sqlx::Row;
use tracing::instrument;

use super::Repository;

pub trait ExecutionClock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Default)]
pub struct SystemExecutionClock;

impl ExecutionClock for SystemExecutionClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Debug, Clone)]
pub struct NewRunner<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub credential_hash: &'a str,
    pub labels: &'a str,
    pub total_capacity: i64,
    pub available_capacity: i64,
    pub capability_snapshot: &'a str,
    pub protocol_version: i64,
}

#[derive(Debug, Clone)]
pub struct NewAgentProfile<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub instructions: &'a str,
    pub tool_policy: &'a str,
    pub limits: &'a str,
}

#[derive(Debug, Clone)]
pub struct NewExecutionRequest<'a> {
    pub id: &'a str,
    pub item_id: &'a str,
    pub idempotency_scope: &'a str,
    pub idempotency_key: &'a str,
    /// A stable canonical representation of immutable request fields. Equal
    /// keys with different fingerprints are conflicts, never silent merges.
    pub request_fingerprint: &'a str,
    pub selector_kind: &'a str,
    pub selector_id: &'a str,
    pub agent_profile_id: Option<&'a str>,
    pub agent_profile_snapshot: &'a str,
    pub requested_harness_kind: Option<&'a str>,
    pub requested_model_provider: Option<&'a str>,
    pub requested_model_id: Option<&'a str>,
    pub repository_snapshot: &'a str,
    pub permission_policy: &'a str,
    pub timeout_seconds: Option<i64>,
    pub budgets: &'a str,
    pub status_map_policy_id: Option<&'a str>,
    pub environment: &'a str,
    pub metadata: &'a str,
    pub request_snapshot: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnqueueResult {
    Created(String),
    Replayed(String),
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lease {
    pub attempt_id: String,
    pub request_id: String,
    pub attempt_number: i64,
    pub runner_id: String,
    pub fencing_token: i64,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewEvent<'a> {
    pub id: &'a str,
    pub event_id: &'a str,
    pub sequence: i64,
    pub source: &'a str,
    pub kind: &'a str,
    pub payload: &'a str,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct EventBatch<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub previous_checkpoint: Option<&'a str>,
    pub checkpoint: &'a str,
}

#[derive(Debug, Clone)]
pub struct Completion<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub completion_id: &'a str,
    pub final_event_checkpoint: Option<&'a str>,
    pub terminal_state: &'a str,
    pub terminal_reason: &'a str,
    pub actual_execution: &'a str,
    pub usage: &'a str,
}

#[derive(Debug, Clone)]
pub struct NewArtifact<'a> {
    pub id: &'a str,
    pub artifact_id: &'a str,
    pub kind: &'a str,
    pub name: &'a str,
    pub media_type: Option<&'a str>,
    pub size_bytes: i64,
    pub sha256: &'a str,
    pub content_disposition: Option<&'a str>,
    pub content_reference: Option<&'a str>,
    pub metadata: &'a str,
}

#[derive(Debug, Clone)]
pub struct NewDecision<'a> {
    pub id: &'a str,
    pub decision_id: &'a str,
    pub kind: &'a str,
    pub prompt: &'a str,
    pub options: &'a str,
    pub metadata: &'a str,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct EnrollmentToken<'a> {
    pub id: &'a str,
    pub runner_id: &'a str,
    pub token_hash: &'a str,
    pub expires_at: DateTime<Utc>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentTokenMetadata {
    pub id: String,
    pub runner_id: String,
    pub expires_at: String,
    pub consumed_at: Option<String>,
    pub revoked_at: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatorRequeueResult {
    Requeued,
    Replayed,
    Conflict,
    InvalidTransition,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedeemEnrollmentResult {
    Redeemed(String),
    InvalidOrExpired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimReplayResult {
    Lease(Lease),
    NoWork,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaimedExecution {
    pub lease: Lease,
    pub request_snapshot: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct HeartbeatLease<'a> {
    pub attempt_id: &'a str,
    pub fencing_token: i64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatLeaseResponse {
    pub attempt_id: String,
    pub fencing_token: i64,
    pub lease_expires_at: DateTime<Utc>,
    pub cancellation_requested: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeartbeatBatchResponse {
    pub heartbeat_id: String,
    pub accepted_at: DateTime<Utc>,
    pub leases: Vec<HeartbeatLeaseResponse>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeartbeatBatchResult {
    Accepted(HeartbeatBatchResponse),
    Replayed(HeartbeatBatchResponse),
    Conflict,
    StaleLease(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventBatchResult {
    pub accepted_event_ids: Vec<String>,
    pub duplicate_event_ids: Vec<String>,
    pub committed_checkpoint: String,
    pub replayed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventApplyResult {
    Applied(EventBatchResult),
    ReplayConflict,
    Stale,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryClassification {
    SafePreSpawnRequeue,
    NeedsOperator,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryObservation {
    ProcessStopped,
    ProcessRunning,
    Ambiguous,
}
#[derive(Debug, Clone)]
pub struct RecoveryObservationInput<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub recovery_key: &'a str,
    pub observation: RecoveryObservation,
    pub details: &'a str,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryDisposition {
    SafePreSpawnRequeue,
    NeedsOperator,
    AlreadyTerminal,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryResponse {
    pub attempt_id: String,
    pub recovery_key: String,
    pub disposition: RecoveryDisposition,
    pub committed_at: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryObservationResult {
    Applied(RecoveryResponse),
    Replayed(RecoveryResponse),
    Conflict,
    Stale,
}
#[derive(Debug, Clone)]
pub struct CancellationObservationInput<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub cancellation_request_id: &'a str,
    pub observed_at: DateTime<Utc>,
    pub details: &'a str,
    pub observation: &'a str,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancellationResponse {
    pub attempt_id: String,
    pub cancellation_request_id: String,
    pub state: String,
    pub committed_at: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancellationObservation {
    Cancelled(CancellationResponse),
    Replayed(CancellationResponse),
    Conflict,
    Stale,
    AlreadyTerminal { state: String },
    Ambiguous { state: String },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionResponse {
    pub completion_id: String,
    pub terminal_state: String,
    pub final_event_checkpoint: Option<String>,
    pub committed_at: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompletionResult {
    Committed(CompletionResponse),
    Replayed(CompletionResponse),
    Conflict,
    Stale,
}

fn stamp(clock: &dyn ExecutionClock) -> String {
    clock.now().to_rfc3339()
}

fn terminal(state: &str) -> bool {
    matches!(state, "succeeded" | "failed" | "cancelled")
}

fn event_batch_fingerprint(
    batch: &EventBatch<'_>,
    events: &[NewEvent<'_>],
) -> Result<String, sqlx::Error> {
    let values: Result<Vec<serde_json::Value>, sqlx::Error> = events
        .iter()
        .map(|event| {
            let payload: serde_json::Value = serde_json::from_str(event.payload)
                .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
            Ok(serde_json::json!({
                "event_id": event.event_id,
                "sequence": event.sequence,
                "source": event.source,
                "kind": event.kind,
                "payload": canonical_json(payload),
                "occurred_at": event.occurred_at.to_rfc3339(),
            }))
        })
        .collect();
    serde_json::to_string(&serde_json::json!({
        "runner_id": batch.runner_id,
        "fencing_token": batch.fencing_token,
        "previous_checkpoint": batch.previous_checkpoint,
        "events": values?,
    }))
    .map_err(|error| sqlx::Error::Protocol(error.to_string()))
}

fn canonical_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<_> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            serde_json::Value::Object(
                entries
                    .into_iter()
                    .map(|(k, v)| (k, canonical_json(v)))
                    .collect(),
            )
        }
        serde_json::Value::Array(v) => {
            serde_json::Value::Array(v.into_iter().map(canonical_json).collect())
        }
        v => v,
    }
}

fn canonical_json_or_string(value: &str) -> serde_json::Value {
    match serde_json::from_str(value) {
        Ok(value) => canonical_json(value),
        Err(_) => serde_json::Value::String(value.into()),
    }
}

fn completion_fingerprint(completion: &Completion<'_>) -> Result<String, sqlx::Error> {
    let actual_execution: serde_json::Value = serde_json::from_str(completion.actual_execution)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let usage: serde_json::Value = serde_json::from_str(completion.usage)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    serde_json::to_string(&serde_json::json!({
        "runner_id": completion.runner_id,
        "attempt_id": completion.attempt_id,
        "fencing_token": completion.fencing_token,
        "completion_id": completion.completion_id,
        "final_event_checkpoint": completion.final_event_checkpoint,
        "terminal_state": completion.terminal_state,
        "terminal_reason": canonical_json_or_string(completion.terminal_reason),
        "actual_execution": canonical_json(actual_execution),
        "usage": canonical_json(usage),
    }))
    .map_err(|error| sqlx::Error::Protocol(error.to_string()))
}

fn replay_response(serialized: &str) -> Result<CompletionResponse, sqlx::Error> {
    let value: serde_json::Value = serde_json::from_str(serialized)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| sqlx::Error::Protocol("invalid completion replay response".into()))?;
    let required = |field: &str| {
        object
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                sqlx::Error::Protocol(format!("invalid completion replay response field: {field}"))
            })
    };
    let final_event_checkpoint = match object.get("final_event_checkpoint") {
        Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(value)) => Some(value.clone()),
        _ => {
            return Err(sqlx::Error::Protocol(
                "invalid completion replay response field: final_event_checkpoint".into(),
            ));
        }
    };
    Ok(CompletionResponse {
        completion_id: required("completion_id")?,
        terminal_state: required("terminal_state")?,
        final_event_checkpoint,
        committed_at: required("committed_at")?,
    })
}

fn heartbeat_fingerprint(
    runner_id: &str,
    heartbeat_id: &str,
    sent_at: DateTime<Utc>,
    available_capacity: i64,
    leases: &[HeartbeatLease<'_>],
    lease_duration: Duration,
) -> Result<String, sqlx::Error> {
    let lease_duration_nanoseconds = lease_duration
        .num_nanoseconds()
        .ok_or_else(|| sqlx::Error::Protocol("heartbeat lease duration is out of range".into()))?;
    let mut leases = leases
        .iter()
        .map(|lease| (lease.attempt_id, lease.fencing_token))
        .collect::<Vec<_>>();
    leases.sort_unstable();
    serde_json::to_string(&serde_json::json!({
        "runner_id": runner_id,
        "heartbeat_id": heartbeat_id,
        "sent_at": sent_at.to_rfc3339(),
        "available_capacity": available_capacity,
        "lease_duration_nanoseconds": lease_duration_nanoseconds,
        "leases": leases
            .into_iter()
            .map(|(attempt_id, fencing_token)| serde_json::json!({
                "attempt_id": attempt_id,
                "fencing_token": fencing_token,
            }))
            .collect::<Vec<_>>(),
    }))
    .map_err(|error| sqlx::Error::Protocol(error.to_string()))
}

fn heartbeat_response(serialized: &str) -> Result<HeartbeatBatchResponse, sqlx::Error> {
    let value: serde_json::Value = serde_json::from_str(serialized)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| sqlx::Error::Protocol("invalid heartbeat replay response".into()))?;
    let required_string = |object: &serde_json::Map<String, serde_json::Value>, field: &str| {
        object
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                sqlx::Error::Protocol(format!("invalid heartbeat replay response field: {field}"))
            })
    };
    let accepted_at = DateTime::parse_from_rfc3339(&required_string(object, "accepted_at")?)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let leases = object
        .get("leases")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            sqlx::Error::Protocol("invalid heartbeat replay response field: leases".into())
        })?
        .iter()
        .map(|value| {
            let value = value.as_object().ok_or_else(|| {
                sqlx::Error::Protocol("invalid heartbeat replay response lease".into())
            })?;
            let lease_expires_at =
                DateTime::parse_from_rfc3339(&required_string(value, "lease_expires_at")?)
                    .map(|value| value.with_timezone(&Utc))
                    .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
            Ok(HeartbeatLeaseResponse {
                attempt_id: required_string(value, "attempt_id")?,
                fencing_token: value
                    .get("fencing_token")
                    .and_then(serde_json::Value::as_i64)
                    .ok_or_else(|| {
                        sqlx::Error::Protocol(
                            "invalid heartbeat replay response field: fencing_token".into(),
                        )
                    })?,
                lease_expires_at,
                cancellation_requested: value
                    .get("cancellation_requested")
                    .and_then(serde_json::Value::as_bool)
                    .ok_or_else(|| {
                        sqlx::Error::Protocol(
                            "invalid heartbeat replay response field: cancellation_requested"
                                .into(),
                        )
                    })?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(HeartbeatBatchResponse {
        heartbeat_id: required_string(object, "heartbeat_id")?,
        accepted_at,
        leases,
    })
}

fn cancellation_fingerprint(
    input: &CancellationObservationInput<'_>,
) -> Result<String, sqlx::Error> {
    let details: serde_json::Value = serde_json::from_str(input.details)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let observation: serde_json::Value = serde_json::from_str(input.observation)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    serde_json::to_string(&serde_json::json!({
        "runner_id": input.runner_id,
        "attempt_id": input.attempt_id,
        "fencing_token": input.fencing_token,
        "cancellation_request_id": input.cancellation_request_id,
        "observed_at": input.observed_at.to_rfc3339(),
        "details": canonical_json(details),
        "observation": canonical_json(observation),
    }))
    .map_err(|error| sqlx::Error::Protocol(error.to_string()))
}

fn cancellation_response(serialized: &str) -> Result<CancellationResponse, sqlx::Error> {
    let value: serde_json::Value = serde_json::from_str(serialized)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| sqlx::Error::Protocol("invalid cancellation replay response".into()))?;
    let required = |field: &str| {
        object
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                sqlx::Error::Protocol(format!(
                    "invalid cancellation replay response field: {field}"
                ))
            })
    };
    Ok(CancellationResponse {
        attempt_id: required("attempt_id")?,
        cancellation_request_id: required("cancellation_request_id")?,
        state: required("state")?,
        committed_at: required("committed_at")?,
    })
}

struct RecoveryDetails {
    journal_state: String,
    process_observed: bool,
}

fn recovery_details(details: &str) -> Result<RecoveryDetails, sqlx::Error> {
    let value: serde_json::Value =
        serde_json::from_str(details).map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| sqlx::Error::Protocol("invalid recovery details".into()))?;
    let journal_state = object
        .get("journal_state")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| sqlx::Error::Protocol("invalid recovery details journal_state".into()))?;
    let process_observed = object
        .get("process_observed")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| sqlx::Error::Protocol("invalid recovery details process_observed".into()))?;
    Ok(RecoveryDetails {
        journal_state,
        process_observed,
    })
}

fn recovery_observation_name(observation: RecoveryObservation) -> &'static str {
    match observation {
        RecoveryObservation::ProcessStopped => "process_stopped",
        RecoveryObservation::ProcessRunning => "process_running",
        RecoveryObservation::Ambiguous => "ambiguous",
    }
}

fn recovery_fingerprint(input: &RecoveryObservationInput<'_>) -> Result<String, sqlx::Error> {
    let details: serde_json::Value = serde_json::from_str(input.details)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    serde_json::to_string(&serde_json::json!({
        "runner_id": input.runner_id,
        "attempt_id": input.attempt_id,
        "fencing_token": input.fencing_token,
        "recovery_key": input.recovery_key,
        "observation": recovery_observation_name(input.observation),
        "details": canonical_json(details),
    }))
    .map_err(|error| sqlx::Error::Protocol(error.to_string()))
}

fn recovery_response(serialized: &str) -> Result<RecoveryResponse, sqlx::Error> {
    let value: serde_json::Value = serde_json::from_str(serialized)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let object = value
        .as_object()
        .ok_or_else(|| sqlx::Error::Protocol("invalid recovery replay response".into()))?;
    let required = |field: &str| {
        object
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                sqlx::Error::Protocol(format!("invalid recovery replay response field: {field}"))
            })
    };
    let disposition = match required("disposition")?.as_str() {
        "safe_pre_spawn_requeue" => RecoveryDisposition::SafePreSpawnRequeue,
        "needs_operator" => RecoveryDisposition::NeedsOperator,
        "already_terminal" => RecoveryDisposition::AlreadyTerminal,
        _ => {
            return Err(sqlx::Error::Protocol(
                "invalid recovery replay response disposition".into(),
            ));
        }
    };
    Ok(RecoveryResponse {
        attempt_id: required("attempt_id")?,
        recovery_key: required("recovery_key")?,
        disposition,
        committed_at: required("committed_at")?,
    })
}

fn lease_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Lease, sqlx::Error> {
    let issued: String = row.get("lease_issued_at");
    let expires: String = row.get("lease_expires_at");
    let parse = |value: String| {
        DateTime::parse_from_rfc3339(&value)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|error| sqlx::Error::Protocol(error.to_string()))
    };
    Ok(Lease {
        attempt_id: row.get("id"),
        request_id: row.get("request_id"),
        attempt_number: row.get("attempt_number"),
        runner_id: row.get("runner_id"),
        fencing_token: row.get("fencing_token"),
        issued_at: parse(issued)?,
        expires_at: parse(expires)?,
    })
}

fn snapshot(row: &sqlx::sqlite::SqliteRow) -> Result<serde_json::Value, sqlx::Error> {
    let value =
        serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("request_snapshot"))
            .map_err(|error| {
                sqlx::Error::Protocol(format!("invalid persisted request_snapshot: {error}"))
            })?;
    if !value.as_object().is_some_and(|object| {
        object.contains_key("created_by")
            && object.contains_key("selector")
            && object.contains_key("repository")
    }) {
        return Err(sqlx::Error::Protocol(
            "persisted request_snapshot is incomplete".into(),
        ));
    }
    Ok(value)
}

impl Repository {
    pub async fn complete_execution_result(
        &self,
        completion: Completion<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<CompletionResult, sqlx::Error> {
        if !terminal(completion.terminal_state) {
            return Ok(CompletionResult::Conflict);
        }
        let now = stamp(clock);
        let fingerprint = completion_fingerprint(&completion)?;
        let mut tx = self.pool().begin().await?;
        let row = sqlx::query(
            "SELECT request_id,runner_id,state,lease_expires_at,event_checkpoint FROM execution_attempts WHERE id=? AND runner_id=? AND fencing_token=?",
        )
        .bind(completion.attempt_id)
        .bind(completion.runner_id)
        .bind(completion.fencing_token)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(CompletionResult::Stale);
        };
        if let Some(row) = sqlx::query(
            "SELECT fingerprint,response FROM execution_completion_replays WHERE attempt_id=? AND completion_id=?",
        )
        .bind(completion.attempt_id)
        .bind(completion.completion_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_fingerprint: String = row.get("fingerprint");
            if stored_fingerprint != fingerprint {
                tx.commit().await?;
                return Ok(CompletionResult::Conflict);
            }
            let response: String = row.get("response");
            let response = replay_response(&response)?;
            tx.commit().await?;
            return Ok(CompletionResult::Replayed(response));
        }
        let state: String = row.get("state");
        let expires: String = row.get("lease_expires_at");
        if terminal(&state) {
            // Pre-M055 terminal attempts have no authoritative response to replay.
            tx.commit().await?;
            return Ok(CompletionResult::Conflict);
        }
        if expires <= now {
            tx.commit().await?;
            return Ok(CompletionResult::Stale);
        }
        let request: String = row.get("request_id");
        let runner: String = row.get("runner_id");
        let updated = sqlx::query("UPDATE execution_attempts SET state=?,completion_id=?,terminal_reason=?,actual_execution=?,usage=?,ended_at=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=? AND event_checkpoint IS ? AND state IN ('leased','preparing','running','waiting_decision') AND lease_expires_at > ?")
            .bind(completion.terminal_state).bind(completion.completion_id).bind(completion.terminal_reason).bind(completion.actual_execution).bind(completion.usage).bind(&now).bind(&now).bind(completion.attempt_id).bind(completion.runner_id).bind(completion.fencing_token).bind(completion.final_event_checkpoint).bind(&now).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(CompletionResult::Conflict);
        }
        sqlx::query("UPDATE execution_requests SET state=?,updated_at=? WHERE id=?")
            .bind(completion.terminal_state)
            .bind(&now)
            .bind(request)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE agent_runners SET available_capacity=MIN(total_capacity,available_capacity+1),updated_at=? WHERE id=?").bind(&now).bind(runner).execute(&mut *tx).await?;
        let response = CompletionResponse {
            completion_id: completion.completion_id.into(),
            terminal_state: completion.terminal_state.into(),
            final_event_checkpoint: completion.final_event_checkpoint.map(str::to_owned),
            committed_at: now.clone(),
        };
        let serialized_response = serde_json::json!({
            "completion_id": response.completion_id,
            "terminal_state": response.terminal_state,
            "final_event_checkpoint": response.final_event_checkpoint,
            "committed_at": response.committed_at,
        })
        .to_string();
        sqlx::query("INSERT INTO execution_completion_replays(attempt_id,completion_id,fingerprint,response,committed_at) VALUES(?,?,?,?,?)").bind(completion.attempt_id).bind(completion.completion_id).bind(fingerprint).bind(serialized_response).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(CompletionResult::Committed(response))
    }
    pub async fn create_pending_runner_and_issue_token(
        &self,
        runner: NewRunner<'_>,
        token: EnrollmentToken<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<(), sqlx::Error> {
        if token.runner_id != runner.id
            || token.expires_at <= clock.now()
            || runner.total_capacity < 0
            || runner.available_capacity < 0
            || runner.available_capacity > runner.total_capacity
        {
            return Err(sqlx::Error::Protocol(
                "invalid pending runner/token input".into(),
            ));
        }
        let now = stamp(clock);
        let mut tx = self.pool().begin().await?;
        sqlx::query("INSERT INTO agent_runners(id,name,credential_hash,state,labels,total_capacity,available_capacity,capability_snapshot,protocol_version,created_at,updated_at) VALUES(?,?,?,'pending_enrollment',?,?,?,?,?,?,?)").bind(runner.id).bind(runner.name).bind("pending:no-credential").bind(runner.labels).bind(runner.total_capacity).bind(runner.available_capacity).bind(runner.capability_snapshot).bind(runner.protocol_version).bind(&now).bind(&now).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO agent_enrollment_tokens(id,runner_id,token_hash,expires_at,created_at) VALUES(?,?,?,?,?)").bind(token.id).bind(token.runner_id).bind(token.token_hash).bind(token.expires_at.to_rfc3339()).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn enrollment_token_metadata(
        &self,
        runner_id: &str,
        token_id: &str,
    ) -> Result<Option<EnrollmentTokenMetadata>, sqlx::Error> {
        sqlx::query("SELECT id,runner_id,expires_at,consumed_at,revoked_at FROM agent_enrollment_tokens WHERE runner_id=? AND id=?").bind(runner_id).bind(token_id).fetch_optional(self.pool()).await?.map(|r|Ok(EnrollmentTokenMetadata{id:r.get("id"),runner_id:r.get("runner_id"),expires_at:r.get("expires_at"),consumed_at:r.get("consumed_at"),revoked_at:r.get("revoked_at")})).transpose()
    }

    pub async fn revoke_enrollment_token_by_id(
        &self,
        runner_id: &str,
        token_id: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let r=sqlx::query("UPDATE agent_enrollment_tokens SET revoked_at=COALESCE(revoked_at,?) WHERE runner_id=? AND id=? AND consumed_at IS NULL").bind(stamp(clock)).bind(runner_id).bind(token_id).execute(self.pool()).await?;
        Ok(r.rows_affected() == 1)
    }

    pub async fn revoke_runner(
        &self,
        runner_id: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let now = stamp(clock);
        let mut tx = self.pool().begin().await?;
        let r=sqlx::query("UPDATE agent_runners SET state='revoked',revoked_at=COALESCE(revoked_at,?),updated_at=? WHERE id=?").bind(&now).bind(&now).bind(runner_id).execute(&mut *tx).await?;
        if r.rows_affected() == 1 {
            sqlx::query("UPDATE agent_enrollment_tokens SET revoked_at=COALESCE(revoked_at,?) WHERE runner_id=? AND consumed_at IS NULL").bind(&now).bind(runner_id).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(r.rows_affected() == 1)
    }

    pub async fn operator_requeue_needs_operator(
        &self,
        request_id: &str,
        recovery_key: &str,
        actor: &str,
        reason_fingerprint: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<OperatorRequeueResult, sqlx::Error> {
        let now = stamp(clock);
        let mut tx = self.pool().begin().await?;
        let attempt:Option<String>=sqlx::query_scalar("SELECT id FROM execution_attempts WHERE request_id=? AND state='needs_operator' ORDER BY attempt_number DESC LIMIT 1").bind(request_id).fetch_optional(&mut *tx).await?;
        let Some(attempt) = attempt else {
            tx.commit().await?;
            return Ok(OperatorRequeueResult::InvalidTransition);
        };
        let details = format!("actor={actor};reason_fingerprint={reason_fingerprint}");
        if let Some(old) = sqlx::query_scalar::<_, String>(
            "SELECT details FROM execution_recovery_audits WHERE attempt_id=? AND recovery_key=?",
        )
        .bind(&attempt)
        .bind(recovery_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            tx.commit().await?;
            return Ok(if old == details {
                OperatorRequeueResult::Replayed
            } else {
                OperatorRequeueResult::Conflict
            });
        };
        if sqlx::query("UPDATE execution_requests SET state='queued',cancellation_requested_at=NULL,updated_at=? WHERE id=? AND state='needs_operator'").bind(&now).bind(request_id).execute(&mut *tx).await?.rows_affected()!=1{tx.rollback().await?;return Ok(OperatorRequeueResult::InvalidTransition)};
        sqlx::query("INSERT INTO execution_recovery_audits(attempt_id,recovery_key,classification,details,created_at) VALUES(?,?, 'operator_requeue', ?,?)").bind(&attempt).bind(recovery_key).bind(details).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(OperatorRequeueResult::Requeued)
    }
    #[allow(clippy::too_many_arguments)] // protocol batch fields must fingerprint together
    pub async fn heartbeat_batch(
        &self,
        runner_id: &str,
        heartbeat_id: &str,
        sent_at: DateTime<Utc>,
        available_capacity: i64,
        leases: &[HeartbeatLease<'_>],
        lease_duration: Duration,
        clock: &dyn ExecutionClock,
    ) -> Result<HeartbeatBatchResult, sqlx::Error> {
        let now = clock.now();
        let now_s = now.to_rfc3339();
        let expires = now + lease_duration;
        let fingerprint = heartbeat_fingerprint(
            runner_id,
            heartbeat_id,
            sent_at,
            available_capacity,
            leases,
            lease_duration,
        )?;
        let mut tx = self.pool().begin().await?;
        let capacity: Option<i64> = sqlx::query_scalar(
            "SELECT total_capacity FROM agent_runners WHERE id=? AND state='active' AND revoked_at IS NULL",
        )
        .bind(runner_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(capacity) = capacity else {
            tx.commit().await?;
            return Ok(HeartbeatBatchResult::StaleLease(runner_id.into()));
        };
        if let Some(row) = sqlx::query(
            "SELECT fingerprint,response FROM execution_heartbeat_replays WHERE runner_id=? AND heartbeat_id=?",
        )
        .bind(runner_id)
        .bind(heartbeat_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_fingerprint: String = row.get("fingerprint");
            if stored_fingerprint != fingerprint {
                tx.commit().await?;
                return Ok(HeartbeatBatchResult::Conflict);
            }
            let response: String = row.get("response");
            let response = heartbeat_response(&response)?;
            tx.commit().await?;
            return Ok(HeartbeatBatchResult::Replayed(response));
        }
        let active_reservations: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM execution_attempts WHERE runner_id=? AND state IN ('leased','preparing','running','waiting_decision')",
        )
        .bind(runner_id)
        .fetch_one(&mut *tx)
        .await?;
        let expected_available_capacity = capacity - active_reservations;
        if expected_available_capacity < 0 || available_capacity != expected_available_capacity {
            tx.rollback().await?;
            return Ok(HeartbeatBatchResult::Conflict);
        }
        let mut result = Vec::with_capacity(leases.len());
        for lease in leases {
            let row=sqlx::query("SELECT r.cancellation_requested_at FROM execution_attempts a JOIN execution_requests r ON r.id=a.request_id WHERE a.id=? AND a.runner_id=? AND a.fencing_token=? AND a.state IN ('leased','preparing','running','waiting_decision') AND a.lease_expires_at>?").bind(lease.attempt_id).bind(runner_id).bind(lease.fencing_token).bind(&now_s).fetch_optional(&mut *tx).await?;
            let Some(row) = row else {
                tx.rollback().await?;
                return Ok(HeartbeatBatchResult::StaleLease(lease.attempt_id.into()));
            };
            let cancellation: Option<String> = row.get("cancellation_requested_at");
            sqlx::query("UPDATE execution_attempts SET last_heartbeat_at=?,lease_expires_at=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=?").bind(&now_s).bind(expires.to_rfc3339()).bind(&now_s).bind(lease.attempt_id).bind(runner_id).bind(lease.fencing_token).execute(&mut *tx).await?;
            result.push(HeartbeatLeaseResponse {
                attempt_id: lease.attempt_id.into(),
                fencing_token: lease.fencing_token,
                lease_expires_at: expires,
                cancellation_requested: cancellation.is_some(),
            });
        }
        sqlx::query("UPDATE agent_runners SET available_capacity=?,last_heartbeat_at=?,updated_at=? WHERE id=?").bind(available_capacity).bind(&now_s).bind(&now_s).bind(runner_id).execute(&mut *tx).await?;
        let response = HeartbeatBatchResponse {
            heartbeat_id: heartbeat_id.into(),
            accepted_at: now,
            leases: result,
        };
        let stored = serde_json::json!({
            "heartbeat_id": response.heartbeat_id,
            "accepted_at": response.accepted_at.to_rfc3339(),
            "leases": response.leases.iter().map(|lease| serde_json::json!({
                "attempt_id": lease.attempt_id,
                "fencing_token": lease.fencing_token,
                "lease_expires_at": lease.lease_expires_at.to_rfc3339(),
                "cancellation_requested": lease.cancellation_requested,
            })).collect::<Vec<_>>(),
        })
        .to_string();
        sqlx::query("INSERT INTO execution_heartbeat_replays(runner_id,heartbeat_id,fingerprint,response,created_at) VALUES(?,?,?,?,?)").bind(runner_id).bind(heartbeat_id).bind(fingerprint).bind(&stored).bind(&now_s).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(HeartbeatBatchResult::Accepted(response))
    }

    pub async fn recover_attempt(
        &self,
        input: RecoveryObservationInput<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<RecoveryObservationResult, sqlx::Error> {
        let now = stamp(clock);
        let details = recovery_details(input.details)?;
        let fingerprint = recovery_fingerprint(&input)?;
        let mut tx = self.pool().begin().await?;
        let row = sqlx::query("SELECT a.request_id,a.runner_id,a.state,a.started_at FROM execution_attempts a JOIN agent_runners r ON r.id=a.runner_id WHERE a.id=? AND a.runner_id=? AND a.fencing_token=? AND r.state='active' AND r.revoked_at IS NULL")
            .bind(input.attempt_id).bind(input.runner_id).bind(input.fencing_token).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(RecoveryObservationResult::Stale);
        };
        if let Some(row) = sqlx::query("SELECT fingerprint,response FROM execution_recovery_audits WHERE attempt_id=? AND recovery_key=?")
            .bind(input.attempt_id).bind(input.recovery_key).fetch_optional(&mut *tx).await? {
            let stored_fingerprint: String = row.get("fingerprint");
            if stored_fingerprint != fingerprint {
                tx.commit().await?;
                return Ok(RecoveryObservationResult::Conflict);
            }
            let response: String = row.get("response");
            let response = recovery_response(&response)?;
            tx.commit().await?;
            return Ok(RecoveryObservationResult::Replayed(response));
        }
        let request: String = row.get("request_id");
        let runner: String = row.get("runner_id");
        let state: String = row.get("state");
        if terminal(&state) {
            let response = RecoveryResponse {
                attempt_id: input.attempt_id.into(),
                recovery_key: input.recovery_key.into(),
                disposition: RecoveryDisposition::AlreadyTerminal,
                committed_at: now.clone(),
            };
            let serialized_response = serde_json::json!({
                "attempt_id": response.attempt_id,
                "recovery_key": response.recovery_key,
                "disposition": "already_terminal",
                "committed_at": response.committed_at,
            })
            .to_string();
            sqlx::query("INSERT INTO execution_recovery_audits(attempt_id,recovery_key,classification,details,fingerprint,response,created_at) VALUES(?,?, 'already_terminal',?,?,?,?)")
                .bind(input.attempt_id).bind(input.recovery_key).bind(input.details).bind(fingerprint).bind(serialized_response).bind(&now).execute(&mut *tx).await?;
            tx.commit().await?;
            return Ok(RecoveryObservationResult::Applied(response));
        }
        if !matches!(
            state.as_str(),
            "leased" | "preparing" | "running" | "waiting_decision"
        ) {
            tx.commit().await?;
            return Ok(RecoveryObservationResult::Stale);
        }
        let started: Option<String> = row.get("started_at");
        let disposition = match input.observation {
            RecoveryObservation::ProcessStopped
                if started.is_none()
                    && details.journal_state == "prepared"
                    && !details.process_observed =>
            {
                RecoveryDisposition::SafePreSpawnRequeue
            }
            RecoveryObservation::ProcessStopped
            | RecoveryObservation::ProcessRunning
            | RecoveryObservation::Ambiguous => RecoveryDisposition::NeedsOperator,
        };
        let (attempt_state, request_state, classification) = match disposition {
            RecoveryDisposition::SafePreSpawnRequeue => {
                ("lost", "queued", "safe_pre_spawn_requeue")
            }
            RecoveryDisposition::NeedsOperator => {
                ("needs_operator", "needs_operator", "needs_operator")
            }
            RecoveryDisposition::AlreadyTerminal => unreachable!("terminal states return above"),
        };
        let updated = sqlx::query("UPDATE execution_attempts SET state=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=? AND state IN ('leased','preparing','running','waiting_decision')")
            .bind(attempt_state)
            .bind(&now)
            .bind(input.attempt_id)
            .bind(input.runner_id)
            .bind(input.fencing_token)
            .execute(&mut *tx)
            .await?;
        if updated.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(RecoveryObservationResult::Stale);
        }
        sqlx::query("UPDATE execution_requests SET state=?,cancellation_requested_at=CASE WHEN ?='queued' THEN NULL ELSE cancellation_requested_at END,updated_at=? WHERE id=?")
            .bind(request_state)
            .bind(request_state)
            .bind(&now)
            .bind(&request)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE agent_runners SET available_capacity=MIN(total_capacity,available_capacity+1),updated_at=? WHERE id=?").bind(&now).bind(&runner).execute(&mut *tx).await?;
        let response = RecoveryResponse {
            attempt_id: input.attempt_id.into(),
            recovery_key: input.recovery_key.into(),
            disposition: disposition.clone(),
            committed_at: now.clone(),
        };
        let serialized_response = serde_json::json!({
            "attempt_id": response.attempt_id,
            "recovery_key": response.recovery_key,
            "disposition": match response.disposition {
                RecoveryDisposition::SafePreSpawnRequeue => "safe_pre_spawn_requeue",
                RecoveryDisposition::NeedsOperator => "needs_operator",
                RecoveryDisposition::AlreadyTerminal => "already_terminal",
            },
            "committed_at": response.committed_at,
        })
        .to_string();
        sqlx::query("INSERT INTO execution_recovery_audits(attempt_id,recovery_key,classification,details,fingerprint,response,created_at) VALUES(?,?,?,?,?,?,?)")
            .bind(input.attempt_id).bind(input.recovery_key).bind(classification).bind(input.details).bind(fingerprint).bind(serialized_response).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(RecoveryObservationResult::Applied(response))
    }

    pub async fn observe_cancellation(
        &self,
        input: CancellationObservationInput<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<CancellationObservation, sqlx::Error> {
        let now = stamp(clock);
        let fingerprint = cancellation_fingerprint(&input)?;
        let mut tx = self.pool().begin().await?;
        let row = sqlx::query("SELECT a.request_id,a.runner_id,a.state,a.lease_expires_at,r.cancellation_requested_at FROM execution_attempts a JOIN execution_requests r ON r.id=a.request_id WHERE a.id=? AND a.runner_id=? AND a.fencing_token=?")
            .bind(input.attempt_id).bind(input.runner_id).bind(input.fencing_token).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(CancellationObservation::Stale);
        };
        if let Some(row) = sqlx::query("SELECT fingerprint,response FROM execution_cancellation_replays WHERE attempt_id=? AND cancellation_request_id=?")
            .bind(input.attempt_id).bind(input.cancellation_request_id).fetch_optional(&mut *tx).await? {
            let stored_fingerprint: String = row.get("fingerprint");
            if stored_fingerprint != fingerprint {
                tx.commit().await?;
                return Ok(CancellationObservation::Conflict);
            }
            let response: String = row.get("response");
            let response = cancellation_response(&response)?;
            tx.commit().await?;
            return Ok(CancellationObservation::Replayed(response));
        }
        let request: String = row.get("request_id");
        let owner: String = row.get("runner_id");
        let state: String = row.get("state");
        if terminal(&state) {
            tx.commit().await?;
            return Ok(CancellationObservation::AlreadyTerminal { state });
        }
        if !matches!(
            state.as_str(),
            "leased" | "preparing" | "running" | "waiting_decision"
        ) {
            tx.commit().await?;
            return Ok(CancellationObservation::Ambiguous { state });
        }
        let expires: String = row.get("lease_expires_at");
        if expires <= now {
            tx.commit().await?;
            return Ok(CancellationObservation::Stale);
        }
        let cancellation_requested_at: Option<String> = row.get("cancellation_requested_at");
        if cancellation_requested_at.is_none() {
            tx.commit().await?;
            return Ok(CancellationObservation::Ambiguous { state });
        }
        let updated = sqlx::query("UPDATE execution_attempts SET state='cancelled',completion_id=?,ended_at=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=? AND state IN ('leased','preparing','running','waiting_decision') AND lease_expires_at>?")
            .bind(input.cancellation_request_id).bind(&now).bind(&now).bind(input.attempt_id).bind(input.runner_id).bind(input.fencing_token).bind(&now).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(CancellationObservation::Ambiguous { state });
        }
        sqlx::query("UPDATE execution_requests SET state='cancelled',updated_at=? WHERE id=?")
            .bind(&now)
            .bind(request)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE agent_runners SET available_capacity=MIN(total_capacity,available_capacity+1),updated_at=? WHERE id=?").bind(&now).bind(owner).execute(&mut *tx).await?;
        let response = CancellationResponse {
            attempt_id: input.attempt_id.into(),
            cancellation_request_id: input.cancellation_request_id.into(),
            state: "cancelled".into(),
            committed_at: now.clone(),
        };
        let serialized_response = serde_json::json!({
            "attempt_id": response.attempt_id,
            "cancellation_request_id": response.cancellation_request_id,
            "state": response.state,
            "committed_at": response.committed_at,
        })
        .to_string();
        sqlx::query("INSERT INTO execution_cancellation_replays(attempt_id,cancellation_request_id,state,fingerprint,response,created_at) VALUES(?,?, 'cancelled',?,?,?)")
            .bind(input.attempt_id).bind(input.cancellation_request_id).bind(fingerprint).bind(serialized_response).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(CancellationObservation::Cancelled(response))
    }
    /// Store only a token hash. Tokens are tied to a pre-created runner so a
    /// redemption can atomically consume the token and activate that identity.
    pub async fn issue_enrollment_token(
        &self,
        token: EnrollmentToken<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("INSERT INTO agent_enrollment_tokens (id, runner_id, token_hash, expires_at, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(token.id).bind(token.runner_id).bind(token.token_hash).bind(token.expires_at.to_rfc3339()).bind(stamp(clock))
            .execute(self.pool()).await?;
        Ok(())
    }

    pub async fn revoke_enrollment_token(
        &self,
        token_hash: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("UPDATE agent_enrollment_tokens SET revoked_at = COALESCE(revoked_at, ?) WHERE token_hash = ? AND consumed_at IS NULL")
            .bind(stamp(clock)).bind(token_hash).execute(self.pool()).await?;
        Ok(result.rows_affected() == 1)
    }

    #[allow(clippy::too_many_arguments)] // protocol exchange fields must commit together
    pub async fn redeem_enrollment_token(
        &self,
        token_hash: &str,
        credential_hash: &str,
        credential_expires_at: DateTime<Utc>,
        runner_version: &str,
        runner_name: &str,
        labels: &str,
        total_capacity: i64,
        available_capacity: i64,
        capability_snapshot: &str,
        protocol_version: i64,
        clock: &dyn ExecutionClock,
    ) -> Result<RedeemEnrollmentResult, sqlx::Error> {
        let now = stamp(clock);
        let mut tx = self.pool().begin().await?;
        let token = sqlx::query("SELECT runner_id FROM agent_enrollment_tokens WHERE token_hash = ? AND consumed_at IS NULL AND revoked_at IS NULL AND expires_at > ?")
            .bind(token_hash).bind(&now).fetch_optional(&mut *tx).await?;
        let Some(token) = token else {
            tx.commit().await?;
            return Ok(RedeemEnrollmentResult::InvalidOrExpired);
        };
        let runner_id: String = token.get("runner_id");
        let used = sqlx::query("UPDATE agent_enrollment_tokens SET consumed_at = ? WHERE token_hash = ? AND consumed_at IS NULL AND revoked_at IS NULL AND expires_at > ?")
            .bind(&now).bind(token_hash).bind(&now).execute(&mut *tx).await?;
        if used.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(RedeemEnrollmentResult::InvalidOrExpired);
        }
        let runner = sqlx::query("UPDATE agent_runners SET credential_hash=?, credential_expires_at=?, credential_rotated_at=?, runner_version=?, name=?, labels=?, total_capacity=?, available_capacity=?, capability_snapshot=?, protocol_version=?, state='active', updated_at=? WHERE id=? AND state='pending_enrollment' AND revoked_at IS NULL")
            .bind(credential_hash).bind(credential_expires_at.to_rfc3339()).bind(&now).bind(runner_version).bind(runner_name).bind(labels).bind(total_capacity).bind(available_capacity).bind(capability_snapshot).bind(protocol_version).bind(&now).bind(&runner_id).execute(&mut *tx).await?;
        if runner.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(RedeemEnrollmentResult::InvalidOrExpired);
        }
        tx.commit().await?;
        Ok(RedeemEnrollmentResult::Redeemed(runner_id))
    }

    /// Strong claim: replay lookup, capacity reservation, attempt and replay
    /// record are committed together, so a crash cannot lose the replay key.
    pub async fn claim_execution_idempotent_with_snapshot(
        &self,
        runner_id: &str,
        claim_request_id: &str,
        attempt_id: &str,
        lease_duration: Duration,
        clock: &dyn ExecutionClock,
    ) -> Result<Option<ClaimedExecution>, sqlx::Error> {
        let now = clock.now();
        let now_s = now.to_rfc3339();
        let expires = now + lease_duration;
        let mut tx = self.pool().begin().await?;
        let replay=sqlx::query("SELECT a.id,a.request_id,a.attempt_number,a.runner_id,a.fencing_token,a.lease_issued_at,a.lease_expires_at,r.request_snapshot FROM execution_claim_replays c JOIN execution_attempts a ON a.id=c.attempt_id JOIN execution_requests r ON r.id=a.request_id WHERE c.runner_id=? AND c.claim_request_id=?").bind(runner_id).bind(claim_request_id).fetch_optional(&mut *tx).await?;
        if let Some(row) = replay {
            let lease = lease_from_row(&row)?;
            let snapshot = snapshot(&row)?;
            tx.commit().await?;
            return Ok(Some(ClaimedExecution {
                lease,
                request_snapshot: snapshot,
            }));
        }
        if sqlx::query("UPDATE agent_runners SET available_capacity=available_capacity-1,updated_at=? WHERE id=? AND state='active' AND revoked_at IS NULL AND available_capacity>0").bind(&now_s).bind(runner_id).execute(&mut *tx).await?.rows_affected()!=1 { tx.commit().await?; return Ok(None); }
        let request=sqlx::query("SELECT id,request_snapshot FROM execution_requests WHERE state='queued' AND ((selector_kind='exact_runner' AND selector_id=?) OR (selector_kind='fleet' AND EXISTS(SELECT 1 FROM agent_fleet_members m WHERE m.fleet_id=selector_id AND m.runner_id=?))) ORDER BY created_at LIMIT 1").bind(runner_id).bind(runner_id).fetch_optional(&mut *tx).await?;
        let Some(request) = request else {
            tx.rollback().await?;
            return Ok(None);
        };
        let request_id: String = request.get("id");
        if sqlx::query("UPDATE execution_requests SET state='leased',updated_at=? WHERE id=? AND state='queued'").bind(&now_s).bind(&request_id).execute(&mut *tx).await?.rows_affected()!=1 { tx.rollback().await?; return Ok(None); }
        let n: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(attempt_number),0)+1 FROM execution_attempts WHERE request_id=?",
        )
        .bind(&request_id)
        .fetch_one(&mut *tx)
        .await?;
        let f: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(fencing_token),0)+1 FROM execution_attempts WHERE request_id=?",
        )
        .bind(&request_id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO execution_attempts(id,request_id,attempt_number,runner_id,fencing_token,lease_issued_at,lease_expires_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(attempt_id).bind(&request_id).bind(n).bind(runner_id).bind(f).bind(&now_s).bind(expires.to_rfc3339()).bind(&now_s).bind(&now_s).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO execution_claim_replays(runner_id,claim_request_id,attempt_id,created_at) VALUES(?,?,?,?)").bind(runner_id).bind(claim_request_id).bind(attempt_id).bind(&now_s).execute(&mut *tx).await?;
        let lease = Lease {
            attempt_id: attempt_id.into(),
            request_id,
            attempt_number: n,
            runner_id: runner_id.into(),
            fencing_token: f,
            issued_at: now,
            expires_at: expires,
        };
        let snapshot = snapshot(&request)?;
        tx.commit().await?;
        Ok(Some(ClaimedExecution {
            lease,
            request_snapshot: snapshot,
        }))
    }

    /// Compatibility form for callers that only need lease facts.
    pub async fn claim_execution_idempotent(
        &self,
        runner_id: &str,
        claim_request_id: &str,
        attempt_id: &str,
        lease_duration: Duration,
        clock: &dyn ExecutionClock,
    ) -> Result<ClaimReplayResult, sqlx::Error> {
        Ok(
            match self
                .claim_execution_idempotent_with_snapshot(
                    runner_id,
                    claim_request_id,
                    attempt_id,
                    lease_duration,
                    clock,
                )
                .await?
            {
                Some(claim) => ClaimReplayResult::Lease(claim.lease),
                None => ClaimReplayResult::NoWork,
            },
        )
    }
    #[instrument(skip(self, input, clock))]
    pub async fn register_runner(
        &self,
        input: NewRunner<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<(), sqlx::Error> {
        let now = stamp(clock);
        sqlx::query(
            "INSERT INTO agent_runners (id, name, credential_hash, labels, total_capacity, \
             available_capacity, capability_snapshot, protocol_version, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(input.id)
        .bind(input.name)
        .bind(input.credential_hash)
        .bind(input.labels)
        .bind(input.total_capacity)
        .bind(input.available_capacity)
        .bind(input.capability_snapshot)
        .bind(input.protocol_version)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    #[instrument(skip(self, input, clock))]
    pub async fn create_agent_profile(
        &self,
        input: NewAgentProfile<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<(), sqlx::Error> {
        let now = stamp(clock);
        sqlx::query(
            "INSERT INTO agent_profiles (id, name, instructions, tool_policy, limits, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(input.id).bind(input.name).bind(input.instructions)
        .bind(input.tool_policy).bind(input.limits).bind(&now).bind(&now)
        .execute(self.pool()).await?;
        Ok(())
    }

    #[instrument(skip(self, input, clock))]
    pub async fn enqueue_execution(
        &self,
        input: NewExecutionRequest<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<EnqueueResult, sqlx::Error> {
        let now = stamp(clock);
        let mut tx = self.pool().begin().await?;
        let existing: Option<(String, String)> = sqlx::query_as(
            "SELECT id, request_fingerprint FROM execution_requests \
             WHERE idempotency_scope = ? AND idempotency_key = ?",
        )
        .bind(input.idempotency_scope)
        .bind(input.idempotency_key)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((id, fingerprint)) = existing {
            tx.commit().await?;
            return Ok(if fingerprint == input.request_fingerprint {
                EnqueueResult::Replayed(id)
            } else {
                EnqueueResult::Conflict
            });
        }
        sqlx::query(
            "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key, \
             request_fingerprint, selector_kind, selector_id, agent_profile_id, agent_profile_snapshot, \
             requested_harness_kind, requested_model_provider, requested_model_id, repository_snapshot, \
             permission_policy, timeout_seconds, budgets, status_map_policy_id, environment, metadata, request_snapshot, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(input.id).bind(input.item_id).bind(input.idempotency_scope).bind(input.idempotency_key)
        .bind(input.request_fingerprint).bind(input.selector_kind).bind(input.selector_id)
        .bind(input.agent_profile_id).bind(input.agent_profile_snapshot).bind(input.requested_harness_kind)
        .bind(input.requested_model_provider).bind(input.requested_model_id).bind(input.repository_snapshot)
        .bind(input.permission_policy).bind(input.timeout_seconds).bind(input.budgets)
        .bind(input.status_map_policy_id).bind(input.environment).bind(input.metadata).bind(input.request_snapshot).bind(&now).bind(&now)
        .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(EnqueueResult::Created(input.id.to_string()))
    }

    /// Atomically reserves one eligible queued request.  The request state,
    /// attempt number/fence, and runner capacity are one transaction, so two
    /// claimers cannot obtain valid leases for the same request.
    #[instrument(skip(self, clock))]
    pub async fn claim_execution(
        &self,
        runner_id: &str,
        attempt_id: &str,
        lease_duration: Duration,
        clock: &dyn ExecutionClock,
    ) -> Result<Option<Lease>, sqlx::Error> {
        let now = clock.now();
        let now_s = now.to_rfc3339();
        let expires = now + lease_duration;
        let mut tx = self.pool().begin().await?;
        let capacity = sqlx::query(
            "UPDATE agent_runners SET available_capacity = available_capacity - 1, updated_at = ? \
             WHERE id = ? AND state = 'active' AND revoked_at IS NULL AND available_capacity > 0",
        )
        .bind(&now_s)
        .bind(runner_id)
        .execute(&mut *tx)
        .await?;
        if capacity.rows_affected() != 1 {
            tx.commit().await?;
            return Ok(None);
        }
        let request: Option<String> = sqlx::query_scalar(
            "SELECT r.id FROM execution_requests r WHERE r.state = 'queued' AND \
             ( (r.selector_kind = 'exact_runner' AND r.selector_id = ?) OR \
               (r.selector_kind = 'fleet' AND EXISTS (SELECT 1 FROM agent_fleet_members m WHERE m.fleet_id = r.selector_id AND m.runner_id = ?)) ) \
             ORDER BY r.created_at LIMIT 1",
        ).bind(runner_id).bind(runner_id).fetch_optional(&mut *tx).await?;
        let Some(request_id) = request else {
            sqlx::query("UPDATE agent_runners SET available_capacity = available_capacity + 1, updated_at = ? WHERE id = ?")
                .bind(&now_s).bind(runner_id).execute(&mut *tx).await?;
            tx.commit().await?;
            return Ok(None);
        };
        let claimed = sqlx::query("UPDATE execution_requests SET state = 'leased', updated_at = ? WHERE id = ? AND state = 'queued'")
            .bind(&now_s).bind(&request_id).execute(&mut *tx).await?;
        if claimed.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(None);
        }
        let attempt_number: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(attempt_number), 0) + 1 FROM execution_attempts WHERE request_id = ?")
            .bind(&request_id).fetch_one(&mut *tx).await?;
        let fence: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(fencing_token), 0) + 1 FROM execution_attempts WHERE request_id = ?")
            .bind(&request_id).fetch_one(&mut *tx).await?;
        sqlx::query("INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, fencing_token, lease_issued_at, lease_expires_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(attempt_id).bind(&request_id).bind(attempt_number).bind(runner_id).bind(fence)
            .bind(&now_s).bind(expires.to_rfc3339()).bind(&now_s).bind(&now_s).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(Some(Lease {
            attempt_id: attempt_id.into(),
            request_id,
            attempt_number,
            runner_id: runner_id.into(),
            fencing_token: fence,
            issued_at: now,
            expires_at: expires,
        }))
    }

    #[instrument(skip(self, clock))]
    pub async fn heartbeat_execution(
        &self,
        runner_id: &str,
        attempt_id: &str,
        fence: i64,
        lease_duration: Duration,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let now = clock.now();
        let result = sqlx::query("UPDATE execution_attempts SET last_heartbeat_at = ?, lease_expires_at = ?, updated_at = ? WHERE id = ? AND runner_id = ? AND fencing_token = ? AND state IN ('leased','preparing','running','waiting_decision') AND lease_expires_at > ?")
            .bind(now.to_rfc3339()).bind((now + lease_duration).to_rfc3339()).bind(now.to_rfc3339())
            .bind(attempt_id).bind(runner_id).bind(fence).bind(now.to_rfc3339()).execute(self.pool()).await?;
        Ok(result.rows_affected() == 1)
    }

    #[instrument(skip(self, events, clock))]
    pub async fn append_execution_events_result(
        &self,
        batch: EventBatch<'_>,
        events: &[NewEvent<'_>],
        clock: &dyn ExecutionClock,
    ) -> Result<EventApplyResult, sqlx::Error> {
        let now = stamp(clock);
        let fingerprint = event_batch_fingerprint(&batch, events)?;
        let mut tx = self.pool().begin().await?;
        let row = sqlx::query("SELECT event_checkpoint FROM execution_attempts WHERE id=? AND runner_id=? AND fencing_token=? AND state IN ('leased','preparing','running','waiting_decision') AND lease_expires_at>?")
            .bind(batch.attempt_id).bind(batch.runner_id).bind(batch.fencing_token).bind(&now).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(EventApplyResult::Stale);
        };
        if let Some(row) = sqlx::query(
            "SELECT fingerprint,response FROM execution_event_batch_replays WHERE attempt_id=? AND checkpoint=?",
        )
        .bind(batch.attempt_id)
        .bind(batch.checkpoint)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored: String = row.get("fingerprint");
            if stored != fingerprint {
                tx.commit().await?;
                return Ok(EventApplyResult::ReplayConflict);
            }
            let response: String = row.get("response");
            let value: serde_json::Value = serde_json::from_str(&response)
                .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
            let accepted = value["accepted_event_ids"]
                .as_array()
                .ok_or_else(|| sqlx::Error::Protocol("invalid event replay response".into()))?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .ok_or_else(|| sqlx::Error::Protocol("invalid event replay id".into()))
                        .map(str::to_owned)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let duplicate = value["duplicate_event_ids"]
                .as_array()
                .ok_or_else(|| sqlx::Error::Protocol("invalid event replay response".into()))?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .ok_or_else(|| sqlx::Error::Protocol("invalid event replay id".into()))
                        .map(str::to_owned)
                })
                .collect::<Result<Vec<_>, _>>()?;
            tx.commit().await?;
            return Ok(EventApplyResult::Applied(EventBatchResult {
                accepted_event_ids: accepted,
                duplicate_event_ids: duplicate,
                committed_checkpoint: batch.checkpoint.into(),
                replayed: true,
            }));
        }
        let current: Option<String> = row.get("event_checkpoint");
        if current.as_deref() == Some(batch.checkpoint) {
            tx.commit().await?;
            return Ok(EventApplyResult::ReplayConflict);
        }
        if current.as_deref() != batch.previous_checkpoint {
            tx.commit().await?;
            return Ok(EventApplyResult::ReplayConflict);
        }
        let mut accepted = Vec::new();
        let mut duplicate = Vec::new();
        for event in events {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM execution_events WHERE attempt_id=? AND event_id=?)",
            )
            .bind(batch.attempt_id)
            .bind(event.event_id)
            .fetch_one(&mut *tx)
            .await?;
            if exists {
                duplicate.push(event.event_id.into())
            } else {
                sqlx::query("INSERT INTO execution_events(id,attempt_id,event_id,sequence,source,kind,payload,occurred_at,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(event.id).bind(batch.attempt_id).bind(event.event_id).bind(event.sequence).bind(event.source).bind(event.kind).bind(event.payload).bind(event.occurred_at.to_rfc3339()).bind(&now).execute(&mut *tx).await?;
                accepted.push(event.event_id.into())
            }
        }
        let changed=sqlx::query("UPDATE execution_attempts SET event_checkpoint=?,updated_at=? WHERE id=? AND runner_id=? AND fencing_token=? AND event_checkpoint IS ?").bind(batch.checkpoint).bind(&now).bind(batch.attempt_id).bind(batch.runner_id).bind(batch.fencing_token).bind(batch.previous_checkpoint).execute(&mut *tx).await?;
        if changed.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(EventApplyResult::ReplayConflict);
        }
        let result = EventBatchResult {
            accepted_event_ids: accepted,
            duplicate_event_ids: duplicate,
            committed_checkpoint: batch.checkpoint.into(),
            replayed: false,
        };
        let response=serde_json::json!({"accepted_event_ids":result.accepted_event_ids,"duplicate_event_ids":result.duplicate_event_ids}).to_string();
        sqlx::query("INSERT INTO execution_event_batch_replays(attempt_id,checkpoint,fingerprint,response,created_at) VALUES(?,?,?,?,?)").bind(batch.attempt_id).bind(batch.checkpoint).bind(fingerprint).bind(response).bind(&now).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(EventApplyResult::Applied(result))
    }

    #[instrument(skip(self, events, clock))]
    pub async fn append_execution_events(
        &self,
        batch: EventBatch<'_>,
        events: &[NewEvent<'_>],
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let now = stamp(clock);
        let mut tx = self.pool().begin().await?;
        let row = sqlx::query("SELECT event_checkpoint FROM execution_attempts WHERE id = ? AND runner_id = ? AND fencing_token = ? AND state IN ('leased','preparing','running','waiting_decision') AND lease_expires_at > ?")
            .bind(batch.attempt_id).bind(batch.runner_id).bind(batch.fencing_token).bind(&now).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(false);
        };
        let current: Option<String> = row.get("event_checkpoint");
        if current.as_deref() == Some(batch.checkpoint) {
            tx.commit().await?;
            return Ok(true);
        }
        if current.as_deref() != batch.previous_checkpoint {
            tx.commit().await?;
            return Ok(false);
        }
        for event in events {
            sqlx::query("INSERT INTO execution_events (id, attempt_id, event_id, sequence, source, kind, payload, occurred_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(attempt_id, event_id) DO NOTHING")
                .bind(event.id).bind(batch.attempt_id).bind(event.event_id).bind(event.sequence).bind(event.source).bind(event.kind).bind(event.payload).bind(event.occurred_at.to_rfc3339()).bind(&now).execute(&mut *tx).await?;
        }
        let updated = sqlx::query("UPDATE execution_attempts SET event_checkpoint = ?, updated_at = ? WHERE id = ? AND runner_id = ? AND fencing_token = ? AND event_checkpoint IS ?")
            .bind(batch.checkpoint).bind(&now).bind(batch.attempt_id).bind(batch.runner_id).bind(batch.fencing_token).bind(batch.previous_checkpoint).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            tx.rollback().await?;
            return Ok(false);
        }
        tx.commit().await?;
        Ok(true)
    }

    #[instrument(skip(self, clock))]
    pub async fn complete_execution(
        &self,
        completion: Completion<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        Ok(matches!(
            self.complete_execution_result(completion, clock).await?,
            CompletionResult::Committed(_) | CompletionResult::Replayed(_)
        ))
    }

    #[instrument(skip(self, clock))]
    pub async fn request_execution_cancellation(
        &self,
        request_id: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let now = stamp(clock);
        let result = sqlx::query("UPDATE execution_requests SET cancellation_requested_at = COALESCE(cancellation_requested_at, ?), updated_at = ? WHERE id = ? AND state NOT IN ('succeeded','failed','cancelled')")
            .bind(&now).bind(&now).bind(request_id).execute(self.pool()).await?;
        Ok(result.rows_affected() == 1)
    }

    #[instrument(skip(self, clock))]
    pub async fn classify_expired_attempt(
        &self,
        attempt_id: &str,
        classification: &str,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        if !matches!(classification, "lost" | "needs_operator") {
            return Ok(false);
        }
        let now = stamp(clock);
        let result = sqlx::query("UPDATE execution_attempts SET state = ?, updated_at = ? WHERE id = ? AND state IN ('leased','preparing','running','waiting_decision') AND lease_expires_at < ?")
            .bind(classification).bind(&now).bind(attempt_id).bind(&now).execute(self.pool()).await?;
        Ok(result.rows_affected() == 1)
    }

    #[instrument(skip(self, artifact, clock))]
    pub async fn record_execution_artifact(
        &self,
        runner_id: &str,
        attempt_id: &str,
        fence: i64,
        artifact: NewArtifact<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let now = stamp(clock);
        let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM execution_attempts WHERE id = ? AND runner_id = ? AND fencing_token = ? AND state NOT IN ('succeeded','failed','cancelled') AND lease_expires_at >= ?)")
            .bind(attempt_id).bind(runner_id).bind(fence).bind(stamp(clock)).fetch_one(self.pool()).await?;
        if !valid {
            return Ok(false);
        }
        sqlx::query("INSERT INTO execution_artifacts (id, attempt_id, artifact_id, kind, name, media_type, size_bytes, sha256, content_disposition, content_reference, metadata, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(attempt_id, artifact_id) DO NOTHING")
            .bind(artifact.id).bind(attempt_id).bind(artifact.artifact_id).bind(artifact.kind).bind(artifact.name).bind(artifact.media_type).bind(artifact.size_bytes).bind(artifact.sha256).bind(artifact.content_disposition).bind(artifact.content_reference).bind(artifact.metadata).bind(now).execute(self.pool()).await?;
        Ok(true)
    }

    #[instrument(skip(self, decision, clock))]
    pub async fn create_execution_decision(
        &self,
        runner_id: &str,
        attempt_id: &str,
        fence: i64,
        decision: NewDecision<'_>,
        clock: &dyn ExecutionClock,
    ) -> Result<bool, sqlx::Error> {
        let now = stamp(clock);
        let valid: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM execution_attempts WHERE id = ? AND runner_id = ? AND fencing_token = ? AND state IN ('running','waiting_decision') AND lease_expires_at >= ?)")
            .bind(attempt_id).bind(runner_id).bind(fence).bind(stamp(clock)).fetch_one(self.pool()).await?;
        if !valid {
            return Ok(false);
        }
        sqlx::query("INSERT INTO execution_decisions (id, attempt_id, decision_id, kind, prompt, options, metadata, expires_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(attempt_id, decision_id) DO NOTHING")
        .bind(decision.id).bind(attempt_id).bind(decision.decision_id).bind(decision.kind).bind(decision.prompt).bind(decision.options).bind(decision.metadata).bind(decision.expires_at.map(|v| v.to_rfc3339())).bind(&now).bind(&now).execute(self.pool()).await?;
        Ok(true)
    }
}
