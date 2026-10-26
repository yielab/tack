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

/// When the attempt's runner last sent a heartbeat for it, or `None` when none was recorded.
/// An id no attempt has is `StoreError::NotFound`.
pub async fn last_heartbeat_at(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar("SELECT last_heartbeat_at FROM execution_attempts WHERE id = ?")
        .bind(attempt_id)
        .fetch_optional(repo.pool())
        .await?
        .ok_or(StoreError::NotFound)
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

/// The `actual_execution` JSON an attempt reported, or `None` when the attempt does not exist
/// or reported none.
pub async fn actual_execution_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<String>, StoreError> {
    let raw: Option<Option<String>> =
        sqlx::query_scalar("SELECT actual_execution FROM execution_attempts WHERE id = ?")
            .bind(attempt_id)
            .fetch_optional(repo.pool())
            .await?;
    Ok(raw.flatten())
}

/// The item an attempt ran for and the attempt's 1-based number within its request.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AttemptItem {
    pub item_id: String,
    pub attempt_number: i64,
}

/// The item and number of an attempt, or `None` when no attempt has this id.
pub async fn item_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<AttemptItem>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT r.item_id, a.attempt_number FROM execution_attempts a \
         JOIN execution_requests r ON r.id = a.request_id WHERE a.id = ?",
    )
    .bind(attempt_id)
    .fetch_optional(repo.pool())
    .await?)
}
