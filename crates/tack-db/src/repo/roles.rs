use crate::StoreError;
use chrono::Utc;
use tracing::{debug, instrument};
use uuid::Uuid;

use tack_core::models::{CreateRole, Role, UpdateRole};

use super::Repository;

impl Repository {
    #[instrument(skip(self))]
    pub async fn create_role(
        &self,
        project_id: Uuid,
        input: CreateRole,
    ) -> Result<Role, StoreError> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let color = input.color.unwrap_or_else(|| "#6366f1".into());

        sqlx::query(
            "INSERT INTO roles (id, project_id, name, color, icon, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(project_id.to_string())
        .bind(&input.name)
        .bind(&color)
        .bind(&input.icon)
        .bind(now.to_rfc3339())
        .execute(self.pool())
        .await?;

        debug!(role_id = %id, name = %input.name, "Role created");

        Ok(Role {
            id,
            project_id,
            name: input.name,
            color,
            icon: input.icon,
            agent_profile_id: None,
            created_at: now,
        })
    }

    /// Applies the fields `input` names; `None` when no such role exists.
    #[instrument(skip(self))]
    pub async fn update_role(
        &self,
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
        .execute(self.pool())
        .await?;
        if result.rows_affected() == 0 {
            return Ok(None);
        }
        let row = sqlx::query_as::<_, RoleRow>(
            "SELECT id, project_id, name, color, icon, agent_profile_id, created_at
             FROM roles WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_one(self.pool())
        .await?;
        Ok(Some(row.into_role()))
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
            "SELECT id, project_id, name, color, icon, agent_profile_id, created_at
             FROM roles WHERE project_id = ? ORDER BY name",
        )
        .bind(project_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_role()).collect())
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
        let same_project: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM items i JOIN roles r ON r.project_id = i.project_id
                           WHERE i.id = ? AND r.id = ?)",
        )
        .bind(item_id.to_string())
        .bind(role_id.to_string())
        .fetch_one(self.pool())
        .await?;
        if !same_project {
            return Err(StoreError::NotFound);
        }
        sqlx::query("INSERT OR IGNORE INTO item_roles (item_id, role_id) VALUES (?, ?)")
            .bind(item_id.to_string())
            .bind(role_id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
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
            "SELECT r.id, r.project_id, r.name, r.color, r.icon, r.agent_profile_id, r.created_at
             FROM roles r
             JOIN item_roles ir ON r.id = ir.role_id
             WHERE ir.item_id = ?
             ORDER BY r.name",
        )
        .bind(item_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_role()).collect())
    }

    #[instrument(skip(self))]
    pub async fn delete_role(&self, id: Uuid) -> Result<bool, StoreError> {
        let result = sqlx::query("DELETE FROM roles WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[derive(sqlx::FromRow)]
struct RoleRow {
    id: String,
    project_id: String,
    name: String,
    color: String,
    icon: Option<String>,
    agent_profile_id: Option<String>,
    created_at: String,
}

impl RoleRow {
    fn into_role(self) -> Role {
        Role {
            id: Uuid::parse_str(&self.id).unwrap(),
            project_id: Uuid::parse_str(&self.project_id).unwrap(),
            name: self.name,
            color: self.color,
            icon: self.icon,
            agent_profile_id: self.agent_profile_id,
            created_at: chrono::DateTime::parse_from_rfc3339(&self.created_at)
                .map(|d| d.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        }
    }
}
