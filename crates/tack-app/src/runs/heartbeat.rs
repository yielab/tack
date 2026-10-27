use chrono::{DateTime, Duration, Utc};
use tack_db::Repository;
use tack_db::repo::execution::{HeartbeatBatchResponse, HeartbeatBatchResult, HeartbeatLease};
use tack_protocol::execution::{ActiveAttempt, AttemptId, RunnerId};

use crate::AppError;

/// A runner's heartbeat as it sent it.
pub struct Beat<'a> {
    pub runner_id: &'a RunnerId,
    pub heartbeat_id: &'a str,
    pub sent_at: DateTime<Utc>,
    pub available_capacity: i64,
    /// The leases the runner says it holds, as the protocol's message carries them.
    pub leases: &'a [ActiveAttempt],
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
    StaleLease(AttemptId),
    /// The runner is revoked or was never enrolled, so it holds nothing.
    RunnerNotActive,
}

/// Records a runner's heartbeat: its free capacity and the leases it still holds, each extended
/// by `lease_duration`. Idempotent on `heartbeat_id` with the same content.
pub async fn heartbeat(
    repo: &Repository,
    now: DateTime<Utc>,
    beat: Beat<'_>,
) -> Result<Heartbeat, AppError> {
    let reported: Vec<HeartbeatLease<'_>> = beat
        .leases
        .iter()
        .map(|lease| HeartbeatLease {
            attempt_id: &lease.attempt_id,
            fencing_token: lease.fencing_token,
            state: lease.state,
            journal_state: lease.journal_state,
            last_event_checkpoint: lease.last_event_checkpoint.as_ref(),
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
            now,
        )
        .await?;
    Ok(match result {
        HeartbeatBatchResult::Accepted(response) => Heartbeat::Accepted(response),
        HeartbeatBatchResult::Replayed(response) => Heartbeat::Replayed(response),
        HeartbeatBatchResult::Conflict => Heartbeat::IdempotencyConflict,
        HeartbeatBatchResult::StaleLease(attempt_id) => Heartbeat::StaleLease(attempt_id),
        HeartbeatBatchResult::RunnerNotActive => Heartbeat::RunnerNotActive,
    })
}
