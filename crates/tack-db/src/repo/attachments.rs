use crate::{StoreError, decode};
use chrono::Utc;
use sqlx::Row;
use tracing::instrument;
use uuid::Uuid;

use tack_core::models::Attachment;

use crate::Repository;

/// Repository for attachment operations
pub struct AttachmentRepo<'a> {
    repo: &'a Repository,
}

impl<'a> AttachmentRepo<'a> {
    pub fn new(repo: &'a Repository) -> Self {
        Self { repo }
    }

    /// Create a new attachment record
    #[instrument(skip(self))]
    pub async fn create_attachment(
        &self,
        item_id: Uuid,
        filename: String,
        mime_type: String,
        storage_path: String,
        size_bytes: u64,
    ) -> Result<Attachment, StoreError> {
        let id = Uuid::new_v4();
        let uploaded_at = Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO attachments (id, item_id, filename, mime_type, storage_path, size_bytes, uploaded_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(id.to_string())
        .bind(item_id.to_string())
        .bind(&filename)
        .bind(&mime_type)
        .bind(&storage_path)
        .bind(size_bytes as i64)
        .bind(&uploaded_at)
        .execute(self.repo.pool())
        .await?;

        Ok(Attachment {
            id,
            item_id,
            filename,
            mime_type,
            storage_path,
            size_bytes,
            uploaded_at: Utc::now(),
        })
    }

    /// Get attachment by ID
    #[instrument(skip(self))]
    pub async fn get_attachment(&self, id: Uuid) -> Result<Option<Attachment>, StoreError> {
        let row = sqlx::query(
            "SELECT id, item_id, filename, mime_type, storage_path, size_bytes, uploaded_at
             FROM attachments WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(self.repo.pool())
        .await?;

        row.as_ref().map(attachment_of).transpose()
    }

    /// List attachments for an item
    #[instrument(skip(self))]
    pub async fn list_attachments(&self, item_id: Uuid) -> Result<Vec<Attachment>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, item_id, filename, mime_type, storage_path, size_bytes, uploaded_at
             FROM attachments WHERE item_id = ? ORDER BY uploaded_at DESC",
        )
        .bind(item_id.to_string())
        .fetch_all(self.repo.pool())
        .await?;

        rows.iter().map(attachment_of).collect()
    }

    /// Delete attachment
    #[instrument(skip(self))]
    pub async fn delete_attachment(&self, id: Uuid) -> Result<bool, StoreError> {
        let result = sqlx::query("DELETE FROM attachments WHERE id = ?")
            .bind(id.to_string())
            .execute(self.repo.pool())
            .await?;

        Ok(result.rows_affected() > 0)
    }
}

fn attachment_of(row: &sqlx::sqlite::SqliteRow) -> Result<Attachment, StoreError> {
    const TABLE: &str = "attachments";
    Ok(Attachment {
        id: decode::uuid(TABLE, "id", &row.get::<String, _>("id"))?,
        item_id: decode::uuid(TABLE, "item_id", &row.get::<String, _>("item_id"))?,
        filename: row.get("filename"),
        mime_type: row.get("mime_type"),
        storage_path: row.get("storage_path"),
        size_bytes: row.get::<i64, _>("size_bytes") as u64,
        uploaded_at: decode::instant(TABLE, "uploaded_at", &row.get::<String, _>("uploaded_at"))?,
    })
}
