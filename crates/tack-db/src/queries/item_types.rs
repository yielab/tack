//! A project's type catalog and how many items use each type. The rules are
//! `tack_core::item_types`'s; `repo::item_types` writes.

use sqlx::SqliteConnection;
use tack_core::item_types::{ItemTypeDef, ItemTypeUse, Level, TypeColor};
use uuid::Uuid;

use crate::{Repository, StoreError};

const TABLE: &str = "item_types";

#[derive(sqlx::FromRow)]
struct TypeRow {
    key: String,
    name: String,
    level: String,
    color: String,
    sort_order: i64,
    default_work: bool,
}

impl TypeRow {
    fn into_def(self) -> Result<ItemTypeDef, StoreError> {
        Ok(ItemTypeDef {
            level: Level::parse(&self.level)
                .ok_or_else(|| StoreError::corrupt(TABLE, "level", "not a level"))?,
            color: TypeColor::parse(&self.color)
                .ok_or_else(|| StoreError::corrupt(TABLE, "color", "not a colour"))?,
            order: u32::try_from(self.sort_order)
                .map_err(|error| StoreError::corrupt(TABLE, "sort_order", error))?,
            key: self.key,
            name: self.name,
            default_work: self.default_work,
        })
    }
}

/// The project's types in display order, on the connection the caller holds.
pub(crate) async fn of_project_in(
    conn: &mut SqliteConnection,
    project_id: Uuid,
) -> Result<Vec<ItemTypeDef>, StoreError> {
    let rows: Vec<TypeRow> = sqlx::query_as(
        "SELECT key, name, level, color, sort_order, default_work FROM item_types
          WHERE project_id = ? ORDER BY sort_order, key",
    )
    .bind(project_id.to_string())
    .fetch_all(&mut *conn)
    .await?;
    rows.into_iter().map(TypeRow::into_def).collect()
}

/// The project's types in display order.
pub async fn of_project(
    repo: &Repository,
    project_id: Uuid,
) -> Result<Vec<ItemTypeDef>, StoreError> {
    let mut conn = repo.pool().acquire().await?;
    of_project_in(&mut conn, project_id).await
}

/// How many items of the project use each type key; keys no item uses are absent.
pub(crate) async fn in_use_in(
    conn: &mut SqliteConnection,
    project_id: Uuid,
) -> Result<Vec<(String, u32)>, StoreError> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT item_type, COUNT(*) FROM items WHERE project_id = ? GROUP BY item_type",
    )
    .bind(project_id.to_string())
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(key, count)| (key, u32::try_from(count).unwrap_or(u32::MAX)))
        .collect())
}

/// The project's types with how many items use each.
pub async fn in_use(repo: &Repository, project_id: Uuid) -> Result<Vec<ItemTypeUse>, StoreError> {
    let mut conn = repo.pool().acquire().await?;
    let used = in_use_in(&mut conn, project_id).await?;
    Ok(of_project_in(&mut conn, project_id)
        .await?
        .into_iter()
        .map(|def| ItemTypeUse {
            items: used
                .iter()
                .find(|(key, _)| *key == def.key)
                .map_or(0, |(_, n)| *n),
            def,
        })
        .collect())
}

/// The key of the project's default work type.
pub(crate) async fn default_work_key(
    conn: &mut SqliteConnection,
    project_id: Uuid,
) -> Result<String, StoreError> {
    sqlx::query_scalar("SELECT key FROM item_types WHERE project_id = ? AND default_work = 1")
        .bind(project_id.to_string())
        .fetch_optional(&mut *conn)
        .await?
        .ok_or(StoreError::not_found("item type"))
}
