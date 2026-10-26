use tack_protocol::execution::ExecutionState;

use crate::repo::execution::{ExecutionArtifactRow, state};
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

/// The bytes the manifest rows of one attempt declare, together.
pub async fn declared_bytes_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<i64, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(SUM(size_bytes),0) FROM execution_artifacts WHERE attempt_id=?",
    )
    .bind(attempt_id)
    .fetch_one(repo.pool())
    .await?)
}

/// How many manifest rows an attempt has.
pub async fn artifact_count_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<i64, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM execution_artifacts WHERE attempt_id = ?")
            .bind(attempt_id)
            .fetch_one(repo.pool())
            .await?,
    )
}

/// The `artifact_id` of every manifest row of an attempt, in order.
pub async fn artifact_ids_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT artifact_id FROM execution_artifacts WHERE attempt_id = ? ORDER BY artifact_id",
    )
    .bind(attempt_id)
    .fetch_all(repo.pool())
    .await?)
}

/// Where the verified content of an artifact is stored, or `None` while only its manifest is
/// recorded. An artifact the attempt never declared is `StoreError::NotFound`.
pub async fn content_reference(
    repo: &Repository,
    attempt_id: &str,
    artifact_id: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar(
        "SELECT content_reference FROM execution_artifacts WHERE attempt_id=? AND artifact_id=?",
    )
    .bind(attempt_id)
    .bind(artifact_id)
    .fetch_optional(repo.pool())
    .await?
    .ok_or(StoreError::NotFound)
}

/// What an upload of one artifact's content is checked against: the manifest row and the
/// lease of the attempt that owns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadFacts {
    pub sha256: String,
    pub size_bytes: i64,
    pub media_type: Option<String>,
    pub content_reference: Option<String>,
    pub attempt_state: ExecutionState,
    pub lease_expires_at: String,
}

/// The upload facts of an artifact, or `None` when the attempt, the artifact, the runner or the
/// fence does not match: every mismatch looks the same, so a caller cannot probe which was wrong.
pub async fn upload_facts(
    repo: &Repository,
    attempt_id: &str,
    artifact_id: &str,
    runner_id: &str,
    fencing_token: i64,
) -> Result<Option<UploadFacts>, StoreError> {
    type Row = (String, i64, Option<String>, Option<String>, String, String);
    let row: Option<Row> = sqlx::query_as(
        "SELECT ea.sha256, ea.size_bytes, ea.media_type, ea.content_reference, \
         eat.state, eat.lease_expires_at \
         FROM execution_artifacts ea \
         JOIN execution_attempts eat ON eat.id = ea.attempt_id \
         WHERE ea.attempt_id = ? AND ea.artifact_id = ? AND eat.runner_id = ? AND eat.fencing_token = ?",
    )
    .bind(attempt_id)
    .bind(artifact_id)
    .bind(runner_id)
    .bind(fencing_token)
    .fetch_optional(repo.pool())
    .await?;
    row.map(
        |(sha256, size_bytes, media_type, content_reference, attempt_state, lease_expires_at)| {
            Ok(UploadFacts {
                sha256,
                size_bytes,
                media_type,
                content_reference,
                attempt_state: state::parse(&attempt_state)?,
                lease_expires_at,
            })
        },
    )
    .transpose()
}

/// The stored content reference of the attempt's newest artifact of `media_type` that has one,
/// or `None` when it has none.
pub async fn latest_content_reference(
    repo: &Repository,
    attempt_id: &str,
    media_type: &str,
) -> Result<Option<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT content_reference FROM execution_artifacts \
         WHERE attempt_id = ? AND media_type = ? AND content_reference IS NOT NULL \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(attempt_id)
    .bind(media_type)
    .fetch_optional(repo.pool())
    .await?)
}

/// Whether an artifact manifest row with this row id exists.
pub async fn artifact_row_exists(repo: &Repository, row_id: &str) -> Result<bool, StoreError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM execution_artifacts WHERE id=?")
        .bind(row_id)
        .fetch_one(repo.pool())
        .await?;
    Ok(count > 0)
}
