use crate::repo::execution::ExecutionArtifactRow;
use crate::{Repository, StoreError};

/// Every artifact manifested for one attempt, identified by its request id and
/// 1-based attempt number, oldest first. `None` means no attempt with that
/// number exists for the request, which differs from `Some(vec![])`: an attempt
/// that has manifested no artifacts yet.
pub async fn artifacts_of_attempt(
    repo: &Repository,
    request_id: &str,
    attempt_number: i64,
) -> Result<Option<Vec<ExecutionArtifactRow>>, StoreError> {
    let attempt_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM execution_attempts WHERE request_id = ? AND attempt_number = ?",
    )
    .bind(request_id)
    .bind(attempt_number)
    .fetch_optional(repo.pool())
    .await?;
    let Some(attempt_id) = attempt_id else {
        return Ok(None);
    };
    let rows = sqlx::query_as::<_, ExecutionArtifactRow>(
        "SELECT id, attempt_id, artifact_id, kind, name, media_type, size_bytes, sha256, \
         content_disposition, content_reference, metadata, created_at \
         FROM execution_artifacts WHERE attempt_id = ? ORDER BY created_at",
    )
    .bind(&attempt_id)
    .fetch_all(repo.pool())
    .await?;
    Ok(Some(rows))
}
