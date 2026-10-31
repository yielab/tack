use crate::{Repository, StoreError};

impl Repository {
    /// Notes that prill took the `kind` record of an attempt, once: a second call changes
    /// nothing.
    pub async fn mark_prill_recorded(
        &self,
        attempt_id: &str,
        kind: &str,
        recorded_at: &str,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT OR IGNORE INTO prill_records (attempt_id, kind, recorded_at) VALUES (?, ?, ?)",
        )
        .bind(attempt_id)
        .bind(kind)
        .bind(recorded_at)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
