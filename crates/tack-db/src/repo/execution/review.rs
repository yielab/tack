use crate::{Repository, StoreError};

impl Repository {
    /// Records a person's verdict on one attempt, identified by its request id and 1-based attempt
    /// number, and returns the attempt's id. `reviewed_by` is the principal's label. When the attempt
    /// has a Merge-Readiness Pack, the review names the newest one. One verdict per attempt: no such
    /// attempt is `NotFound`, a second verdict is `Conflict("already_reviewed")`. Finding the attempt
    /// and writing the verdict are one transaction.
    pub async fn record_attempt_review(
        &self,
        request_id: &str,
        attempt_number: i64,
        verdict: &str,
        reason: Option<&str>,
        reviewed_by: &str,
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
        let attempt_id = attempt_id.ok_or(StoreError::not_found("attempt"))?;
        insert_review(
            &mut tx,
            &attempt_id,
            verdict,
            reason,
            reviewed_by,
            reviewed_at,
        )
        .await?;
        tx.commit().await?;
        Ok(attempt_id)
    }

    /// Records a person's verdict on an attempt known by its id, for a decision taken outside the
    /// review screen (merging over a rejection). Same single verdict per attempt:
    /// `Conflict("already_reviewed")` when one stands, which the caller leaves as it is.
    pub async fn record_review_of_attempt(
        &self,
        attempt_id: &str,
        verdict: &str,
        reason: Option<&str>,
        reviewed_by: &str,
        reviewed_at: &str,
    ) -> Result<(), StoreError> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        insert_review(
            &mut tx,
            attempt_id,
            verdict,
            reason,
            reviewed_by,
            reviewed_at,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

/// The one insert behind both ways to record a verdict; the review names the attempt's newest pack.
async fn insert_review(
    tx: &mut sqlx::SqliteConnection,
    attempt_id: &str,
    verdict: &str,
    reason: Option<&str>,
    reviewed_by: &str,
    reviewed_at: &str,
) -> Result<(), StoreError> {
    let inserted = sqlx::query(
        "INSERT OR IGNORE INTO attempt_reviews (attempt_id, verdict, reason, reviewed_by, reviewed_at, artifact_id) \
         VALUES (?, ?, ?, ?, ?, \
           (SELECT artifact_id FROM execution_artifacts \
             WHERE attempt_id = ? AND media_type = 'application/vnd.tack.mrp+json' \
             ORDER BY created_at DESC, rowid DESC LIMIT 1))",
    )
    .bind(attempt_id)
    .bind(verdict)
    .bind(reason)
    .bind(reviewed_by)
    .bind(reviewed_at)
    .bind(attempt_id)
    .execute(tx)
    .await?;
    if inserted.rows_affected() == 0 {
        return Err(StoreError::Conflict("already_reviewed".into()));
    }
    Ok(())
}
