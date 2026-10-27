//! A stored value this layer cannot read back is an error that names the table and the column,
//! never a panic and never a default made up in its place; and a call about a row that is not
//! there names what kind of row it is.

use crate::common::{self, create_test_workspace, make_item, make_project};
use crate::repository::execution_fixture::Fixture;

use tack_core::models::{CreateRole, CustomFieldType};
use tack_db::StoreError;
use tack_db::queries::{execution_attempts, execution_requests};
use tack_db::repo::custom_fields;
use uuid::Uuid;

#[track_caller]
fn assert_corrupt<T: std::fmt::Debug>(result: Result<T, StoreError>, table: &str, column: &str) {
    match result {
        Err(StoreError::Corrupt {
            table: t,
            column: c,
            ..
        }) => assert_eq!((t, c), (table, column)),
        other => panic!("expected Corrupt {{ {table}, {column} }}, got {other:?}"),
    }
}

#[tokio::test]
async fn a_corrupt_item_column_is_an_error_naming_the_column() {
    for (column, bad) in [
        ("id", "not-a-uuid"),
        ("priority", "urgent"),
        ("tags", "[unterminated"),
        ("estimate_unit", "{\"nope\":"),
        ("due_date", "next tuesday"),
        ("created_at", "yesterday"),
    ] {
        let repo = common::setup_test_db().await;
        let ws = create_test_workspace(&repo).await;
        let project = make_project(&repo, ws).await;
        let item = make_item(&repo, &project).await;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE items SET {column} = ? WHERE id = ?"
        )))
        .bind(bad)
        .bind(item.id.to_string())
        .execute(repo.pool())
        .await
        .unwrap();

        let listed = repo.list_items(project.id, &Default::default()).await;
        assert_corrupt(listed, "items", column);
        if column != "id" {
            assert_corrupt(repo.get_item(item.id).await, "items", column);
        }
    }
}

#[tokio::test]
async fn a_corrupt_project_column_is_an_error_naming_the_column() {
    for (column, bad) in [
        ("project_type", "spaceship"),
        ("vocabulary", "not json"),
        ("workflow", "{\"statuses\":"),
        ("code_origin", "somewhere"),
        ("workspace_mode", "sideways"),
        ("created_at", "long ago"),
    ] {
        let repo = common::setup_test_db().await;
        let ws = create_test_workspace(&repo).await;
        let project = make_project(&repo, ws).await;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE projects SET {column} = ? WHERE id = ?"
        )))
        .bind(bad)
        .bind(project.id.to_string())
        .execute(repo.pool())
        .await
        .unwrap();

        assert_corrupt(repo.get_project(project.id).await, "projects", column);
        assert_corrupt(repo.list_projects(ws).await, "projects", column);
    }
}

#[tokio::test]
async fn a_corrupt_execution_attempt_state_is_an_error_not_a_panic() {
    let fx = Fixture::new().await;
    fx.ready_completion_attempt("request-c", "attempt-c").await;
    sqlx::query("UPDATE execution_attempts SET state = 'sleeping' WHERE id = 'attempt-c'")
        .execute(fx.repo.pool())
        .await
        .unwrap();

    assert_corrupt(
        execution_attempts::attempt_state(&fx.repo, "attempt-c").await,
        "execution_attempts",
        "state",
    );
    assert_corrupt(
        fx.repo.list_attempts_for_request("request-c").await,
        "execution_attempts",
        "state",
    );
}

#[tokio::test]
async fn a_corrupt_execution_request_state_is_an_error_not_a_panic() {
    let fx = Fixture::new().await;
    fx.ready_completion_attempt("request-f", "attempt-f").await;
    sqlx::query("UPDATE execution_requests SET state = 'sleeping' WHERE id = 'request-f'")
        .execute(fx.repo.pool())
        .await
        .unwrap();

    assert_corrupt(
        execution_requests::request_state(&fx.repo, "request-f").await,
        "execution_requests",
        "state",
    );
}

#[tokio::test]
async fn a_corrupt_custom_field_value_is_an_error_not_a_panic() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let item = make_item(&repo, &project).await;
    let field = custom_fields::create_field(
        &repo,
        project.id,
        tack_core::models::CreateCustomField {
            name: "Customer".into(),
            field_type: CustomFieldType::Text,
            description: None,
            required: None,
            default_value: None,
            options: None,
            validation: None,
        },
    )
    .await
    .unwrap();
    custom_fields::set_field_value(&repo, item.id, field.id, serde_json::json!("acme"))
        .await
        .unwrap();
    sqlx::query("UPDATE custom_field_values SET created_at = 'never'")
        .execute(repo.pool())
        .await
        .unwrap();

    assert_corrupt(
        custom_fields::get_field_value(&repo, item.id, field.id).await,
        "custom_field_values",
        "created_at",
    );
    assert_corrupt(
        custom_fields::get_all_field_values_for_item(&repo, item.id).await,
        "custom_field_values",
        "created_at",
    );
}

#[tokio::test]
async fn a_row_that_is_not_there_is_not_found_and_names_its_kind() {
    let repo = common::setup_test_db().await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let item = make_item(&repo, &project).await;
    let role = repo
        .create_role(
            project.id,
            CreateRole {
                name: "Reviewer".into(),
                color: None,
                icon: None,
            },
        )
        .await
        .unwrap();

    let no_item = repo.assign_role_to_item(Uuid::new_v4(), role.id).await;
    assert!(
        matches!(no_item, Err(StoreError::NotFound { entity: "item" })),
        "{no_item:?}"
    );
    let no_role = repo.assign_role_to_item(item.id, Uuid::new_v4()).await;
    assert!(
        matches!(no_role, Err(StoreError::NotFound { entity: "role" })),
        "{no_role:?}"
    );
    let no_field = custom_fields::get_field(&repo, Uuid::new_v4()).await;
    assert!(
        matches!(
            no_field,
            Err(StoreError::NotFound {
                entity: "custom field"
            })
        ),
        "{no_field:?}"
    );
}
