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

/// The request an attempt belongs to, as the follow-up of a rejected run needs it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RequestOfAttempt {
    pub id: String,
    pub item_id: String,
    pub idempotency_scope: String,
    pub request_snapshot: String,
}

/// The request an attempt belongs to. An attempt that does not exist is `StoreError::NotFound`.
pub async fn request_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<RequestOfAttempt, StoreError> {
    sqlx::query_as(
        "SELECT r.id, r.item_id, r.idempotency_scope, r.request_snapshot \
         FROM execution_attempts a JOIN execution_requests r ON r.id = a.request_id \
         WHERE a.id = ?",
    )
    .bind(attempt_id)
    .fetch_optional(repo.pool())
    .await?
    .ok_or(StoreError::NotFound)
}

/// A request in the chain of follow-ups one run started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainLink {
    pub id: String,
    pub state: ExecutionState,
}

/// The request `root` and every follow-up made from it, oldest first.
pub async fn rework_chain(repo: &Repository, root: &str) -> Result<Vec<ChainLink>, StoreError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, state FROM execution_requests \
         WHERE id = ?1 OR json_extract(metadata, '$.rework_of') = ?1 ORDER BY created_at, rowid",
    )
    .bind(root)
    .fetch_all(repo.pool())
    .await?;
    rows.into_iter()
        .map(|(id, state_text)| {
            Ok(ChainLink {
                id,
                state: state::parse(&state_text)?,
            })
        })
        .collect()
}

/// What the status map needs to know about a finished attempt.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StatusMapFacts {
    pub item_id: String,
    /// The `status_map_policy_id` the request was stored with, when it is text.
    pub policy_id: Option<String>,
    /// The attempt's terminal reason as stored JSON text.
    pub terminal_reason: Option<String>,
}

/// The item, the status map policy and the terminal reason of an attempt, or `None` when no
/// attempt has this id.
pub async fn status_map_facts_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<StatusMapFacts>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT r.item_id AS item_id, \
                CASE WHEN json_type(r.request_snapshot, '$.status_map_policy_id') = 'text' \
                     THEN json_extract(r.request_snapshot, '$.status_map_policy_id') END AS policy_id, \
                a.terminal_reason AS terminal_reason \
         FROM execution_attempts a JOIN execution_requests r ON r.id = a.request_id \
         WHERE a.id = ?",
    )
    .bind(attempt_id)
    .fetch_optional(repo.pool())
    .await?)
}

/// How many follow-ups a rejected run of this item may start, as its project allows, or `None`
/// when the item or the limit is not set.
pub async fn rework_cap_of_item(
    repo: &Repository,
    item_id: &str,
) -> Result<Option<i64>, StoreError> {
    let cap: Option<Option<i64>> = sqlx::query_scalar(
        "SELECT p.verify_loop_max_attempts FROM items i JOIN projects p ON p.id = i.project_id \
         WHERE i.id = ?",
    )
    .bind(item_id)
    .fetch_optional(repo.pool())
    .await?;
    Ok(cap.flatten())
}
