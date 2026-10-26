use crate::{Repository, StoreError};

/// The columns of an `execution_requests` row a caller reads back to check what
/// a route stored. `metadata` and `request_snapshot` stay raw JSON text.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ExecutionRequestRow {
    pub id: String,
    pub idempotency_scope: String,
    pub idempotency_key: String,
    pub state: String,
    pub selector_id: Option<String>,
    pub waiting_reason: Option<String>,
    pub metadata: String,
    pub request_snapshot: String,
    pub created_at: String,
}

/// The execution request with this id, or `None` when there is none.
pub async fn execution_request_row(
    repo: &Repository,
    request_id: &str,
) -> Result<Option<ExecutionRequestRow>, StoreError> {
    Ok(sqlx::query_as::<_, ExecutionRequestRow>(
        "SELECT id, idempotency_scope, idempotency_key, state, selector_id, waiting_reason, \
         metadata, request_snapshot, created_at FROM execution_requests WHERE id = ?",
    )
    .bind(request_id)
    .fetch_optional(repo.pool())
    .await?)
}

/// Every execution request raised against one item.
pub async fn execution_requests_of_item(
    repo: &Repository,
    item_id: &str,
) -> Result<Vec<ExecutionRequestRow>, StoreError> {
    Ok(sqlx::query_as::<_, ExecutionRequestRow>(
        "SELECT id, idempotency_scope, idempotency_key, state, selector_id, waiting_reason, \
         metadata, request_snapshot, created_at FROM execution_requests WHERE item_id = ?",
    )
    .bind(item_id)
    .fetch_all(repo.pool())
    .await?)
}

/// How many execution requests the install holds.
pub async fn execution_request_count(repo: &Repository) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_requests")
            .fetch_one(repo.pool())
            .await?,
    )
}
