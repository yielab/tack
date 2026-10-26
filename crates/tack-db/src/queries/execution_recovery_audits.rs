use crate::{Repository, StoreError};

/// How many recovery audits of `classification` an attempt has.
pub async fn audits_of_attempt(
    repo: &Repository,
    attempt_id: &str,
    classification: &str,
) -> Result<i64, StoreError> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM execution_recovery_audits WHERE attempt_id = ? AND classification = ?",
    )
    .bind(attempt_id)
    .bind(classification)
    .fetch_one(repo.pool())
    .await?)
}
