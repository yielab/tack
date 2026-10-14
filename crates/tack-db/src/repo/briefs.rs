use chrono::{DateTime, Utc};
use sqlx::Row;
use tracing::instrument;
use uuid::Uuid;

use sha2::{Digest, Sha256};

use tack_core::brief::ItemBriefVersion;
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

fn version_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ItemBriefVersion, sqlx::Error> {
    let item_id: String = row.get("item_id");
    let acceptance: String = row.get("acceptance");
    let constraints: String = row.get("constraints");
    let risk: Option<String> = row.get("risk");
    let created_at: String = row.get("created_at");
    Ok(ItemBriefVersion {
        item_id: Uuid::parse_str(&item_id).map_err(decode_error)?,
        version: row.get("version"),
        acceptance: serde_json::from_str(&acceptance).map_err(decode_error)?,
        constraints: serde_json::from_str(&constraints).map_err(decode_error)?,
        definition_of_done: row.get("definition_of_done"),
        risk: risk
            .map(|r| serde_json::from_value(serde_json::Value::String(r)))
            .transpose()
            .map_err(decode_error)?,
        brief_sha256: row.get("brief_sha256"),
        created_at: parse_instant(&created_at)?,
    })
}

/// A stored instant: RFC 3339 as this layer writes it, or SQLite's
/// `datetime('now')` form as the 087 backfill and column default write it.
fn parse_instant(text: &str) -> Result<DateTime<Utc>, sqlx::Error> {
    text.parse::<DateTime<Utc>>().or_else(|_| {
        chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
            .map(|naive| naive.and_utc())
            .map_err(decode_error)
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
    /// A brief that differs from the stored one also writes the next
    /// `item_brief_versions` row in the same transaction, hashed as the runner
    /// stages it; an identical brief writes nothing.
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
        let acceptance = serde_json::to_string(&input.acceptance).map_err(decode_error)?;
        let constraints = serde_json::to_string(&input.constraints).map_err(decode_error)?;
        let mut tx = self.pool().begin().await?;
        let stored = sqlx::query(
            "SELECT acceptance, constraints, definition_of_done, risk
             FROM item_briefs WHERE item_id = ?",
        )
        .bind(item_id.to_string())
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(row) = &stored {
            let same = row.get::<String, _>("acceptance") == acceptance
                && row.get::<String, _>("constraints") == constraints
                && row.get::<Option<String>, _>("definition_of_done") == input.definition_of_done
                && row.get::<Option<String>, _>("risk") == risk;
            if same {
                drop(tx);
                return self
                    .get_item_brief(item_id)
                    .await?
                    .ok_or(sqlx::Error::RowNotFound);
            }
        }
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
        .bind(&acceptance)
        .bind(&constraints)
        .bind(&input.definition_of_done)
        .bind(&risk)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        let row = sqlx::query(
            "SELECT item_id, acceptance, constraints, definition_of_done, risk, created_at, updated_at
             FROM item_briefs WHERE item_id = ?",
        )
        .bind(item_id.to_string())
        .fetch_one(&mut *tx)
        .await?;
        let brief = brief_from_row(&row)?;
        // `brief.json` as the runner stages it: the brief as a JSON value, pretty-printed.
        let staged =
            serde_json::to_vec_pretty(&serde_json::to_value(&brief).map_err(decode_error)?)
                .map_err(decode_error)?;
        let brief_sha256: String = Sha256::digest(&staged)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        sqlx::query(
            "INSERT INTO item_brief_versions
                 (item_id, version, acceptance, constraints, definition_of_done, risk, brief_sha256, created_at)
             VALUES (?, (SELECT COALESCE(MAX(version), 0) + 1 FROM item_brief_versions WHERE item_id = ?),
                     ?, ?, ?, ?, ?, ?)",
        )
        .bind(item_id.to_string())
        .bind(item_id.to_string())
        .bind(&acceptance)
        .bind(&constraints)
        .bind(&input.definition_of_done)
        .bind(&risk)
        .bind(&brief_sha256)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(brief)
    }

    /// Every saved version of an item's brief, oldest first.
    #[instrument(skip(self))]
    pub async fn list_item_brief_versions(
        &self,
        item_id: Uuid,
    ) -> Result<Vec<ItemBriefVersion>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT v.item_id, v.version, v.acceptance, v.constraints, v.definition_of_done, v.risk, v.brief_sha256, v.created_at FROM item_brief_versions v
             WHERE v.item_id = ? ORDER BY v.version ASC",
        )
        .bind(item_id.to_string())
        .fetch_all(self.pool())
        .await?;
        rows.iter().map(version_from_row).collect()
    }

    /// Every brief version of a project's items, for the project export.
    #[instrument(skip(self))]
    pub async fn list_item_brief_versions_for_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ItemBriefVersion>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT v.item_id, v.version, v.acceptance, v.constraints, v.definition_of_done, v.risk, v.brief_sha256, v.created_at FROM item_brief_versions v JOIN items i ON i.id = v.item_id
             WHERE i.project_id = ? ORDER BY v.item_id ASC, v.version ASC",
        )
        .bind(project_id.to_string())
        .fetch_all(self.pool())
        .await?;
        rows.iter().map(version_from_row).collect()
    }

    /// Replace an item's saved versions with `versions`, as an import restores
    /// them; each keeps its number, content, hash and time.
    #[instrument(skip(self, versions))]
    pub async fn restore_item_brief_versions(
        &self,
        item_id: Uuid,
        versions: &[ItemBriefVersion],
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM item_brief_versions WHERE item_id = ?")
            .bind(item_id.to_string())
            .execute(&mut *tx)
            .await?;
        for version in versions {
            let risk = version
                .risk
                .as_ref()
                .map(|r| serde_json::to_value(r).map_err(decode_error))
                .transpose()?
                .and_then(|v| v.as_str().map(str::to_owned));
            sqlx::query(
                "INSERT INTO item_brief_versions
                     (item_id, version, acceptance, constraints, definition_of_done, risk, brief_sha256, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(item_id.to_string())
            .bind(version.version)
            .bind(serde_json::to_string(&version.acceptance).map_err(decode_error)?)
            .bind(serde_json::to_string(&version.constraints).map_err(decode_error)?)
            .bind(&version.definition_of_done)
            .bind(risk)
            .bind(&version.brief_sha256)
            .bind(version.created_at.to_rfc3339())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
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
