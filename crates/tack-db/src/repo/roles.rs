use crate::repo::{Parent, require_row};
use crate::{StoreError, decode};
use chrono::Utc;
use sqlx::SqliteConnection;
use tracing::{debug, instrument};
use uuid::Uuid;

use tack_core::models::{CreateRole, Role, UpdateRole};

use super::Repository;
use super::project_file::{key_conflict, key_for};

impl Repository {
    #[instrument(skip(self))]
    pub async fn create_role(
        &self,
        project_id: Uuid,
        input: CreateRole,
    ) -> Result<Role, StoreError> {
        let mut conn = self.pool().acquire().await?;
        create_role_in(&mut conn, project_id, input).await
    }

    /// Applies the fields `input` names; `None` when no such role exists.
    #[instrument(skip(self))]
    pub async fn update_role(
        &self,
        id: Uuid,
        input: UpdateRole,
    ) -> Result<Option<Role>, StoreError> {
        let mut conn = self.pool().acquire().await?;
        update_role_in(&mut conn, id, input).await
    }

    /// The item's roles, each with the profile it names: `(role name, profile id)`, by name.
    #[instrument(skip(self))]
    pub async fn profiles_for_item(
        &self,
        item_id: Uuid,
    ) -> Result<Vec<(String, Option<String>)>, StoreError> {
        sqlx::query_as(
            "SELECT r.name, r.agent_profile_id FROM roles r
             JOIN item_roles ir ON r.id = ir.role_id
             WHERE ir.item_id = ? ORDER BY r.name",
        )
        .bind(item_id.to_string())
        .fetch_all(self.pool())
        .await
        .map_err(StoreError::from)
    }

    #[instrument(skip(self))]
    pub async fn list_roles(&self, project_id: Uuid) -> Result<Vec<Role>, StoreError> {
        let rows = sqlx::query_as::<_, RoleRow>(
            "SELECT id, project_id, key, name, color, icon, agent_profile_id, created_at
             FROM roles WHERE project_id = ? ORDER BY name",
        )
        .bind(project_id.to_string())
        .fetch_all(self.pool())
        .await?;

        rows.into_iter().map(RoleRow::into_role).collect()
    }

    /// Every `(item, role)` pair of the project's items.
    #[instrument(skip(self))]
    pub async fn item_roles_for_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<(Uuid, Uuid)>, StoreError> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT ir.item_id, ir.role_id FROM item_roles ir
             JOIN items i ON i.id = ir.item_id
             WHERE i.project_id = ? ORDER BY ir.item_id, ir.role_id",
        )
        .bind(project_id.to_string())
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(item, role)| {
                Some((Uuid::parse_str(&item).ok()?, Uuid::parse_str(&role).ok()?))
            })
            .collect())
    }

    #[instrument(skip(self))]
    pub async fn assign_role_to_item(
        &self,
        item_id: Uuid,
        role_id: Uuid,
    ) -> Result<(), StoreError> {
        let mut conn = self.pool().acquire().await?;
        assign_role_to_item_in(&mut conn, item_id, role_id).await
    }

    #[instrument(skip(self))]
    pub async fn remove_role_from_item(
        &self,
        item_id: Uuid,
        role_id: Uuid,
    ) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM item_roles WHERE item_id = ? AND role_id = ?")
            .bind(item_id.to_string())
            .bind(role_id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_roles_for_item(&self, item_id: Uuid) -> Result<Vec<Role>, StoreError> {
        let rows = sqlx::query_as::<_, RoleRow>(
            "SELECT r.id, r.project_id, r.key, r.name, r.color, r.icon, r.agent_profile_id, r.created_at
             FROM roles r
             JOIN item_roles ir ON r.id = ir.role_id
             WHERE ir.item_id = ?
             ORDER BY r.name",
        )
        .bind(item_id.to_string())
        .fetch_all(self.pool())
        .await?;

        rows.into_iter().map(RoleRow::into_role).collect()
    }

    #[instrument(skip(self))]
    pub async fn delete_role(&self, id: Uuid) -> Result<bool, StoreError> {
        let mut conn = self.pool().acquire().await?;
        delete_role_in(&mut conn, id).await
    }
}

/// The role `INSERT`, on whatever connection the caller holds.
pub(crate) async fn create_role_in(
    conn: &mut SqliteConnection,
    project_id: Uuid,
    input: CreateRole,
) -> Result<Role, StoreError> {
    require_row(conn, Parent::Project, project_id).await?;
    let id = Uuid::new_v4();
    let now = Utc::now();
    let color = input.color.unwrap_or_else(|| "#6366f1".into());
    let key = key_for(
        conn,
        "roles",
        Some(&project_id.to_string()),
        input.key.as_deref(),
        &input.name,
    )
    .await?;

    sqlx::query(
        "INSERT INTO roles (id, project_id, key, name, color, icon, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(project_id.to_string())
    .bind(&key)
    .bind(&input.name)
    .bind(&color)
    .bind(&input.icon)
    .bind(now.to_rfc3339())
    .execute(&mut *conn)
    .await
    .map_err(key_conflict)?;

    debug!(role_id = %id, name = %input.name, "Role created");

    Ok(Role {
        id,
        project_id,
        key,
        name: input.name,
        color,
        icon: input.icon,
        agent_profile_id: None,
        created_at: now,
    })
}

/// The role `UPDATE` (see [`Repository::update_role`]), on the caller's connection.
pub(crate) async fn update_role_in(
    conn: &mut SqliteConnection,
    id: Uuid,
    input: UpdateRole,
) -> Result<Option<Role>, StoreError> {
    let (set_profile, profile) = match input.agent_profile_id {
        Some(value) => (1, value),
        None => (0, None),
    };
    let result = sqlx::query(
        "UPDATE roles SET name = COALESCE(?, name), color = COALESCE(?, color),
             icon = COALESCE(?, icon),
             agent_profile_id = CASE WHEN ? = 1 THEN ? ELSE agent_profile_id END
         WHERE id = ?",
    )
    .bind(&input.name)
    .bind(&input.color)
    .bind(&input.icon)
    .bind(set_profile)
    .bind(&profile)
    .bind(id.to_string())
    .execute(&mut *conn)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    let row = sqlx::query_as::<_, RoleRow>(
        "SELECT id, project_id, key, name, color, icon, agent_profile_id, created_at
         FROM roles WHERE id = ?",
    )
    .bind(id.to_string())
    .fetch_one(&mut *conn)
    .await?;
    row.into_role().map(Some)
}

/// Gives the item the role (see [`Repository::assign_role_to_item`]), on the caller's connection.
pub(crate) async fn assign_role_to_item_in(
    conn: &mut SqliteConnection,
    item_id: Uuid,
    role_id: Uuid,
) -> Result<(), StoreError> {
    let project_id: Option<String> =
        sqlx::query_scalar("SELECT project_id FROM items WHERE id = ?")
            .bind(item_id.to_string())
            .fetch_optional(&mut *conn)
            .await?;
    let project_id = project_id.ok_or(StoreError::not_found("item"))?;
    let in_project: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM roles WHERE id = ? AND project_id = ?)")
            .bind(role_id.to_string())
            .bind(project_id)
            .fetch_one(&mut *conn)
            .await?;
    if !in_project {
        return Err(StoreError::not_found("role"));
    }
    sqlx::query("INSERT OR IGNORE INTO item_roles (item_id, role_id) VALUES (?, ?)")
        .bind(item_id.to_string())
        .bind(role_id.to_string())
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Whether a role went (see [`Repository::delete_role`]), on the caller's connection.
pub(crate) async fn delete_role_in(
    conn: &mut SqliteConnection,
    id: Uuid,
) -> Result<bool, StoreError> {
    let result = sqlx::query("DELETE FROM roles WHERE id = ?")
        .bind(id.to_string())
        .execute(&mut *conn)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[derive(sqlx::FromRow)]
struct RoleRow {
    id: String,
    project_id: String,
    key: String,
    name: String,
    color: String,
    icon: Option<String>,
    agent_profile_id: Option<String>,
    created_at: String,
}

impl RoleRow {
    fn into_role(self) -> Result<Role, StoreError> {
        const TABLE: &str = "roles";
        Ok(Role {
            id: decode::uuid(TABLE, "id", &self.id)?,
            project_id: decode::uuid(TABLE, "project_id", &self.project_id)?,
            key: self.key,
            name: self.name,
            color: self.color,
            icon: self.icon,
            agent_profile_id: self.agent_profile_id,
            created_at: decode::instant(TABLE, "created_at", &self.created_at)?,
        })
    }
}
