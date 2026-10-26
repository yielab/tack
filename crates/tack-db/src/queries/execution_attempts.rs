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

/// The newest succeeded attempt of the task that pushed a branch, `None` when it has none.
pub async fn newest_pushed_attempt_of_item(
    repo: &Repository,
    item_id: &str,
) -> Result<Option<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT a.id FROM execution_attempts a
         JOIN execution_requests r ON r.id = a.request_id
         WHERE r.item_id = ? AND a.state = 'succeeded'
           AND json_extract(a.actual_execution, '$.git.pushed') = 1
           AND json_extract(a.actual_execution, '$.git.head_commit') IS NOT NULL
         ORDER BY a.created_at DESC, a.rowid DESC LIMIT 1",
    )
    .bind(item_id)
    .fetch_optional(repo.pool())
    .await?)
}
