//! The small key/value settings the install keeps in `app_meta`.

use crate::{Repository, StoreError};

impl Repository {
    /// Stores `value` under `key`, replacing what was there.
    pub async fn set_app_meta(&self, key: &str, value: &str) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO app_meta (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
