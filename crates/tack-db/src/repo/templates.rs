use sqlx::{FromRow, SqlitePool};
use tack_core::models::*;
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
        serde_json::to_string(&tack_core::workflow::simple_workflow())
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

// ─── Built-in template seeding ───────────────────────────────────────────────

struct BuiltinSpec {
    name: &'static str,
    description: &'static str,
    project_type: ProjectType,
}

/// Seed one built-in template per `ProjectType` if none exist yet.
/// Safe to call on every startup — inserts are skipped for already-seeded types.
#[instrument(skip(pool))]
pub async fn seed_builtin_templates(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    use tack_core::{vocabulary::vocabulary_for_type, workflow::workflow_for_type};

    let specs: &[BuiltinSpec] = &[
        BuiltinSpec {
            name: "Software Project",
            description: "Scrum workflow for software development — Backlog, To Do, In Progress, In Review, Done.",
            project_type: ProjectType::Software,
        },
        BuiltinSpec {
            name: "Web App",
            description: "Scrum workflow for web application development.",
            project_type: ProjectType::Web,
        },
        BuiltinSpec {
            name: "Mobile App",
            description: "Scrum workflow for mobile application development.",
            project_type: ProjectType::Mobile,
        },
        BuiltinSpec {
            name: "Construction Project",
            description: "Linear phase workflow: Permit → Procurement → Build → Inspect → Handover.",
            project_type: ProjectType::Construction,
        },
        BuiltinSpec {
            name: "Personal Tasks",
            description: "Simple To Do / Doing / Done board for personal goals and actions.",
            project_type: ProjectType::Personal,
        },
        BuiltinSpec {
            name: "Homework Tracker",
            description: "Simple workflow for tracking assignments, quizzes, and exams.",
            project_type: ProjectType::Homework,
        },
        BuiltinSpec {
            name: "Maintenance Board",
            description: "Kanban workflow for ongoing maintenance tickets and service requests.",
            project_type: ProjectType::Maintenance,
        },
        BuiltinSpec {
            name: "Custom Project",
            description: "Blank template — minimal workflow with no domain-specific vocabulary.",
            project_type: ProjectType::Custom,
        },
    ];

    for spec in specs {
        let type_str = match spec.project_type {
            ProjectType::Software => "software",
            ProjectType::Web => "web",
            ProjectType::Mobile => "mobile",
            ProjectType::Construction => "construction",
            ProjectType::Personal => "personal",
            ProjectType::Homework => "homework",
            ProjectType::Maintenance => "maintenance",
            ProjectType::Custom => "custom",
        };

        // Skip if a builtin already exists for this project type
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM project_templates WHERE project_type = ? AND is_builtin = 1)")
                .bind(type_str)
                .fetch_one(pool)
                .await?;

        if exists {
            continue;
        }

        let workflow = workflow_for_type(&spec.project_type);
        let vocabulary = vocabulary_for_type(&spec.project_type);

        // Build a default board whose columns mirror the workflow statuses
        let columns: Vec<tack_core::models::BoardColumn> = workflow
            .statuses
            .iter()
            .map(|s| tack_core::models::BoardColumn {
                status: s.name.clone(),
                wip_limit: s.wip_limit,
                collapsed: false,
            })
            .collect();

        let default_boards = vec![tack_core::models::BoardTemplate {
            name: "Main Board".to_string(),
            description: None,
            columns,
            filters: None,
            grouping: Some("status".to_string()),
        }];

        let id = Uuid::new_v4();
        let now = chrono::Utc::now();

        sqlx::query(
            "INSERT INTO project_templates
             (id, name, description, project_type, vocabulary, workflow, custom_fields, default_boards, is_builtin, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, '[]', ?, 1, ?, ?)"
        )
        .bind(id.to_string())
        .bind(spec.name)
        .bind(spec.description)
        .bind(type_str)
        .bind(serde_json::to_string(&vocabulary).unwrap_or_else(|_| "{}".to_string()))
        .bind(serde_json::to_string(&workflow).unwrap_or_else(|_| "{}".to_string()))
        .bind(serde_json::to_string(&default_boards).unwrap_or_else(|_| "[]".to_string()))
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(pool)
        .await?;

        tracing::info!(
            template_name = spec.name,
            project_type = type_str,
            "Seeded built-in template"
        );
    }

    Ok(())
}
