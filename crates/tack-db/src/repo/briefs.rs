use chrono::{DateTime, Utc};
use sqlx::Row;
use tracing::instrument;
use uuid::Uuid;

use tack_core::models::{ItemBrief, UpsertItemBrief};

use super::Repository;

fn decode_error(e: impl std::error::Error + Send + Sync + 'static) -> sqlx::Error {
    sqlx::Error::Decode(Box::new(e))
}

fn brief_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ItemBrief, sqlx::Error> {
    let item_id: String = row.get("item_id");
    let acceptance: String = row.get("acceptance");
    let constraints: String = row.get("constraints");
    let risk: Option<String> = row.get("risk");
    let created_at: String = row.get("created_at");
    let updated_at: String = row.get("updated_at");
    Ok(ItemBrief {
        item_id: Uuid::parse_str(&item_id).map_err(decode_error)?,
        acceptance: serde_json::from_str(&acceptance).map_err(decode_error)?,
        constraints: serde_json::from_str(&constraints).map_err(decode_error)?,
        definition_of_done: row.get("definition_of_done"),
        risk: risk
            .map(|r| serde_json::from_value(serde_json::Value::String(r)))
            .transpose()
            .map_err(decode_error)?,
        created_at: created_at.parse::<DateTime<Utc>>().map_err(decode_error)?,
        updated_at: updated_at.parse::<DateTime<Utc>>().map_err(decode_error)?,
    })
}

impl Repository {
    /// The brief of one item, if it has one.
    #[instrument(skip(self))]
    pub async fn get_item_brief(&self, item_id: Uuid) -> Result<Option<ItemBrief>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT item_id, acceptance, constraints, definition_of_done, risk, created_at, updated_at
             FROM item_briefs WHERE item_id = ?",
        )
        .bind(item_id.to_string())
        .fetch_optional(self.pool())
        .await?;
        row.as_ref().map(brief_from_row).transpose()
    }

    /// Every brief of a project's items, for the project export.
    #[instrument(skip(self))]
    pub async fn list_item_briefs_for_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ItemBrief>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT b.item_id, b.acceptance, b.constraints, b.definition_of_done, b.risk,
                    b.created_at, b.updated_at
             FROM item_briefs b JOIN items i ON i.id = b.item_id
             WHERE i.project_id = ? ORDER BY b.created_at ASC, b.item_id ASC",
        )
        .bind(project_id.to_string())
        .fetch_all(self.pool())
        .await?;
        rows.iter().map(brief_from_row).collect()
    }

    /// Create or replace an item's brief; `created_at` survives a replace.
    #[instrument(skip(self, input))]
    pub async fn upsert_item_brief(
        &self,
        item_id: Uuid,
        input: UpsertItemBrief,
    ) -> Result<ItemBrief, sqlx::Error> {
        let now = Utc::now().to_rfc3339();
        let risk = input
            .risk
            .as_ref()
            .map(|r| serde_json::to_value(r).map_err(decode_error))
            .transpose()?
            .and_then(|v| v.as_str().map(str::to_owned));
        sqlx::query(
            "INSERT INTO item_briefs
                 (item_id, acceptance, constraints, definition_of_done, risk, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(item_id) DO UPDATE SET
                 acceptance = excluded.acceptance,
                 constraints = excluded.constraints,
                 definition_of_done = excluded.definition_of_done,
                 risk = excluded.risk,
                 updated_at = excluded.updated_at",
        )
        .bind(item_id.to_string())
        .bind(serde_json::to_string(&input.acceptance).map_err(decode_error)?)
        .bind(serde_json::to_string(&input.constraints).map_err(decode_error)?)
        .bind(&input.definition_of_done)
        .bind(risk)
        .bind(&now)
        .bind(&now)
        .execute(self.pool())
        .await?;
        self.get_item_brief(item_id)
            .await?
            .ok_or(sqlx::Error::RowNotFound)
    }

    /// Delete an item's brief; `false` when it had none.
    #[instrument(skip(self))]
    pub async fn delete_item_brief(&self, item_id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM item_briefs WHERE item_id = ?")
            .bind(item_id.to_string())
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
