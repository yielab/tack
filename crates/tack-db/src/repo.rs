pub mod attachments;
pub mod autonomy;
pub mod backup;
pub mod boards;
pub mod briefs;
pub mod budgets;
pub mod comments;
pub mod custom_fields;
pub mod dependencies;
pub mod execution;
pub mod github_links;
pub mod items;
pub mod metrics;
pub mod prill_records;
pub mod projects;
pub mod pull_requests;
pub mod rate_card;
pub mod roles;
pub mod sprints;
pub mod templates;
pub mod workspaces;

use crate::StoreError;
use sqlx::SqlitePool;
use tack_core::models::*;
use uuid::Uuid;

/// Central repository providing access to all data operations.
/// Holds a reference to the database pool.
#[derive(Clone)]
pub struct Repository {
    pool: SqlitePool,
}

impl Repository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The project a write names must be there: `NotFound { entity: "project" }` when it is not,
    /// so a write whose foreign key would point at nothing answers 404 and never the database's
    /// constraint error as a 500.
    pub(crate) async fn require_project(&self, id: Uuid) -> Result<(), StoreError> {
        let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM projects WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        found.map(|_| ()).ok_or(StoreError::not_found("project"))
    }

    /// The item a write names must be there: `NotFound { entity: "item" }` when it is not (see
    /// [`Repository::require_project`]).
    pub(crate) async fn require_item(&self, id: Uuid) -> Result<(), StoreError> {
        let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM items WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        found.map(|_| ()).ok_or(StoreError::not_found("item"))
    }

    /// The raw connection pool, for the seeds and assertions of `tack-test-support`.
    /// Only that crate turns the `raw-pool` feature on; cargo unifies features across a
    /// build, so the feature alone does not keep other crates out, and the real gate is
    /// `scripts/check-crate-graph.sh` and the ban rules in `deny.toml`.
    #[cfg(feature = "raw-pool")]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    #[cfg(not(feature = "raw-pool"))]
    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    // ────────────────────────────────────────────────────────────────────────────────
    // GitHub link methods (status and comment sync, both directions)
    // ────────────────────────────────────────────────────────────────────────────────

    pub async fn set_github_link(
        &self,
        item_id: Uuid,
        repo: &str,
        issue_number: i64,
    ) -> Result<(), StoreError> {
        github_links::set_link(self.pool(), item_id, repo, issue_number).await
    }

    pub async fn get_github_link(
        &self,
        item_id: Uuid,
    ) -> Result<Option<(String, i64)>, StoreError> {
        github_links::get_link(self.pool(), item_id).await
    }

    pub async fn remove_github_link(&self, item_id: Uuid) -> Result<(), StoreError> {
        github_links::remove_link(self.pool(), item_id).await
    }

    // ────────────────────────────────────────────────────────────────────────────────
    // Board Methods
    // ────────────────────────────────────────────────────────────────────────────────

    pub async fn create_board(
        &self,
        project_id: Uuid,
        data: CreateBoard,
    ) -> Result<Board, StoreError> {
        boards::create_board(self, project_id, data).await
    }

    pub async fn get_board(&self, id: Uuid) -> Result<Board, StoreError> {
        boards::get_board(self, id).await
    }

    pub async fn get_default_board(&self, project_id: Uuid) -> Result<Option<Board>, StoreError> {
        boards::get_default_board(self, project_id).await
    }

    pub async fn list_boards(&self, project_id: Uuid) -> Result<Vec<Board>, StoreError> {
        boards::list_boards(self, project_id).await
    }

    pub async fn update_board(&self, id: Uuid, data: UpdateBoard) -> Result<Board, StoreError> {
        boards::update_board(self, id, data).await
    }

    pub async fn delete_board(&self, id: Uuid) -> Result<(), StoreError> {
        boards::delete_board(self, id).await
    }

    // ────────────────────────────────────────────────────────────────────────────────
    // Template Methods
    // ────────────────────────────────────────────────────────────────────────────────

    pub async fn create_template(
        &self,
        data: CreateProjectTemplate,
    ) -> Result<ProjectTemplate, StoreError> {
        templates::create_template(self, data).await
    }

    pub async fn get_template(&self, id: Uuid) -> Result<ProjectTemplate, StoreError> {
        templates::get_template(self, id).await
    }

    pub async fn list_templates(
        &self,
        project_type: Option<ProjectType>,
    ) -> Result<Vec<ProjectTemplate>, StoreError> {
        templates::list_templates(self, project_type).await
    }

    pub async fn delete_template(&self, id: Uuid) -> Result<(), StoreError> {
        templates::delete_template(self, id).await
    }

    // ────────────────────────────────────────────────────────────────────────────────
    // Custom Field Methods
    // ────────────────────────────────────────────────────────────────────────────────

    pub async fn create_field(
        &self,
        project_id: Uuid,
        data: CreateCustomField,
    ) -> Result<CustomFieldDefinition, StoreError> {
        custom_fields::create_field(self, project_id, data).await
    }

    pub async fn get_field(&self, id: Uuid) -> Result<CustomFieldDefinition, StoreError> {
        custom_fields::get_field(self, id).await
    }

    pub async fn list_fields_for_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<CustomFieldDefinition>, StoreError> {
        custom_fields::list_fields_for_project(self, project_id).await
    }

    pub async fn update_field(
        &self,
        id: Uuid,
        data: UpdateCustomField,
    ) -> Result<CustomFieldDefinition, StoreError> {
        custom_fields::update_field(self, id, data).await
    }

    pub async fn delete_field(&self, id: Uuid) -> Result<(), StoreError> {
        custom_fields::delete_field(self, id).await
    }

    pub async fn set_field_value(
        &self,
        item_id: Uuid,
        field_id: Uuid,
        value: serde_json::Value,
    ) -> Result<CustomFieldValue, StoreError> {
        custom_fields::set_field_value(self, item_id, field_id, value).await
    }

    pub async fn get_field_value(
        &self,
        item_id: Uuid,
        field_id: Uuid,
    ) -> Result<CustomFieldValue, StoreError> {
        custom_fields::get_field_value(self, item_id, field_id).await
    }

    pub async fn get_all_field_values_for_item(
        &self,
        item_id: Uuid,
    ) -> Result<Vec<CustomFieldValue>, StoreError> {
        custom_fields::get_all_field_values_for_item(self, item_id).await
    }

    pub async fn field_values_for_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<CustomFieldValue>, StoreError> {
        custom_fields::field_values_for_project(self, project_id).await
    }

    pub async fn delete_field_value(
        &self,
        item_id: Uuid,
        field_id: Uuid,
    ) -> Result<(), StoreError> {
        custom_fields::delete_field_value(self, item_id, field_id).await
    }

    // ────────────────────────────────────────────────────────────────────────────────
    // Attachment Methods
    // ────────────────────────────────────────────────────────────────────────────────

    pub async fn create_attachment(
        &self,
        item_id: Uuid,
        filename: String,
        mime_type: String,
        storage_path: String,
        size_bytes: u64,
    ) -> Result<Attachment, StoreError> {
        let repo = attachments::AttachmentRepo::new(self);
        repo.create_attachment(item_id, filename, mime_type, storage_path, size_bytes)
            .await
    }

    pub async fn get_attachment(&self, id: Uuid) -> Result<Option<Attachment>, StoreError> {
        let repo = attachments::AttachmentRepo::new(self);
        repo.get_attachment(id).await
    }

    pub async fn list_attachments(&self, item_id: Uuid) -> Result<Vec<Attachment>, StoreError> {
        let repo = attachments::AttachmentRepo::new(self);
        repo.list_attachments(item_id).await
    }

    pub async fn delete_attachment(&self, id: Uuid) -> Result<bool, StoreError> {
        let repo = attachments::AttachmentRepo::new(self);
        repo.delete_attachment(id).await
    }
}
