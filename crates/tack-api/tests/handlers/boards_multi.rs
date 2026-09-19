//! HTTP tests for the multi-board routes: create/list/get/update/delete and
//! the grouped board view. `crud.rs` already proves the default grouping, by
//! status, end to end; this file covers the other routes' success and
//! documented errors, and the view's other grouping modes: priority, item
//! type, assignee, sprint and custom field.

use crate::common;
use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

async fn setup() -> (axum::Router, Uuid) {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    (app, pid)
}

async fn create_board(app: &axum::Router, project_id: Uuid, name: &str) -> Value {
    let uri = format!("/api/projects/{project_id}/boards");
    let (status, board) = common::send(app, "POST", &uri, json!({"name": name}), &[]).await;
    assert_eq!(status, StatusCode::OK, "{board}");
    board
}

/// Create a board with a given `grouping` value (already JSON-shaped, e.g.
/// `json!("priority")` or `json!({"custom_field": field_id})`).
async fn create_board_with_grouping(
    app: &axum::Router,
    project_id: Uuid,
    grouping: Value,
) -> Value {
    let uri = format!("/api/projects/{project_id}/boards");
    let (status, board) = common::send(
        app,
        "POST",
        &uri,
        json!({"name": "Grouped", "grouping": grouping}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{board}");
    board
}

/// Create an item with fields the default `common::create_item` doesn't expose
/// (priority, assignee, sprint_id) — needed to exercise the view's other
/// grouping modes.
async fn create_item_with(app: &axum::Router, project_id: Uuid, mut fields: Value) -> Value {
    let body = fields.as_object_mut().unwrap();
    body.entry("title").or_insert(json!("Item"));
    body.entry("item_type").or_insert(json!("task"));
    let uri = format!("/api/projects/{project_id}/items");
    let (status, item) = common::send(app, "POST", &uri, Value::Object(body.clone()), &[]).await;
    assert_eq!(status, StatusCode::OK, "{item}");
    item
}

/// Find the named column in a board view's `columns` array and return its items.
fn column<'a>(view: &'a Value, name: &str) -> &'a [Value] {
    view["columns"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == name)
        .unwrap_or_else(|| panic!("no column named {name:?} in {view}"))["items"]
        .as_array()
        .unwrap()
}

// ─── POST /api/projects/{project_id}/boards ─────────────────────────────────

#[tokio::test]
async fn create_board_outcomes() {
    let (app, pid) = setup().await;
    let cases: Vec<(&str, Uuid, Value, StatusCode)> = vec![
        (
            "valid name is created",
            pid,
            json!({"name": "Sprint Board"}),
            StatusCode::OK,
        ),
        (
            "empty name fails validation",
            pid,
            json!({"name": ""}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "unknown project is 404",
            Uuid::new_v4(),
            json!({"name": "X"}),
            StatusCode::NOT_FOUND,
        ),
    ];
    for (name, project_id, body, expected) in cases {
        let uri = format!("/api/projects/{project_id}/boards");
        let (status, resp) = common::send(&app, "POST", &uri, body, &[]).await;
        assert_eq!(status, expected, "{name}: {resp}");
    }

    let (_, boards) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/boards"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(
        boards.as_array().unwrap().len(),
        1,
        "only the valid row must have written a board"
    );
}

// ─── GET /api/projects/{project_id}/boards ──────────────────────────────────

#[tokio::test]
async fn list_boards_returns_created_board() {
    let (app, pid) = setup().await;
    create_board(&app, pid, "Sprint Board").await;

    let (status, boards) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/boards"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(boards[0]["name"], "Sprint Board");
}

// ─── GET /api/boards/{id} ────────────────────────────────────────────────────

#[tokio::test]
async fn get_board_found_and_unknown() {
    let (app, pid) = setup().await;
    let board = create_board(&app, pid, "Sprint Board").await;
    let bid = board["id"].as_str().unwrap();

    let (status, got) =
        common::send(&app, "GET", &format!("/api/boards/{bid}"), Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["name"], "Sprint Board");

    let (status, _) = common::send(
        &app,
        "GET",
        &format!("/api/boards/{}", Uuid::new_v4()),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── PATCH /api/boards/{id} ──────────────────────────────────────────────────

#[tokio::test]
async fn update_board_persists_and_rejects_invalid_name() {
    let (app, pid) = setup().await;
    let board = create_board(&app, pid, "Sprint Board").await;
    let bid = board["id"].as_str().unwrap().to_owned();

    let (status, updated) = common::send(
        &app,
        "PATCH",
        &format!("/api/boards/{bid}"),
        json!({"name": "Renamed"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["name"], "Renamed");

    let (status, _) = common::send(
        &app,
        "PATCH",
        &format!("/api/boards/{bid}"),
        json!({"name": ""}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (_, still) =
        common::send(&app, "GET", &format!("/api/boards/{bid}"), Value::Null, &[]).await;
    assert_eq!(
        still["name"], "Renamed",
        "a rejected update must not change the stored name"
    );
}

// ─── DELETE /api/boards/{id} ─────────────────────────────────────────────────

#[tokio::test]
async fn delete_board_removes_it() {
    let (app, pid) = setup().await;
    let board = create_board(&app, pid, "Sprint Board").await;
    let bid = board["id"].as_str().unwrap().to_owned();

    let (status, _) = common::send(
        &app,
        "DELETE",
        &format!("/api/boards/{bid}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) =
        common::send(&app, "GET", &format!("/api/boards/{bid}"), Value::Null, &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "board must be gone");
}

// ─── GET /api/boards/{id}/view ───────────────────────────────────────────────

#[tokio::test]
async fn get_board_view_groups_items_and_rejects_unknown_board() {
    let (app, pid) = setup().await;
    let board = create_board(&app, pid, "Sprint Board").await;
    let bid = board["id"].as_str().unwrap().to_owned();
    common::send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/items"),
        json!({"title": "Item", "item_type": "task"}),
        &[],
    )
    .await;

    let (status, view) = common::send(
        &app,
        "GET",
        &format!("/api/boards/{bid}/view"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let columns = view["columns"].as_array().unwrap();
    let total: usize = columns
        .iter()
        .map(|c| c["items"].as_array().unwrap().len())
        .sum();
    assert_eq!(total, 1, "the one item must appear in exactly one column");

    let (status, _) = common::send(
        &app,
        "GET",
        &format!("/api/boards/{}/view", Uuid::new_v4()),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── GET /api/boards/{id}/view — grouping modes ─────────────────────────────

/// Priority grouping always emits all five priority columns, even empty
/// ones, unlike item-type grouping below — pins that every item lands in the
/// column matching its own priority.
#[tokio::test]
async fn get_board_view_groups_by_priority() {
    let (app, pid) = setup().await;
    let board = create_board_with_grouping(&app, pid, json!("priority")).await;
    let bid = board["id"].as_str().unwrap();
    create_item_with(&app, pid, json!({"priority": "high"})).await;

    let (status, view) = common::send(
        &app,
        "GET",
        &format!("/api/boards/{bid}/view"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let columns = view["columns"].as_array().unwrap();
    assert_eq!(
        columns.len(),
        5,
        "all five priority columns are always present"
    );
    assert_eq!(column(&view, "high").len(), 1);
    assert!(column(&view, "medium").is_empty());
}

/// Item-type grouping, unlike priority, drops columns with no items — pins
/// that only the type actually used shows up.
#[tokio::test]
async fn get_board_view_groups_by_item_type() {
    let (app, pid) = setup().await;
    let board = create_board_with_grouping(&app, pid, json!("item_type")).await;
    let bid = board["id"].as_str().unwrap();
    create_item_with(&app, pid, json!({"item_type": "bug"})).await;

    let (status, view) = common::send(
        &app,
        "GET",
        &format!("/api/boards/{bid}/view"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let columns = view["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 1, "empty item-type columns are omitted");
    assert_eq!(column(&view, "bug").len(), 1);
}

/// Assignee grouping buckets by the free-text `assignee` field, with an
/// unset assignee falling into an "Unassigned" lane.
#[tokio::test]
async fn get_board_view_groups_by_assignee() {
    let (app, pid) = setup().await;
    let board = create_board_with_grouping(&app, pid, json!("assignee")).await;
    let bid = board["id"].as_str().unwrap();
    create_item_with(&app, pid, json!({"assignee": "Alice"})).await;
    create_item_with(&app, pid, json!({})).await;

    let (status, view) = common::send(
        &app,
        "GET",
        &format!("/api/boards/{bid}/view"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(column(&view, "Alice").len(), 1);
    assert_eq!(column(&view, "Unassigned").len(), 1);
}

/// Sprint grouping always leads with a "Backlog" column for items with no
/// sprint, then one column per sprint the project has.
#[tokio::test]
async fn get_board_view_groups_by_sprint() {
    let (app, pid) = setup().await;
    let (status, sprint) = common::send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/sprints"),
        json!({"name": "Sprint 1"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{sprint}");
    let sprint_id = sprint["id"].as_str().unwrap();

    let board = create_board_with_grouping(&app, pid, json!("sprint")).await;
    let bid = board["id"].as_str().unwrap();
    create_item_with(&app, pid, json!({"sprint_id": sprint_id})).await;
    create_item_with(&app, pid, json!({})).await;

    let (status, view) = common::send(
        &app,
        "GET",
        &format!("/api/boards/{bid}/view"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let columns = view["columns"].as_array().unwrap();
    assert_eq!(columns[0]["name"], "Backlog");
    assert_eq!(column(&view, "Backlog").len(), 1);
    assert_eq!(column(&view, "Sprint 1").len(), 1);
}

/// Custom-field grouping buckets by the value's own JSON rendering, with
/// items that never got a value for this field landing in "Unset".
#[tokio::test]
async fn get_board_view_groups_by_custom_field() {
    let (app, pid) = setup().await;
    let (status, field) = common::send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/custom-fields"),
        json!({"name": "Score", "field_type": "number"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{field}");
    let field_id = field["id"].as_str().unwrap().to_owned();

    let board = create_board_with_grouping(&app, pid, json!({"custom_field": field_id})).await;
    let bid = board["id"].as_str().unwrap();
    let valued = create_item_with(&app, pid, json!({})).await;
    let valued_id = valued["id"].as_str().unwrap();
    create_item_with(&app, pid, json!({})).await; // never gets a value

    common::send(
        &app,
        "PUT",
        &format!("/api/items/{valued_id}/custom-fields/{field_id}"),
        json!(5),
        &[],
    )
    .await;

    let (status, view) = common::send(
        &app,
        "GET",
        &format!("/api/boards/{bid}/view"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(column(&view, "5").len(), 1);
    assert_eq!(column(&view, "Unset").len(), 1);
}

/// Custom-field grouping on a field id that doesn't exist is a 404, not a
/// silently empty board.
#[tokio::test]
async fn get_board_view_custom_field_grouping_unknown_field_is_404() {
    let (app, pid) = setup().await;
    let board =
        create_board_with_grouping(&app, pid, json!({"custom_field": Uuid::new_v4()})).await;
    let bid = board["id"].as_str().unwrap();

    let (status, _) = common::send(
        &app,
        "GET",
        &format!("/api/boards/{bid}/view"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
