use chrono::{DateTime, Utc};
use tack_db::repo::execution::{OperatorRequeueResult, state};
use tack_db::{Repository, queries};
use tack_protocol::execution::ExecutionState;

use super::start_run::fingerprint;
use crate::AppError;

/// A run that went back to `queued`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Requeued {
    /// The same recovery key and reason had already requeued it.
    pub replayed: bool,
}

/// Puts a `needs_operator` run back in the queue after an operator confirmed the recovery. The
/// confirmation is audited under `recovery_key`: the same key with the same reason replays, with
/// another reason it is `AppError::Conflict`. A run that is not an authoritatively recovered
/// `needs_operator` one is `AppError::InvalidTransition`, naming the state of its newest attempt
/// (`unknown` when it has none).
pub async fn requeue(
    repo: &Repository,
    now: DateTime<Utc>,
    request_id: &str,
    recovery_key: &str,
    reason: &str,
    actor: &str,
) -> Result<Requeued, AppError> {
    let result = repo
        .operator_requeue_needs_operator(request_id, recovery_key, actor, &fingerprint(reason), now)
        .await?;
    match result {
        OperatorRequeueResult::Requeued => Ok(Requeued { replayed: false }),
        OperatorRequeueResult::Replayed => Ok(Requeued { replayed: true }),
        OperatorRequeueResult::Conflict => Err(AppError::Conflict(
            "the recovery key was used with a different confirmation".to_owned(),
        )),
        OperatorRequeueResult::InvalidTransition | OperatorRequeueResult::NotFound => {
            let from = queries::execution_attempts::latest_state_of_request(repo, request_id)
                .await
                .ok()
                .flatten()
                .map_or_else(
                    || "unknown".to_owned(),
                    |state| state::text(state).to_owned(),
                );
            Err(AppError::InvalidTransition {
                from,
                to: state::text(ExecutionState::Queued).to_owned(),
            })
        }
    }
}
