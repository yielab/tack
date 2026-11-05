//! Writes of a project's type catalog. The rules (what a catalog is, what may change while items
//! use a type) are `tack_core::item_types`'s; this asks them inside one transaction with the facts
//! only the store can read atomically, and writes the catalog that comes back.

use sqlx::{Sqlite, Transaction};
use tack_core::CoreError;
use tack_core::item_types::{self, CreateItemType, ItemTypeDef, UpdateItemType, check_catalog};
use uuid::Uuid;

use super::Repository;
use crate::StoreError;
use crate::queries::item_types::{in_use_in, of_project_in};

/// What stops a change to the catalog.
#[derive(Debug, thiserror::Error)]
pub enum ItemTypeError {
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A rule of the catalog.
    #[error(transparent)]
    Refused(#[from] CoreError),
}

impl From<sqlx::Error> for ItemTypeError {
    fn from(error: sqlx::Error) -> Self {
        ItemTypeError::Store(error.into())
    }
}

/// Replaces the project's rows with `types`, which the caller checked: the default is cleared
/// first so the one-default index never sees two.
pub(crate) async fn write_catalog(
    tx: &mut Transaction<'_, Sqlite>,
    project_id: Uuid,
    types: &[ItemTypeDef],
) -> Result<(), StoreError> {
    let project = project_id.to_string();
    sqlx::query("UPDATE item_types SET default_work = 0 WHERE project_id = ?")
        .bind(&project)
        .execute(&mut **tx)
        .await?;
    let keep: Vec<&str> = types.iter().map(|t| t.key.as_str()).collect();
    let present: Vec<String> =
        sqlx::query_scalar("SELECT key FROM item_types WHERE project_id = ?")
            .bind(&project)
            .fetch_all(&mut **tx)
            .await?;
    for key in present.iter().filter(|key| !keep.contains(&key.as_str())) {
        sqlx::query("DELETE FROM item_types WHERE project_id = ? AND key = ?")
            .bind(&project)
            .bind(key)
            .execute(&mut **tx)
            .await?;
    }
    for def in types {
        sqlx::query(
            "INSERT INTO item_types (project_id, key, name, level, color, sort_order, default_work)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (project_id, key) DO UPDATE SET
                 name = excluded.name, level = excluded.level, color = excluded.color,
                 sort_order = excluded.sort_order, default_work = excluded.default_work",
        )
        .bind(&project)
        .bind(&def.key)
        .bind(&def.name)
        .bind(def.level.as_str())
        .bind(def.color.as_str())
        .bind(i64::from(def.order))
        .bind(def.default_work)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// Gives a new project the catalog of its type (`tack_core::item_types::preset`).
pub(crate) async fn seed(
    tx: &mut Transaction<'_, Sqlite>,
    project_id: Uuid,
    types: &[ItemTypeDef],
) -> Result<(), StoreError> {
    write_catalog(tx, project_id, types).await
}

impl Repository {
    /// Writes the whole catalog of a project, for an import: types the file names are added or
    /// renamed, and a type the catalog no longer lists goes only if no item uses it.
    pub async fn set_item_types(
        &self,
        project_id: Uuid,
        types: &[ItemTypeDef],
    ) -> Result<(), ItemTypeError> {
        check_catalog(types)?;
        self.require_project(project_id).await?;
        let mut tx = self.pool().begin().await?;
        let used = in_use_in(&mut tx, project_id).await?;
        let before = of_project_in(&mut tx, project_id).await?;
        for old in &before {
            let items = used
                .iter()
                .find(|(key, _)| *key == old.key)
                .map_or(0, |(_, n)| *n);
            let still = types.iter().find(|t| t.key == old.key);
            if items > 0 && still.is_none_or(|t| t.level != old.level) {
                return Err(CoreError::TypeInUse {
                    key: old.key.clone(),
                    items,
                }
                .into());
            }
        }
        write_catalog(&mut tx, project_id, types).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Adds a type to the project's catalog.
    pub async fn create_item_type(
        &self,
        project_id: Uuid,
        new: &CreateItemType,
    ) -> Result<Vec<ItemTypeDef>, ItemTypeError> {
        self.require_project(project_id).await?;
        let mut tx = self.pool().begin().await?;
        let before = of_project_in(&mut tx, project_id).await?;
        let after = item_types::add(&before, new)?;
        write_catalog(&mut tx, project_id, &after).await?;
        tx.commit().await?;
        Ok(after)
    }

    /// Changes the type `key`: `NotFound` when the project lacks it.
    pub async fn update_item_type(
        &self,
        project_id: Uuid,
        key: &str,
        patch: &UpdateItemType,
    ) -> Result<Vec<ItemTypeDef>, ItemTypeError> {
        self.require_project(project_id).await?;
        let mut tx = self.pool().begin().await?;
        let before = of_project_in(&mut tx, project_id).await?;
        if !before.iter().any(|t| t.key == key) {
            return Err(StoreError::not_found("item type").into());
        }
        let used = in_use_in(&mut tx, project_id).await?;
        let items = used.iter().find(|(k, _)| k == key).map_or(0, |(_, n)| *n);
        let after = item_types::update(&before, key, patch, items)?;
        write_catalog(&mut tx, project_id, &after).await?;
        tx.commit().await?;
        Ok(after)
    }

    /// Deletes the type `key`; the items that use it move to `merge_into` in the same transaction.
    pub async fn delete_item_type(
        &self,
        project_id: Uuid,
        key: &str,
        merge_into: Option<&str>,
    ) -> Result<Vec<ItemTypeDef>, ItemTypeError> {
        self.require_project(project_id).await?;
        let mut tx = self.pool().begin().await?;
        let before = of_project_in(&mut tx, project_id).await?;
        if !before.iter().any(|t| t.key == key) {
            return Err(StoreError::not_found("item type").into());
        }
        let used = in_use_in(&mut tx, project_id).await?;
        let items = used.iter().find(|(k, _)| k == key).map_or(0, |(_, n)| *n);
        let after = item_types::remove(&before, key, merge_into, items)?;
        if let Some(target) = merge_into {
            sqlx::query("UPDATE items SET item_type = ? WHERE project_id = ? AND item_type = ?")
                .bind(target)
                .bind(project_id.to_string())
                .bind(key)
                .execute(&mut *tx)
                .await?;
        }
        write_catalog(&mut tx, project_id, &after).await?;
        tx.commit().await?;
        Ok(after)
    }
}
