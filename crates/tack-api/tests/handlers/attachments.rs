//! HTTP tests for the four attachment routes: upload, download, list and
//! delete, each against the real router with a `tempfile` storage directory
//! so the handler's own filesystem writes are proven, not mocked.

use crate::common;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use tack_api::config::AppConfig;
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

/// A fresh app rooted at a scratch storage directory, one project and one
/// item to attach files to. The `TempDir` guard must outlive every request.
async fn setup() -> (Router, TempDir, Uuid) {
    let storage = tempfile::tempdir().expect("temp storage dir");
    let config = AppConfig {
        storage_dir: storage.path().display().to_string(),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let pid = common::create_project(&app, "P", "software").await;
    let iid = common::create_item(&app, pid, "Item").await;
    (app, storage, iid)
}

/// Hand-built `multipart/form-data` body carrying one `file` field, matching
/// what `axum::extract::Multipart` (via `multer`) parses.
fn multipart(filename: &str, content_type: &str, content: &[u8]) -> (String, Vec<u8>) {
    let boundary = "tack-test-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(content);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

async fn upload(
    app: &Router,
    item_id: Uuid,
    filename: &str,
    content: &[u8],
) -> (StatusCode, Value) {
    let (content_type, body) = multipart(filename, "text/plain", content);
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/items/{item_id}/attachments"))
                .header("content-type", content_type)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

// ─── POST /api/items/{item_id}/attachments ────────────────────────────────

#[tokio::test]
async fn upload_attachment_writes_row_and_file() {
    let (app, storage, iid) = setup().await;
    let (status, body) = upload(&app, iid, "notes.txt", b"hello world").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["filename"], "notes.txt");
    assert_eq!(body["size_bytes"], 11);

    let storage_path = body["storage_path"].as_str().unwrap();
    let on_disk = std::fs::read(storage.path().join(storage_path)).expect("file on disk");
    assert_eq!(on_disk, b"hello world");
}

/// A "file" multipart part with no `Content-Type` header of its own — a
/// client is allowed to omit it — falls back to `application/octet-stream`
/// rather than leaving the field empty.
#[tokio::test]
async fn upload_attachment_without_content_type_defaults_to_octet_stream() {
    let (app, _storage, iid) = setup().await;
    let boundary = "tack-test-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.txt\"\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(b"no content type header");
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/items/{iid}/attachments"))
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["mime_type"], "application/octet-stream");
}

/// `create_dir_all` for the per-item storage subdirectory fails when a path
/// component above it is a plain file, not a directory — the handler must
/// report that as a 500, not panic or write a corrupt row.
#[tokio::test]
async fn upload_attachment_storage_dir_creation_failure_is_500() {
    let storage = tempfile::tempdir().expect("temp storage dir");
    let blocker = storage.path().join("blocker");
    std::fs::write(&blocker, b"not a directory").expect("write blocker file");
    let config = AppConfig {
        storage_dir: blocker.display().to_string(),
        ..AppConfig::default()
    };
    let (app, _) = common::test_app_with_config(config).await;
    let pid = common::create_project(&app, "P", "software").await;
    let iid = common::create_item(&app, pid, "Item").await;

    let (status, _) = upload(&app, iid, "a.txt", b"x").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

/// The per-item storage subdirectory already exists (so `create_dir_all` is
/// a no-op) but is read-only, so the actual file write is what fails.
#[tokio::test]
async fn upload_attachment_file_write_failure_is_500() {
    let (app, storage, iid) = setup().await;
    let item_dir = storage.path().join(iid.to_string());
    std::fs::create_dir_all(&item_dir).expect("pre-create item dir");
    std::fs::set_permissions(&item_dir, std::fs::Permissions::from_mode(0o555))
        .expect("make item dir read-only");

    let (status, _) = upload(&app, iid, "a.txt", b"x").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    // Restore write access so the `TempDir` guard can clean up on drop.
    std::fs::set_permissions(&item_dir, std::fs::Permissions::from_mode(0o755))
        .expect("restore item dir permissions");
}

#[tokio::test]
async fn upload_attachment_unknown_item_is_404() {
    let (app, _storage, _iid) = setup().await;
    let (status, _) = upload(&app, Uuid::new_v4(), "a.txt", b"x").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn upload_attachment_without_file_field_is_400() {
    let (app, storage, iid) = setup().await;
    let boundary = "tack-test-boundary";
    let empty_body = format!("--{boundary}--\r\n").into_bytes();
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/items/{iid}/attachments"))
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(empty_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let entries: Vec<_> = std::fs::read_dir(storage.path()).unwrap().collect();
    assert!(entries.is_empty(), "a rejected upload must write nothing");
}

// ─── GET /api/attachments/{id} ─────────────────────────────────────────────

#[tokio::test]
async fn download_attachment_returns_bytes_and_headers() {
    let (app, _storage, iid) = setup().await;
    let (_, uploaded) = upload(&app, iid, "notes.txt", b"downloadable").await;
    let id = uploaded["id"].as_str().unwrap();

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/attachments/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let disposition = res
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(disposition.contains("notes.txt"), "{disposition}");
    let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    assert_eq!(bytes, "downloadable".as_bytes());
}

/// The attachment row survives but its file was removed from disk out of
/// band (a restore from an older backup, manual cleanup, …) — the read the
/// handler does after loading the row must surface as a 500, not a panic.
#[tokio::test]
async fn download_attachment_disk_read_failure_is_500() {
    let (app, storage, iid) = setup().await;
    let (_, uploaded) = upload(&app, iid, "notes.txt", b"soon missing").await;
    let id = uploaded["id"].as_str().unwrap();
    let storage_path = uploaded["storage_path"].as_str().unwrap();
    std::fs::remove_file(storage.path().join(storage_path)).expect("remove file out of band");

    let (status, _) = common::send(
        &app,
        "GET",
        &format!("/api/attachments/{id}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn download_attachment_unknown_id_is_404() {
    let (app, _storage, _iid) = setup().await;
    let (status, _) = common::send(
        &app,
        "GET",
        &format!("/api/attachments/{}", Uuid::new_v4()),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── GET /api/items/{item_id}/attachments ──────────────────────────────────

#[tokio::test]
async fn list_attachments_returns_uploaded_file() {
    let (app, _storage, iid) = setup().await;
    upload(&app, iid, "notes.txt", b"one").await;

    let (status, list) = common::send(
        &app,
        "GET",
        &format!("/api/items/{iid}/attachments"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let arr = list.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["filename"], "notes.txt");
}

#[tokio::test]
async fn list_attachments_unknown_item_is_404() {
    let (app, _storage, _iid) = setup().await;
    let (status, _) = common::send(
        &app,
        "GET",
        &format!("/api/items/{}/attachments", Uuid::new_v4()),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

// ─── DELETE /api/attachments/{id} ──────────────────────────────────────────

#[tokio::test]
async fn delete_attachment_removes_row_and_file() {
    let (app, storage, iid) = setup().await;
    let (_, uploaded) = upload(&app, iid, "notes.txt", b"gone soon").await;
    let id = uploaded["id"].as_str().unwrap().to_owned();
    let storage_path = uploaded["storage_path"].as_str().unwrap().to_owned();
    let on_disk = storage.path().join(&storage_path);
    assert!(on_disk.exists());

    let (status, _) = common::send(
        &app,
        "DELETE",
        &format!("/api/attachments/{id}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(!on_disk.exists(), "file must be removed from disk");

    let (status, _) = common::send(
        &app,
        "GET",
        &format!("/api/attachments/{id}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "the row must be gone too");
}

/// The file still exists (`file_path.exists()` is true going in) but its
/// directory is read-only, so `remove_file` itself is what fails — proving
/// the handler surfaces a disk error rather than deleting the row anyway.
#[tokio::test]
async fn delete_attachment_disk_remove_failure_is_500() {
    let (app, storage, iid) = setup().await;
    let (_, uploaded) = upload(&app, iid, "notes.txt", b"stuck").await;
    let id = uploaded["id"].as_str().unwrap();
    let item_dir = storage.path().join(iid.to_string());
    std::fs::set_permissions(&item_dir, std::fs::Permissions::from_mode(0o555))
        .expect("make item dir read-only");

    let (status, _) = common::send(
        &app,
        "DELETE",
        &format!("/api/attachments/{id}"),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    // Restore write access so the `TempDir` guard can clean up on drop.
    std::fs::set_permissions(&item_dir, std::fs::Permissions::from_mode(0o755))
        .expect("restore item dir permissions");
}

#[tokio::test]
async fn delete_attachment_unknown_id_is_404() {
    let (app, _storage, _iid) = setup().await;
    let (status, _) = common::send(
        &app,
        "DELETE",
        &format!("/api/attachments/{}", Uuid::new_v4()),
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
