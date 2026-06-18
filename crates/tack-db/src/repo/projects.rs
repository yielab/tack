use chrono::Utc;
use tracing::{debug, instrument};
use uuid::Uuid;

use tack_core::models::{CreateProject, Project, ProjectType, UpdateProject};
use tack_core::vocabulary::vocabulary_for_type;
use tack_core::workflow::workflow_for_type;

use super::Repository;

impl Repository {
    #[instrument(skip(self))]
    pub async fn create_project(
        &self,
        workspace_id: Uuid,
        input: CreateProject,
    ) -> Result<Project, sqlx::Error> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let vocabulary = vocabulary_for_type(&input.project_type);
        let workflow = workflow_for_type(&input.project_type);
        let vocab_json = serde_json::to_string(&vocabulary).unwrap();
        let workflow_json = serde_json::to_string(&workflow).unwrap();
        let project_type_str = input.project_type.to_string();
        let now_str = now.to_rfc3339();

        sqlx::query(
            "INSERT INTO projects (id, workspace_id, name, description, project_type, vocabulary, workflow, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(id.to_string())
        .bind(workspace_id.to_string())
        .bind(&input.name)
        .bind(&input.description)
        .bind(&project_type_str)
        .bind(&vocab_json)
        .bind(&workflow_json)
        .bind(&now_str)
        .bind(&now_str)
        .execute(self.pool())
        .await?;

        debug!(project_id = %id, name = %input.name, "Project created");

        Ok(Project {
            id,
            workspace_id,
            name: input.name,
            description: input.description,
            project_type: input.project_type,
            vocabulary,
            workflow,
            created_at: now,
            updated_at: now,
            archived: false,
        })
    }

    #[instrument(skip(self))]
    pub async fn get_project(&self, id: Uuid) -> Result<Option<Project>, sqlx::Error> {
        let row = sqlx::query_as::<_, ProjectRow>(
            "SELECT id, workspace_id, name, description, project_type, vocabulary, workflow, archived, created_at, updated_at
             FROM projects WHERE id = ?"
        )
        .bind(id.to_string())
        .fetch_optional(self.pool())
        .await?;

        Ok(row.map(|r| r.into_project()))
    }

    #[instrument(skip(self))]
    pub async fn list_projects(&self, workspace_id: Uuid) -> Result<Vec<Project>, sqlx::Error> {
        let rows = sqlx::query_as::<_, ProjectRow>(
            "SELECT id, workspace_id, name, description, project_type, vocabulary, workflow, archived, created_at, updated_at
             FROM projects WHERE workspace_id = ? AND archived = 0 ORDER BY updated_at DESC"
        )
        .bind(workspace_id.to_string())
        .fetch_all(self.pool())
        .await?;

        Ok(rows.into_iter().map(|r| r.into_project()).collect())
    }

    #[instrument(skip(self))]
    pub async fn update_project(
        &self,
        id: Uuid,
        input: UpdateProject,
    ) -> Result<Option<Project>, sqlx::Error> {
        let now = Utc::now().to_rfc3339();

        if let Some(ref name) = input.name {
            sqlx::query("UPDATE projects SET name = ?, updated_at = ? WHERE id = ?")
                .bind(name)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        if let Some(ref desc) = input.description {
            sqlx::query("UPDATE projects SET description = ?, updated_at = ? WHERE id = ?")
                .bind(desc)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        if let Some(ref vocab) = input.vocabulary {
            let json = serde_json::to_string(vocab).unwrap();
            sqlx::query("UPDATE projects SET vocabulary = ?, updated_at = ? WHERE id = ?")
                .bind(&json)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        if let Some(ref workflow) = input.workflow {
            let json = serde_json::to_string(workflow).unwrap();
            sqlx::query("UPDATE projects SET workflow = ?, updated_at = ? WHERE id = ?")
                .bind(&json)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        if let Some(archived) = input.archived {
            sqlx::query("UPDATE projects SET archived = ?, updated_at = ? WHERE id = ?")
                .bind(archived as i32)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }

        self.get_project(id).await
    }

    #[instrument(skip(self))]
    pub async fn delete_project(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM projects WHERE id = ?")
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

// ─── Row mapping ─────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct ProjectRow {
    id: String,
    workspace_id: String,
    name: String,
    description: Option<String>,
    project_type: String,
    vocabulary: String,
    workflow: String,
    archived: i32,
    created_at: String,
    updated_at: String,
}

impl ProjectRow {
    fn into_project(self) -> Project {
        Project {
            id: Uuid::parse_str(&self.id).unwrap(),
            workspace_id: Uuid::parse_str(&self.workspace_id).unwrap(),
            name: self.name,
            description: self.description,
            project_type: serde_json::from_str(&format!("\"{}\"", self.project_type))
                .unwrap_or(ProjectType::Custom),
            vocabulary: serde_json::from_str(&self.vocabulary).unwrap_or_default(),
            workflow: serde_json::from_str(&self.workflow)
                .unwrap_or_else(|_| tack_core::workflow::simple_workflow()),
            created_at: chrono::DateTime::parse_from_rfc3339(&self.created_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&self.updated_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            archived: self.archived != 0,
        }
    }
}
