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
