use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{StatusCode, header},
    response::Response,
};
use serde_json::json;
use tracing::instrument;
use uuid::Uuid;

use crate::router::AppState;

/// GET /api/backup — VACUUM INTO snapshot streamed as application/octet-stream.
#[instrument(skip(state))]
pub async fn get_backup(
    State(state): State<AppState>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let db_path = state.config.db_file_path().ok_or((
        StatusCode::BAD_REQUEST,
        Json(json!({"error": "backup requires a file-based database"})),
    ))?;

    // Checkpoint WAL into the main file so the snapshot is current.
    sqlx::query("PRAGMA wal_checkpoint(FULL)")
        .execute(state.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    let temp_path = std::env::temp_dir().join(format!("flexpm-backup-{}.db", Uuid::new_v4()));
    let path_str = temp_path.to_string_lossy().replace('\'', "''");

    sqlx::query(&format!("VACUUM INTO '{path_str}'"))
        .execute(state.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": e.to_string()})),
            )
        })?;

    let bytes = tokio::fs::read(&temp_path).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;

    let _ = tokio::fs::remove_file(&temp_path).await;

    let filename = db_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("flexpm-backup.db");

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

/// POST /api/restore — Validate a SQLite backup and stage it for the next restart.
///
/// The file is written as `<db-path>.restore`. On the next server startup,
/// `main.rs` applies the staged restore automatically.
#[instrument(skip(state, body))]
pub async fn post_restore(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    const SQLITE_MAGIC: &[u8] = b"SQLite format 3\x00";
    if body.len() < 16 || &body[..16] != SQLITE_MAGIC {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "not a valid SQLite database file"})),
        ));
    }

    let db_path = state.config.db_file_path().ok_or((
        StatusCode::BAD_REQUEST,
        Json(json!({"error": "restore requires a file-based database"})),
    ))?;

    let restore_path = format!("{}.restore", db_path.to_string_lossy());

    tokio::fs::write(&restore_path, &body).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": e.to_string()})),
        )
    })?;

    Ok(Json(json!({
        "status": "staged",
        "restore_path": restore_path,
        "message": "Restore staged. Stop the server and restart to apply."
    })))
}
