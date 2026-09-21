//! CRUD and lifecycle coverage for the operator-facing HTTP handlers: health,
//! the API token gate, request body limits, input validation, vocabulary and
//! workflow configuration, backup/restore, the embedded SPA, custom fields,
//! board filtering, item update/delete, sprints, roles, comments,
//! dependencies, search, export, item provenance, and the GitHub push sync.

use crate::common;
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use std::sync::Arc;
use tack_api::LocalRunnerControl;
use tack_api::config::AppConfig;
use tower::ServiceExt;
use uuid::Uuid;

// ─── Health ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn health_returns_ok() {
    let (app, _) = common::test_app().await;
    let (status, _) = common::send(&app, "GET", "/api/health", Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn health_response_contains_version_and_migration_count() {
    let (app, _) = common::test_app().await;
    let (status, json) = common::send(&app, "GET", "/api/health", Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "ok");
    assert!(json["version"].is_string(), "version must be a string");
    assert!(
        json["migrations_applied"].as_i64().unwrap_or(0) > 0,
        "migrations_applied must be positive"
    );
}

// ─── API token ───────────────────────────────────────────────────────

#[tokio::test]
async fn token_gate_by_config_and_header() {
    let cases: Vec<(&str, Option<&str>, Option<&str>, bool)> = vec![
        ("no token configured allows the request", None, None, false),
        (
            "the correct token allows the request",
            Some("secret-test-token"),
            Some("Bearer secret-test-token"),
            false,
        ),
        (
            "the wrong token is rejected",
            Some("real-token"),
            Some("Bearer wrong-token"),
            true,
        ),
        (
            "a missing token is rejected",
            Some("real-token"),
            None,
            true,
        ),
    ];
    for (name, configured, header, expect_unauthorized) in cases {
        let config = AppConfig {
            api_token: configured.map(String::from),
            ..AppConfig::default()
        };
        let (app, _) = common::test_app_with_config(config).await;
        let headers: Vec<(&str, &str)> = header
            .map(|h| vec![("authorization", h)])
            .unwrap_or_default();
        let (status, _) = common::send(&app, "GET", "/api/projects", Value::Null, &headers).await;
        if expect_unauthorized {
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{name}");
        } else {
            assert_ne!(status, StatusCode::UNAUTHORIZED, "{name}");
        }
    }
}

#[tokio::test]
async fn health_bypasses_token_check() {
    // Even with a token configured, /api/health must remain public.
    let config = AppConfig {
        api_token: Some("real-token".into()),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let (status, _) = common::send(&app, "GET", "/api/health", Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK);
}

// ─── Body limit ──────────────────────────────────────────────────────

#[tokio::test]
async fn oversized_body_rejected() {
    let config = AppConfig {
        max_body_size_bytes: 512, // very small for this test
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let big_body = "x".repeat(1024); // 1 KB > 512 B limit
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/projects")
                .header("Content-Type", "application/json")
                .body(Body::from(big_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

// ─── Input validation ────────────────────────────────────────────────

#[tokio::test]
async fn create_project_empty_name_rejected() {
    let (app, _) = common::test_app().await;
    let body = json!({"name":"","project_type":"software"});
    let (status, _) = common::send(&app, "POST", "/api/projects", body, &[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_project_valid_accepted() {
    let (app, _) = common::test_app().await;
    let body = json!({"name":"My Project","project_type":"software"});
    let (status, _) = common::send(&app, "POST", "/api/projects", body, &[]).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn create_item_empty_title_rejected() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let uri = format!("/api/projects/{pid}/items");
    let (status, _) = common::send(&app, "POST", &uri, json!({"title":""}), &[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ─── Vocabulary + workflow ───────────────────────────────────────────

#[tokio::test]
async fn update_project_vocabulary_persists() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "Vocab Project", "software").await;

    let (status, body) = common::send(
        &app,
        "PATCH",
        &format!("/api/projects/{pid}"),
        json!({"vocabulary": { "task": "Work Order", "sprint": "Phase" }}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["vocabulary"]["task"], "Work Order");
    assert_eq!(body["vocabulary"]["sprint"], "Phase");
}

#[tokio::test]
async fn update_project_workflow_statuses_valid() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "Vocab Project", "software").await;

    let patch = json!({
        "workflow": {
            "workflow_type": "custom",
            "statuses": [
                { "name": "Queue",  "category": "todo",        "wip_limit": null, "order": 0 },
                { "name": "Active", "category": "in_progress", "wip_limit": 3,    "order": 1 },
                { "name": "Done",   "category": "done",        "wip_limit": null, "order": 2 }
            ],
            "transitions": null
        }
    });
    let (status, body) =
        common::send(&app, "PATCH", &format!("/api/projects/{pid}"), patch, &[]).await;
    assert_eq!(status, StatusCode::OK);
    let statuses = body["workflow"]["statuses"].as_array().unwrap();
    assert_eq!(statuses.len(), 3);
    assert_eq!(statuses[1]["name"], "Active");
    assert_eq!(statuses[1]["wip_limit"], 3);
}

// ─── Backup / restore ────────────────────────────────────────────────

#[tokio::test]
async fn backup_in_memory_db_returns_bad_request() {
    let (app, _) = common::test_app().await;
    let (status, _) = common::send(&app, "GET", "/api/backup", Value::Null, &[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

type HeaderPair = (&'static str, &'static str);

#[tokio::test]
async fn restore_rejects_non_sqlite_body() {
    let cases: Vec<(&str, &[HeaderPair], &str)> = vec![
        (
            "with an octet-stream content-type header",
            &[("content-type", "application/octet-stream")],
            "not a sqlite file",
        ),
        ("with no content-type header", &[], "this is not a database"),
    ];
    for (name, headers, body) in cases {
        let (app, _) = common::test_app().await;
        let mut builder = Request::builder().method(Method::POST).uri("/api/restore");
        for (key, value) in headers {
            builder = builder.header(*key, *value);
        }
        let res = app
            .oneshot(builder.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{name}");
        // Unified envelope: { "error": { "status", "message" } } — not a flat string.
        let bytes = axum::body::to_bytes(res.into_body(), 65536).await.unwrap();
        let parsed: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed["error"]["status"], 400, "{name}");
        assert!(parsed["error"]["message"].is_string(), "{name}");
    }
}

#[tokio::test]
async fn backup_roundtrip_with_file_db() {
    use std::path::PathBuf;

    let tmp_dir = tempfile::tempdir().expect("temporary directory");
    let db_path = tmp_dir.path().join("test.db");
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
    let (app, _) = common::test_app_with_file_db(&db_url).await;

    // A project carrying a GitHub token *reference* must never appear in the
    // backed-up bytes — `scrub_snapshot_secrets` nulls it before the
    // snapshot leaves the process.
    let pid = common::create_project(&app, "P", "software").await;
    let (status, _) = common::send(
        &app,
        "PATCH",
        &format!("/api/projects/{pid}"),
        json!({"github_token_ref": "store:gh"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Backup should succeed and return a SQLite file.
    let (status, backup_bytes) = raw_bytes(&app, Method::GET, "/api/backup", &[], Vec::new()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        backup_bytes.starts_with(b"SQLite format 3\x00"),
        "backup must be a valid SQLite file"
    );
    assert!(
        !backup_bytes
            .windows(b"store:gh".len())
            .any(|window| window == b"store:gh"),
        "the project's github_token_ref reference must never appear in the backed-up bytes"
    );

    // Staging the backup should succeed and write a .restore file.
    let headers = [("content-type", "application/octet-stream")];
    let (status, _) = raw_bytes(&app, Method::POST, "/api/restore", &headers, backup_bytes).await;
    assert_eq!(status, StatusCode::OK);

    let restore_path = PathBuf::from(format!("{}.restore", db_path.display()));
    assert!(restore_path.exists(), ".restore file should be staged");
}

/// A oneshot request that returns the raw response bytes rather than
/// parsed JSON — for the SQLite-binary backup/restore endpoints, which
/// `common::send*` (JSON in, JSON out) cannot round-trip.
async fn raw_bytes(
    app: &Router,
    method: Method,
    uri: &str,
    headers: &[(&str, &str)],
    body: Vec<u8>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let res = app
        .clone()
        .oneshot(builder.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, bytes.to_vec())
}

#[tokio::test]
async fn backup_settings_invalid_returns_422_envelope() {
    let (app, _) = common::test_app().await;
    let req_body = json!({"retention": 0});
    let (status, body) = common::send(&app, "PUT", "/api/settings/backup", req_body, &[]).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["status"], 422);
    assert!(body["error"]["message"].is_string());
}

// ─── Embedded SPA — only compiled with --features embed-spa ──────────

#[cfg(feature = "embed-spa")]
#[tokio::test]
async fn spa_root_serves_html() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(ct.contains("text/html"), "expected text/html, got {ct}");
}

#[cfg(feature = "embed-spa")]
#[tokio::test]
async fn spa_unknown_route_returns_index_html() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/projects/some-client-side-route")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let ct = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(
        ct.contains("text/html"),
        "SPA fallback must return index.html"
    );
}

#[cfg(feature = "embed-spa")]
#[tokio::test]
async fn api_routes_take_priority_over_spa_fallback() {
    let (app, _) = common::test_app().await;
    let (status, json) = common::send(&app, "GET", "/api/health", Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "ok");
}

// ─── Custom field value validation (handler integration) ─────────────────────

/// Helper: create a custom field and return its id string.
async fn make_custom_field(app: &axum::Router, project_id: Uuid, body: &str) -> String {
    let uri = format!("/api/projects/{project_id}/custom-fields");
    let (_, f) = common::send(app, "POST", &uri, serde_json::from_str(body).unwrap(), &[]).await;
    f["id"].as_str().unwrap().to_owned()
}

/// Helper: create a default item and return its id string.
async fn make_item(app: &axum::Router, project_id: Uuid) -> String {
    let uri = format!("/api/projects/{project_id}/items");
    let body = json!({"title":"Item","item_type":"task"});
    let (_, i) = common::send(app, "POST", &uri, body, &[]).await;
    i["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn custom_field_value_validated_by_type_and_rule() {
    let unproc = StatusCode::UNPROCESSABLE_ENTITY;
    let score = r#"{"name":"Score","field_type":"number"}"#;
    let select = r#"{"name":"Priority","field_type":"select","options":["Low","High"]}"#;
    let code = r#"{"name":"Code","field_type":"text","validation":{"pattern":"^[A-Z]{3}$"}}"#;
    let ranged = r#"{"name":"Score","field_type":"number","validation":{"min":0,"max":100}}"#;
    let cases: Vec<(&str, &str, Value, StatusCode)> = vec![
        ("number accepts", score, json!(42), StatusCode::OK),
        ("number rejects a string", score, json!("bad"), unproc),
        (
            "select rejects undeclared option",
            select,
            json!("Critical"),
            unproc,
        ),
        (
            "text matches its pattern",
            code,
            json!("ABC"),
            StatusCode::OK,
        ),
        ("text fails its pattern", code, json!("lowercase"), unproc),
        ("number outside its range", ranged, json!(150), unproc),
    ];
    for (name, field_def, value, expected) in cases {
        let (app, fid, iid) = setup_custom_field_case(field_def).await;
        let uri = format!("/api/items/{iid}/custom-fields/{fid}");
        let (status, _) = common::send(&app, "PUT", &uri, value, &[]).await;
        assert_eq!(status, expected, "{name}");
    }
}

/// A fresh app with one project, one item, and one custom field defined by
/// `field_def` on that project — the setup every custom-field-value case
/// needs before it can PUT a value.
async fn setup_custom_field_case(field_def: &str) -> (Router, String, String) {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let fid = make_custom_field(&app, pid, field_def).await;
    let iid = make_item(&app, pid).await;
    (app, fid, iid)
}

// ─── Board filter integration ─────────────────────────────────────────────────

#[tokio::test]
async fn board_view_filter_by_item_type_returns_only_matching_items() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let items_uri = format!("/api/projects/{pid}/items");

    // Create a task and a bug
    for (title, item_type) in [("Task A", "task"), ("Bug B", "bug")] {
        let body = json!({"title": title, "item_type": item_type});
        common::send(&app, "POST", &items_uri, body, &[]).await;
    }

    // Create a board that filters to only "task" items
    let boards_uri = format!("/api/projects/{pid}/boards");
    let filter = json!({"name":"Tasks Only","filters":{"item_type":"task"}});
    let (_, board) = common::send(&app, "POST", &boards_uri, filter, &[]).await;
    let board_id = board["id"].as_str().unwrap();

    // Fetch the board view
    let view_uri = format!("/api/boards/{board_id}/view");
    let (status, view) = common::send(&app, "GET", &view_uri, Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK);

    // All items across all columns must be of type "task"
    let all_items: Vec<&Value> = view["columns"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|col| col["items"].as_array().unwrap())
        .collect();
    assert!(!all_items.is_empty(), "board should have at least one item");
    assert!(
        all_items.iter().all(|i| i["item_type"] == "task"),
        "all items must be of type 'task', got: {:?}",
        all_items
            .iter()
            .map(|i| &i["item_type"])
            .collect::<Vec<_>>()
    );
}

// ─── Item update and delete ───────────────────────────────────────────────────

#[tokio::test]
async fn update_item_title_persists() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let iid = make_item(&app, pid).await;

    let (status, _) = common::send(
        &app,
        "PATCH",
        &format!("/api/items/{iid}"),
        json!({"title":"Updated Title"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = common::send(&app, "GET", &format!("/api/items/{iid}"), Value::Null, &[]).await;
    assert_eq!(body["item"]["title"], "Updated Title");
}

#[tokio::test]
async fn update_item_status_moves_to_in_progress() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let iid = make_item(&app, pid).await;

    let (status, _) = common::send(
        &app,
        "PATCH",
        &format!("/api/items/{iid}"),
        json!({"status":"In Progress"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = common::send(&app, "GET", &format!("/api/items/{iid}"), Value::Null, &[]).await;
    assert_eq!(body["item"]["status"], "In Progress");
}

#[tokio::test]
async fn delete_item_returns_404_on_subsequent_get() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let iid = make_item(&app, pid).await;

    let (status, _) = common::send(
        &app,
        "DELETE",
        &format!("/api/items/{iid}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) =
        common::send(&app, "GET", &format!("/api/items/{iid}"), Value::Null, &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── Sprint lifecycle ─────────────────────────────────────────────────────────

#[tokio::test]
async fn create_sprint_appears_in_list() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;

    let (status, _) = common::send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/sprints"),
        json!({"name":"Sprint 1"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, sprints) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/sprints"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(sprints.as_array().unwrap().len(), 1);
    assert_eq!(sprints[0]["name"], "Sprint 1");
    assert_eq!(sprints[0]["status"], "planning");
}

#[tokio::test]
async fn sprint_status_transitions_to_active() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;

    let (_, sprint) = common::send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/sprints"),
        json!({"name":"Sprint A"}),
        &[],
    )
    .await;
    let sid = sprint["id"].as_str().unwrap().to_owned();

    let (status, _) = common::send(
        &app,
        "PATCH",
        &format!("/api/sprints/{sid}/status"),
        json!({"status":"active"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, s) = common::send(
        &app,
        "GET",
        &format!("/api/sprints/{sid}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(s["status"], "active");
}

#[tokio::test]
async fn sprint_edit_replaces_fields_clears_omitted_ones() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let sprints_uri = format!("/api/projects/{pid}/sprints");
    let seed = json!({"name":"Sprint A","goal":"Ship the MVP","start_date":"2026-01-01T00:00:00Z"});
    let (_, sprint) = common::send(&app, "POST", &sprints_uri, seed, &[]).await;
    let sid = sprint["id"].as_str().unwrap().to_owned();
    let sprint_uri = format!("/api/sprints/{sid}");

    // The edit form sends every editable field it holds, so a goal and a start
    // date the user emptied arrive omitted and must end up NULL — not left at
    // their old values.
    let rename = json!({"name":"Sprint A, renamed"});
    let (status, _) = common::send(&app, "PATCH", &sprint_uri, rename, &[]).await;
    assert_eq!(status, StatusCode::OK);

    let (_, s) = common::send(&app, "GET", &sprint_uri, Value::Null, &[]).await;
    assert_eq!(s["name"], "Sprint A, renamed");
    assert!(
        s["goal"].is_null(),
        "an omitted goal must clear, got {}",
        s["goal"]
    );
    assert!(
        s["start_date"].is_null(),
        "an omitted start_date must clear, got {}",
        s["start_date"]
    );
    // The edit route leaves the lifecycle alone; only the status route moves it.
    assert_eq!(s["status"], "planning");
}

#[tokio::test]
async fn sprint_edit_on_unknown_id_is_404() {
    let (app, _) = common::test_app().await;
    let missing = Uuid::new_v4();

    let (status, _) = common::send(
        &app,
        "PATCH",
        &format!("/api/sprints/{missing}"),
        json!({"name":"Nowhere"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── Role CRUD ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_and_list_roles() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;

    let (status, _) = common::send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/roles"),
        json!({"name":"Backend Dev","color":"#3B82F6"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, roles) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/roles"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(roles.as_array().unwrap().len(), 1);
    assert_eq!(roles[0]["name"], "Backend Dev");
}

// ─── Comment CRUD ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_and_list_comments() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let iid = make_item(&app, pid).await;

    let (status, _) = common::send(
        &app,
        "POST",
        &format!("/api/items/{iid}/comments"),
        json!({"content":"Great progress!","author":"alice"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, comments) = common::send(
        &app,
        "GET",
        &format!("/api/items/{iid}/comments"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(comments.as_array().unwrap().len(), 1);
    assert_eq!(comments[0]["content"], "Great progress!");
    assert_eq!(comments[0]["author"], "alice");
}

// ─── Dependencies ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn add_dependency_blocks_relationship() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let item_a = make_item(&app, pid).await;
    let item_b = make_item(&app, pid).await;

    let (status, _) = common::send(
        &app,
        "POST",
        &format!("/api/items/{item_a}/dependencies"),
        json!({"target_item_id": item_b, "dependency_type":"blocks"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, deps) = common::send(
        &app,
        "GET",
        &format!("/api/items/{item_a}/dependencies"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(deps.as_array().unwrap().len(), 1);
    assert_eq!(deps[0]["dependency_type"], "blocks");
}

#[tokio::test]
async fn self_dependency_rejected() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let iid = make_item(&app, pid).await;

    let (status, _) = common::send(
        &app,
        "POST",
        &format!("/api/items/{iid}/dependencies"),
        json!({"target_item_id": iid, "dependency_type":"blocks"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ─── Search ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn project_search_finds_matching_item() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;

    // Create an item with a distinctive title
    common::send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/items"),
        json!({"title":"xyzzy unique search token","item_type":"task"}),
        &[],
    )
    .await;

    let (status, results) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/search?q=xyzzy"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr = results.as_array().unwrap();
    assert!(!arr.is_empty(), "search should return at least one result");
    assert!(
        arr[0]["title"].as_str().unwrap().contains("xyzzy"),
        "first result should contain the search token"
    );
}

#[tokio::test]
async fn global_search_finds_item_across_projects() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;

    common::send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/items"),
        json!({"title":"qwerty global search token","item_type":"task"}),
        &[],
    )
    .await;

    let (status, results) =
        common::send(&app, "GET", "/api/search?q=qwerty", Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK);
    let arr = results.as_array().unwrap();
    assert!(
        !arr.is_empty(),
        "global search should return at least one result"
    );
}

// ─── Export ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_items_returns_pagination_envelope_and_slices_pages() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;

    // Three items so a per_page=2 page 1 has a remainder on page 2.
    for _ in 0..3 {
        make_item(&app, pid).await;
    }

    // Page 1: envelope shape + total count + first slice.
    let page1_uri = format!("/api/projects/{pid}/items?per_page=2&page=1");
    let (status, page1) = common::send(&app, "GET", &page1_uri, Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page1["total"], 3, "total must count all matching items");
    assert_eq!(page1["page"], 1);
    assert_eq!(page1["per_page"], 2);
    assert_eq!(
        page1["data"].as_array().unwrap().len(),
        2,
        "page 1 holds per_page items"
    );

    // Page 2: the remaining slice.
    let page2_uri = format!("/api/projects/{pid}/items?per_page=2&page=2");
    let (_, page2) = common::send(&app, "GET", &page2_uri, Value::Null, &[]).await;
    assert_eq!(page2["total"], 3);
    assert_eq!(page2["page"], 2);
    assert_eq!(
        page2["data"].as_array().unwrap().len(),
        1,
        "page 2 holds the remaining item"
    );
}

#[tokio::test]
async fn export_json_contains_project_and_items() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    make_item(&app, pid).await;

    let (status, export) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/export?format=json"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        export["project"].is_object(),
        "export must contain a project object"
    );
    assert!(
        export["items"].is_array(),
        "export must contain an items array"
    );
    assert_eq!(export["items"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn export_csv_starts_with_header_row() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    make_item(&app, pid).await;

    let (status, _, csv) = common::send_with_raw(
        &app,
        "GET",
        &format!("/api/projects/{pid}/export?format=csv"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let first_line = csv.lines().next().unwrap_or("");
    assert!(
        first_line.contains("id") && first_line.contains("title"),
        "CSV header must contain id and title columns, got: {first_line}"
    );
    assert!(
        csv.lines().count() >= 2,
        "CSV must have header + at least one data row"
    );
}

/// POSTs a YAML export body to `/api/projects/import`, which only accepts
/// this format via a real `Content-Type: application/x-yaml` header — no
/// `common::send*` helper sets that content type, so this stays a direct
/// request build. Returns the parsed JSON response body.
async fn import_yaml(app: &Router, yaml: String) -> Value {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/projects/import")
                .header("Content-Type", "application/x-yaml")
                .body(Body::from(yaml))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 131072).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn export_yaml_round_trips_through_import() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    make_item(&app, pid).await;

    // Export as YAML.
    let export_uri = format!("/api/projects/{pid}/export?format=yaml");
    let (status, _, yaml) = common::send_with_raw(&app, "GET", &export_uri, Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK);
    // It must be YAML (block mappings), not JSON braces.
    assert!(
        yaml.contains("project:") && yaml.contains("items:"),
        "got: {yaml}"
    );
    let parsed: Value = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(parsed["items"].as_array().unwrap().len(), 1);
    assert_eq!(
        parsed["items"][0]["source"], "manual",
        "an ordinarily-created item's provenance marker must round-trip through export"
    );

    // Import the same YAML back: a new project is created with the item.
    let out = import_yaml(&app, yaml).await;
    assert_eq!(out["success"], true, "import response: {out}");
    let new_pid = out["project"]["id"].as_str().unwrap();
    assert_ne!(new_pid, pid.to_string(), "import must create a new project");

    // The imported project has the round-tripped item.
    let items_uri = format!("/api/projects/{new_pid}/items");
    let (_, items) = common::send(&app, "GET", &items_uri, Value::Null, &[]).await;
    assert_eq!(
        items["data"].as_array().unwrap().len(),
        1,
        "imported items: {items}"
    );
    assert_eq!(
        items["data"][0]["source"], "manual",
        "the imported item must still be recorded as manual/trusted, not reset to unknown"
    );
}

// ─── Item provenance / trust boundary ────────────────────────────

/// Mount a GitHub `GET /repos/acme/widgets/issues` mock returning one open issue.
async fn mount_single_issue(gh: &wiremock::MockServer, number: u32, title: &str) {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, ResponseTemplate};
    Mock::given(method("GET"))
        .and(path("/repos/acme/widgets/issues"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {
                "number": number, "title": title, "body": "", "state": "open",
                "labels": [], "assignee": null,
                "html_url": format!("https://github.com/acme/widgets/issues/{number}")
            }
        ])))
        .mount(gh)
        .await;
}

/// Starts a fresh app against `gh`, imports the one mounted issue into a
/// new project, and returns the app, project id and that project's items.
async fn import_single_issue(
    gh: &wiremock::MockServer,
    github_token: Option<&str>,
) -> (Router, Uuid, Value) {
    let config = AppConfig {
        github_api_base: gh.uri(),
        github_token: github_token.map(String::from),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let pid = common::create_project(&app, "P", "software").await;

    let (status, _) = common::send(
        &app,
        "POST",
        &format!("/api/projects/{pid}/import-github"),
        json!({"repo":"acme/widgets"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (_, items) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/items"),
        Value::Null,
        &[],
    )
    .await;
    (app, pid, items)
}

/// An item imported
/// from GitHub is marked untrusted at creation time, and that marker
/// survives an export → import round trip rather than resetting to
/// trusted — proven end to end through the real HTTP import/export/import
/// path.
#[tokio::test]
async fn github_import_source_untrusted_survives_export_reimport() {
    use wiremock::MockServer;

    let gh = MockServer::start().await;
    mount_single_issue(&gh, 7, "Untrusted issue").await;
    let (app, pid, items) = import_single_issue(&gh, None).await;
    assert_eq!(
        items["data"][0]["source"], "github",
        "a GitHub import must record source: github"
    );

    // Export the linked project, then re-import that snapshot into a fresh
    // project — the item's `source` must survive, not reset to `manual`.
    let export_uri = format!("/api/projects/{pid}/export?format=json");
    let (_, _, export_raw) =
        common::send_with_raw(&app, "GET", &export_uri, Value::Null, &[]).await;
    let (status, out) =
        common::send_str_strict(&app, "POST", "/api/projects/import", export_raw, &[]).await;
    assert_eq!(status, StatusCode::OK);
    let new_pid = out["project"]["id"].as_str().unwrap();
    assert_ne!(new_pid, pid.to_string());

    let items_uri = format!("/api/projects/{new_pid}/items");
    let (_, reimported) = common::send(&app, "GET", &items_uri, Value::Null, &[]).await;
    assert_eq!(
        reimported["data"][0]["source"], "github",
        "the trust marker must survive an export -> import round trip, never reset to trusted"
    );
}

#[tokio::test]
async fn github_import_redirect_never_leaks_user_token() {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let origin = MockServer::start().await;
    let private_destination = MockServer::start().await;
    let redirect_target = format!("{}/instance-metadata", private_destination.uri());

    Mock::given(method("GET"))
        .and(path("/repos/acme/widgets/issues"))
        .and(header("authorization", "Bearer user-pat"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", redirect_target))
        .mount(&origin)
        .await;

    let config = AppConfig {
        github_api_base: origin.uri(),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let pid = common::create_project(&app, "P", "software").await;
    let import_uri = format!("/api/projects/{pid}/import-github");
    let body = json!({"repo":"acme/widgets","token":"user-pat"});
    let (status, _) = common::send(&app, "POST", &import_uri, body, &[]).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        private_destination
            .received_requests()
            .await
            .expect("inspect private destination")
            .is_empty(),
        "a redirect must never forward a user-supplied GitHub token"
    );
}

#[tokio::test]
async fn csv_import_marks_items_with_csv_import_source() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;

    let csv = "title,description\nFrom a spreadsheet,could be anyone's data\n";
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/import-csv"))
                .header("Content-Type", "text/csv")
                .body(Body::from(csv))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let (_, items) = common::send(
        &app,
        "GET",
        &format!("/api/projects/{pid}/items"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(
        items["data"][0]["source"], "csv_import",
        "a CSV-imported row must be recorded with source: csv_import (untrusted for dispatch)"
    );
}

// ─── GitHub push sync ───────────────────────────────────────────────

#[tokio::test]
async fn completing_github_item_pushes_issue_close() {
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let gh = MockServer::start().await;
    mount_single_issue(&gh, 42, "Fix the thing").await;

    // The close we expect once the item is completed.
    Mock::given(method("PATCH"))
        .and(path("/repos/acme/widgets/issues/42"))
        .and(body_json(json!({ "state": "closed" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "number": 42 })))
        .mount(&gh)
        .await;

    // Import → creates one item linked to issue #42.
    let (app, _pid, items) = import_single_issue(&gh, Some("tok")).await;
    let item_id = items["data"][0]["id"].as_str().unwrap();

    // Move it to Done → fires a best-effort close to GitHub.
    let item_uri = format!("/api/items/{item_id}");
    let (status, _) = common::send(&app, "PATCH", &item_uri, json!({"status":"Done"}), &[]).await;
    assert_eq!(status, StatusCode::OK);

    assert!(
        wait_for_gh_request(&gh, "/repos/acme/widgets/issues/42").await,
        "expected a PATCH closing GitHub issue #42 after completion"
    );
}

// ─── Manual GitHub link ──────────────────────────────────────────────

/// Link, read, unlink, then confirm the link is gone — plus one bad PUT.
#[tokio::test]
async fn manual_github_link_is_set_read_and_removed() {
    let (app, _) = common::test_app().await;
    let pid = common::create_project(&app, "P", "software").await;
    let item_id = common::create_item(&app, pid, "Item").await;
    let link_uri = format!("/api/items/{item_id}/github-link");

    let (status, _) = common::send(
        &app,
        "PUT",
        &link_uri,
        json!({"repo": "acme/widgets", "issue_number": 42}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = common::send(&app, "GET", &link_uri, Value::Null, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["repo"], "acme/widgets");
    assert_eq!(body["issue_number"], 42);

    let (status, _) = common::send(&app, "DELETE", &link_uri, Value::Null, &[]).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = common::send(&app, "GET", &link_uri, Value::Null, &[]).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "unlinked reads back as 404");

    let (status, _) = common::send(
        &app,
        "PUT",
        &link_uri,
        json!({"repo": "not a repo", "issue_number": 1}),
        &[],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "an unparseable repo is the ordinary validation error"
    );
}

/// A manually linked item (never imported) still closes its GitHub issue on
/// Done — the "UI row links an item and the item then closes its issue on
/// Done" done-when, proven on the API side.
#[tokio::test]
async fn manual_github_link_closes_the_issue_on_done() {
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let gh = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/repos/acme/widgets/issues/42"))
        .and(body_json(json!({ "state": "closed" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "number": 42 })))
        .mount(&gh)
        .await;

    let config = AppConfig {
        github_api_base: gh.uri(),
        github_token: Some("tok".into()),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let pid = common::create_project(&app, "P", "software").await;
    let item_id = common::create_item(&app, pid, "Item").await;

    let (status, _) = common::send(
        &app,
        "PUT",
        &format!("/api/items/{item_id}/github-link"),
        json!({"repo": "acme/widgets", "issue_number": 42}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = common::send(
        &app,
        "PATCH",
        &format!("/api/items/{item_id}"),
        json!({"status":"Done"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    assert!(
        wait_for_gh_request(&gh, "/repos/acme/widgets/issues/42").await,
        "expected a PATCH closing GitHub issue #42 after a manual link and completion"
    );
}

/// A project-level `github_token_ref` (resolved through the embedded
/// runner's secret store) is used for that project's push, in preference to
/// the environment's `TACK_GITHUB_TOKEN` — and the project response never
/// carries the resolved value, only the reference.
#[tokio::test]
async fn push_uses_the_project_token_before_the_environment_token() {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let gh = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/repos/acme/widgets/issues/42"))
        .and(header("authorization", "Bearer project-tok"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "number": 42 })))
        .expect(1)
        .mount(&gh)
        .await;

    let control = Arc::new(crate::local_runner::FakeControl::default());
    control
        .set_secret("gh", "project-tok")
        .await
        .expect("seed the fake secret store");
    let control: Arc<dyn LocalRunnerControl> = control;

    let config = AppConfig {
        github_api_base: gh.uri(),
        github_token: Some("env-tok".into()),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_local_runner(config, Some(control)).await;
    let pid = common::create_project(&app, "P", "software").await;

    let (status, project_body) = common::send(
        &app,
        "PATCH",
        &format!("/api/projects/{pid}"),
        json!({"github_token_ref": "store:gh"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(project_body["github_token_ref"], "store:gh");
    assert!(
        !project_body.to_string().contains("project-tok"),
        "the project response must carry the reference, never the resolved token value"
    );

    let item_id = common::create_item(&app, pid, "Item").await;
    let (status, _) = common::send(
        &app,
        "PUT",
        &format!("/api/items/{item_id}/github-link"),
        json!({"repo": "acme/widgets", "issue_number": 42}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = common::send(
        &app,
        "PATCH",
        &format!("/api/items/{item_id}"),
        json!({"status":"Done"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    assert!(
        wait_for_gh_request(&gh, "/repos/acme/widgets/issues/42").await,
        "expected the push to reach GitHub using the project token"
    );

    let all_requests = gh.received_requests().await.unwrap();
    assert!(
        !all_requests.iter().any(
            |r| r.headers.get("authorization").and_then(|v| v.to_str().ok())
                == Some("Bearer env-tok")
        ),
        "the environment token must never be used once a project token resolves: {all_requests:?}"
    );
}

/// The inbound poll: `poll_once` takes an `AppState`, not a `Router`, so
/// there is no HTTP path into it — the project/item/link are seeded
/// straight through the repo layer instead of via `import-github`. Three
/// polls in sequence: closed → the first Done status, reopened (with
/// `since` now carrying the prior poll's `updated_at`) → the first Todo
/// status, then a 304 → no write. The final assertion pins the one
/// invariant the plan calls out: an inbound move must never itself fire an
/// outbound PATCH (`handlers::items::maybe_sync_github`'s echo).
#[tokio::test]
async fn github_poll_moves_item_through_issue_state_then_a_304_writes_nothing() {
    use wiremock::matchers::{method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let gh = MockServer::start().await;

    let repo = tack_test_support::setup_test_db().await;
    let workspace_id = tack_test_support::create_test_workspace(&repo).await;
    let project = tack_test_support::make_project(&repo, workspace_id).await;
    let initial_status = project.workflow.initial_status().unwrap();
    assert_eq!(
        initial_status, "Backlog",
        "scrum's first Todo-category status"
    );
    let item = repo
        .create_item(
            project.id,
            &initial_status,
            tack_core::models::CreateItem {
                title: "Fix the thing".into(),
                description: None,
                item_type: Some(tack_core::models::ItemType::Task),
                parent_id: None,
                priority: Some(tack_core::models::Priority::Medium),
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .expect("create item");
    repo.set_github_link(item.id, "acme/widgets", 42)
        .await
        .expect("link item to issue #42");

    let (broadcast_tx, _rx) = tokio::sync::broadcast::channel(4);
    let state = tack_api::AppState {
        repo,
        config: AppConfig {
            github_token: Some("tok".into()),
            github_api_base: gh.uri(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx,
        webhook: None,
        local_runner: None,
    };
    let mut etags = std::collections::HashMap::new();

    // Poll 1: closed on GitHub, no prior `synced_at` so no `since` param yet.
    Mock::given(method("GET"))
        .and(path("/repos/acme/widgets/issues"))
        .and(query_param("state", "all"))
        .and(query_param_is_missing("since"))
        .respond_with(ResponseTemplate::new(200).insert_header("ETag", "\"etag-1\"").set_body_json(
            json!([{"number": 42, "state": "closed", "updated_at": "2026-02-01T00:00:00Z"}]),
        ))
        .expect(1)
        .mount(&gh)
        .await;

    let summary = tack_api::github_sync::poll_once(&state, &mut etags)
        .await
        .expect("poll_once");
    assert_eq!(summary.issues_updated, 1);
    let after_close = state.repo.get_item(item.id).await.unwrap().unwrap();
    assert_eq!(
        after_close.status, "Done",
        "closed on GitHub moves the item to the first Done status"
    );

    // Poll 2: reopened on GitHub; `since` now carries poll 1's `updated_at`.
    Mock::given(method("GET"))
        .and(path("/repos/acme/widgets/issues"))
        .and(query_param("state", "all"))
        .and(query_param("since", "2026-02-01T00:00:00Z"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"etag-2\"")
                .set_body_json(
                    json!([{"number": 42, "state": "open", "updated_at": "2026-03-01T00:00:00Z"}]),
                ),
        )
        .expect(1)
        .mount(&gh)
        .await;

    let summary = tack_api::github_sync::poll_once(&state, &mut etags)
        .await
        .expect("poll_once");
    assert_eq!(summary.issues_updated, 1);
    let after_reopen = state.repo.get_item(item.id).await.unwrap().unwrap();
    assert_eq!(
        after_reopen.status, "Backlog",
        "reopened on GitHub moves the item to the first Todo status"
    );

    // Poll 3: the item is being worked on and the issue is still open →
    // categories agree, so the poll leaves it in progress (never back to Todo).
    let in_progress = project
        .workflow
        .statuses
        .iter()
        .find(|s| s.category == tack_core::workflow::StatusCategory::InProgress)
        .map(|s| s.name.clone())
        .expect("scrum has an in-progress status");
    state
        .repo
        .update_item_atomically(
            item.id,
            tack_core::models::UpdateItem {
                status: Some(in_progress.clone()),
                ..Default::default()
            },
            &project.workflow,
            None,
        )
        .await
        .expect("move to in progress");
    Mock::given(method("GET"))
        .and(path("/repos/acme/widgets/issues"))
        .and(query_param("state", "all"))
        .and(query_param("since", "2026-03-01T00:00:00Z"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"etag-3\"")
                .set_body_json(
                    json!([{"number": 42, "state": "open", "updated_at": "2026-03-02T00:00:00Z"}]),
                ),
        )
        .expect(1)
        .mount(&gh)
        .await;

    let summary = tack_api::github_sync::poll_once(&state, &mut etags)
        .await
        .expect("poll_once");
    assert_eq!(
        summary.issues_updated, 0,
        "same category on both sides: no move"
    );
    let still_working = state.repo.get_item(item.id).await.unwrap().unwrap();
    assert_eq!(still_working.status, in_progress);

    // Poll 4: GitHub answers 304 (nothing changed since poll 3) → no write.
    Mock::given(method("GET"))
        .and(path("/repos/acme/widgets/issues"))
        .and(query_param("state", "all"))
        .and(query_param("since", "2026-03-02T00:00:00Z"))
        .respond_with(ResponseTemplate::new(304))
        .expect(1)
        .mount(&gh)
        .await;

    let summary = tack_api::github_sync::poll_once(&state, &mut etags)
        .await
        .expect("poll_once");
    assert_eq!(summary.issues_updated, 0, "a 304 must do no write");
    let after_304 = state.repo.get_item(item.id).await.unwrap().unwrap();
    assert_eq!(after_304.status, in_progress);

    // The whole exchange only ever sent GET: an inbound poll must never
    // itself trigger the outbound push. Polls 1–3 each also fetch the
    // linked issue's comments (unmocked here, so each gets wiremock's
    // default 404 and is logged and skipped) — poll 4's 304 short-circuits
    // before that fetch, so seven GETs in all, still never a PATCH or POST.
    let all_requests = gh.received_requests().await.unwrap();
    assert_eq!(
        all_requests.len(),
        7,
        "expected the four issue-state GETs plus a comments GET for polls 1-3"
    );
    assert!(
        all_requests.iter().all(|r| r.method.as_str() == "GET"),
        "an inbound poll must never itself fire an outbound PATCH: {all_requests:?}"
    );
}

/// Outbound: posting a comment through the API on a linked item pushes it to
/// the issue, best-effort, and stores the id GitHub returns — shaped like
/// `completing_github_item_pushes_issue_close` above but for
/// `handlers::comments::create_comment`'s hook.
#[tokio::test]
async fn creating_comment_on_linked_item_pushes_it_to_github_and_stores_the_id() {
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let gh = MockServer::start().await;

    let repo = tack_test_support::setup_test_db().await;
    let workspace_id = tack_test_support::create_test_workspace(&repo).await;
    let project = tack_test_support::make_project(&repo, workspace_id).await;
    let initial_status = project.workflow.initial_status().unwrap();
    let item = repo
        .create_item(
            project.id,
            &initial_status,
            tack_core::models::CreateItem {
                title: "Fix the thing".into(),
                description: None,
                item_type: Some(tack_core::models::ItemType::Task),
                parent_id: None,
                priority: Some(tack_core::models::Priority::Medium),
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .expect("create item");
    repo.set_github_link(item.id, "acme/widgets", 42)
        .await
        .expect("link item to issue #42");

    Mock::given(method("POST"))
        .and(path("/repos/acme/widgets/issues/42/comments"))
        .and(body_json(json!({ "body": "hello from tack" })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": 555 })))
        .expect(1)
        .mount(&gh)
        .await;

    let (broadcast_tx, _rx) = tokio::sync::broadcast::channel(4);
    let state = tack_api::AppState {
        repo,
        config: AppConfig {
            github_token: Some("tok".into()),
            github_api_base: gh.uri(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx,
        webhook: None,
        local_runner: None,
    };
    let app = tack_api::router::build_router(state.clone());

    let (status, body) = common::send(
        &app,
        "POST",
        &format!("/api/items/{}/comments", item.id),
        json!({"content": "hello from tack", "author": "alice"}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["content"], "hello from tack");

    assert!(
        wait_for_gh_request(&gh, "/repos/acme/widgets/issues/42/comments").await,
        "expected a POST mirroring the comment onto GitHub issue #42"
    );

    // The push is spawned fire-and-forget; poll (bounded by wall-clock time,
    // not a fixed sleep) until the id it stores lands.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut stored = Vec::new();
    while std::time::Instant::now() < deadline {
        stored = state.repo.list_github_comment_ids(item.id).await.unwrap();
        if !stored.is_empty() {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(
        stored,
        vec![555],
        "the id GitHub returned for the new comment must be stored"
    );
}

/// Inbound: the poll creates a Tack comment for each GitHub comment id it
/// hasn't stored yet, attributed to the GitHub login on the first line. A
/// second poll of the same response creates nothing more, and a comment
/// mirrored in is never posted back out — the mock server only ever sees
/// GET, never the POST `creating_comment_on_linked_item_pushes_it_to_github_
/// and_stores_the_id` above exercises.
#[tokio::test]
async fn github_poll_mirrors_comments_in_and_never_pushes_them_back() {
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let gh = MockServer::start().await;

    let repo = tack_test_support::setup_test_db().await;
    let workspace_id = tack_test_support::create_test_workspace(&repo).await;
    let project = tack_test_support::make_project(&repo, workspace_id).await;
    let initial_status = project.workflow.initial_status().unwrap();
    let item = repo
        .create_item(
            project.id,
            &initial_status,
            tack_core::models::CreateItem {
                title: "Fix the thing".into(),
                description: None,
                item_type: Some(tack_core::models::ItemType::Task),
                parent_id: None,
                priority: Some(tack_core::models::Priority::Medium),
                estimate: None,
                estimate_unit: None,
                tags: None,
                due_date: None,
                sprint_id: None,
                assignee: None,
            },
        )
        .await
        .expect("create item");
    repo.set_github_link(item.id, "acme/widgets", 42)
        .await
        .expect("link item to issue #42");

    // The issue is in the `since` list on both polls (a new comment bumps
    // its `updated_at`), open both times: no state change, only comments.
    // Neither mock pins `since`, so the second poll, which sends one, still
    // reads the same two comments and has to recognise them by stored id.
    Mock::given(method("GET"))
        .and(path("/repos/acme/widgets/issues"))
        .and(query_param("state", "all"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"number": 42, "state": "open", "updated_at": "2026-03-01T10:00:00Z"}
        ])))
        .mount(&gh)
        .await;

    Mock::given(method("GET"))
        .and(path("/repos/acme/widgets/issues/42/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": 101, "body": "first comment", "user": {"login": "alice"}},
            {"id": 102, "body": "second comment", "user": {"login": "bob"}},
        ])))
        .mount(&gh)
        .await;

    let (broadcast_tx, _rx) = tokio::sync::broadcast::channel(4);
    let state = tack_api::AppState {
        repo,
        config: AppConfig {
            github_token: Some("tok".into()),
            github_api_base: gh.uri(),
            ..AppConfig::default()
        },
        workspace_id,
        broadcast_tx,
        webhook: None,
        local_runner: None,
    };
    let mut etags = std::collections::HashMap::new();

    tack_api::github_sync::poll_once(&state, &mut etags)
        .await
        .expect("poll_once");
    let comments = state.repo.list_comments(item.id).await.unwrap();
    assert_eq!(
        comments.len(),
        2,
        "both GitHub comments must be mirrored in"
    );
    assert!(
        comments[0].content.starts_with("@alice on GitHub:\n\n"),
        "got: {:?}",
        comments[0].content
    );
    assert!(
        comments[1].content.starts_with("@bob on GitHub:\n\n"),
        "got: {:?}",
        comments[1].content
    );

    tack_api::github_sync::poll_once(&state, &mut etags)
        .await
        .expect("poll_once");
    let comments_after_second_poll = state.repo.list_comments(item.id).await.unwrap();
    assert_eq!(
        comments_after_second_poll.len(),
        2,
        "a poll reading the same GitHub comments again must create nothing more"
    );

    let all_requests = gh.received_requests().await.unwrap();
    assert_eq!(
        all_requests.len(),
        4,
        "two polls, each one issues GET and one comments GET: {all_requests:?}"
    );
    assert!(
        all_requests.iter().all(|r| r.method.as_str() == "GET"),
        "a comment mirrored in must never itself be pushed back out: {all_requests:?}"
    );
}

/// Polls a mock GitHub server's received requests, bounded by wall-clock
/// time rather than a fixed per-iteration sleep, for a fire-and-forget push
/// that already reached the given path.
async fn wait_for_gh_request(gh: &wiremock::MockServer, path: &str) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        let reqs = gh.received_requests().await.unwrap_or_default();
        if reqs.iter().any(|r| r.url.path() == path) {
            return true;
        }
        tokio::task::yield_now().await;
    }
    false
}
