//! A new item, sprint, role or field reads the keys its project has taken and then writes its own. Rows added at
//! once whose names give the same key must each get a key of their own: on a plain connection two writers read the
//! same free key and the second failed against the unique index.

use serde_json::{Value, json};
use tack_db::Repository;
use tack_test_support::{create_test_workspace, make_project, setup_file_test_db};
use uuid::Uuid;

const AT_ONCE: usize = 12;

/// The names of rows added at once: different names that all give the key `ops`.
fn names() -> Vec<String> {
    (1..=AT_ONCE)
        .map(|n| format!("Ops{}", "!".repeat(n)))
        .collect()
}

fn input<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).unwrap()
}

async fn keys(repo: &Repository, table: &str, project: Uuid) -> Vec<String> {
    let mut keys: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT key FROM {table} WHERE project_id = ?"
    )))
    .bind(project.to_string())
    .fetch_all(repo.pool())
    .await
    .unwrap();
    keys.sort();
    keys.dedup();
    keys
}

#[tokio::test]
async fn rows_added_at_once_whose_names_give_one_key_each_get_their_own() {
    let dir = tempfile::tempdir().unwrap();
    let repo = setup_file_test_db(&dir.path().join("key-race.db")).await;
    let ws = create_test_workspace(&repo).await;
    let project = make_project(&repo, ws).await;
    let pid = project.id;

    let mut adds = tokio::task::JoinSet::new();
    for name in names() {
        let repo = repo.clone();
        adds.spawn(async move {
            let item = repo
                .create_item(pid, "backlog", input(json!({ "title": name })))
                .await
                .map(drop)
                .map_err(|e| format!("item {name}: {e}"));
            let sprint = repo
                .create_sprint(pid, input(json!({ "name": name })))
                .await
                .map(drop)
                .map_err(|e| format!("sprint {name}: {e}"));
            let role = repo
                .create_role(pid, input(json!({ "name": name })))
                .await
                .map(drop)
                .map_err(|e| format!("role {name}: {e}"));
            let field = repo
                .create_field(pid, input(json!({ "name": name, "field_type": "text" })))
                .await
                .map(drop)
                .map_err(|e| format!("field {name}: {e}"));
            [item, sprint, role, field]
        });
    }
    let failed: Vec<String> = adds
        .join_all()
        .await
        .into_iter()
        .flatten()
        .filter_map(Result::err)
        .collect();
    assert!(failed.is_empty(), "every row is stored: {failed:#?}");

    for table in ["items", "sprints", "roles", "custom_field_definitions"] {
        assert_eq!(
            keys(&repo, table, pid).await.len(),
            AT_ONCE,
            "{table}: one key each"
        );
    }
}
