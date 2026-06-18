use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{StatusCode, header},
    response::Response,
};
use serde::Deserialize;
use serde_json::json;
use tracing::instrument;
use uuid::Uuid;

use crate::remote_backup;
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

    let temp_path = std::env::temp_dir().join(format!("tack-backup-{}.db", Uuid::new_v4()));
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
        .unwrap_or("tack-backup.db");

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

// ── Remote backup endpoints ────────────────────────────────────────────────────

fn remote_not_configured() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::CONFLICT,
        Json(
            json!({"error": "remote backup is not configured — set TACK_BACKUP_BUCKET, TACK_BACKUP_ACCESS_KEY, TACK_BACKUP_SECRET_KEY"}),
        ),
    )
}

fn backup_err(e: remote_backup::BackupError) -> (StatusCode, Json<serde_json::Value>) {
    let status = match &e {
        remote_backup::BackupError::NotConfigured => StatusCode::CONFLICT,
        remote_backup::BackupError::SchemaTooNew { .. } => StatusCode::CONFLICT,
        remote_backup::BackupError::InMemoryDb => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(json!({"error": e.to_string()})))
}

/// POST /api/backup/remote — create a bundle and upload it to the configured S3 store.
#[instrument(skip(state))]
pub async fn post_remote_backup(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if !state.config.remote_backup_enabled() {
        return Err(remote_not_configured());
    }

    let store = remote_backup::store_from_config(&state.config).map_err(backup_err)?;
    let (bundle, manifest) = remote_backup::create_bundle(state.pool(), &state.config)
        .await
        .map_err(backup_err)?;

    remote_backup::upload(store.as_ref(), &manifest, bundle)
        .await
        .map_err(backup_err)?;

    remote_backup::prune(
        store.as_ref(),
        &state.config.backup_prefix,
        state.config.backup_retention,
    )
    .await
    .map_err(backup_err)?;

    Ok(Json(serde_json::to_value(&manifest).unwrap()))
}

/// GET /api/backup/remote — list remote backups newest-first.
#[instrument(skip(state))]
pub async fn get_remote_backups(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if !state.config.remote_backup_enabled() {
        return Err(remote_not_configured());
    }

    let store = remote_backup::store_from_config(&state.config).map_err(backup_err)?;
    let manifests = remote_backup::list(store.as_ref(), &state.config.backup_prefix)
        .await
        .map_err(backup_err)?;

    Ok(Json(serde_json::to_value(&manifests).unwrap()))
}

#[derive(Debug, Deserialize)]
pub struct RestoreRemoteRequest {
    /// Object key to restore. Defaults to the latest backup when omitted.
    pub key: Option<String>,
}

/// POST /api/backup/remote/restore — download a bundle and stage it for next restart.
#[instrument(skip(state, raw_body))]
pub async fn post_remote_restore(
    State(state): State<AppState>,
    raw_body: Bytes,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if !state.config.remote_backup_enabled() {
        return Err(remote_not_configured());
    }

    let store = remote_backup::store_from_config(&state.config).map_err(backup_err)?;

    let req_key: Option<String> = if raw_body.is_empty() {
        None
    } else {
        serde_json::from_slice::<RestoreRemoteRequest>(&raw_body)
            .ok()
            .and_then(|r| r.key)
    };

    // Resolve key: use provided key or pick the latest.
    let key = if let Some(k) = req_key.clone() {
        k
    } else {
        let manifests = remote_backup::list(store.as_ref(), &state.config.backup_prefix)
            .await
            .map_err(backup_err)?;
        manifests
            .into_iter()
            .next()
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(json!({"error": "no remote backups found"})),
                )
            })?
            .object_key
    };

    let bundle_bytes = remote_backup::download(store.as_ref(), &key)
        .await
        .map_err(backup_err)?;

    // Parse the sidecar manifest to get migration_version.
    let sidecar_key = format!("{}.manifest.json", key);
    let sidecar_bytes = remote_backup::download(store.as_ref(), &sidecar_key)
        .await
        .map_err(backup_err)?;
    let manifest: remote_backup::BackupManifest =
        serde_json::from_slice(&sidecar_bytes).map_err(|e| backup_err(e.into()))?;

    // Get the current local migration version.
    let local_version: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _migrations")
        .fetch_one(state.pool())
        .await
        .map_err(|e| backup_err(e.into()))?;

    let db_path = state.config.db_file_path().ok_or((
        StatusCode::BAD_REQUEST,
        Json(json!({"error": "restore requires a file-based database"})),
    ))?;

    remote_backup::stage_restore(
        bundle_bytes,
        &manifest,
        local_version as u32,
        &db_path,
        &state.config.storage_dir,
    )
    .await
    .map_err(backup_err)?;

    Ok(Json(json!({
        "staged": true,
        "restart_required": true,
        "object_key": key,
        "message": "Restore staged. Restart the server to apply."
    })))
}
