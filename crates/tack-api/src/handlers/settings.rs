//! App-level settings persisted in the `app_meta` key/value table.
//!
//! Currently this holds the **cloud backup** (S3-compatible) configuration so it
//! can be edited from the UI. Values saved here override the `TACK_BACKUP_*`
//! environment defaults — see [`effective_backup_config`].

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::instrument;
use validator::Validate;

use crate::config::AppConfig;
use crate::error::{ApiError, ApiResult};
use crate::router::AppState;

const BACKUP_KEY: &str = "backup_config";

/// Cloud-backup settings as stored in the DB. Every field is optional so a
/// partially-configured destination round-trips cleanly. The secret key is
/// stored here but never returned to clients.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BackupSettings {
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub bucket: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub access_key: Option<String>,
    #[serde(default)]
    pub secret_key: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub retention: Option<usize>,
}

async fn load(pool: &sqlx::SqlitePool) -> BackupSettings {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM app_meta WHERE key = ?")
        .bind(BACKUP_KEY)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    raw.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

async fn save(pool: &sqlx::SqlitePool, settings: &BackupSettings) -> Result<(), sqlx::Error> {
    let value = serde_json::to_string(settings).unwrap_or_else(|_| "{}".into());
    sqlx::query(
        "INSERT INTO app_meta (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(BACKUP_KEY)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// The backup config the server should actually use: the env/startup [`AppConfig`]
/// with any UI-saved DB settings overlaid on top.
pub async fn effective_backup_config(state: &AppState) -> AppConfig {
    let mut cfg = state.config.clone();
    let s = load(state.pool()).await;

    let nonempty = |o: Option<String>| o.filter(|v| !v.trim().is_empty());

    if let Some(v) = nonempty(s.endpoint) {
        cfg.backup_endpoint = Some(v);
    }
    if let Some(v) = nonempty(s.bucket) {
        cfg.backup_bucket = Some(v);
    }
    if let Some(v) = nonempty(s.region) {
        cfg.backup_region = v;
    }
    if let Some(v) = nonempty(s.access_key) {
        cfg.backup_access_key = Some(v);
    }
    if let Some(v) = nonempty(s.secret_key) {
        cfg.backup_secret_key = Some(v);
    }
    if let Some(v) = nonempty(s.prefix) {
        cfg.backup_prefix = v;
    }
    if let Some(rt) = s.retention {
        cfg.backup_retention = rt;
    }
    cfg
}

/// Client-safe view of the effective config — the secret key is replaced by a
/// boolean flag and never sent to the browser.
fn public_view(cfg: &AppConfig) -> Value {
    json!({
        "configured": cfg.remote_backup_enabled(),
        "endpoint": cfg.backup_endpoint,
        "bucket": cfg.backup_bucket,
        "region": cfg.backup_region,
        "access_key": cfg.backup_access_key,
        "secret_key_set": cfg.backup_secret_key.is_some(),
        "prefix": cfg.backup_prefix,
        "retention": cfg.backup_retention,
    })
}

/// GET /api/settings/backup — current cloud-backup configuration (secret masked).
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/settings/backup",
    tag = "settings",
    responses(
        (status = 200, description = "Cloud-backup config (secret masked as `secret_key_set`)", body = serde_json::Value),
    ),
)]
pub async fn get_backup_settings(State(state): State<AppState>) -> Json<Value> {
    let cfg = effective_backup_config(&state).await;
    Json(public_view(&cfg))
}

/// Incoming update. Any omitted/blank string field clears that override and
/// falls back to the environment default. A blank `secret_key` keeps the
/// existing stored secret (so the masked UI field can be left untouched).
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
pub struct UpdateBackupSettings {
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub bucket: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub access_key: Option<String>,
    #[serde(default)]
    pub secret_key: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    /// Number of backups to keep. Must be at least 1 — `retention: 0` would
    /// prune the backup that was just created (and, when it is the only one,
    /// leave zero backups).
    #[serde(default)]
    #[validate(range(min = 1, message = "retention must be at least 1"))]
    pub retention: Option<usize>,
}

fn clean(o: Option<String>) -> Option<String> {
    o.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// PUT /api/settings/backup — save cloud-backup configuration.
#[instrument(skip(state, input))]
#[utoipa::path(
    put,
    path = "/api/settings/backup",
    tag = "settings",
    request_body = UpdateBackupSettings,
    responses(
        (status = 200, description = "Updated config (secret masked)", body = serde_json::Value),
        (status = 422, description = "Validation error", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn put_backup_settings(
    State(state): State<AppState>,
    Json(input): Json<UpdateBackupSettings>,
) -> ApiResult<Json<Value>> {
    input
        .validate()
        .map_err(|e| ApiError::Unprocessable(e.to_string()))?;

    let mut current = load(state.pool()).await;

    current.endpoint = clean(input.endpoint);
    current.bucket = clean(input.bucket);
    current.region = clean(input.region);
    current.access_key = clean(input.access_key);
    // Only replace the secret when a non-blank value is supplied; otherwise keep
    // whatever is stored (the UI sends the field blank when it is unchanged).
    if let Some(secret) = clean(input.secret_key) {
        current.secret_key = Some(secret);
    }
    current.prefix = clean(input.prefix);
    current.retention = input.retention;

    save(state.pool(), &current).await?;

    let cfg = effective_backup_config(&state).await;
    Ok(Json(public_view(&cfg)))
}
