use uuid::Uuid;

use crate::{Repository, StoreError};

impl Repository {
    /// The id of the one workspace this board runs. A first boot creates it; finding it and
    /// creating it are one transaction.
    pub async fn ensure_default_workspace(&self) -> Result<Uuid, StoreError> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let existing: Option<String> = sqlx::query_scalar("SELECT id FROM workspaces LIMIT 1")
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(id) = existing {
            return Uuid::parse_str(&id)
                .map_err(|error| sqlx::Error::Decode(Box::new(error)).into());
        }
        let id = Uuid::new_v4();
        let vocabulary = serde_json::to_string(&tack_core::vocabulary::default_vocabulary())
            .map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
        sqlx::query(
            "INSERT INTO workspaces (id, name, description, default_vocabulary) VALUES (?, 'Default Workspace', 'Auto-created workspace', ?)",
        )
        .bind(id.to_string())
        .bind(&vocabulary)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        tracing::info!(%id, "Created default workspace");
        Ok(id)
    }
}
