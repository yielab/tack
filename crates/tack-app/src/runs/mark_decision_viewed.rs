use chrono::{DateTime, Utc};
use tack_db::Repository;

use crate::AppError;

/// Records the first time an operator opened a decision and returns that timestamp. Idempotent:
/// a later call returns the first one. A decision that does not exist for the attempt is
/// `AppError::NotFound`.
pub async fn mark_decision_viewed(
    repo: &Repository,
    now: DateTime<Utc>,
    attempt_id: &str,
    decision_id: &str,
) -> Result<String, AppError> {
    repo.mark_execution_decision_viewed(attempt_id, decision_id, now)
        .await?
        .ok_or(AppError::NotFound)
}
