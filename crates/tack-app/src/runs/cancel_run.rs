use chrono::{DateTime, Utc};
use tack_db::Repository;
use tack_db::repo::execution::CancellationOutcome;

use crate::AppError;

/// What a cancellation did to a run that was not yet over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cancellation {
    /// Nobody held the run (its attempt waited on a person, or its lease had expired): it ended
    /// `cancelled` at once.
    Ended,
    /// A runner holds the run: the request is recorded and that runner reports the outcome.
    Requested,
}

/// Asks for a run to be cancelled. A run nobody holds ends at once; any other is marked and its
/// runner stops it. An unknown request is `AppError::NotFound`, and one that already reached a
/// terminal state is `AppError::Conflict`.
pub async fn cancel_run(
    repo: &Repository,
    now: DateTime<Utc>,
    request_id: &str,
) -> Result<Cancellation, AppError> {
    match repo.cancel_execution_request(request_id, now).await? {
        CancellationOutcome::Ended => Ok(Cancellation::Ended),
        CancellationOutcome::Requested => Ok(Cancellation::Requested),
        CancellationOutcome::AlreadyTerminal => Err(AppError::Conflict(
            "the execution already reached a terminal state".to_owned(),
        )),
        CancellationOutcome::NotFound => Err(AppError::NotFound),
    }
}
