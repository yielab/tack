use crate::{StoreError, decode};
use chrono::Utc;
use tracing::{debug, instrument};
use uuid::Uuid;

use tack_core::auto_start::DROPPED_TO_L3;
use tack_core::models::{CreateProject, Project, UpdateProject};
use tack_core::vocabulary::vocabulary_for_type;
use tack_core::workflow::workflow_for_type;

use super::Repository;

impl Repository {
    #[instrument(skip(self))]
    pub async fn create_project(
        &self,
        workspace_id: Uuid,
        input: CreateProject,
    ) -> Result<Project, StoreError> {
        let id = Uuid::new_v4();
        let now = Utc::now();
        let vocabulary = vocabulary_for_type(&input.project_type);
        let workflow = workflow_for_type(&input.project_type);
        let vocab_json = decode::to_json(&vocabulary)?;
        let workflow_json = decode::to_json(&workflow)?;
        let project_type_str = input.project_type.to_string();
        let now_str = now.to_rfc3339();
        let code_origin = input.code_origin.unwrap_or_default();
        let workspace_mode = input.workspace_mode.unwrap_or_default();

        sqlx::query(
            "INSERT INTO projects (id, workspace_id, name, description, project_type, vocabulary, workflow, created_at, updated_at,
                                   code_origin, repository, default_branch, workspace_mode)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
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
        .bind(code_origin.to_string())
        .bind(&input.repository)
        .bind(&input.default_branch)
        .bind(workspace_mode.to_string())
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
            default_model: None,
            // Left NULL: a fresh project has no project-level token
            // reference until an operator sets one via `update_project`.
            github_token_ref: None,
            created_at: now,
            updated_at: now,
            archived: false,
            code_origin,
            repository: input.repository,
            default_branch: input.default_branch,
            workspace_mode,
            push_after_run: false,
            default_harness: None,
            default_profile_id: None,
            on_finish_status: None,
            definition_of_done: None,
            auto_start_enabled: false,
            auto_start_status: None,
            auto_start_max_open: 3,
            prepare_command: None,
            auto_start_disabled_reason: None,
        })
    }

    #[instrument(skip(self))]
    pub async fn get_project(&self, id: Uuid) -> Result<Option<Project>, StoreError> {
        let row = sqlx::query_as::<_, ProjectRow>(
            "SELECT id, workspace_id, name, description, project_type, vocabulary, workflow, default_model, github_token_ref, archived, created_at, updated_at,
                    code_origin, repository, default_branch, workspace_mode, push_after_run, default_harness, default_profile_id, on_finish_status, definition_of_done,
                    auto_start_enabled, auto_start_status, auto_start_max_open, prepare_command,
                    CASE WHEN auto_start_enabled = 0 AND autonomy_level = 'L3' AND EXISTS (
                        SELECT 1 FROM comments c JOIN items i ON i.id = c.item_id
                        WHERE i.project_id = projects.id AND c.content = ?
                    ) THEN ? END AS auto_start_disabled_reason
             FROM projects WHERE id = ?"
        )
        .bind(DROPPED_TO_L3)
        .bind(DROPPED_TO_L3)
        .bind(id.to_string())
        .fetch_optional(self.pool())
        .await?;

        row.map(|r| r.into_project()).transpose()
    }

    #[instrument(skip(self))]
    pub async fn list_projects(&self, workspace_id: Uuid) -> Result<Vec<Project>, StoreError> {
        let rows = sqlx::query_as::<_, ProjectRow>(
            "SELECT id, workspace_id, name, description, project_type, vocabulary, workflow, default_model, github_token_ref, archived, created_at, updated_at,
                    code_origin, repository, default_branch, workspace_mode, push_after_run, default_harness, default_profile_id, on_finish_status, definition_of_done,
                    auto_start_enabled, auto_start_status, auto_start_max_open, prepare_command,
                    CASE WHEN auto_start_enabled = 0 AND autonomy_level = 'L3' AND EXISTS (
                        SELECT 1 FROM comments c JOIN items i ON i.id = c.item_id
                        WHERE i.project_id = projects.id AND c.content = ?
                    ) THEN ? END AS auto_start_disabled_reason
             FROM projects WHERE workspace_id = ? AND archived = 0 ORDER BY updated_at DESC"
        )
        .bind(DROPPED_TO_L3)
        .bind(DROPPED_TO_L3)
        .bind(workspace_id.to_string())
        .fetch_all(self.pool())
        .await?;

        rows.into_iter().map(|r| r.into_project()).collect()
    }

    #[instrument(skip(self))]
    pub async fn update_project(
        &self,
        id: Uuid,
        input: UpdateProject,
    ) -> Result<Option<Project>, StoreError> {
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
            let json = decode::to_json(vocab)?;
            sqlx::query("UPDATE projects SET vocabulary = ?, updated_at = ? WHERE id = ?")
                .bind(&json)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        if let Some(ref workflow) = input.workflow {
            let json = decode::to_json(workflow)?;
            sqlx::query("UPDATE projects SET workflow = ?, updated_at = ? WHERE id = ?")
                .bind(&json)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        if let Some(ref token_ref) = input.github_token_ref {
            // The empty string is how the client spells "clear" for this
            // plain (non-double-`Option`) field — stored as SQL NULL, never
            // as a literal empty string. See `UpdateProject::github_token_ref`'s
            // doc comment.
            let value: Option<&str> = if token_ref.is_empty() {
                None
            } else {
                Some(token_ref.as_str())
            };
            sqlx::query("UPDATE projects SET github_token_ref = ?, updated_at = ? WHERE id = ?")
                .bind(value)
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

        if let Some(push_after_run) = input.push_after_run {
            sqlx::query("UPDATE projects SET push_after_run = ?, updated_at = ? WHERE id = ?")
                .bind(push_after_run as i32)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
        }
        for (column, value) in [
            (
                "auto_start_enabled",
                input.auto_start_enabled.map(i64::from),
            ),
            ("auto_start_max_open", input.auto_start_max_open),
        ] {
            if let Some(value) = value {
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "UPDATE projects SET {column} = ?, updated_at = ? WHERE id = ?"
                )))
                .bind(value)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
            }
        }
        // A new default agent starts without the old one's model.
        if let Some(ref harness) = input.default_harness {
            sqlx::query(
                "UPDATE projects SET default_model = NULL, updated_at = ? \
                 WHERE id = ? AND default_harness IS NOT ?",
            )
            .bind(&now)
            .bind(id.to_string())
            .bind(harness.as_deref())
            .execute(self.pool())
            .await?;
        }
        // Each column name is a literal here, never client input.
        for (column, value) in [
            (
                "code_origin",
                input.code_origin.map(|v| Some(v.to_string())),
            ),
            (
                "workspace_mode",
                input.workspace_mode.map(|v| Some(v.to_string())),
            ),
            ("repository", input.repository),
            ("default_branch", input.default_branch),
            ("default_harness", input.default_harness),
            ("default_profile_id", input.default_profile_id),
            ("on_finish_status", input.on_finish_status),
            ("definition_of_done", input.definition_of_done),
            ("default_model", input.default_model),
            ("auto_start_status", input.auto_start_status),
            ("prepare_command", input.prepare_command),
        ] {
            if let Some(value) = value {
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "UPDATE projects SET {column} = ?, updated_at = ? WHERE id = ?"
                )))
                .bind(value)
                .bind(&now)
                .bind(id.to_string())
                .execute(self.pool())
                .await?;
            }
        }

        self.get_project(id).await
    }

    #[instrument(skip(self))]
    pub async fn delete_project(&self, id: Uuid) -> Result<bool, StoreError> {
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
    default_model: Option<String>,
    github_token_ref: Option<String>,
    archived: i32,
    created_at: String,
    updated_at: String,
    code_origin: String,
    repository: Option<String>,
    default_branch: Option<String>,
    workspace_mode: String,
    push_after_run: i32,
    default_harness: Option<String>,
    default_profile_id: Option<String>,
    on_finish_status: Option<String>,
    definition_of_done: Option<String>,
    auto_start_enabled: i32,
    auto_start_status: Option<String>,
    auto_start_max_open: i64,
    prepare_command: Option<String>,
    auto_start_disabled_reason: Option<String>,
}

impl ProjectRow {
    fn into_project(self) -> Result<Project, StoreError> {
        const TABLE: &str = "projects";
        Ok(Project {
            id: decode::uuid(TABLE, "id", &self.id)?,
            workspace_id: decode::uuid(TABLE, "workspace_id", &self.workspace_id)?,
            name: self.name,
            description: self.description,
            project_type: decode::variant(TABLE, "project_type", &self.project_type)?,
            vocabulary: decode::json(TABLE, "vocabulary", &self.vocabulary)?,
            workflow: decode::json(TABLE, "workflow", &self.workflow)?,
            default_model: self.default_model,
            github_token_ref: self.github_token_ref,
            created_at: decode::instant(TABLE, "created_at", &self.created_at)?,
            updated_at: decode::instant(TABLE, "updated_at", &self.updated_at)?,
            archived: self.archived != 0,
            code_origin: decode::variant(TABLE, "code_origin", &self.code_origin)?,
            repository: self.repository,
            default_branch: self.default_branch,
            workspace_mode: decode::variant(TABLE, "workspace_mode", &self.workspace_mode)?,
            push_after_run: self.push_after_run != 0,
            default_harness: self.default_harness,
            default_profile_id: self.default_profile_id,
            on_finish_status: self.on_finish_status,
            definition_of_done: self.definition_of_done,
            auto_start_enabled: self.auto_start_enabled != 0,
            auto_start_status: self.auto_start_status,
            auto_start_max_open: self.auto_start_max_open,
            prepare_command: self.prepare_command,
            auto_start_disabled_reason: self.auto_start_disabled_reason,
        })
    }
}

impl Repository {
    /// Turn the project's auto-start off. Whether it was on.
    pub async fn disable_auto_start(&self, project_id: &str) -> Result<bool, crate::StoreError> {
        let done = sqlx::query(
            "UPDATE projects SET auto_start_enabled = 0 WHERE id = ? AND auto_start_enabled = 1",
        )
        .bind(project_id)
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }
}

impl Repository {
    /// The id of the board's first workspace, made (named "Default Workspace") when there is none.
    pub async fn first_workspace_or_create(&self) -> Result<Uuid, crate::StoreError> {
        let existing: Option<String> = sqlx::query_scalar("SELECT id FROM workspaces LIMIT 1")
            .fetch_optional(self.pool())
            .await?;
        if let Some(id) = existing {
            return decode::uuid("workspaces", "id", &id);
        }
        let id = Uuid::new_v4();
        let now = Utc::now();
        sqlx::query(
            "INSERT INTO workspaces (id, name, description, default_vocabulary, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind("Default Workspace")
        .bind("Auto-created default workspace")
        .bind("{}")
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(self.pool())
        .await?;
        Ok(id)
    }
}
