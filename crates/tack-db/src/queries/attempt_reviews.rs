use crate::{Repository, StoreError};

/// A person's verdict on one attempt: `verdict` is `accepted` or `rejected`; `reviewed_by` is the
/// principal's label, `None` for a review recorded before the reviewer was kept.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AttemptReviewRow {
    pub verdict: String,
    pub reason: Option<String>,
    pub reviewed_by: Option<String>,
    pub reviewed_at: String,
}

/// The verdict recorded for an attempt, `None` when it has not been reviewed.
pub async fn review_of_attempt(
    repo: &Repository,
    attempt_id: &str,
) -> Result<Option<AttemptReviewRow>, StoreError> {
    Ok(sqlx::query_as(
        "SELECT verdict, reason, reviewed_by, reviewed_at FROM attempt_reviews WHERE attempt_id = ?",
    )
    .bind(attempt_id)
    .fetch_optional(repo.pool())
    .await?)
}
