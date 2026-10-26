use tack_protocol::execution::ExecutionState;

use crate::repo::execution::state;
use crate::{Repository, StoreError};

/// The media type of a Merge-Readiness Pack artifact.
const MRP_MEDIA_TYPE: &str = "application/vnd.tack.mrp+json";

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

/// The content reference of the newest Merge-Readiness Pack the attempt uploaded, or `None`
/// when it uploaded none.
pub async fn pack_content_reference(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<String>, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT content_reference FROM execution_artifacts \
         WHERE attempt_id = ? AND media_type = ? \
         AND content_reference IS NOT NULL ORDER BY created_at DESC LIMIT 1",
    )
    .bind(attempt_id)
    .bind(MRP_MEDIA_TYPE)
    .fetch_optional(repo.pool())
    .await?)
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
