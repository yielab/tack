mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tack_api::config::AppConfig;
use tower::ServiceExt;

// ─── Health ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn health_returns_ok() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn health_response_contains_version_and_migration_count() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert!(json["version"].is_string(), "version must be a string");
    assert!(
        json["migrations_applied"].as_i64().unwrap_or(0) > 0,
        "migrations_applied must be positive"
    );
}

// ─── API token (T-104) ───────────────────────────────────────────────────────

#[tokio::test]
async fn no_token_configured_allows_request() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Without a token configured, the API is open
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn correct_token_allows_request() {
    let config = AppConfig {
        api_token: Some("secret-test-token".into()),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/projects")
                .header("Authorization", "Bearer secret-test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_token_rejected() {
    let config = AppConfig {
        api_token: Some("real-token".into()),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/projects")
                .header("Authorization", "Bearer wrong-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn missing_token_rejected() {
    let config = AppConfig {
        api_token: Some("real-token".into()),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn health_bypasses_token_check() {
    // Even with a token configured, /api/health must remain public
    let config = AppConfig {
        api_token: Some("real-token".into()),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

// ─── Body limit (T-103) ──────────────────────────────────────────────────────

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

// ─── Input validation (T-105) ────────────────────────────────────────────────

#[tokio::test]
async fn create_project_empty_name_rejected() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/projects")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"name":"","project_type":"software"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_project_valid_accepted() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/projects")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"name":"My Project","project_type":"software"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn create_item_empty_title_rejected() {
    let (app, workspace_id) = common::test_app().await;

    // First create a project
    let create_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/projects")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"name":"P","project_type":"software"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create_res.status(), StatusCode::OK);

    // Extract project ID from body
    let body_bytes = axum::body::to_bytes(create_res.into_body(), 65536)
        .await
        .unwrap();
    let project: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    let project_id = project["id"].as_str().expect("id field");
    let _ = workspace_id; // used via app state

    // Now try to create an item with empty title
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{project_id}/items"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"title":""}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// ─── Vocabulary + workflow (T-302) ───────────────────────────────────────────

async fn create_test_project(app: &axum::Router) -> String {
    use axum::body::to_bytes;
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/projects")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"name":"Vocab Project","project_type":"software"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    v["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn update_project_vocabulary_persists() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let project_id = create_test_project(&app).await;

    let patch = serde_json::json!({
        "vocabulary": { "task": "Work Order", "sprint": "Phase" }
    });
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/projects/{project_id}"))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&patch).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["vocabulary"]["task"], "Work Order");
    assert_eq!(v["vocabulary"]["sprint"], "Phase");
}

#[tokio::test]
async fn update_project_workflow_statuses_valid() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let project_id = create_test_project(&app).await;

    let patch = serde_json::json!({
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
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/projects/{project_id}"))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&patch).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let statuses = v["workflow"]["statuses"].as_array().unwrap();
    assert_eq!(statuses.len(), 3);
    assert_eq!(statuses[1]["name"], "Active");
    assert_eq!(statuses[1]["wip_limit"], 3);
}

// ─── Backup / restore (T-401) ────────────────────────────────────────────────

#[tokio::test]
async fn backup_in_memory_db_returns_bad_request() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/backup")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn restore_invalid_bytes_returns_bad_request() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/restore")
                .header("content-type", "application/octet-stream")
                .body(Body::from("not a sqlite file"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn backup_roundtrip_with_file_db() {
    use axum::body::to_bytes;
    use std::path::PathBuf;
    use uuid::Uuid;

    let tmp_dir = std::env::temp_dir().join(format!("tack-test-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&tmp_dir).unwrap();
    let db_path = tmp_dir.join("test.db");
    let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

    let (app, _) = common::test_app_with_file_db(&db_url).await;

    // Backup should succeed and return a SQLite file.
    let backup_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/backup")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(backup_res.status(), StatusCode::OK);

    let backup_bytes = to_bytes(backup_res.into_body(), usize::MAX).await.unwrap();
    assert!(
        backup_bytes.starts_with(b"SQLite format 3\x00"),
        "backup must be a valid SQLite file"
    );

    // Staging the backup should succeed and write a .restore file.
    let restore_res = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/restore")
                .header("content-type", "application/octet-stream")
                .body(Body::from(backup_bytes.to_vec()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(restore_res.status(), StatusCode::OK);

    let restore_path = PathBuf::from(format!("{}.restore", db_path.display()));
    assert!(restore_path.exists(), ".restore file should be staged");

    // Clean up
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

// ─── Embedded SPA (T-403) — only compiled with --features embed-spa ──────────

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

// ─── Custom field value validation (handler integration) ─────────────────────

/// Helper: create a project and return its id string.
async fn make_project(app: &axum::Router) -> String {
    use axum::body::to_bytes;
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/projects")
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"name":"P","project_type":"software"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let p: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    p["id"].as_str().unwrap().to_owned()
}

/// Helper: create a custom field and return its id string.
async fn make_custom_field(app: &axum::Router, project_id: &str, body: &str) -> String {
    use axum::body::to_bytes;
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{project_id}/custom-fields"))
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let f: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    f["id"].as_str().unwrap().to_owned()
}

/// Helper: create a default item and return its id string.
async fn make_item(app: &axum::Router, project_id: &str) -> String {
    use axum::body::to_bytes;
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{project_id}/items"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"title":"Item","item_type":"task"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let i: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    i["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn set_custom_field_value_correct_type_returns_ok() {
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let fid = make_custom_field(&app, &pid, r#"{"name":"Score","field_type":"number"}"#).await;
    let iid = make_item(&app, &pid).await;

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/items/{iid}/custom-fields/{fid}"))
                .header("Content-Type", "application/json")
                .body(Body::from("42"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn set_custom_field_value_wrong_type_returns_422() {
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let fid = make_custom_field(&app, &pid, r#"{"name":"Score","field_type":"number"}"#).await;
    let iid = make_item(&app, &pid).await;

    // Send a string value to a Number field — must be rejected
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/items/{iid}/custom-fields/{fid}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#""not a number""#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn set_custom_field_select_invalid_option_returns_422() {
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let fid = make_custom_field(
        &app,
        &pid,
        r#"{"name":"Priority","field_type":"select","options":["Low","High"]}"#,
    )
    .await;
    let iid = make_item(&app, &pid).await;

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/items/{iid}/custom-fields/{fid}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#""Critical""#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn set_custom_field_value_passes_pattern_validation() {
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let fid = make_custom_field(
        &app,
        &pid,
        r#"{"name":"Code","field_type":"text","validation":{"pattern":"^[A-Z]{3}$"}}"#,
    )
    .await;
    let iid = make_item(&app, &pid).await;

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/items/{iid}/custom-fields/{fid}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#""ABC""#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn set_custom_field_value_fails_pattern_validation() {
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let fid = make_custom_field(
        &app,
        &pid,
        r#"{"name":"Code","field_type":"text","validation":{"pattern":"^[A-Z]{3}$"}}"#,
    )
    .await;
    let iid = make_item(&app, &pid).await;

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/items/{iid}/custom-fields/{fid}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#""lowercase""#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn set_custom_field_number_out_of_range_returns_422() {
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let fid = make_custom_field(
        &app,
        &pid,
        r#"{"name":"Score","field_type":"number","validation":{"min":0,"max":100}}"#,
    )
    .await;
    let iid = make_item(&app, &pid).await;

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::PUT)
                .uri(format!("/api/items/{iid}/custom-fields/{fid}"))
                .header("Content-Type", "application/json")
                .body(Body::from("150"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ─── Board filter integration ─────────────────────────────────────────────────

#[tokio::test]
async fn board_view_filter_by_item_type_returns_only_matching_items() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;

    // Create a task and a bug
    for (title, item_type) in [("Task A", "task"), ("Bug B", "bug")] {
        app.clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/projects/{pid}/items"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"title":"{title}","item_type":"{item_type}"}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
    }

    // Create a board that filters to only "task" items
    let board_res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/boards"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"name":"Tasks Only","filters":{"item_type":"task"}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(board_res.into_body(), 65536).await.unwrap();
    let board: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let board_id = board["id"].as_str().unwrap();

    // Fetch the board view
    let view_res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/boards/{board_id}/view"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(view_res.status(), StatusCode::OK);
    let bytes = to_bytes(view_res.into_body(), 65536).await.unwrap();
    let view: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    // All items across all columns must be of type "task"
    let all_items: Vec<&serde_json::Value> = view["columns"]
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

#[cfg(feature = "embed-spa")]
#[tokio::test]
async fn api_routes_take_priority_over_spa_fallback() {
    let (app, _) = common::test_app().await;
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

// ─── Item update and delete ───────────────────────────────────────────────────

#[tokio::test]
async fn update_item_title_persists() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let iid = make_item(&app, &pid).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/items/{iid}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"title":"Updated Title"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let get = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/items/{iid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(get.into_body(), 65536).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["item"]["title"], "Updated Title");
}

#[tokio::test]
async fn update_item_status_moves_to_in_progress() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let iid = make_item(&app, &pid).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/items/{iid}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"status":"In Progress"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let get = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/items/{iid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(get.into_body(), 65536).await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["item"]["status"], "In Progress");
}

#[tokio::test]
async fn delete_item_returns_404_on_subsequent_get() {
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let iid = make_item(&app, &pid).await;

    let del = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/items/{iid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(del.status(), StatusCode::OK);

    let get = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/items/{iid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::NOT_FOUND);
}

// ─── Sprint lifecycle ─────────────────────────────────────────────────────────

#[tokio::test]
async fn create_sprint_appears_in_list() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/sprints"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"name":"Sprint 1"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let list = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/sprints"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(list.into_body(), 65536).await.unwrap();
    let sprints: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(sprints.as_array().unwrap().len(), 1);
    assert_eq!(sprints[0]["name"], "Sprint 1");
    assert_eq!(sprints[0]["status"], "planning");
}

#[tokio::test]
async fn sprint_status_transitions_to_active() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/sprints"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"name":"Sprint A"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let sprint: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let sid = sprint["id"].as_str().unwrap().to_owned();

    let patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/sprints/{sid}/status"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"status":"active"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::OK);

    let get = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/sprints/{sid}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(get.into_body(), 65536).await.unwrap();
    let s: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(s["status"], "active");
}

// ─── Role CRUD ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_and_list_roles() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/roles"))
                .header("Content-Type", "application/json")
                .body(Body::from(r##"{"name":"Backend Dev","color":"#3B82F6"}"##))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let list = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/roles"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(list.into_body(), 65536).await.unwrap();
    let roles: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(roles.as_array().unwrap().len(), 1);
    assert_eq!(roles[0]["name"], "Backend Dev");
}

// ─── Comment CRUD ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_and_list_comments() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let iid = make_item(&app, &pid).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/items/{iid}/comments"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"content":"Great progress!","author":"alice"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let list = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/items/{iid}/comments"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(list.into_body(), 65536).await.unwrap();
    let comments: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(comments.as_array().unwrap().len(), 1);
    assert_eq!(comments[0]["content"], "Great progress!");
    assert_eq!(comments[0]["author"], "alice");
}

// ─── Dependencies ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn add_dependency_blocks_relationship() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let item_a = make_item(&app, &pid).await;
    let item_b = make_item(&app, &pid).await;

    let body = format!(r#"{{"target_item_id":"{item_b}","dependency_type":"blocks"}}"#);
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/items/{item_a}/dependencies"))
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let list = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/items/{item_a}/dependencies"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(list.into_body(), 65536).await.unwrap();
    let deps: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(deps.as_array().unwrap().len(), 1);
    assert_eq!(deps[0]["dependency_type"], "blocks");
}

#[tokio::test]
async fn self_dependency_rejected() {
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    let iid = make_item(&app, &pid).await;

    let body = format!(r#"{{"target_item_id":"{iid}","dependency_type":"blocks"}}"#);
    let res = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/items/{iid}/dependencies"))
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// ─── Search ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn project_search_finds_matching_item() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;

    // Create an item with a distinctive title
    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/items"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"title":"xyzzy unique search token","item_type":"task"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/search?q=xyzzy"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let results: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let arr = results.as_array().unwrap();
    assert!(!arr.is_empty(), "search should return at least one result");
    assert!(
        arr[0]["title"].as_str().unwrap().contains("xyzzy"),
        "first result should contain the search token"
    );
}

#[tokio::test]
async fn global_search_finds_item_across_projects() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;

    app.clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/projects/{pid}/items"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"title":"qwerty global search token","item_type":"task"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/search?q=qwerty")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 65536).await.unwrap();
    let results: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let arr = results.as_array().unwrap();
    assert!(
        !arr.is_empty(),
        "global search should return at least one result"
    );
}

// ─── Export ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn export_json_contains_project_and_items() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    make_item(&app, &pid).await;

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/export?format=json"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 131072).await.unwrap();
    let export: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
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
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    make_item(&app, &pid).await;

    let res = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/export?format=csv"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 131072).await.unwrap();
    let csv = std::str::from_utf8(&bytes).unwrap();
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

#[tokio::test]
async fn export_yaml_round_trips_through_import() {
    use axum::body::to_bytes;
    let (app, _) = common::test_app().await;
    let pid = make_project(&app).await;
    make_item(&app, &pid).await;

    // Export as YAML.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{pid}/export?format=yaml"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 131072).await.unwrap();
    let yaml = String::from_utf8(bytes.to_vec()).unwrap();
    // It must be YAML (block mappings), not JSON braces.
    assert!(yaml.contains("project:") && yaml.contains("items:"), "got: {yaml}");
    let parsed: serde_json::Value = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(parsed["items"].as_array().unwrap().len(), 1);

    // Import the same YAML back: a new project is created with the item.
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
    let bytes = to_bytes(res.into_body(), 131072).await.unwrap();
    let out: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(out["success"], true, "import response: {out}");
    let new_pid = out["project"]["id"].as_str().unwrap();
    assert_ne!(new_pid, pid, "import must create a new project");

    // The imported project has the round-tripped item.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/projects/{new_pid}/items"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(res.into_body(), 131072).await.unwrap();
    let items: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(items.as_array().unwrap().len(), 1, "imported items: {items}");
}
