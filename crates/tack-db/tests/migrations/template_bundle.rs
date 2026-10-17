//! Migration 091: a template carries roles, profiles and automation.

use tack_db::{init_pool, migrations};

#[tokio::test]
async fn migration_091_gives_an_existing_template_empty_roles_profiles_and_no_automation() {
    let pool = init_pool("sqlite::memory:").await.expect("pool");
    migrations::run_up_to(&pool, "090_auto_start")
        .await
        .expect("up to 090");
    sqlx::query(
        "INSERT INTO project_templates (id, name, project_type, vocabulary, workflow, custom_fields, default_boards, is_builtin, created_at, updated_at) \
         VALUES ('t', 'Mine', 'web', '{}', '{}', '[]', '[]', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("template at 090");
    migrations::run_all(&pool).await.expect("upgrade to 091");

    let row: (String, String, Option<String>) =
        sqlx::query_as("SELECT roles, profiles, automation FROM project_templates WHERE id = 't'")
            .fetch_one(&pool)
            .await
            .expect("template row");
    assert_eq!(row, ("[]".to_string(), "[]".to_string(), None));
}
