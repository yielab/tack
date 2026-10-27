//! The routes no other handler test names: `/api/debug/*`, the project's item tree, deleting a
//! dependency, and assigning or removing an item's role.

use crate::common;
use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

async fn create_item(app: &axum::Router, project_id: Uuid, body: Value) -> String {
    let (status, item) = common::send(
        app,
        "POST",
        &format!("/api/projects/{project_id}/items"),
        body,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{item}");
    item["id"].as_str().unwrap().to_owned()
}

// ─── /api/debug/* ────────────────────────────────────────────────────────────

#[tokio::test]
async fn debug_info_reports_the_build_without_the_database_url() {
    let (app, _) = common::test_app().await;
    let (status, body) = common::send(&app, "GET", "/api/debug/info", Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(body["build"], "debug");
    assert!(
        body["database"]["size_bytes"].as_i64().unwrap_or(0) > 0,
        "{body}"
    );
    assert_eq!(body["config"]["log_level"], "info");
    assert!(
        body["database"].get("url").is_none(),
        "the database url can hold a path or a credential and must not leave: {body}"
    );
}

#[tokio::test]
async fn debug_routes_sit_behind_the_api_token() {
    let (app, _) = common::test_app_with_config(tack_api::config::AppConfig {
        api_token: Some("debug-token".into()),
        ..Default::default()
    })
    .await;
    for uri in ["/api/debug/info", "/api/debug/db-stats"] {
        let (status, body) = common::send(&app, "GET", uri, Value::Null, &[]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}: {body}");
    }
    let (status, _) = common::send(
        &app,
        "GET",
        "/api/debug/db-stats",
        Value::Null,
        &[("authorization", "Bearer debug-token")],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn db_stats_counts_rows_per_table() {
    let (app, _) = common::test_app().await;
    let (status, empty) = common::send(&app, "GET", "/api/debug/db-stats", Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK, "{empty}");
    assert_eq!(empty["tables"]["projects"], 0, "{empty}");
    assert_eq!(empty["tables"]["items"], 0, "{empty}");

    let pid = common::create_project(&app, "P", "software").await;
    let a = create_item(&app, pid, json!({"title": "A"})).await;
    let b = create_item(&app, pid, json!({"title": "B"})).await;
    let (status, _) = common::send(
        &app,
        "POST",
        &format!("/api/items/{a}/comments"),
        json!({"content": "hi", "author": "ana"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = common::send(
        &app,
        "POST",
        &format!("/api/items/{a}/dependencies"),
        json!({"target_item_id": b, "dependency_type": "blocks"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, stats) = common::send(&app, "GET", "/api/debug/db-stats", Value::Null, &[]).await;
    let tables = &stats["tables"];
    assert_eq!(tables["projects"], 1, "{stats}");
    assert_eq!(tables["items"], 2, "{stats}");
    assert_eq!(tables["comments"], 1, "{stats}");
    assert_eq!(tables["dependencies"], 1, "{stats}");
    assert_eq!(tables["attachments"], 0, "{stats}");
}

// ─── GET /api/projects/{id}/items/tree ───────────────────────────────────────

#[tokio::test]
async fn item_tree_lists_parents_before_their_children_in_sort_order() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let epic = create_item(&app, pid, json!({"title": "Epic", "item_type": "epic"})).await;
    let child = create_item(&app, pid, json!({"title": "Child", "parent_id": epic})).await;
    let other = create_item(&app, pid, json!({"title": "Other root"})).await;

    // An item of another project never appears.
    let other_project = common::create_project(&app, "Q", "software").await;
    create_item(&app, other_project, json!({"title": "Elsewhere"})).await;

    let (status, tree) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/items/tree"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{tree}");
    let rows = tree.as_array().unwrap();
    let ids: Vec<&str> = rows.iter().map(|i| i["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), 3, "{tree}");
    assert_eq!(
        ids.last().copied(),
        Some(child.as_str()),
        "the root items come first, then the children: {tree}"
    );
    assert!(ids.contains(&epic.as_str()) && ids.contains(&other.as_str()));
    let child_row = rows.iter().find(|i| i["id"] == child.as_str()).unwrap();
    assert_eq!(child_row["parent_id"], epic.as_str());
}

#[tokio::test]
async fn item_tree_of_a_project_without_items_is_empty_and_a_bad_id_is_refused() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "Empty", "software").await;
    let (status, tree) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/items/tree"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{tree}");
    assert_eq!(tree, json!([]));

    let (status, _) = common::send(
        &app,
        "GET",
        "/api/projects/not-a-uuid/items/tree",
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ─── DELETE /api/items/{item_id}/dependencies/{dep_id} ───────────────────────

async fn add_dependency(app: &axum::Router, from: &str, to: &str) -> String {
    let (status, dep) = common::send(
        app,
        "POST",
        &format!("/api/items/{from}/dependencies"),
        json!({"target_item_id": to, "dependency_type": "blocks"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{dep}");
    dep["id"].as_str().unwrap().to_owned()
}

async fn dependency_count(app: &axum::Router, item: &str) -> usize {
    let (_, deps) = common::send(
        app,
        "GET",
        &format!("/api/items/{item}/dependencies"),
        Value::Null,
        &[],
    )
    .await;
    deps.as_array().unwrap().len()
}

#[tokio::test]
async fn deleting_a_dependency_removes_the_edge_and_a_second_delete_is_404() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let a = create_item(&app, pid, json!({"title": "A"})).await;
    let b = create_item(&app, pid, json!({"title": "B"})).await;
    let dep = add_dependency(&app, &a, &b).await;
    assert_eq!(dependency_count(&app, &a).await, 1);

    let uri = format!("/api/items/{a}/dependencies/{dep}");
    let (status, body) = common::send(&app, "DELETE", &uri, Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["deleted"], true);
    assert_eq!(dependency_count(&app, &a).await, 0);

    let (status, body) = common::send(&app, "DELETE", &uri, Value::Null, &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(
        body["error"]["message"].as_str().unwrap().contains(&dep),
        "the refusal names the dependency: {body}"
    );
}

#[tokio::test]
async fn deleting_a_dependency_with_a_bad_id_is_refused() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let a = create_item(&app, pid, json!({"title": "A"})).await;
    let (status, _) = common::send(
        &app,
        "DELETE",
        &format!("/api/items/{a}/dependencies/not-a-uuid"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// The dependency belongs to item A's path; item C's path must not reach it.
#[tokio::test]
#[ignore = "defect: DELETE /api/items/{item_id}/dependencies/{dep_id} ignores item_id and deletes a dependency of another item"]
async fn deleting_a_dependency_through_another_items_path_is_refused() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let a = create_item(&app, pid, json!({"title": "A"})).await;
    let b = create_item(&app, pid, json!({"title": "B"})).await;
    let c = create_item(&app, pid, json!({"title": "C"})).await;
    let dep = add_dependency(&app, &a, &b).await;

    let (status, body) = common::send(
        &app,
        "DELETE",
        &format!("/api/items/{c}/dependencies/{dep}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert_eq!(dependency_count(&app, &a).await, 1);
}

// ─── PUT / DELETE /api/items/{item_id}/roles/{role_id} ───────────────────────

async fn create_role(app: &axum::Router, project_id: Uuid, name: &str) -> String {
    let (status, role) = common::send(
        app,
        "POST",
        &format!("/api/projects/{project_id}/roles"),
        json!({"name": name}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{role}");
    role["id"].as_str().unwrap().to_owned()
}

async fn role_names(app: &axum::Router, item: &str) -> Vec<String> {
    let (status, item) =
        common::send(app, "GET", &format!("/api/items/{item}"), Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    item["roles"]
        .as_array()
        .expect("the item carries its roles")
        .iter()
        .map(|r| r["name"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn assigning_and_removing_a_role_shows_on_the_item() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let item = create_item(&app, pid, json!({"title": "A"})).await;
    let dev = create_role(&app, pid, "Backend Dev").await;
    let qa = create_role(&app, pid, "QA").await;

    let (status, body) = common::send(
        &app,
        "PUT",
        &format!("/api/items/{item}/roles/{dev}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["assigned"], true);
    // Assigning twice is not an error and does not duplicate the role.
    let (status, _) = common::send(
        &app,
        "PUT",
        &format!("/api/items/{item}/roles/{dev}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    common::send(
        &app,
        "PUT",
        &format!("/api/items/{item}/roles/{qa}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(role_names(&app, &item).await, ["Backend Dev", "QA"]);

    let (status, body) = common::send(
        &app,
        "DELETE",
        &format!("/api/items/{item}/roles/{dev}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["removed"], true);
    assert_eq!(role_names(&app, &item).await, ["QA"]);
}

/// The role is for another item's project, or gone: a client error, not a server fault.
#[tokio::test]
#[ignore = "defect: PUT /api/items/{item_id}/roles/{role_id} answers 500 (foreign-key failure) for a role that does not exist; it should be 404"]
async fn assigning_a_role_that_does_not_exist_is_404_and_nothing_is_assigned() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let item = create_item(&app, pid, json!({"title": "A"})).await;

    let (status, body) = common::send(
        &app,
        "PUT",
        &format!("/api/items/{item}/roles/{}", Uuid::new_v4()),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    assert!(role_names(&app, &item).await.is_empty());
}

#[tokio::test]
async fn assigning_a_role_with_a_bad_id_is_refused() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let item = create_item(&app, pid, json!({"title": "A"})).await;
    let (status, _) = common::send(
        &app,
        "PUT",
        &format!("/api/items/{item}/roles/not-a-uuid"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(role_names(&app, &item).await.is_empty());
}
