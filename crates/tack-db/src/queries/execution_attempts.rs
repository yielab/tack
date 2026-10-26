use tack_protocol::execution::ExecutionState;

use crate::repo::execution::state;
use crate::{Repository, StoreError};

/// The state an attempt is in, or `None` when no attempt has this id.
pub async fn attempt_state(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<ExecutionState>, StoreError> {
    let text: Option<String> =
        sqlx::query_scalar("SELECT state FROM execution_attempts WHERE id = ?")
            .bind(attempt_id)
            .fetch_optional(repo.pool())
            .await?;
    text.map(|text| state::parse(&text)).transpose()
}

/// The state of the newest attempt of a request, or `None` when the request has no attempt.
pub async fn latest_state_of_request(
    repo: &Repository,
    request_id: &str,
) -> Result<Option<ExecutionState>, StoreError> {
    let text: Option<String> = sqlx::query_scalar(
        "SELECT state FROM execution_attempts WHERE request_id = ? ORDER BY attempt_number DESC LIMIT 1",
    )
    .bind(request_id)
    .fetch_optional(repo.pool())
    .await?;
    text.map(|text| state::parse(&text)).transpose()
}

/// How many `operator_requeue` audits an attempt has.
pub async fn operator_requeues_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<i64, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_recovery_audits WHERE attempt_id = ? AND classification = 'operator_requeue'",
    )
    .bind(attempt_id)
    .fetch_one(repo.pool())
    .await?)
}
