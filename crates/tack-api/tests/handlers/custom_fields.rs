//! HTTP tests for the custom-field-definition and custom-field-value routes.
//! Per-type/per-rule value validation already has its table in
//! `handlers/crud.rs`; this file proves each route's success and
//! documented-error outcomes without repeating that matrix.

use crate::common;
use axum::Router;
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::handlers::websocket::BoardEvent;
use tack_api::{AppState, config::AppConfig, router::build_router};
use tokio::sync::broadcast;
use uuid::Uuid;

async fn setup() -> (axum::Router, Uuid) {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    (app, pid)
}

/// A test app built the same way `common::test_app` is, with the pool handed
/// back so a test can sever it mid-test (`common::test_app` doesn't expose
/// it) — used to pin the database-failure branches on the routes below that
/// have no existence pre-check to catch a closed pool first.
async fn setup_with_pool() -> (Router, sqlx::SqlitePool) {
    let repo = tack_test_support::setup_test_db().await;
    let workspace_id = tack_test_support::create_test_workspace(&repo).await;
    let pool = repo.pool().clone();

    let (tx, _rx) = broadcast::channel::<BoardEvent>(16);
    let config = AppConfig {
        database_url: "sqlite::memory:".to_string(),
        ..AppConfig::default()
    };
    let state = AppState {
        repo,
        config,
        workspace_id,
        broadcast_tx: tx,
        webhook: None,
        local_runner: None,
    };
    (build_router(state), pool)
}

async fn create_field(app: &axum::Router, project_id: Uuid, body: Value) -> Value {
    let uri = format!("/api/projects/{project_id}/custom-fields");
    let (status, field) = common::send(app, "POST", &uri, body, &[]).await;
    assert_eq!(status, StatusCode::OK, "{field}");
    field
}

// ─── POST /api/projects/{project_id}/custom-fields ─────────────────────────

#[tokio::test]
async fn create_field_persists_and_rejects_unknown_project() {
    let (app, pid) = setup().await;
    let created = create_field(&app, pid, json!({"name":"Score","field_type":"number"})).await;
    assert_eq!(created["name"], "Score");
    assert_eq!(created["project_id"], pid.to_string());

    let (status, _) = common::send(
        &app,
        "POST",
        &format!("/api/projects/{}/custom-fields", Uuid::new_v4()),
        json!({"name":"X","field_type":"text"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── GET /api/projects/{project_id}/custom-fields ───────────────────────────

#[tokio::test]
async fn list_fields_returns_created_field() {
    let (app, pid) = setup().await;
    create_field(&app, pid, json!({"name":"Score","field_type":"number"})).await;

    let (status, list) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/custom-fields"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list.as_array().unwrap().len(), 1);
}

// ─── GET /api/custom-fields/{id} ────────────────────────────────────────────

#[tokio::test]
async fn get_field_found_and_unknown() {
    let (app, pid) = setup().await;
    let field = create_field(&app, pid, json!({"name":"Score","field_type":"number"})).await;
    let fid = field["id"].as_str().unwrap();

    let (status, got) = common::send(
        &app,
        "GET",
        &format!("/api/custom-fields/{fid}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["name"], "Score");

    let (status, _) = common::send(
        &app,
        "GET",
        &format!("/api/custom-fields/{}", Uuid::new_v4()),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── PATCH /api/custom-fields/{id} ──────────────────────────────────────────

#[tokio::test]
async fn update_field_persists_and_500s_on_unknown_id() {
    let (app, pid) = setup().await;
    let field = create_field(&app, pid, json!({"name":"Score","field_type":"number"})).await;
    let fid = field["id"].as_str().unwrap().to_owned();

    let (status, updated) = common::send(
        &app,
        "PATCH",
        &format!("/api/custom-fields/{fid}"),
        json!({"name":"Renamed"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["name"], "Renamed");

    // Documented as 500 ("Update failed") — the handler re-fetches after
    // writing and an unknown id has no row to re-fetch.
    let (status, _) = common::send(
        &app,
        "PATCH",
        &format!("/api/custom-fields/{}", Uuid::new_v4()),
        json!({"name":"Nowhere"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

// ─── DELETE /api/custom-fields/{id} ─────────────────────────────────────────

#[tokio::test]
async fn delete_field_removes_it_and_is_idempotent() {
    let (app, pid) = setup().await;
    let field = create_field(&app, pid, json!({"name":"Score","field_type":"number"})).await;
    let fid = field["id"].as_str().unwrap().to_owned();

    let (status, _) = common::send(
        &app,
        "DELETE",
        &format!("/api/custom-fields/{fid}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, list) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/custom-fields"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(list.as_array().unwrap().is_empty(), "field must be gone");

    // Deleting again is a no-op, not an error.
    let (status, _) = common::send(
        &app,
        "DELETE",
        &format!("/api/custom-fields/{fid}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

// ─── PUT /api/items/{item_id}/custom-fields/{field_id} ──────────────────────

#[tokio::test]
async fn set_field_value_persists_and_rejects_missing_item_or_field() {
    let (app, pid) = setup().await;
    let field = create_field(&app, pid, json!({"name":"Score","field_type":"number"})).await;
    let fid = field["id"].as_str().unwrap().to_owned();
    let iid = common::create_item(&app, pid, "Item").await;

    let (status, value) = common::send(
        &app,
        "PUT",
        &format!("/api/items/{iid}/custom-fields/{fid}"),
        json!(42),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["value"], 42);

    let cases = [
        format!("/api/items/{}/custom-fields/{fid}", Uuid::new_v4()),
        format!("/api/items/{iid}/custom-fields/{}", Uuid::new_v4()),
    ];
    for uri in cases {
        let (status, _) = common::send(&app, "PUT", &uri, json!(1), &[]).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
    }

    // One documented-error row: value fails the field's own type check
    // (the full type/rule matrix lives in crud.rs).
    let (status, _) = common::send(
        &app,
        "PUT",
        &format!("/api/items/{iid}/custom-fields/{fid}"),
        json!("not a number"),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

// ─── GET /api/items/{item_id}/custom-fields ─────────────────────────────────

#[tokio::test]
async fn get_all_field_values_returns_what_was_set() {
    let (app, pid) = setup().await;
    let field = create_field(&app, pid, json!({"name":"Score","field_type":"number"})).await;
    let fid = field["id"].as_str().unwrap().to_owned();
    let iid = common::create_item(&app, pid, "Item").await;
    common::send(
        &app,
        "PUT",
        &format!("/api/items/{iid}/custom-fields/{fid}"),
        json!(7),
        &[],
    )
    .await;

    let (status, values) = common::send(
        &app,
        "GET",
        &format!("/api/items/{iid}/custom-fields"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr = values.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["value"], 7);
}

// ─── GET /api/items/{item_id}/custom-fields/{field_id} ──────────────────────

#[tokio::test]
async fn get_field_value_found_and_unknown() {
    let (app, pid) = setup().await;
    let field = create_field(&app, pid, json!({"name":"Score","field_type":"number"})).await;
    let fid = field["id"].as_str().unwrap().to_owned();
    let iid = common::create_item(&app, pid, "Item").await;
    common::send(
        &app,
        "PUT",
        &format!("/api/items/{iid}/custom-fields/{fid}"),
        json!(9),
        &[],
    )
    .await;

    let (status, value) = common::send(
        &app,
        "GET",
        &format!("/api/items/{iid}/custom-fields/{fid}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["value"], 9);

    // No value was ever set on this field for a bare new item.
    let other_item = common::create_item(&app, pid, "Other").await;
    let (status, _) = common::send(
        &app,
        "GET",
        &format!("/api/items/{other_item}/custom-fields/{fid}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── DELETE /api/items/{item_id}/custom-fields/{field_id} ───────────────────

#[tokio::test]
async fn delete_field_value_removes_it() {
    let (app, pid) = setup().await;
    let field = create_field(&app, pid, json!({"name":"Score","field_type":"number"})).await;
    let fid = field["id"].as_str().unwrap().to_owned();
    let iid = common::create_item(&app, pid, "Item").await;
    common::send(
        &app,
        "PUT",
        &format!("/api/items/{iid}/custom-fields/{fid}"),
        json!(5),
        &[],
    )
    .await;

    let (status, _) = common::send(
        &app,
        "DELETE",
        &format!("/api/items/{iid}/custom-fields/{fid}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = common::send(
        &app,
        "GET",
        &format!("/api/items/{iid}/custom-fields/{fid}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "value must be gone");
}

// ─── database failures ───────────────────────────────────────────────────

/// `list_fields`, `delete_field`, `get_all_field_values` and
/// `delete_field_value` each make their database call with no existence
/// pre-check to catch a failure first — closing the pool out from under a
/// live router pins that every one reports 500, not a hang or a panic.
#[tokio::test]
async fn db_failure_on_routes_without_a_precheck_is_500() {
    let (app, pool) = setup_with_pool().await;
    let pid = common::create_project(&app, "P", "software").await;
    let field = create_field(&app, pid, json!({"name":"Score","field_type":"number"})).await;
    let fid = field["id"].as_str().unwrap().to_owned();
    let iid = common::create_item(&app, pid, "Item").await;

    pool.close().await;

    let cases = [
        ("GET", format!("/api/projects/{pid}/custom-fields")),
        ("DELETE", format!("/api/custom-fields/{fid}")),
        ("GET", format!("/api/items/{iid}/custom-fields")),
        ("DELETE", format!("/api/items/{iid}/custom-fields/{fid}")),
    ];
    for (method, uri) in cases {
        let (status, resp) = common::send(&app, method, &uri, Value::Null, &[]).await;
        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "{method} {uri}: {resp}"
        );
    }
}
