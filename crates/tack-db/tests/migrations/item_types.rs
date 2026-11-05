//! Migration 107: a project's types become a catalog. Every project gets the preset of its type
//! (named as its vocabulary renamed them), the types its items already use are kept as work, a
//! subtask becomes the default work type and keeps its parent, and the type words leave the
//! vocabularies.

use tack_core::item_types::{ItemTypeDef, Level, TypeColor, default_work_key, preset};
use tack_core::models::ProjectType;
use tack_db::{Repository, init_pool, migrations, queries};
use uuid::Uuid;

const WORKSPACE: &str = "22222222-2222-2222-2222-222222222222";

const EVERY_PROJECT_TYPE: [ProjectType; 11] = [
    ProjectType::Software,
    ProjectType::Web,
    ProjectType::Mobile,
    ProjectType::Construction,
    ProjectType::Personal,
    ProjectType::Homework,
    ProjectType::Maintenance,
    ProjectType::Legal,
    ProjectType::Research,
    ProjectType::Event,
    ProjectType::Custom,
];

async fn database_at_106() -> sqlx::SqlitePool {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "106_template_teams")
        .await
        .expect("up to 106");
    sqlx::query("INSERT INTO workspaces (id, name, default_vocabulary) VALUES (?, 'W', ?)")
        .bind(WORKSPACE)
        .bind(r#"{"epic":"Epic","sprint":"Sprint"}"#)
        .execute(&pool)
        .await
        .expect("workspace");
    pool
}

async fn project(pool: &sqlx::SqlitePool, id: Uuid, project_type: &str, vocabulary: &str) {
    sqlx::query(
        "INSERT INTO projects (id, workspace_id, name, project_type, vocabulary, workflow, created_at, updated_at)
         VALUES (?, ?, 'P', ?, ?, '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(id.to_string())
    .bind(WORKSPACE)
    .bind(project_type)
    .bind(vocabulary)
    .execute(pool)
    .await
    .expect("project");
}

async fn item(
    pool: &sqlx::SqlitePool,
    id: &str,
    project: Uuid,
    item_type: &str,
    parent: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO items (id, project_id, parent_id, title, item_type, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, 'To Do', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(id)
    .bind(project.to_string())
    .bind(parent)
    .bind(id)
    .bind(item_type)
    .execute(pool)
    .await
    .expect("item");
}

/// The migration's frozen preset list is the core's `preset()` as of this migration: changing the
/// preset later is a decision, and this is where it shows.
#[tokio::test]
async fn the_frozen_preset_list_is_the_cores_preset_for_every_project_type() {
    let pool = database_at_106().await;
    let mut projects = Vec::new();
    for project_type in &EVERY_PROJECT_TYPE {
        let id = Uuid::new_v4();
        project(&pool, id, &project_type.to_string(), "{}").await;
        sqlx::query(
            "INSERT INTO project_templates (id, name, project_type, vocabulary, workflow) VALUES (?, 'T', ?, '{}', '{}')",
        )
        .bind(id.to_string())
        .bind(project_type.to_string())
        .execute(&pool)
        .await
        .expect("template");
        projects.push((id, project_type));
    }
    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool.clone());
    for (id, project_type) in projects {
        let catalog = queries::item_types::of_project(&repo, id)
            .await
            .expect("catalog");
        assert_eq!(catalog, preset(project_type), "{project_type} project");
        let stored: String =
            sqlx::query_scalar("SELECT item_types FROM project_templates WHERE id = ?")
                .bind(id.to_string())
                .fetch_one(&pool)
                .await
                .expect("template item_types");
        let template: Vec<ItemTypeDef> = serde_json::from_str(&stored).expect("json");
        assert_eq!(template, preset(project_type), "{project_type} template");
    }
}

#[tokio::test]
async fn items_keep_their_types_a_subtask_becomes_the_default_and_the_words_become_names() {
    let pool = database_at_106().await;
    let personal = Uuid::new_v4();
    project(
        &pool,
        personal,
        "personal",
        r#"{"epic":"Goal","task":"Action","subtask":"Step","sprint":"Cycle"}"#,
    )
    .await;
    item(&pool, "goal", personal, "epic", None).await;
    item(&pool, "step", personal, "subtask", Some("goal")).await;
    item(&pool, "idea", personal, "spike", None).await;
    let software = Uuid::new_v4();
    project(&pool, software, "software", "{}").await;
    item(&pool, "bug", software, "bug", None).await;
    item(&pool, "other-step", software, "subtask", Some("bug")).await;
    sqlx::query(
        "INSERT INTO project_templates (id, name, project_type, vocabulary, workflow) VALUES ('t', 'T', 'legal', ?, '{}')",
    )
    .bind(r#"{"task":"Motion","milestone":"Hearing"}"#)
    .execute(&pool)
    .await
    .expect("template");

    migrations::run_all(&pool).await.expect("migrations");
    let repo = Repository::new(pool.clone());

    let catalog = queries::item_types::of_project(&repo, personal)
        .await
        .unwrap();
    let name = |key: &str| {
        catalog
            .iter()
            .find(|t| t.key == key)
            .map(|t| t.name.as_str())
    };
    assert_eq!(
        name("epic"),
        Some("Goal"),
        "the renamed word is the type's name"
    );
    assert_eq!(name("task"), Some("Action"));
    assert_eq!(
        name("bug"),
        Some("Bug"),
        "a word left alone keeps the preset's name"
    );
    let spike = catalog
        .iter()
        .find(|t| t.key == "spike")
        .expect("the custom type is kept");
    assert_eq!(
        (spike.level, spike.color),
        (Level::Work, TypeColor::Neutral)
    );
    assert_eq!(spike.name, "spike");
    assert_eq!(default_work_key(&catalog), Some("task"));
    assert!(catalog.iter().all(|t| t.key != "subtask"));

    let (item_type, parent): (String, Option<String>) =
        sqlx::query_as("SELECT item_type, parent_id FROM items WHERE id = 'step'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (item_type.as_str(), parent.as_deref()),
        ("task", Some("goal"))
    );
    let other: String = sqlx::query_scalar("SELECT item_type FROM items WHERE id = 'other-step'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(other, "task");
    let untouched: String = sqlx::query_scalar("SELECT item_type FROM items WHERE id = 'idea'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(untouched, "spike");

    // The six type words leave the vocabularies; the others stay.
    let vocabulary: String = sqlx::query_scalar("SELECT vocabulary FROM projects WHERE id = ?")
        .bind(personal.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(vocabulary, r#"{"sprint":"Cycle"}"#);
    let workspace: String = sqlx::query_scalar("SELECT default_vocabulary FROM workspaces")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(workspace, r#"{"sprint":"Sprint"}"#);
    let template_vocabulary: String =
        sqlx::query_scalar("SELECT vocabulary FROM project_templates WHERE id = 't'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(template_vocabulary, r#"{"milestone":"Hearing"}"#);
    let stored: String =
        sqlx::query_scalar("SELECT item_types FROM project_templates WHERE id = 't'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let template: Vec<ItemTypeDef> = serde_json::from_str(&stored).unwrap();
    let motion = template.iter().find(|t| t.key == "task").unwrap();
    assert_eq!(
        motion.name, "Motion",
        "a template's renamed word is the type's name"
    );
    assert!(motion.default_work);
}
