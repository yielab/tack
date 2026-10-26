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

/// What the operator's detail of one execution request shows. `instructions` is the resolved
/// agent profile's instructions out of the stored snapshot.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RequestDetailRow {
    pub id: String,
    pub item_id: String,
    pub state: String,
    pub cancellation_requested_at: Option<String>,
    pub created_at: String,
    pub waiting_reason: Option<String>,
    pub waiting_since: Option<String>,
    pub selector_kind: String,
    pub selector_id: String,
    pub instructions: Option<String>,
}

/// The detail of one execution request, or `None` when no request has this id.
pub async fn request_detail(
    repo: &Repository,
    request_id: &str,
) -> Result<Option<RequestDetailRow>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT id, item_id, state, cancellation_requested_at, created_at, waiting_reason, waiting_since, selector_kind, selector_id, json_extract(request_snapshot, '$.resolved_agent_profile.instructions') AS instructions FROM execution_requests WHERE id = ?",
    )
    .bind(request_id)
    .fetch_optional(repo.pool())
    .await?)
}

/// The model a request asked for. The outer `None` means no request has this id; the inner one
/// that the request named no model.
pub async fn requested_model_of_request(
    repo: &Repository,
    request_id: &str,
) -> Result<Option<Option<String>>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT requested_model FROM execution_requests WHERE id = ?")
            .bind(request_id)
            .fetch_optional(repo.pool())
            .await?,
    )
}

/// What the limit of one request is read from: its stored snapshot, the harness that ran it
/// (the newest attempt's, else the one requested) and its item's project.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct RunLimitInputRow {
    pub id: String,
    pub budgets: Option<String>,
    pub source: Option<String>,
    pub profile: Option<String>,
    pub project_id: String,
    pub harness_kind: Option<String>,
}

/// The limit inputs of each listed request; an id no request has is left out.
pub async fn run_limit_inputs(
    repo: &Repository,
    request_ids: &[String],
) -> Result<Vec<RunLimitInputRow>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT r.id,
                json_extract(r.request_snapshot, '$.budgets') AS budgets,
                json_extract(r.request_snapshot, '$.metadata.limit_source') AS source,
                json_extract(r.request_snapshot, '$.resolved_agent_profile.name') AS profile,
                i.project_id AS project_id,
                COALESCE((SELECT json_extract(a.actual_execution, '$.harness_kind')
                            FROM execution_attempts a
                           WHERE a.request_id = r.id AND a.actual_execution IS NOT NULL
                           ORDER BY a.attempt_number DESC LIMIT 1),
                         r.requested_harness_kind) AS harness_kind
           FROM execution_requests r
           JOIN items i ON i.id = r.item_id
          WHERE r.id IN (SELECT value FROM json_each(?))",
    )
    .bind(serde_json::json!(request_ids).to_string())
    .fetch_all(repo.pool())
    .await?)
}
