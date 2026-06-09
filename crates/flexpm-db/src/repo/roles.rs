use chrono::Utc;
use tracing::{debug, instrument};
use uuid::Uuid;

use flexpm_core::models::{CreateRole, Role};

use super::Repository;

impl Repository {
    #[instrument(skip(self))]
    pub async fn create_role(
        &self,
        project_id: Uuid,
        input: CreateRole,
    ) -> Result<Role, sqlx::Error> {
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
            created_at: now,
        })
    }

    #[instrument(skip(self))]
    pub async fn list_roles(&self, project_id: Uuid) -> Result<Vec<Role>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RoleRow>(
            "SELECT id, project_id, name, color, icon, created_at
             FROM roles WHERE project_id = ? ORDER BY name",
        )
        .bind(project_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_role()).collect())
    }

    #[instrument(skip(self))]
    pub async fn assign_role_to_item(
        &self,
        item_id: Uuid,
        role_id: Uuid,
    ) -> Result<(), sqlx::Error> {
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
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM item_roles WHERE item_id = ? AND role_id = ?")
            .bind(item_id.to_string())
            .bind(role_id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn get_roles_for_item(&self, item_id: Uuid) -> Result<Vec<Role>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RoleRow>(
            "SELECT r.id, r.project_id, r.name, r.color, r.icon, r.created_at
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
    pub async fn delete_role(&self, id: Uuid) -> Result<bool, sqlx::Error> {
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
            created_at: chrono::DateTime::parse_from_rfc3339(&self.created_at)
                .map(|d| d.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        }
    }
}
