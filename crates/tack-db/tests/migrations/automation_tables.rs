//! Migration 082: project automation configuration, agent profiles,
//! and attempt review verdicts.

use crate::common;

#[tokio::test]
async fn migration_082_creates_automation_schema() {
    let repo = common::setup_test_db().await;

    // Verify project automation columns
    let project_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('projects')")
            .fetch_all(repo.pool())
            .await
            .unwrap_or_else(|e| panic!("pragma_table_info(projects): {e}"));

    let project_expected = &[
        "code_origin",
        "repository",
        "default_branch",
        "workspace_mode",
        "push_after_run",
        "default_harness",
        "default_profile_id",
        "on_finish_status",
        "definition_of_done",
    ];

    for column in project_expected {
        assert!(
            project_columns.iter().any(|c| c == column),
            "projects is missing column {column}; has {project_columns:?}"
        );
    }

    // Verify items.run_settings column
    let items_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('items')")
            .fetch_all(repo.pool())
            .await
            .unwrap_or_else(|e| panic!("pragma_table_info(items): {e}"));

    assert!(
        items_columns.iter().any(|c| c == "run_settings"),
        "items is missing column run_settings; has {items_columns:?}"
    );

    // Verify agent_profiles columns
    let profiles_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('agent_profiles')")
            .fetch_all(repo.pool())
            .await
            .unwrap_or_else(|e| panic!("pragma_table_info(agent_profiles): {e}"));

    let profiles_expected = &["kind", "builtin", "summary"];

    for column in profiles_expected {
        assert!(
            profiles_columns.iter().any(|c| c == column),
            "agent_profiles is missing column {column}; has {profiles_columns:?}"
        );
    }

    // Verify attempt_reviews table exists
    let attempt_reviews_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('attempt_reviews')")
            .fetch_all(repo.pool())
            .await
            .unwrap_or_else(|e| panic!("pragma_table_info(attempt_reviews): {e}"));

    let attempt_reviews_expected = &["attempt_id", "verdict", "note", "reviewed_at"];

    for column in attempt_reviews_expected {
        assert!(
            attempt_reviews_columns.iter().any(|c| c == column),
            "attempt_reviews is missing column {column}; has {attempt_reviews_columns:?}"
        );
    }

    // Verify defaults for integer and text columns
    let project_info: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT name, dflt_value FROM pragma_table_info('projects') WHERE name IN ('code_origin', 'workspace_mode', 'push_after_run')"
    )
        .fetch_all(repo.pool())
        .await
        .unwrap_or_else(|e| panic!("pragma_table_info defaults query: {e}"));

    for (col_name, dflt) in &project_info {
        if col_name == "code_origin" {
            assert_eq!(
                dflt,
                &Some("'none'".to_string()),
                "code_origin default should be 'none'"
            );
        } else if col_name == "workspace_mode" {
            assert_eq!(
                dflt,
                &Some("'local_branch'".to_string()),
                "workspace_mode default should be 'local_branch'"
            );
        } else if col_name == "push_after_run" {
            assert_eq!(
                dflt,
                &Some("0".to_string()),
                "push_after_run default should be 0"
            );
        }
    }

    let profiles_info: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT name, dflt_value FROM pragma_table_info('agent_profiles') WHERE name IN ('kind', 'builtin')"
    )
        .fetch_all(repo.pool())
        .await
        .unwrap_or_else(|e| panic!("pragma_table_info agent_profiles defaults: {e}"));

    for (col_name, dflt) in &profiles_info {
        if col_name == "kind" {
            assert_eq!(
                dflt,
                &Some("'custom'".to_string()),
                "agent_profiles.kind default should be 'custom'"
            );
        } else if col_name == "builtin" {
            assert_eq!(
                dflt,
                &Some("0".to_string()),
                "agent_profiles.builtin default should be 0"
            );
        }
    }
}

#[tokio::test]
async fn migration_083_adds_items_source_artifact_id() {
    let repo = common::setup_test_db().await;
    let items_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('items')")
            .fetch_all(repo.pool())
            .await
            .unwrap_or_else(|e| panic!("pragma_table_info(items): {e}"));
    assert!(
        items_columns.iter().any(|c| c == "source_artifact_id"),
        "items is missing column source_artifact_id; has {items_columns:?}"
    );
}
