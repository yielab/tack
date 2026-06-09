use flexpm_core::models::*;
use sqlx::{FromRow, SqlitePool};
use tracing::instrument;
use uuid::Uuid;

#[derive(FromRow)]
struct TemplateRow {
    id: String,
    name: String,
    description: Option<String>,
    project_type: String,
    vocabulary: String,
    workflow: String,
    custom_fields: String,
    default_boards: String,
    is_builtin: i32,
    created_at: String,
    updated_at: String,
}

/// Create a new project template
#[instrument(skip(pool))]
pub async fn create_template(
    pool: &SqlitePool,
    data: CreateProjectTemplate,
) -> Result<ProjectTemplate, sqlx::Error> {
    let id = Uuid::new_v4();
    let now = chrono::Utc::now();

    let vocabulary = serde_json::to_string(&data.vocabulary.unwrap_or_default())
        .unwrap_or_else(|_| "{}".to_string());

    let workflow = if let Some(wf) = data.workflow {
        serde_json::to_string(&wf).unwrap_or_else(|_| "{}".to_string())
    } else {
        serde_json::to_string(&flexpm_core::workflow::simple_workflow())
            .unwrap_or_else(|_| "{}".to_string())
    };

    let custom_fields = serde_json::to_string(&data.custom_fields.unwrap_or_default())
        .unwrap_or_else(|_| "[]".to_string());

    let default_boards = serde_json::to_string(&data.default_boards.unwrap_or_default())
        .unwrap_or_else(|_| "[]".to_string());

    let project_type_str = match data.project_type {
        ProjectType::Software => "software",
        ProjectType::Web => "web",
        ProjectType::Mobile => "mobile",
        ProjectType::Construction => "construction",
        ProjectType::Personal => "personal",
        ProjectType::Homework => "homework",
        ProjectType::Maintenance => "maintenance",
        ProjectType::Custom => "custom",
    };

    sqlx::query(
        "INSERT INTO project_templates
         (id, name, description, project_type, vocabulary, workflow, custom_fields, default_boards, is_builtin, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?)"
    )
    .bind(id.to_string())
    .bind(&data.name)
    .bind(&data.description)
    .bind(project_type_str)
    .bind(&vocabulary)
    .bind(&workflow)
    .bind(&custom_fields)
    .bind(&default_boards)
    .bind(now.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;

    get_template(pool, id).await
}

/// Get a template by ID
#[instrument(skip(pool))]
pub async fn get_template(pool: &SqlitePool, id: Uuid) -> Result<ProjectTemplate, sqlx::Error> {
    let row = sqlx::query_as::<_, TemplateRow>(
        "SELECT id, name, description, project_type, vocabulary, workflow, custom_fields, default_boards, is_builtin, created_at, updated_at
         FROM project_templates
         WHERE id = ?"
    )
    .bind(id.to_string())
    .fetch_one(pool)
    .await?;

    let vocabulary: serde_json::Value =
        serde_json::from_str(&row.vocabulary).unwrap_or(serde_json::json!({}));

    let workflow: serde_json::Value =
        serde_json::from_str(&row.workflow).unwrap_or(serde_json::json!({}));

    let custom_fields: Vec<CustomFieldDefinition> =
        serde_json::from_str(&row.custom_fields).unwrap_or_default();

    let default_boards: Vec<BoardTemplate> =
        serde_json::from_str(&row.default_boards).unwrap_or_default();

    Ok(ProjectTemplate {
        id: Uuid::parse_str(&row.id).unwrap(),
        name: row.name,
        description: row.description,
        project_type: serde_json::from_value(serde_json::Value::String(row.project_type)).unwrap(),
        vocabulary: serde_json::from_value(vocabulary).unwrap(),
        workflow: serde_json::from_value(workflow).unwrap(),
        custom_fields,
        default_boards,
        is_builtin: row.is_builtin != 0,
        created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
            .unwrap()
            .into(),
        updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
            .unwrap()
            .into(),
    })
}

/// List all templates, optionally filtered by project type
#[instrument(skip(pool))]
pub async fn list_templates(
    pool: &SqlitePool,
    project_type: Option<ProjectType>,
) -> Result<Vec<ProjectTemplate>, sqlx::Error> {
    let rows = if let Some(ptype) = project_type {
        let type_str = match ptype {
            ProjectType::Software => "software",
            ProjectType::Web => "web",
            ProjectType::Mobile => "mobile",
            ProjectType::Construction => "construction",
            ProjectType::Personal => "personal",
            ProjectType::Homework => "homework",
            ProjectType::Maintenance => "maintenance",
            ProjectType::Custom => "custom",
        };

        sqlx::query_as::<_, TemplateRow>(
            "SELECT id, name, description, project_type, vocabulary, workflow, custom_fields, default_boards, is_builtin, created_at, updated_at
             FROM project_templates
             WHERE project_type = ?
             ORDER BY is_builtin DESC, name ASC"
        )
        .bind(type_str)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, TemplateRow>(
            "SELECT id, name, description, project_type, vocabulary, workflow, custom_fields, default_boards, is_builtin, created_at, updated_at
             FROM project_templates
             ORDER BY is_builtin DESC, name ASC"
        )
        .fetch_all(pool)
        .await?
    };

    let templates: Vec<ProjectTemplate> = rows
        .into_iter()
        .map(|row| {
            let vocabulary: serde_json::Value =
                serde_json::from_str(&row.vocabulary).unwrap_or(serde_json::json!({}));

            let workflow: serde_json::Value =
                serde_json::from_str(&row.workflow).unwrap_or(serde_json::json!({}));

            let custom_fields: Vec<CustomFieldDefinition> =
                serde_json::from_str(&row.custom_fields).unwrap_or_default();

            let default_boards: Vec<BoardTemplate> =
                serde_json::from_str(&row.default_boards).unwrap_or_default();

            ProjectTemplate {
                id: Uuid::parse_str(&row.id).unwrap(),
                name: row.name,
                description: row.description,
                project_type: serde_json::from_value(serde_json::Value::String(row.project_type))
                    .unwrap(),
                vocabulary: serde_json::from_value(vocabulary).unwrap(),
                workflow: serde_json::from_value(workflow).unwrap(),
                custom_fields,
                default_boards,
                is_builtin: row.is_builtin != 0,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                    .unwrap()
                    .into(),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                    .unwrap()
                    .into(),
            }
        })
        .collect();

    Ok(templates)
}

/// Delete a template (only non-builtin templates can be deleted)
#[instrument(skip(pool))]
pub async fn delete_template(pool: &SqlitePool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM project_templates WHERE id = ? AND is_builtin = 0")
        .bind(id.to_string())
        .execute(pool)
        .await?;

    Ok(())
}
