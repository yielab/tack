use tack_db::repo::execution::{
    RecoveryDisposition as StoredDisposition, RecoveryObservation as StoredObservation,
    RecoveryObservationInput, RecoveryObservationResult,
};
use tack_db::{Repository, StoreError};
use tack_protocol::Clock;
use tack_protocol::execution::{RecoveryDisposition, RecoveryObservation};

use crate::AppError;

/// What a restarted runner saw of an attempt it was running.
pub struct Observation<'a> {
    pub runner_id: &'a str,
    pub attempt_id: &'a str,
    pub fencing_token: i64,
    pub recovery_key: &'a str,
    pub observation: RecoveryObservation,
    /// The runner's details, already encoded as JSON.
    pub details_json: &'a str,
}

/// What the board decided about the attempt after the observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovered {
    pub attempt_id: String,
    pub recovery_key: String,
    pub disposition: RecoveryDisposition,
    /// `true` when this `recovery_key` had already been recorded and this is the first answer
    /// again.
    pub replayed: bool,
    /// When the first recording committed, as the RFC 3339 text the store keeps.
    pub committed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryObserved {
    Recorded(Recovered),
    /// `recovery_key` was already used with different content.
    IdempotencyConflict,
    /// The fence is no longer the attempt's current one.
    Stale,
}

/// Records what a runner observed after a restart and settles the attempt: a process proven
/// stopped before it spawned the agent is requeued, one that may still run needs an operator,
/// and one already finished stays finished. Idempotent on `recovery_key`.
pub async fn observe_recovery(
    repo: &Repository,
    clock: &dyn Clock,
    observation: Observation<'_>,
) -> Result<RecoveryObserved, AppError> {
    let stored = match observation.observation {
        RecoveryObservation::ProcessStopped => StoredObservation::ProcessStopped,
        RecoveryObservation::ProcessRunning => StoredObservation::ProcessRunning,
        RecoveryObservation::Ambiguous => StoredObservation::Ambiguous,
    };
    let result = repo
        .recover_attempt(
            RecoveryObservationInput {
                runner_id: observation.runner_id,
                attempt_id: observation.attempt_id,
                fencing_token: observation.fencing_token,
                recovery_key: observation.recovery_key,
                observation: stored,
                details: observation.details_json,
            },
            clock,
        )
        .await
        .map_err(StoreError::from)?;
    let (response, replayed) = match result {
        RecoveryObservationResult::Applied(response) => (response, false),
        RecoveryObservationResult::Replayed(response) => (response, true),
        RecoveryObservationResult::Conflict => return Ok(RecoveryObserved::IdempotencyConflict),
        RecoveryObservationResult::Stale => return Ok(RecoveryObserved::Stale),
    };
    let disposition = match response.disposition {
        StoredDisposition::SafePreSpawnRequeue => RecoveryDisposition::SafePreSpawnRequeue,
        StoredDisposition::NeedsOperator => RecoveryDisposition::NeedsOperator,
        StoredDisposition::AlreadyTerminal => RecoveryDisposition::AlreadyTerminal,
    };
    Ok(RecoveryObserved::Recorded(Recovered {
        attempt_id: response.attempt_id,
        recovery_key: response.recovery_key,
        disposition,
        replayed,
        committed_at: response.committed_at,
    }))
}
