mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use flexpm_api::config::AppConfig;
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

    let tmp_dir = std::env::temp_dir().join(format!("flexpm-test-{}", Uuid::new_v4()));
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
