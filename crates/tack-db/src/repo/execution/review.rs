use crate::{Repository, StoreError};

impl Repository {
    /// Records the operator's verdict on one attempt, identified by its request
    /// id and 1-based attempt number, and returns the attempt's id. One verdict
    /// per attempt: no such attempt is `NotFound`, a second verdict is
    /// `Conflict("already_reviewed")`. Finding the attempt and writing the
    /// verdict are one transaction.
    pub async fn record_attempt_review(
        &self,
        request_id: &str,
        attempt_number: i64,
        verdict: &str,
        note: Option<&str>,
        reviewed_at: &str,
    ) -> Result<String, StoreError> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let attempt_id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM execution_attempts WHERE request_id = ? AND attempt_number = ?",
        )
        .bind(request_id)
        .bind(attempt_number)
        .fetch_optional(&mut *tx)
        .await?;
        let attempt_id = attempt_id.ok_or(StoreError::NotFound)?;
        let inserted = sqlx::query(
            "INSERT OR IGNORE INTO attempt_reviews (attempt_id, verdict, note, reviewed_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&attempt_id)
        .bind(verdict)
        .bind(note)
        .bind(reviewed_at)
        .execute(&mut *tx)
        .await?;
        if inserted.rows_affected() == 0 {
            return Err(StoreError::Conflict("already_reviewed".into()));
        }
        tx.commit().await?;
        Ok(attempt_id)
    }
}
