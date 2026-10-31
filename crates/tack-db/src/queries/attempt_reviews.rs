use crate::{Repository, StoreError};

/// The operator's verdict on one attempt.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AttemptReviewRow {
    pub verdict: String,
    pub note: Option<String>,
    pub reviewed_at: String,
}

/// The verdict recorded for an attempt, `None` when it has not been reviewed.
pub async fn review_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<AttemptReviewRow>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT verdict, note, reviewed_at FROM attempt_reviews WHERE attempt_id = ?",
    )
    .bind(attempt_id)
    .fetch_optional(repo.pool())
    .await?)
}
