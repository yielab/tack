use flexpm_core::models::*;
use sqlx::{FromRow, SqlitePool};
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

#[derive(FromRow)]
struct CustomFieldValueRow {
    id: String,
    item_id: String,
    field_id: String,
    value: String,
    created_at: String,
    updated_at: String,
}

/// Create a custom field definition for a project
#[instrument(skip(pool))]
pub async fn create_field(
    pool: &SqlitePool,
    project_id: Uuid,
    data: CreateCustomField,
) -> Result<CustomFieldDefinition, sqlx::Error> {
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
    let options = data.options.map(|opts| serde_json::to_string(&opts).unwrap());
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
    .execute(pool)
    .await?;

    get_field(pool, id).await
}

/// Get a custom field definition by ID
#[instrument(skip(pool))]
pub async fn get_field(
    pool: &SqlitePool,
    id: Uuid,
) -> Result<CustomFieldDefinition, sqlx::Error> {
    let row = sqlx::query_as::<_, CustomFieldRow>(
        "SELECT id, project_id, name, field_type, description, required, default_value, options, validation, created_at, updated_at
         FROM custom_field_definitions
         WHERE id = ?"
    )
    .bind(id.to_string())
    .fetch_one(pool)
    .await?;

    let field_type = match row.field_type.as_str() {
        "text" => CustomFieldType::Text,
        "number" => CustomFieldType::Number,
        "date" => CustomFieldType::Date,
        "boolean" => CustomFieldType::Boolean,
        "select" => CustomFieldType::Select,
        "multi_select" => CustomFieldType::MultiSelect,
        "url" => CustomFieldType::Url,
        "email" => CustomFieldType::Email,
        "long_text" => CustomFieldType::LongText,
        _ => CustomFieldType::Text,
    };

    let default_value = row.default_value.as_ref().and_then(|v| serde_json::from_str(v).ok());
    let options = row.options.as_ref().and_then(|o| serde_json::from_str(o).ok());
    let validation = row.validation.as_ref().and_then(|v| serde_json::from_str(v).ok());

    Ok(CustomFieldDefinition {
        id: Uuid::parse_str(&row.id).unwrap(),
        project_id: row.project_id.as_ref().map(|pid| Uuid::parse_str(pid).unwrap()),
        name: row.name,
        field_type,
        description: row.description,
        required: row.required != 0,
        default_value,
        options,
        validation,
        created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at).unwrap().into(),
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at).unwrap().into(),
    })
}

/// List all custom fields for a project
#[instrument(skip(pool))]
pub async fn list_fields_for_project(
    pool: &SqlitePool,
    project_id: Uuid,
) -> Result<Vec<CustomFieldDefinition>, sqlx::Error> {
    let rows = sqlx::query_as::<_, CustomFieldRow>(
        "SELECT id, project_id, name, field_type, description, required, default_value, options, validation, created_at, updated_at
         FROM custom_field_definitions
         WHERE project_id = ?
         ORDER BY name ASC"
    )
    .bind(project_id.to_string())
    .fetch_all(pool)
    .await?;

    let fields = rows.into_iter().map(|row| {
        let field_type = match row.field_type.as_str() {
            "text" => CustomFieldType::Text,
            "number" => CustomFieldType::Number,
            "date" => CustomFieldType::Date,
            "boolean" => CustomFieldType::Boolean,
            "select" => CustomFieldType::Select,
            "multi_select" => CustomFieldType::MultiSelect,
            "url" => CustomFieldType::Url,
            "email" => CustomFieldType::Email,
            "long_text" => CustomFieldType::LongText,
            _ => CustomFieldType::Text,
        };

        let default_value = row.default_value.as_ref().and_then(|v| serde_json::from_str(v).ok());
        let options = row.options.as_ref().and_then(|o| serde_json::from_str(o).ok());
        let validation = row.validation.as_ref().and_then(|v| serde_json::from_str(v).ok());

        CustomFieldDefinition {
            id: Uuid::parse_str(&row.id).unwrap(),
            project_id: row.project_id.as_ref().map(|pid| Uuid::parse_str(pid).unwrap()),
            name: row.name,
            field_type,
            description: row.description,
            required: row.required != 0,
            default_value,
            options,
            validation,
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at).unwrap().into(),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at).unwrap().into(),
        }
    }).collect();

    Ok(fields)
}

/// Update a custom field definition
#[instrument(skip(pool))]
pub async fn update_field(
    pool: &SqlitePool,
    id: Uuid,
    data: UpdateCustomField,
) -> Result<CustomFieldDefinition, sqlx::Error> {
    let now = chrono::Utc::now();

    if let Some(name) = &data.name {
        sqlx::query("UPDATE custom_field_definitions SET name = ?, updated_at = ? WHERE id = ?")
            .bind(name)
            .bind(now.to_rfc3339())
            .bind(id.to_string())
            .execute(pool).await?;
    }

    if let Some(description) = &data.description {
        sqlx::query("UPDATE custom_field_definitions SET description = ?, updated_at = ? WHERE id = ?")
            .bind(description)
            .bind(now.to_rfc3339())
            .bind(id.to_string())
            .execute(pool).await?;
    }

    if let Some(required) = data.required {
        sqlx::query("UPDATE custom_field_definitions SET required = ?, updated_at = ? WHERE id = ?")
            .bind(required as i32)
            .bind(now.to_rfc3339())
            .bind(id.to_string())
            .execute(pool).await?;
    }

    if let Some(default_value) = data.default_value {
        let value_str = default_value.to_string();
        sqlx::query("UPDATE custom_field_definitions SET default_value = ?, updated_at = ? WHERE id = ?")
            .bind(value_str)
            .bind(now.to_rfc3339())
            .bind(id.to_string())
            .execute(pool).await?;
    }

    if let Some(options) = data.options {
        let options_str = serde_json::to_string(&options).unwrap();
        sqlx::query("UPDATE custom_field_definitions SET options = ?, updated_at = ? WHERE id = ?")
            .bind(options_str)
            .bind(now.to_rfc3339())
            .bind(id.to_string())
            .execute(pool).await?;
    }

    if let Some(validation) = data.validation {
        let validation_str = validation.to_string();
        sqlx::query("UPDATE custom_field_definitions SET validation = ?, updated_at = ? WHERE id = ?")
            .bind(validation_str)
            .bind(now.to_rfc3339())
            .bind(id.to_string())
            .execute(pool).await?;
    }

    get_field(pool, id).await
}

/// Delete a custom field definition
#[instrument(skip(pool))]
pub async fn delete_field(
    pool: &SqlitePool,
    id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM custom_field_definitions WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

// ─── Custom Field Values ─────────────────────────────────────

/// Set a custom field value for an item
#[instrument(skip(pool))]
pub async fn set_field_value(
    pool: &SqlitePool,
    item_id: Uuid,
    field_id: Uuid,
    value: serde_json::Value,
) -> Result<CustomFieldValue, sqlx::Error> {
    let id = Uuid::new_v4();
    let now = chrono::Utc::now();
    let value_str = value.to_string();

    // Upsert: insert or replace
    sqlx::query(
        "INSERT INTO custom_field_values (id, item_id, field_id, value, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(item_id, field_id) DO UPDATE SET value = ?, updated_at = ?"
    )
    .bind(id.to_string())
    .bind(item_id.to_string())
    .bind(field_id.to_string())
    .bind(&value_str)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .bind(&value_str)
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;

    get_field_value(pool, item_id, field_id).await
}

/// Get a custom field value for an item
#[instrument(skip(pool))]
pub async fn get_field_value(
    pool: &SqlitePool,
    item_id: Uuid,
    field_id: Uuid,
) -> Result<CustomFieldValue, sqlx::Error> {
    let row = sqlx::query_as::<_, CustomFieldValueRow>(
        "SELECT id, item_id, field_id, value, created_at, updated_at
         FROM custom_field_values
         WHERE item_id = ? AND field_id = ?"
    )
    .bind(item_id.to_string())
    .bind(field_id.to_string())
    .fetch_one(pool)
    .await?;

    let value: serde_json::Value = serde_json::from_str(&row.value).unwrap_or(serde_json::Value::Null);

    Ok(CustomFieldValue {
        id: Uuid::parse_str(&row.id).unwrap(),
        item_id: Uuid::parse_str(&row.item_id).unwrap(),
        field_id: Uuid::parse_str(&row.field_id).unwrap(),
        value,
        created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at).unwrap().into(),
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at).unwrap().into(),
    })
}

/// Get all custom field values for an item
#[instrument(skip(pool))]
pub async fn get_all_field_values_for_item(
    pool: &SqlitePool,
    item_id: Uuid,
) -> Result<Vec<CustomFieldValue>, sqlx::Error> {
    let rows = sqlx::query_as::<_, CustomFieldValueRow>(
        "SELECT id, item_id, field_id, value, created_at, updated_at
         FROM custom_field_values
         WHERE item_id = ?"
    )
    .bind(item_id.to_string())
    .fetch_all(pool)
    .await?;

    let values = rows.into_iter().map(|row| {
        let value: serde_json::Value = serde_json::from_str(&row.value).unwrap_or(serde_json::Value::Null);

        CustomFieldValue {
            id: Uuid::parse_str(&row.id).unwrap(),
            item_id: Uuid::parse_str(&row.item_id).unwrap(),
            field_id: Uuid::parse_str(&row.field_id).unwrap(),
            value,
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at).unwrap().into(),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at).unwrap().into(),
        }
    }).collect();

    Ok(values)
}

/// Delete a custom field value
#[instrument(skip(pool))]
pub async fn delete_field_value(
    pool: &SqlitePool,
    item_id: Uuid,
    field_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM custom_field_values WHERE item_id = ? AND field_id = ?")
        .bind(item_id.to_string())
        .bind(field_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}
