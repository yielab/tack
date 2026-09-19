//! HTTP tests for the template routes: create, list, get, delete, and
//! creating a project from a template (the "apply" route — there is no
//! separate instantiate endpoint) plus saving a project as a new template.
//! Workflow-shape and select-option validation, the not-found paths, and
//! the built-in-template delete rule each get their own case.

use crate::common;
use axum::Router;
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_api::handlers::websocket::BoardEvent;
use tack_api::{AppState, config::AppConfig, router::build_router};
use tack_db::repo;
use tokio::sync::broadcast;
use uuid::Uuid;

async fn setup() -> (axum::Router, Uuid) {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    (app, pid)
}

async fn create_template(app: &axum::Router, body: Value) -> Value {
    let (status, template) = common::send(app, "POST", "/api/templates", body, &[]).await;
    assert_eq!(status, StatusCode::OK, "{template}");
    template
}

fn minimal_template_body(name: &str) -> Value {
    json!({"name": name, "project_type": "software"})
}

/// A well-formed `WorkflowConfig` with one status in each required category —
/// the shape `WorkflowConfig::validate` demands.
fn valid_workflow() -> Value {
    json!({
        "workflow_type": "kanban",
        "statuses": [
            {"name": "To Do", "category": "todo", "wip_limit": null, "order": 0},
            {"name": "Doing", "category": "in_progress", "wip_limit": null, "order": 1},
            {"name": "Done", "category": "done", "wip_limit": null, "order": 2},
        ],
        "transitions": null,
    })
}

// ─── POST /api/templates ────────────────────────────────────────────────────

#[tokio::test]
async fn create_template_persists_minimal_fields() {
    let (app, _) = common::test_app().await;
    let created = create_template(&app, minimal_template_body("Blank")).await;
    assert_eq!(created["name"], "Blank");
    assert_eq!(created["project_type"], "software");
    assert_eq!(
        created["is_builtin"], false,
        "templates created over HTTP are never builtin"
    );
}

/// A template's own field rules are enforced: an empty name is 422 and
/// nothing is stored.
#[tokio::test]
async fn create_template_rejects_an_empty_name() {
    let (app, _) = common::test_app().await;
    let (status, _) = common::send(
        &app,
        "POST",
        "/api/templates",
        json!({"name": "", "project_type": "software"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (_, listed) = common::send(&app, "GET", "/api/templates", Value::Null, &[]).await;
    assert!(
        listed
            .as_array()
            .expect("a bare array")
            .iter()
            .all(|template| template["name"] != ""),
        "{listed}"
    );
}

/// A workflow that fails `WorkflowConfig::validate` (no `in_progress` or
/// `done` status) is rejected with 422 and never reaches storage.
#[tokio::test]
async fn create_template_rejects_invalid_workflow_shape() {
    let (app, _) = common::test_app().await;
    let mut body = minimal_template_body("Bad Workflow");
    body["workflow"] = json!({
        "workflow_type": "kanban",
        "statuses": [
            {"name": "Only Todo", "category": "todo", "wip_limit": null, "order": 0},
        ],
        "transitions": null,
    });
    let (status, resp) = common::send(&app, "POST", "/api/templates", body, &[]).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{resp}");

    let (_, list) = common::send(&app, "GET", "/api/templates", Value::Null, &[]).await;
    assert!(
        list.as_array().unwrap().is_empty(),
        "a rejected template must not be stored"
    );
}

#[tokio::test]
async fn create_template_accepts_valid_workflow_shape() {
    let (app, _) = common::test_app().await;
    let mut body = minimal_template_body("Good Workflow");
    body["workflow"] = valid_workflow();
    let created = create_template(&app, body).await;
    assert_eq!(created["workflow"]["workflow_type"], "kanban");
}

/// A `select`/`multi_select` custom field with no `options` is rejected —
/// table-driven because both field types hit the same validation branch.
#[tokio::test]
async fn create_template_rejects_select_fields_without_options() {
    let cases = [("select", json!(null)), ("multi_select", json!([]))];
    for (field_type, options) in cases {
        let (app, _) = common::test_app().await;
        let body = json!({
            "name": "T",
            "project_type": "software",
            "custom_fields": [{
                "id": Uuid::new_v4(),
                "project_id": null,
                "name": "Pick",
                "field_type": field_type,
                "description": null,
                "required": false,
                "default_value": null,
                "options": options,
                "validation": null,
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z",
            }],
        });
        let (status, resp) = common::send(&app, "POST", "/api/templates", body, &[]).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{field_type}: {resp}"
        );
    }
}

#[tokio::test]
async fn create_template_accepts_select_field_with_options() {
    let (app, _) = common::test_app().await;
    let body = json!({
        "name": "T",
        "project_type": "software",
        "custom_fields": [{
            "id": Uuid::new_v4(),
            "project_id": null,
            "name": "Pick",
            "field_type": "select",
            "description": null,
            "required": false,
            "default_value": null,
            "options": ["a", "b"],
            "validation": null,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
        }],
    });
    let created = create_template(&app, body).await;
    assert_eq!(created["custom_fields"][0]["name"], "Pick");
}

// ─── GET /api/templates ─────────────────────────────────────────────────────

#[tokio::test]
async fn list_templates_returns_created_and_filters_by_project_type() {
    let (app, _) = common::test_app().await;
    create_template(&app, minimal_template_body("Soft")).await;
    let mut web_body = minimal_template_body("Web One");
    web_body["project_type"] = json!("web");
    create_template(&app, web_body).await;

    let (status, all) = common::send(&app, "GET", "/api/templates", Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all.as_array().unwrap().len(), 2);

    let (status, software_only) = common::send(
        &app,
        "GET",
        "/api/templates?project_type=software",
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr = software_only.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "Soft");
}

// ─── GET /api/templates/{id} ────────────────────────────────────────────────

#[tokio::test]
async fn get_template_found_and_unknown() {
    let (app, _) = common::test_app().await;
    let template = create_template(&app, minimal_template_body("T")).await;
    let tid = template["id"].as_str().unwrap();

    let (status, got) = common::send(
        &app,
        "GET",
        &format!("/api/templates/{tid}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(got["name"], "T");

    let (status, _) = common::send(
        &app,
        "GET",
        &format!("/api/templates/{}", Uuid::new_v4()),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── DELETE /api/templates/{id} ─────────────────────────────────────────────

#[tokio::test]
async fn delete_template_removes_a_user_created_template() {
    let (app, _) = common::test_app().await;
    let template = create_template(&app, minimal_template_body("Deleteme")).await;
    let tid = template["id"].as_str().unwrap().to_owned();

    let (status, _) = common::send(
        &app,
        "DELETE",
        &format!("/api/templates/{tid}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = common::send(
        &app,
        "GET",
        &format!("/api/templates/{tid}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "template must be gone");
}

/// `DELETE /api/templates/{id}` on a builtin template returns 204 exactly
/// like a real deletion, but the repository's `WHERE is_builtin = 0` clause
/// silently matches zero rows, so the row survives — the same "success"
/// response an unknown id gets below. Pins current behaviour; see the
/// handler-note finding in the task report.
#[tokio::test]
async fn delete_template_on_builtin_template_reports_success_but_keeps_the_row() {
    let (app, pool) = setup_with_builtins().await;
    let builtin_id = builtin_template_id(&pool).await;

    let (status, _) = common::send(
        &app,
        "DELETE",
        &format!("/api/templates/{builtin_id}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let still_there = repo::templates::get_template(&pool, builtin_id).await;
    assert!(
        still_there.is_ok(),
        "the builtin row must still exist even though the route reported success"
    );
}

/// `DELETE` on an id that was never a template also reports 204 — the
/// handler never distinguishes "deleted" from "matched nothing" because the
/// repository layer doesn't report affected-row counts. Pins current
/// behaviour; see the handler-note finding in the task report.
#[tokio::test]
async fn delete_template_unknown_id_also_reports_success() {
    let (app, _) = common::test_app().await;
    let (status, _) = common::send(
        &app,
        "DELETE",
        &format!("/api/templates/{}", Uuid::new_v4()),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

// ─── POST /api/projects/from-template/{id} ──────────────────────────────────

#[tokio::test]
async fn create_project_from_template_applies_workflow_fields_and_boards() {
    let (app, _) = common::test_app().await;
    let mut body = minimal_template_body("Launcher");
    body["workflow"] = valid_workflow();
    body["custom_fields"] = json!([{
        "id": Uuid::new_v4(),
        "project_id": null,
        "name": "Score",
        "field_type": "number",
        "description": null,
        "required": false,
        "default_value": null,
        "options": null,
        "validation": null,
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
    }]);
    body["default_boards"] = json!([{
        "name": "Main",
        "description": null,
        "columns": [{"status": "To Do", "wip_limit": null, "collapsed": false}],
        "filters": null,
        "grouping": "status",
    }]);
    let template = create_template(&app, body).await;
    let tid = template["id"].as_str().unwrap();

    let (status, project) = common::send(
        &app,
        "POST",
        &format!("/api/projects/from-template/{tid}"),
        json!({"name": "New Project"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{project}");
    assert_eq!(project["name"], "New Project");
    assert_eq!(project["workflow"]["workflow_type"], "kanban");
    let pid = project["id"].as_str().unwrap();

    let (_, fields) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/custom-fields"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(fields.as_array().unwrap().len(), 1, "custom field copied");

    let (_, boards) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/boards"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(boards.as_array().unwrap().len(), 1, "board copied");
}

#[tokio::test]
async fn create_project_from_template_unknown_template_is_404() {
    let (app, _) = common::test_app().await;
    let (status, _) = common::send(
        &app,
        "POST",
        &format!("/api/projects/from-template/{}", Uuid::new_v4()),
        json!({"name": "New Project"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn create_project_from_template_empty_name_is_422() {
    let (app, _) = common::test_app().await;
    let template = create_template(&app, minimal_template_body("Launcher")).await;
    let tid = template["id"].as_str().unwrap();

    let (status, _) = common::send(
        &app,
        "POST",
        &format!("/api/projects/from-template/{tid}"),
        json!({"name": ""}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

/// `build_project_from_template` looks up the first row in `workspaces` and
/// creates a "Default Workspace" when none exists — the one branch a normal
/// test app never takes, since `common::test_app` always seeds a workspace
/// up front. Needs a bare pool with zero workspace rows to reach it.
#[tokio::test]
async fn create_project_from_template_creates_default_workspace_when_none_exists() {
    let app = setup_without_workspace().await;
    let template = create_template(&app, minimal_template_body("NoWorkspaceYet")).await;
    let tid = template["id"].as_str().unwrap();

    let (status, project) = common::send(
        &app,
        "POST",
        &format!("/api/projects/from-template/{tid}"),
        json!({"name": "First Project"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{project}");
    assert_eq!(project["name"], "First Project");
}

/// `default_boards[].grouping` is free-text on the template row, parsed back
/// into a `BoardGrouping` per board when a project is created — pins every
/// arm of that parse, including a string that matches none of them.
#[tokio::test]
async fn create_project_from_template_parses_every_grouping_string() {
    let (app, _) = common::test_app().await;
    let field_id = Uuid::new_v4();
    let groupings: Vec<String> = vec![
        "status".to_string(),
        "priority".to_string(),
        "item_type".to_string(),
        "sprint".to_string(),
        "assignee".to_string(),
        format!("custom_field:{field_id}"),
        "unrecognized-string".to_string(),
    ];
    let mut body = minimal_template_body("Grouped");
    body["default_boards"] = Value::Array(
        groupings
            .iter()
            .enumerate()
            .map(|(i, g)| {
                json!({
                    "name": format!("Board {i}"),
                    "description": null,
                    "columns": [],
                    "filters": null,
                    "grouping": g,
                })
            })
            .collect(),
    );
    let template = create_template(&app, body).await;
    let tid = template["id"].as_str().unwrap();

    let (status, project) = common::send(
        &app,
        "POST",
        &format!("/api/projects/from-template/{tid}"),
        json!({"name": "Every Grouping"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{project}");
    let pid = project["id"].as_str().unwrap();

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
        groupings.len(),
        "every grouping string, recognized or not, still creates its board"
    );
}

// ─── POST /api/projects/{project_id}/save-as-template ───────────────────────

#[tokio::test]
async fn save_project_as_template_snapshots_fields_and_boards() {
    let (app, pid) = setup().await;
    common::send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/custom-fields"),
        json!({"name": "Score", "field_type": "number"}),
        &[],
    )
    .await;
    common::send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/boards"),
        json!({"name": "Snapshot Board"}),
        &[],
    )
    .await;

    let (status, template) = common::send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/save-as-template"),
        json!({"name": "Snapshot", "description": "from project"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{template}");
    assert_eq!(template["name"], "Snapshot");
    assert_eq!(template["custom_fields"].as_array().unwrap().len(), 1);
    assert_eq!(template["default_boards"].as_array().unwrap().len(), 1);
    assert_eq!(
        template["default_boards"][0]["name"], "Snapshot Board",
        "board name carried over"
    );
}

#[tokio::test]
async fn save_project_as_template_unknown_project_is_404() {
    let (app, _) = common::test_app().await;
    let (status, _) = common::send(
        &app,
        "POST",
        &format!("/api/projects/{}/save-as-template", Uuid::new_v4()),
        json!({"name": "Snapshot"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── database failures ───────────────────────────────────────────────────

/// `list_templates`, `delete_template` and `save_project_as_template` each
/// make their first database call with no existence pre-check to catch a
/// failure first — closing the pool out from under a live router pins that
/// every one reports 500, not a hang or a panic.
#[tokio::test]
async fn db_failure_on_routes_without_a_precheck_is_500() {
    let (app, pool) = setup_with_pool().await;
    let template = create_template(&app, minimal_template_body("T")).await;
    let tid = template["id"].as_str().unwrap().to_owned();
    let pid = common::create_project(&app, "P", "software").await;

    pool.close().await;

    let cases = [
        ("GET", "/api/templates".to_string()),
        ("DELETE", format!("/api/templates/{tid}")),
        ("POST", format!("/api/projects/{pid}/save-as-template")),
    ];
    for (method, uri) in cases {
        let (status, resp) = common::send(&app, method, &uri, json!({"name": "X"}), &[]).await;
        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "{method} {uri}: {resp}"
        );
    }
}

// ─── shared: test apps that need the pool, or a bare one, back ─────────────

/// A test app built the same way `common::test_app` is, with the pool handed
/// back so a test can sever it mid-test (`common::test_app` doesn't expose
/// it) — used to pin the database-failure branches above.
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

/// A test app whose database has no workspace row at all — `common::test_app`
/// always seeds one, so this is the only way to reach the "create a default
/// workspace" branch of `build_project_from_template`. `workspace_id` is
/// otherwise-unused router state; the route under test looks the workspace
/// up in the database itself, not through this field.
async fn setup_without_workspace() -> Router {
    let repo = tack_test_support::setup_test_db().await;
    let workspace_id = Uuid::new_v4();

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
    build_router(state)
}

// ─── shared: seeding a builtin template for the delete-rule test ───────────

/// A test app built the same way `common::test_app` is, but also seeded with
/// the built-in templates `server.rs` seeds at real startup
/// (`repo::templates::seed_builtin_templates`), and with the pool handed
/// back — `common::test_app` doesn't expose it, and the HTTP surface has no
/// way to create a builtin template (`create_template` always inserts
/// `is_builtin = 0`), so the one test that needs a real builtin row seeds it
/// directly against the same in-memory pool the router runs on.
async fn setup_with_builtins() -> (Router, sqlx::SqlitePool) {
    let repo = tack_test_support::setup_test_db().await;
    let workspace_id = tack_test_support::create_test_workspace(&repo).await;
    repo::templates::seed_builtin_templates(repo.pool())
        .await
        .expect("seed builtin templates");
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

/// Any one seeded builtin template's id — which one doesn't matter, the
/// delete rule applies to all of them alike.
async fn builtin_template_id(pool: &sqlx::SqlitePool) -> Uuid {
    let templates = repo::templates::list_templates(pool, None)
        .await
        .expect("list templates");
    templates
        .iter()
        .find(|t| t.is_builtin)
        .expect("at least one builtin template was seeded")
        .id
}
