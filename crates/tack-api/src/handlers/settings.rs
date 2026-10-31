//! App-level settings persisted in the `app_meta` key/value table.
//!
//! Holds **cloud backup** (S3-compatible configuration) — a value stored in
//! `app_meta` overrides the `TACK_*` environment default. See
//! [`remote_backup::effective_config`](tack_app::remote_backup::effective_config).

use crate::extract::Json;
use axum::{
    extract::State,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tack_app::remote_backup::{self, BackupConfig};
use tack_protocol::execution::StableErrorCode;
use tracing::instrument;
use validator::Validate;

use crate::error::ApiError;
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

async fn load(state: &AppState) -> BackupSettings {
    tack_db::queries::app_meta::app_meta_value(&state.repo, BACKUP_KEY)
        .await
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

async fn save(state: &AppState, settings: &BackupSettings) -> Result<(), ApiError> {
    let value = serde_json::to_string(settings).unwrap_or_else(|_| "{}".into());
    state.repo.set_app_meta(BACKUP_KEY, &value).await?;
    Ok(())
}

/// Client-safe view of the effective config — the secret key is replaced by a
/// boolean flag and never sent to the browser.
fn public_view(cfg: &BackupConfig) -> Value {
    json!({
        "configured": cfg.enabled(),
        "endpoint": cfg.endpoint,
        "bucket": cfg.bucket,
        "region": cfg.region,
        "access_key": cfg.access_key,
        "secret_key_set": cfg.secret_key.is_some(),
        "prefix": cfg.prefix,
        "retention": cfg.retention,
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
    let cfg = remote_backup::effective_config(&state.repo, &state.config.backup_config()).await;
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
) -> Result<Response, ApiError> {
    input
        .validate()
        .map_err(|e| ApiError::Unprocessable(e.to_string()))?;

    let endpoint = clean(input.endpoint);
    if let Some(url) = &endpoint
        && let Err(err) = crate::config::validate_outbound_url("endpoint", url)
    {
        return Ok(
            ApiError::field(StableErrorCode::InvalidRequest, "endpoint", err.to_string())
                .into_response(),
        );
    }

    let mut current = load(&state).await;

    let endpoint_changed = endpoint != current.endpoint;
    current.endpoint = endpoint;
    current.bucket = clean(input.bucket);
    current.region = clean(input.region);
    current.access_key = clean(input.access_key);
    // Only replace the secret when a non-blank value is supplied; otherwise keep
    // whatever is stored (the UI sends the field blank when it is unchanged).
    // A secret stored for one endpoint is never sent to another: moving the
    // endpoint without supplying a new secret drops the stored one.
    let mut secret_cleared = false;
    if let Some(secret) = clean(input.secret_key) {
        current.secret_key = Some(secret);
    } else if endpoint_changed && current.secret_key.take().is_some() {
        secret_cleared = true;
    }
    current.prefix = clean(input.prefix);
    current.retention = input.retention;

    save(&state, &current).await?;

    let cfg = remote_backup::effective_config(&state.repo, &state.config.backup_config()).await;
    let mut view = public_view(&cfg);
    if secret_cleared {
        view["secret_cleared"] = Value::Bool(true);
    }
    Ok(Json(view).into_response())
}
