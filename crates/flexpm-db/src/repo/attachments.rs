use chrono::Utc;
use sqlx::{Row, SqlitePool};
use tracing::instrument;
use uuid::Uuid;

use flexpm_core::models::Attachment;

/// Repository for attachment operations
pub struct AttachmentRepo<'a> {
    pool: &'a SqlitePool,
}

impl<'a> AttachmentRepo<'a> {
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self { pool }
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
    ) -> Result<Attachment, sqlx::Error> {
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
        .execute(self.pool)
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
    pub async fn get_attachment(&self, id: Uuid) -> Result<Option<Attachment>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT id, item_id, filename, mime_type, storage_path, size_bytes, uploaded_at
             FROM attachments WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(self.pool)
        .await?;

        Ok(row.map(|r| Attachment {
            id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap(),
            item_id: Uuid::parse_str(&r.get::<String, _>("item_id")).unwrap(),
            filename: r.get("filename"),
            mime_type: r.get("mime_type"),
            storage_path: r.get("storage_path"),
            size_bytes: r.get::<i64, _>("size_bytes") as u64,
            uploaded_at: r.get::<String, _>("uploaded_at").parse().unwrap(),
        }))
    }

    /// List attachments for an item
    #[instrument(skip(self))]
    pub async fn list_attachments(&self, item_id: Uuid) -> Result<Vec<Attachment>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT id, item_id, filename, mime_type, storage_path, size_bytes, uploaded_at
             FROM attachments WHERE item_id = ? ORDER BY uploaded_at DESC",
        )
        .bind(item_id.to_string())
        .fetch_all(self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Attachment {
                id: Uuid::parse_str(&r.get::<String, _>("id")).unwrap(),
                item_id: Uuid::parse_str(&r.get::<String, _>("item_id")).unwrap(),
                filename: r.get("filename"),
                mime_type: r.get("mime_type"),
                storage_path: r.get("storage_path"),
                size_bytes: r.get::<i64, _>("size_bytes") as u64,
                uploaded_at: r.get::<String, _>("uploaded_at").parse().unwrap(),
            })
            .collect())
    }

    /// Delete attachment
    #[instrument(skip(self))]
    pub async fn delete_attachment(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM attachments WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}
