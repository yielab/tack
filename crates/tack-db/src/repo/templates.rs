use serde_json::json;
use sqlx::{FromRow, SqlitePool};
use tack_core::models::*;
use tack_core::outline::{BundleAutomation, BundleAutonomy, BundleProfile, BundleRole};
use tack_core::workflow::WorkflowConfig;
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
    roles: String,
    profiles: String,
    automation: Option<String>,
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

    let project_type_str = data.project_type.to_string();

    sqlx::query(
        "INSERT INTO project_templates
         (id, name, description, project_type, vocabulary, workflow, custom_fields, default_boards, roles, profiles, automation, is_builtin, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?)"
    )
    .bind(id.to_string())
    .bind(&data.name)
    .bind(&data.description)
    .bind(project_type_str)
    .bind(&vocabulary)
    .bind(&workflow)
    .bind(&custom_fields)
    .bind(&default_boards)
    .bind(serde_json::to_string(&data.roles).unwrap_or_else(|_| "[]".to_string()))
    .bind(serde_json::to_string(&data.profiles).unwrap_or_else(|_| "[]".to_string()))
    .bind(
        data.automation
            .as_ref()
            .and_then(|a| serde_json::to_string(a).ok()),
    )
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
        "SELECT id, name, description, project_type, vocabulary, workflow, custom_fields, default_boards, roles, profiles, automation, is_builtin, created_at, updated_at
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
        roles: serde_json::from_str(&row.roles).unwrap_or_default(),
        profiles: serde_json::from_str(&row.profiles).unwrap_or_default(),
        automation: row
            .automation
            .as_deref()
            .and_then(|a| serde_json::from_str(a).ok()),
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
        let type_str = ptype.to_string();

        sqlx::query_as::<_, TemplateRow>(
            "SELECT id, name, description, project_type, vocabulary, workflow, custom_fields, default_boards, roles, profiles, automation, is_builtin, created_at, updated_at
             FROM project_templates
             WHERE project_type = ?
             ORDER BY is_builtin DESC, name ASC"
        )
        .bind(type_str)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, TemplateRow>(
            "SELECT id, name, description, project_type, vocabulary, workflow, custom_fields, default_boards, roles, profiles, automation, is_builtin, created_at, updated_at
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
                roles: serde_json::from_str(&row.roles).unwrap_or_default(),
                profiles: serde_json::from_str(&row.profiles).unwrap_or_default(),
                automation: row
                    .automation
                    .as_deref()
                    .and_then(|a| serde_json::from_str(a).ok()),
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
    /// Optional workflow override. When `None`, the workflow is derived from
    /// `project_type` via `workflow_for_type`. Sub-preset templates (e.g. the
    /// construction verticals) supply a tailored per-template workflow here.
    workflow: Option<WorkflowConfig>,
    /// Template-level custom field definitions (empty for the base presets).
    custom_fields: Vec<CustomFieldDefinition>,
    roles: Vec<BundleRole>,
    profiles: Vec<BundleProfile>,
    automation: Option<BundleAutomation>,
}

impl BuiltinSpec {
    /// Base preset: workflow + vocabulary derived entirely from the project type.
    fn base(name: &'static str, description: &'static str, project_type: ProjectType) -> Self {
        Self {
            name,
            description,
            project_type,
            workflow: None,
            custom_fields: Vec::new(),
            roles: Vec::new(),
            profiles: Vec::new(),
            automation: None,
        }
    }
}

/// Build a template-level custom field definition. `project_id` is `None`
/// (template-scoped); ids/timestamps are generated fresh per seed.
fn builtin_field(
    name: &str,
    field_type: CustomFieldType,
    options: Option<Vec<String>>,
) -> CustomFieldDefinition {
    let now = chrono::Utc::now();
    CustomFieldDefinition {
        id: Uuid::new_v4(),
        project_id: None,
        name: name.to_string(),
        field_type,
        description: None,
        required: false,
        default_value: None,
        options,
        validation: None,
        created_at: now,
        updated_at: now,
    }
}

fn builtin_role(name: &str, color: &str, profile: &str) -> BundleRole {
    BundleRole {
        name: name.to_string(),
        color: Some(color.to_string()),
        icon: None,
        profile: Some(profile.to_string()),
    }
}

/// A profile run by the docket agent.
fn builtin_profile(name: &str, kind: &str, instructions: &str) -> BundleProfile {
    BundleProfile {
        name: name.to_string(),
        kind: Some(kind.to_string()),
        instructions: instructions.to_string(),
        tool_policy: json!({"agent": "docket", "tools": {"docket": ["*"]}}),
        limits: json!({}),
        fleet: None,
        summary: None,
    }
}

/// A reviewer that only reads.
fn read_only_review(name: &str, instructions: &str) -> BundleProfile {
    BundleProfile {
        tool_policy: json!({"agent": "claude-code", "tools": {"claude-code": ["Read", "Grep", "Glob"]}}),
        ..builtin_profile(name, "reviewer", instructions)
    }
}

/// The automation every built-in template ships: L3, no merge on done, no repository.
fn level_three() -> Option<BundleAutomation> {
    Some(BundleAutomation {
        autonomy: Some(BundleAutonomy {
            level: Some("L3".to_string()),
            merge_on_done: Some(false),
            ..BundleAutonomy::default()
        }),
        ..BundleAutomation::default()
    })
}

fn security_review() -> BundleProfile {
    read_only_review(
        "Security review",
        "Review for secrets, injection and permissions. You only read; you change nothing.",
    )
}

fn legal_review() -> BundleProfile {
    read_only_review(
        "Legal review",
        "Review licences, privacy and terms. You only read; you change nothing.",
    )
}

fn ops_review() -> BundleProfile {
    read_only_review(
        "Ops review",
        "Review deployment, backups and monitoring. You only read; you change nothing.",
    )
}

fn content_writer() -> BundleProfile {
    builtin_profile(
        "Content writer",
        "custom",
        "Write and edit the project's copy and content. Keep the tone of what is already there.",
    )
}

fn builtin_choice(name: &str, options: &[&str]) -> CustomFieldDefinition {
    builtin_field(
        name,
        CustomFieldType::Select,
        Some(options.iter().map(|o| o.to_string()).collect()),
    )
}

/// Seed the built-in project templates if they don't exist yet.
/// Safe to call on every startup — inserts are skipped by template name, so
/// multiple built-ins can share a `ProjectType` (e.g. the construction
/// verticals all reuse `ProjectType::Construction`).
#[instrument(skip(pool))]
pub async fn seed_builtin_templates(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    use tack_core::{
        vocabulary::vocabulary_for_type,
        workflow::{
            sip_panel_workflow, steel_frame_workflow, wood_frame_workflow, workflow_for_type,
        },
    };

    let specs: Vec<BuiltinSpec> = vec![
        BuiltinSpec::base(
            "Software Project",
            "Scrum workflow for software development — Backlog, To Do, In Progress, In Review, Done.",
            ProjectType::Software,
        ),
        BuiltinSpec::base(
            "Web App",
            "Scrum workflow for web application development.",
            ProjectType::Web,
        ),
        BuiltinSpec::base(
            "Mobile App",
            "Scrum workflow for mobile application development.",
            ProjectType::Mobile,
        ),
        BuiltinSpec::base(
            "Construction Project",
            "Linear phase workflow: Permit → Procurement → Build → Inspect → Handover.",
            ProjectType::Construction,
        ),
        // ── Construction verticals (all reuse ProjectType::Construction) ──
        BuiltinSpec {
            name: "Wood Frame Build",
            description: "Stick-frame build: Permit → Foundation → Framing → Rough-In (MEP) → Insulation & Drywall → Finish → Inspect → Handover.",
            project_type: ProjectType::Construction,
            workflow: Some(wood_frame_workflow()),
            custom_fields: vec![
                builtin_field(
                    "Stud Spacing",
                    CustomFieldType::Select,
                    Some(vec!["16\" o.c.".to_string(), "24\" o.c.".to_string()]),
                ),
                builtin_field("Lumber Grade", CustomFieldType::Text, None),
                builtin_field("Sheathing Type", CustomFieldType::Text, None),
                builtin_field("Shear-Wall Schedule Ref", CustomFieldType::Text, None),
            ],
            roles: Vec::new(),
            profiles: Vec::new(),
            automation: None,
        },
        BuiltinSpec {
            name: "Steel Frame Build",
            description: "Structural-steel build: Permit → Engineering → Fabrication → Erection → Decking/MEP → Fireproofing → Inspect → Handover.",
            project_type: ProjectType::Construction,
            workflow: Some(steel_frame_workflow()),
            custom_fields: vec![
                builtin_field("Steel Grade", CustomFieldType::Text, None),
                builtin_field("Bolt Spec", CustomFieldType::Text, None),
                builtin_field("Weld Inspection Class", CustomFieldType::Text, None),
                builtin_field("Torque Log Ref", CustomFieldType::Text, None),
            ],
            roles: Vec::new(),
            profiles: Vec::new(),
            automation: None,
        },
        BuiltinSpec {
            name: "SIP Panel Build",
            description: "Structural insulated panel build: Design/Shop Drawings → Panel Fabrication → Delivery → Foundation → Panel Set → Seal & Penetrations → MEP Chases → Inspect → Handover.",
            project_type: ProjectType::Construction,
            workflow: Some(sip_panel_workflow()),
            custom_fields: vec![
                builtin_field("Panel Count", CustomFieldType::Number, None),
                builtin_field("Panel Thickness", CustomFieldType::Text, None),
                builtin_field("Spline Type", CustomFieldType::Text, None),
                builtin_field("Sealant Spec", CustomFieldType::Text, None),
            ],
            roles: Vec::new(),
            profiles: Vec::new(),
            automation: None,
        },
        BuiltinSpec::base(
            "Personal Tasks",
            "Simple To Do / Doing / Done board for personal goals and actions.",
            ProjectType::Personal,
        ),
        BuiltinSpec::base(
            "Homework Tracker",
            "Simple workflow for tracking assignments, quizzes, and exams.",
            ProjectType::Homework,
        ),
        BuiltinSpec::base(
            "Maintenance Board",
            "Kanban workflow for ongoing maintenance tickets and service requests.",
            ProjectType::Maintenance,
        ),
        BuiltinSpec::base(
            "Legal Matter",
            "Case management workflow: Intake → Discovery → Drafting → Review → Closed.",
            ProjectType::Legal,
        ),
        BuiltinSpec::base(
            "Research Study",
            "Lab workflow: Hypothesis → Design → Experiment → Analysis → Published.",
            ProjectType::Research,
        ),
        BuiltinSpec::base(
            "Event Plan",
            "Event planning workflow: Ideas → Booked → In Progress → Confirmed → Done.",
            ProjectType::Event,
        ),
        BuiltinSpec::base(
            "Custom Project",
            "Blank template — minimal workflow with no domain-specific vocabulary.",
            ProjectType::Custom,
        ),
        // ── Templates that come with their roles and profiles (ADR 0081) ──
        BuiltinSpec {
            name: "Web product with content",
            description: "A web product with content: roles for content, development, growth, security, legal and ops, each with a profile; the reviewers only read.",
            project_type: ProjectType::Web,
            workflow: None,
            custom_fields: vec![
                builtin_choice("Channel", &["app", "newsletter", "social"]),
                builtin_choice("Audience", &["new", "returning"]),
            ],
            roles: vec![
                builtin_role("Content", "#f59e0b", "Content writer"),
                builtin_role("Development", "#3b82f6", "Product implementer"),
                builtin_role("Growth", "#10b981", "Growth"),
                builtin_role("Security", "#ef4444", "Security review"),
                builtin_role("Legal", "#8b5cf6", "Legal review"),
                builtin_role("Ops", "#6b7280", "Ops review"),
            ],
            profiles: vec![
                content_writer(),
                builtin_profile(
                    "Product implementer",
                    "implementer",
                    "Implement the change on a branch; run the project's checks before you finish.",
                ),
                builtin_profile(
                    "Growth",
                    "custom",
                    "Draft campaigns and measure them; never publish anything yourself.",
                ),
                security_review(),
                legal_review(),
                ops_review(),
            ],
            automation: level_three(),
        },
        BuiltinSpec {
            name: "Library or CLI",
            description: "A library or command-line tool: roles for development, docs, security and ops, each with a profile; the reviewers only read.",
            project_type: ProjectType::Software,
            workflow: None,
            custom_fields: vec![
                builtin_field(
                    "Platform",
                    CustomFieldType::MultiSelect,
                    Some(vec![
                        "linux".to_string(),
                        "macos".to_string(),
                        "windows".to_string(),
                    ]),
                ),
                builtin_field("Breaking", CustomFieldType::Boolean, None),
            ],
            roles: vec![
                builtin_role("Development", "#3b82f6", "Library implementer"),
                builtin_role("Docs", "#f59e0b", "Docs writer"),
                builtin_role("Security", "#ef4444", "Security review"),
                builtin_role("Ops", "#6b7280", "Ops review"),
            ],
            profiles: vec![
                BundleProfile {
                    tool_policy: json!({}),
                    ..builtin_profile(
                        "Library implementer",
                        "implementer",
                        "Implement the change on a branch; keep the public interface stable and run the tests before you finish.",
                    )
                },
                BundleProfile {
                    tool_policy: json!({}),
                    ..builtin_profile(
                        "Docs writer",
                        "custom",
                        "Write and correct the documentation so it matches what the code does today.",
                    )
                },
                security_review(),
                ops_review(),
            ],
            automation: level_three(),
        },
        BuiltinSpec {
            name: "Static content site",
            description: "A static content site on a kanban board: roles for content, design, legal and ops, each with a profile; the reviewers only read.",
            project_type: ProjectType::Web,
            workflow: Some(tack_core::workflow::kanban_workflow()),
            custom_fields: vec![
                builtin_choice("Section", &["home", "blog", "docs"]),
                builtin_field("Published", CustomFieldType::Date, None),
            ],
            roles: vec![
                builtin_role("Content", "#f59e0b", "Content writer"),
                builtin_role("Design", "#ec4899", "Design review"),
                builtin_role("Legal", "#8b5cf6", "Legal review"),
                builtin_role("Ops", "#6b7280", "Ops review"),
            ],
            profiles: vec![
                content_writer(),
                read_only_review(
                    "Design review",
                    "Review layout, spacing and accessibility. You only read; you change nothing.",
                ),
                legal_review(),
                ops_review(),
            ],
            automation: level_three(),
        },
    ];

    for spec in &specs {
        let type_str = spec.project_type.to_string();

        // Skip if a builtin with this name already exists (per-name dedup lets
        // several builtins share one project_type).
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM project_templates WHERE name = ? AND is_builtin = 1)",
        )
        .bind(spec.name)
        .fetch_one(pool)
        .await?;

        if exists {
            continue;
        }

        let workflow = spec
            .workflow
            .clone()
            .unwrap_or_else(|| workflow_for_type(&spec.project_type));
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
             (id, name, description, project_type, vocabulary, workflow, custom_fields, default_boards, roles, profiles, automation, is_builtin, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)"
        )
        .bind(id.to_string())
        .bind(spec.name)
        .bind(spec.description)
        .bind(type_str.as_str())
        .bind(serde_json::to_string(&vocabulary).unwrap_or_else(|_| "{}".to_string()))
        .bind(serde_json::to_string(&workflow).unwrap_or_else(|_| "{}".to_string()))
        .bind(serde_json::to_string(&spec.custom_fields).unwrap_or_else(|_| "[]".to_string()))
        .bind(serde_json::to_string(&default_boards).unwrap_or_else(|_| "[]".to_string()))
        .bind(serde_json::to_string(&spec.roles).unwrap_or_else(|_| "[]".to_string()))
        .bind(serde_json::to_string(&spec.profiles).unwrap_or_else(|_| "[]".to_string()))
        .bind(
            spec.automation
                .as_ref()
                .and_then(|a| serde_json::to_string(a).ok()),
        )
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .execute(pool)
        .await?;

        tracing::info!(
            template_name = spec.name,
            project_type = type_str.as_str(),
            "Seeded built-in template"
        );
    }

    Ok(())
}

#[cfg(test)]
#[path = "templates/tests.rs"]
mod tests;
