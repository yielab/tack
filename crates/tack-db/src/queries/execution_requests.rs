use tack_protocol::execution::ExecutionState;

use crate::repo::execution::state;
use crate::{Repository, StoreError};

/// Whether an execution request with this id exists.
pub async fn execution_request_exists(
    repo: &Repository,
    request_id: &str,
) -> Result<bool, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM execution_requests WHERE id = ?)")
            .bind(request_id)
            .fetch_one(repo.pool())
            .await?,
    )
}

/// The state an execution request is in, or `None` when no request has this id.
pub async fn request_state(
    repo: &Repository,
    request_id: &str,
) -> Result<Option<ExecutionState>, StoreError> {
    let text: Option<String> =
        sqlx::query_scalar("SELECT state FROM execution_requests WHERE id = ?")
            .bind(request_id)
            .fetch_optional(repo.pool())
            .await?;
    text.map(|text| state::parse(&text)).transpose()
}

/// The stored snapshot of the request an operator already created under this idempotency key,
/// or `None` when the key is new.
pub async fn snapshot_of_idempotency_key(
    repo: &Repository,
    scope: &str,
    key: &str,
) -> Result<Option<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT request_snapshot FROM execution_requests WHERE idempotency_scope=? AND idempotency_key=?",
    )
    .bind(scope)
    .bind(key)
    .fetch_optional(repo.pool())
    .await?)
}

/// The snapshot a request was stored with, or `None` when no request has this id.
pub async fn request_snapshot(
    repo: &Repository,
    request_id: &str,
) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT request_snapshot FROM execution_requests WHERE id = ?")
            .bind(request_id)
            .fetch_optional(repo.pool())
            .await?,
    )
}

/// The tasks of the project that have a request still occupying a run: queued, leased,
/// preparing, running or waiting for a decision.
pub async fn items_with_an_open_request(
    repo: &Repository,
    project_id: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT r.item_id FROM execution_requests r JOIN items i ON i.id = r.item_id
         WHERE i.project_id = ?
           AND r.state IN ('queued','leased','preparing','running','waiting_decision')",
    )
    .bind(project_id)
    .fetch_all(repo.pool())
    .await?)
}

/// A request a failed verification queued after another.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FollowUp {
    pub id: String,
    pub metadata: String,
    pub request_snapshot: String,
}

/// The requests that rework `request_id`, oldest first.
pub async fn follow_ups_of_request(
    repo: &Repository,
    request_id: &str,
) -> Result<Vec<FollowUp>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT id, metadata, request_snapshot FROM execution_requests \
         WHERE json_extract(metadata, '$.rework_of') = ? ORDER BY created_at, rowid",
    )
    .bind(request_id)
    .fetch_all(repo.pool())
    .await?)
}

/// The agent profile a request ran, `None` when it names none or the request is gone.
pub async fn agent_profile_of_request(
    repo: &Repository,
    request_id: &str,
) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT agent_profile_id FROM execution_requests WHERE id = ?")
            .bind(request_id)
            .fetch_optional(repo.pool())
            .await?
            .flatten(),
    )
}
