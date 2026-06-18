pub mod attachments;
pub mod boards;
pub mod comments;
pub mod custom_fields;
pub mod dependencies;
pub mod items;
pub mod projects;
pub mod roles;
pub mod sprints;
pub mod templates;

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

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    // ────────────────────────────────────────────────────────────────────────────────
    // Board Methods
    // ────────────────────────────────────────────────────────────────────────────────

    pub async fn create_board(
        &self,
        project_id: Uuid,
        data: CreateBoard,
    ) -> Result<Board, sqlx::Error> {
        boards::create_board(self.pool(), project_id, data).await
    }

    pub async fn get_board(&self, id: Uuid) -> Result<Board, sqlx::Error> {
        boards::get_board(self.pool(), id).await
    }

    pub async fn get_default_board(&self, project_id: Uuid) -> Result<Option<Board>, sqlx::Error> {
        boards::get_default_board(self.pool(), project_id).await
    }

    pub async fn list_boards(&self, project_id: Uuid) -> Result<Vec<Board>, sqlx::Error> {
        boards::list_boards(self.pool(), project_id).await
    }

    pub async fn update_board(&self, id: Uuid, data: UpdateBoard) -> Result<Board, sqlx::Error> {
        boards::update_board(self.pool(), id, data).await
    }

    pub async fn delete_board(&self, id: Uuid) -> Result<(), sqlx::Error> {
        boards::delete_board(self.pool(), id).await
    }

    // ────────────────────────────────────────────────────────────────────────────────
    // Template Methods
    // ────────────────────────────────────────────────────────────────────────────────

    pub async fn create_template(
        &self,
        data: CreateProjectTemplate,
    ) -> Result<ProjectTemplate, sqlx::Error> {
        templates::create_template(self.pool(), data).await
    }

    pub async fn get_template(&self, id: Uuid) -> Result<ProjectTemplate, sqlx::Error> {
        templates::get_template(self.pool(), id).await
    }

    pub async fn list_templates(
        &self,
        project_type: Option<ProjectType>,
    ) -> Result<Vec<ProjectTemplate>, sqlx::Error> {
        templates::list_templates(self.pool(), project_type).await
    }

    pub async fn delete_template(&self, id: Uuid) -> Result<(), sqlx::Error> {
        templates::delete_template(self.pool(), id).await
    }

    // ────────────────────────────────────────────────────────────────────────────────
    // Custom Field Methods
    // ────────────────────────────────────────────────────────────────────────────────

    pub async fn create_field(
        &self,
        project_id: Uuid,
        data: CreateCustomField,
    ) -> Result<CustomFieldDefinition, sqlx::Error> {
        custom_fields::create_field(self.pool(), project_id, data).await
    }

    pub async fn get_field(&self, id: Uuid) -> Result<CustomFieldDefinition, sqlx::Error> {
        custom_fields::get_field(self.pool(), id).await
    }

    pub async fn list_fields_for_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<CustomFieldDefinition>, sqlx::Error> {
        custom_fields::list_fields_for_project(self.pool(), project_id).await
    }

    pub async fn update_field(
        &self,
        id: Uuid,
        data: UpdateCustomField,
    ) -> Result<CustomFieldDefinition, sqlx::Error> {
        custom_fields::update_field(self.pool(), id, data).await
    }

    pub async fn delete_field(&self, id: Uuid) -> Result<(), sqlx::Error> {
        custom_fields::delete_field(self.pool(), id).await
    }

    pub async fn set_field_value(
        &self,
        item_id: Uuid,
        field_id: Uuid,
        value: serde_json::Value,
    ) -> Result<CustomFieldValue, sqlx::Error> {
        custom_fields::set_field_value(self.pool(), item_id, field_id, value).await
    }

    pub async fn get_field_value(
        &self,
        item_id: Uuid,
        field_id: Uuid,
    ) -> Result<CustomFieldValue, sqlx::Error> {
        custom_fields::get_field_value(self.pool(), item_id, field_id).await
    }

    pub async fn get_all_field_values_for_item(
        &self,
        item_id: Uuid,
    ) -> Result<Vec<CustomFieldValue>, sqlx::Error> {
        custom_fields::get_all_field_values_for_item(self.pool(), item_id).await
    }

    pub async fn delete_field_value(
        &self,
        item_id: Uuid,
        field_id: Uuid,
    ) -> Result<(), sqlx::Error> {
        custom_fields::delete_field_value(self.pool(), item_id, field_id).await
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
    ) -> Result<Attachment, sqlx::Error> {
        let repo = attachments::AttachmentRepo::new(self.pool());
        repo.create_attachment(item_id, filename, mime_type, storage_path, size_bytes)
            .await
    }

    pub async fn get_attachment(&self, id: Uuid) -> Result<Option<Attachment>, sqlx::Error> {
        let repo = attachments::AttachmentRepo::new(self.pool());
        repo.get_attachment(id).await
    }

    pub async fn list_attachments(&self, item_id: Uuid) -> Result<Vec<Attachment>, sqlx::Error> {
        let repo = attachments::AttachmentRepo::new(self.pool());
        repo.list_attachments(item_id).await
    }

    pub async fn delete_attachment(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let repo = attachments::AttachmentRepo::new(self.pool());
        repo.delete_attachment(id).await
    }
}
