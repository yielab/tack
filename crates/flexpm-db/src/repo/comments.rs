use chrono::Utc;
use tracing::{debug, instrument};
use uuid::Uuid;

use flexpm_core::models::{Comment, CommentType, CreateComment};

use super::Repository;

impl Repository {
    #[instrument(skip(self))]
    pub async fn create_comment(
        &self,
        item_id: Uuid,
        input: CreateComment,
    ) -> Result<Comment, sqlx::Error> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        sqlx::query(
            "INSERT INTO comments (id, item_id, author, content, comment_type, created_at, updated_at)
             VALUES (?, ?, ?, ?, 'comment', ?, ?)"
        )
        .bind(id.to_string())
        .bind(item_id.to_string())
        .bind(&input.author)
        .bind(&input.content)
        .bind(&now_str)
        .bind(&now_str)
        .execute(self.pool())
        .await?;

        debug!(comment_id = %id, item_id = %item_id, "Comment created");

        Ok(Comment {
            id,
            item_id,
            author: input.author,
            content: input.content,
            comment_type: CommentType::Comment,
            created_at: now,
            updated_at: now,
        })
    }

    #[instrument(skip(self))]
    pub async fn list_comments(&self, item_id: Uuid) -> Result<Vec<Comment>, sqlx::Error> {
        let rows = sqlx::query_as::<_, CommentRow>(
            "SELECT id, item_id, author, content, comment_type, created_at, updated_at
             FROM comments WHERE item_id = ? ORDER BY created_at ASC"
        )
        .bind(item_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_comment()).collect())
    }

    /// Create a system comment for audit trail (status changes, edits, etc.)
    #[instrument(skip(self))]
    pub async fn create_system_comment(
        &self,
        item_id: Uuid,
        content: String,
        comment_type: CommentType,
    ) -> Result<Comment, sqlx::Error> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let type_str = match comment_type {
            CommentType::StatusChange => "status_change",
            CommentType::Edit => "edit",
            CommentType::System => "system",
            CommentType::Comment => "comment",
        };

        sqlx::query(
            "INSERT INTO comments (id, item_id, author, content, comment_type, created_at, updated_at)
             VALUES (?, ?, 'system', ?, ?, ?, ?)"
        )
        .bind(id.to_string())
        .bind(item_id.to_string())
        .bind(&content)
        .bind(type_str)
        .bind(&now_str)
        .bind(&now_str)
        .execute(self.pool())
        .await?;

        Ok(Comment {
            id,
            item_id,
            author: Some("system".into()),
            content,
            comment_type,
            created_at: now,
            updated_at: now,
        })
    }
}

#[derive(sqlx::FromRow)]
struct CommentRow {
    id: String,
    item_id: String,
    author: Option<String>,
    content: String,
    comment_type: String,
    created_at: String,
    updated_at: String,
}

impl CommentRow {
    fn into_comment(self) -> Comment {
        Comment {
            id: Uuid::parse_str(&self.id).unwrap(),
            item_id: Uuid::parse_str(&self.item_id).unwrap(),
            author: self.author,
            content: self.content,
            comment_type: match self.comment_type.as_str() {
                "status_change" => CommentType::StatusChange,
                "edit" => CommentType::Edit,
                "system" => CommentType::System,
                _ => CommentType::Comment,
            },
            created_at: chrono::DateTime::parse_from_rfc3339(&self.created_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&self.updated_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        }
    }
}
