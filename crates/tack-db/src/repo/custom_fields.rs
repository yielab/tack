use crate::Repository;
use crate::{StoreError, decode};
use sqlx::FromRow;
use tack_core::models::*;
use tracing::instrument;
use uuid::Uuid;

#[derive(FromRow)]
struct CustomFieldRow {
    id: String,
    project_id: Option<String>,
    name: String,
    field_type: String,
    description: Option<String>,
    required: i32,
    default_value: Option<String>,
    options: Option<String>,
    validation: Option<String>,
    created_at: String,
    updated_at: String,
}

impl CustomFieldRow {
    fn into_definition(self) -> Result<CustomFieldDefinition, StoreError> {
        const TABLE: &str = "custom_field_definitions";
        Ok(CustomFieldDefinition {
            id: decode::uuid(TABLE, "id", &self.id)?,
            project_id: decode::optional_uuid(TABLE, "project_id", self.project_id.as_deref())?,
            name: self.name,
            field_type: decode::variant(TABLE, "field_type", &self.field_type)?,
            description: self.description,
            required: self.required != 0,
            default_value: decode::optional_json(
                TABLE,
                "default_value",
                self.default_value.as_deref(),
            )?,
            options: decode::optional_json(TABLE, "options", self.options.as_deref())?,
            validation: decode::optional_json(TABLE, "validation", self.validation.as_deref())?,
            created_at: decode::instant(TABLE, "created_at", &self.created_at)?,
            updated_at: decode::instant(TABLE, "updated_at", &self.updated_at)?,
        })
    }
}

#[derive(FromRow)]
struct CustomFieldValueRow {
    id: String,
    item_id: String,
    field_id: String,
    value: String,
    created_at: String,
    updated_at: String,
}

impl CustomFieldValueRow {
    fn into_value(self) -> Result<CustomFieldValue, StoreError> {
        const TABLE: &str = "custom_field_values";
        Ok(CustomFieldValue {
            id: decode::uuid(TABLE, "id", &self.id)?,
            item_id: decode::uuid(TABLE, "item_id", &self.item_id)?,
            field_id: decode::uuid(TABLE, "field_id", &self.field_id)?,
            value: decode::json(TABLE, "value", &self.value)?,
            created_at: decode::instant(TABLE, "created_at", &self.created_at)?,
            updated_at: decode::instant(TABLE, "updated_at", &self.updated_at)?,
        })
    }
}

/// Create a custom field definition for a project
#[instrument(skip(repo))]
pub async fn create_field(
    repo: &Repository,
    project_id: Uuid,
    data: CreateCustomField,
) -> Result<CustomFieldDefinition, StoreError> {
    let id = Uuid::new_v4();
    let now = chrono::Utc::now();

    let field_type_str = match data.field_type {
        CustomFieldType::Text => "text",
        CustomFieldType::Number => "number",
        CustomFieldType::Date => "date",
        CustomFieldType::Boolean => "boolean",
        CustomFieldType::Select => "select",
        CustomFieldType::MultiSelect => "multi_select",
        CustomFieldType::Url => "url",
        CustomFieldType::Email => "email",
        CustomFieldType::LongText => "long_text",
    };

    let default_value = data.default_value.map(|v| v.to_string());
    let options = data
        .options
        .map(|opts| decode::to_json(&opts))
        .transpose()?;
    let validation = data.validation.map(|v| v.to_string());

    sqlx::query(
        "INSERT INTO custom_field_definitions
         (id, project_id, name, field_type, description, required, default_value, options, validation, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(id.to_string())
    .bind(project_id.to_string())
    .bind(&data.name)
    .bind(field_type_str)
    .bind(&data.description)
    .bind(data.required.unwrap_or(false) as i32)
    .bind(default_value)
    .bind(options)
    .bind(validation)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(repo.pool())
    .await?;

    get_field(repo, id).await
}

/// Get a custom field definition by ID
#[instrument(skip(repo))]
pub async fn get_field(repo: &Repository, id: Uuid) -> Result<CustomFieldDefinition, StoreError> {
    let row = sqlx::query_as::<_, CustomFieldRow>(
        "SELECT id, project_id, name, field_type, description, required, default_value, options, validation, created_at, updated_at
         FROM custom_field_definitions
         WHERE id = ?"
    )
    .bind(id.to_string())
    .fetch_optional(repo.pool())
    .await?
    .ok_or(StoreError::not_found("custom field"))?;

    row.into_definition()
}

/// List all custom fields for a project
#[instrument(skip(repo))]
pub async fn list_fields_for_project(
    repo: &Repository,
    project_id: Uuid,
) -> Result<Vec<CustomFieldDefinition>, StoreError> {
    let rows = sqlx::query_as::<_, CustomFieldRow>(
        "SELECT id, project_id, name, field_type, description, required, default_value, options, validation, created_at, updated_at
         FROM custom_field_definitions
         WHERE project_id = ?
         ORDER BY name ASC"
    )
    .bind(project_id.to_string())
    .fetch_all(repo.pool())
    .await?;

    rows.into_iter()
        .map(CustomFieldRow::into_definition)
        .collect()
}

/// Update a custom field definition
#[instrument(skip(repo))]
pub async fn update_field(
    repo: &Repository,
    id: Uuid,
    data: UpdateCustomField,
) -> Result<CustomFieldDefinition, StoreError> {
    let now = chrono::Utc::now();

    if let Some(name) = &data.name {
        sqlx::query("UPDATE custom_field_definitions SET name = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(now.to_rfc3339())
            .bind(id.to_string())
            .execute(repo.pool())
            .await?;
    }

    if let Some(description) = &data.description {
        sqlx::query(
            "UPDATE custom_field_definitions SET description = ?, updated_at = ? WHERE id = ?",
        )
        .bind(description)
        .bind(now.to_rfc3339())
        .bind(id.to_string())
        .execute(repo.pool())
        .await?;
    }

    if let Some(required) = data.required {
        sqlx::query(
            "UPDATE custom_field_definitions SET required = ?, updated_at = ? WHERE id = ?",
        )
        .bind(required as i32)
        .bind(now.to_rfc3339())
        .bind(id.to_string())
        .execute(repo.pool())
        .await?;
    }

    if let Some(default_value) = data.default_value {
        let value_str = default_value.to_string();
        sqlx::query(
            "UPDATE custom_field_definitions SET default_value = ?, updated_at = ? WHERE id = ?",
        )
        .bind(value_str)
        .bind(now.to_rfc3339())
        .bind(id.to_string())
        .execute(repo.pool())
        .await?;
    }

    if let Some(options) = data.options {
        let options_str = decode::to_json(&options)?;
        sqlx::query("UPDATE custom_field_definitions SET options = ?, updated_at = ? WHERE id = ?")
            .bind(options_str)
            .bind(now.to_rfc3339())
            .bind(id.to_string())
            .execute(repo.pool())
            .await?;
    }

    if let Some(validation) = data.validation {
        let validation_str = validation.to_string();
        sqlx::query(
            "UPDATE custom_field_definitions SET validation = ?, updated_at = ? WHERE id = ?",
        )
        .bind(validation_str)
        .bind(now.to_rfc3339())
        .bind(id.to_string())
        .execute(repo.pool())
        .await?;
    }

    get_field(repo, id).await
}

/// Delete a custom field definition
#[instrument(skip(repo))]
pub async fn delete_field(repo: &Repository, id: Uuid) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM custom_field_definitions WHERE id = ?")
        .bind(id.to_string())
        .execute(repo.pool())
        .await?;
    Ok(())
}

// ─── Custom Field Values ─────────────────────────────────────

/// Set a custom field value for an item
#[instrument(skip(repo))]
pub async fn set_field_value(
    repo: &Repository,
    item_id: Uuid,
    field_id: Uuid,
    value: serde_json::Value,
) -> Result<CustomFieldValue, StoreError> {
    let id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let value_str = value.to_string();

    // Upsert: insert or replace
    sqlx::query(
        "INSERT INTO custom_field_values (id, item_id, field_id, value, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(item_id, field_id) DO UPDATE SET value = ?, updated_at = ?",
    )
    .bind(id.to_string())
    .bind(item_id.to_string())
    .bind(field_id.to_string())
    .bind(&value_str)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .bind(&value_str)
    .bind(now.to_rfc3339())
    .execute(repo.pool())
    .await?;

    get_field_value(repo, item_id, field_id).await
}

/// Get a custom field value for an item
#[instrument(skip(repo))]
pub async fn get_field_value(
    repo: &Repository,
    item_id: Uuid,
    field_id: Uuid,
) -> Result<CustomFieldValue, StoreError> {
    let row = sqlx::query_as::<_, CustomFieldValueRow>(
        "SELECT id, item_id, field_id, value, created_at, updated_at
         FROM custom_field_values
         WHERE item_id = ? AND field_id = ?",
    )
    .bind(item_id.to_string())
    .bind(field_id.to_string())
    .fetch_optional(repo.pool())
    .await?
    .ok_or(StoreError::not_found("custom field value"))?;

    row.into_value()
}

/// Get all custom field values for an item
#[instrument(skip(repo))]
pub async fn get_all_field_values_for_item(
    repo: &Repository,
    item_id: Uuid,
) -> Result<Vec<CustomFieldValue>, StoreError> {
    let rows = sqlx::query_as::<_, CustomFieldValueRow>(
        "SELECT id, item_id, field_id, value, created_at, updated_at
         FROM custom_field_values
         WHERE item_id = ?",
    )
    .bind(item_id.to_string())
    .fetch_all(repo.pool())
    .await?;

    rows.into_iter()
        .map(CustomFieldValueRow::into_value)
        .collect()
}

/// Delete a custom field value
#[instrument(skip(repo))]
pub async fn delete_field_value(
    repo: &Repository,
    item_id: Uuid,
    field_id: Uuid,
) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM custom_field_values WHERE item_id = ? AND field_id = ?")
        .bind(item_id.to_string())
        .bind(field_id.to_string())
        .execute(repo.pool())
        .await?;
    Ok(())
}

/// Every value of every item of the project.
#[instrument(skip(repo))]
pub async fn field_values_for_project(
    repo: &Repository,
    project_id: Uuid,
) -> Result<Vec<CustomFieldValue>, StoreError> {
    let rows = sqlx::query_as::<_, CustomFieldValueRow>(
        "SELECT v.id, v.item_id, v.field_id, v.value, v.created_at, v.updated_at
         FROM custom_field_values v JOIN items i ON i.id = v.item_id
         WHERE i.project_id = ?
         ORDER BY v.created_at, v.id",
    )
    .bind(project_id.to_string())
    .fetch_all(repo.pool())
    .await?;

    rows.into_iter()
        .map(CustomFieldValueRow::into_value)
        .collect()
}
