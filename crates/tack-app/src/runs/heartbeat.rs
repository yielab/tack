use chrono::{DateTime, Duration, Utc};
use tack_db::Repository;
use tack_db::repo::execution::{HeartbeatBatchResponse, HeartbeatBatchResult, HeartbeatLease};
use tack_protocol::Clock;
use tack_protocol::execution::ExecutionState;

use crate::AppError;

/// One lease a runner says it holds, as it reports it in a heartbeat.
#[derive(Debug, Clone)]
pub struct ReportedLease {
    pub attempt_id: String,
    pub fencing_token: i64,
    pub state: ExecutionState,
    pub journal_state: String,
    pub last_event_checkpoint: Option<String>,
}

/// A runner's heartbeat as it sent it.
pub struct Beat<'a> {
    pub runner_id: &'a str,
    pub heartbeat_id: &'a str,
    pub sent_at: DateTime<Utc>,
    pub available_capacity: i64,
    pub leases: &'a [ReportedLease],
    /// How far each reported lease is extended.
    pub lease_duration: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heartbeat {
    /// The beat was recorded and each reported lease extended.
    Accepted(HeartbeatBatchResponse),
    /// The same beat arrived again; this is the answer given the first time.
    Replayed(HeartbeatBatchResponse),
    /// `heartbeat_id` was already used with different content.
    IdempotencyConflict,
    /// A reported lease is no longer the runner's; names its attempt.
    StaleLease(String),
}

/// Records a runner's heartbeat: its free capacity and the leases it still holds, each extended
/// by `lease_duration`. Idempotent on `heartbeat_id` with the same content.
pub async fn heartbeat(
    repo: &Repository,
    clock: &dyn Clock,
    beat: Beat<'_>,
) -> Result<Heartbeat, AppError> {
    let reported: Vec<HeartbeatLease<'_>> = beat
        .leases
        .iter()
        .map(|lease| HeartbeatLease {
            attempt_id: &lease.attempt_id,
            fencing_token: lease.fencing_token,
            state: lease.state,
            journal_state: &lease.journal_state,
            last_event_checkpoint: lease.last_event_checkpoint.as_deref(),
        })
        .collect();
    let result = repo
        .heartbeat_batch(
            beat.runner_id,
            beat.heartbeat_id,
            beat.sent_at,
            beat.available_capacity,
            &reported,
            beat.lease_duration,
            clock,
        )
        .await?;
    Ok(match result {
        HeartbeatBatchResult::Accepted(response) => Heartbeat::Accepted(response),
        HeartbeatBatchResult::Replayed(response) => Heartbeat::Replayed(response),
        HeartbeatBatchResult::Conflict => Heartbeat::IdempotencyConflict,
        HeartbeatBatchResult::StaleLease(attempt_id) => Heartbeat::StaleLease(attempt_id),
    })
}
