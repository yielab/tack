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

/// The item a request was made for and the status-map policy its snapshot
/// names, if any.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RequestItemPolicy {
    pub item_id: String,
    pub status_map_policy_id: Option<String>,
}

/// The item and status-map policy of one request, or `None` when no request
/// has this id.
pub async fn item_and_status_map_policy(
    repo: &Repository,
    request_id: &str,
) -> Result<Option<RequestItemPolicy>, StoreError> {
    Ok(sqlx::query_as::<_, RequestItemPolicy>(
        "SELECT item_id, json_extract(request_snapshot, '$.status_map_policy_id') AS status_map_policy_id \
         FROM execution_requests WHERE id = ?",
    )
    .bind(request_id)
    .fetch_optional(repo.pool())
    .await?)
}
