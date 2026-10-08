//! Turns the embedded runner on/off from the UI and lets a UI-only user hand
//! it a provider key — ADR 0061 decisions 2 and 6.
//!
//! Before this, the embedded runner started only from a boot-time flag
//! (`tack serve --with-runner`/`TACK_LOCAL_RUNNER_ENABLE`) and a provider
//! key could only be set on the runner's own machine via the console
//! (`tack runner secret set`). Both remain true — this module adds a second
//! way to reach the exact same on/off gate and the exact same secret
//! store, from the UI, when the operator and the runner share a machine.
//!
//! **This crate never depends on `tack-runner`** (`CLAUDE.md`'s crate map).
//! [`LocalRunnerControl`] is the seam: a trait this crate defines and calls,
//! implemented by whichever binary actually wires an embedded runner in
//! (`tack-cli`'s `local_runner` module). `AppState::local_runner` is `None`
//! for any caller that never wired one in (a bare `tack_api::serve()`, or a
//! test) — every route in this module treats that exactly like a
//! non-loopback bind: absent, not refusing (see `router.rs`'s
//! `local_runner_routes`).
//!
//! Follows the Cloud Backup / Orchestration precedent in `handlers/
//! settings.rs` for the persisted on/off flag: an `app_meta`-stored value
//! overrides the env/CLI-flag default, computed fresh on every read, never
//! cached. Unlike those two, the *secret* half of this module never touches
//! `app_meta` at all — a value handed to [`LocalRunnerControl::set_secret`]
//! goes straight to wherever the concrete implementation's own store lives
//! (the runner's OS keychain or its owner-only file) and this crate never
//! learns which.

use std::sync::Arc;

use async_trait::async_trait;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tracing::instrument;

use crate::error::{ApiError, ApiResult};
use crate::router::AppState;

const LOCAL_RUNNER_KEY: &str = "local_runner_config";

#[derive(Debug, Default, Serialize, Deserialize)]
struct LocalRunnerSettings {
    #[serde(default)]
    enabled: Option<bool>,
}

async fn load(pool: &sqlx::SqlitePool) -> LocalRunnerSettings {
    let raw: Option<String> = sqlx::query_scalar("SELECT value FROM app_meta WHERE key = ?")
        .bind(LOCAL_RUNNER_KEY)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    raw.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

async fn save(pool: &sqlx::SqlitePool, settings: &LocalRunnerSettings) -> Result<(), sqlx::Error> {
    let value = serde_json::to_string(settings).unwrap_or_else(|_| "{}".into());
    sqlx::query(
        "INSERT INTO app_meta (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(LOCAL_RUNNER_KEY)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// The on/off preference Tack should actually use right now: the
/// `app_meta`-stored value if the UI has ever set one, else the env/CLI-flag
/// default (`--with-runner`/`TACK_LOCAL_RUNNER_ENABLE`, folded into
/// `AppConfig::local_runner_enable` at load time). Read fresh on every call,
/// never cached, so a toggle a moment ago is always reflected.
pub async fn effective_local_runner_enabled(state: &AppState) -> bool {
    load(state.pool())
        .await
        .enabled
        .unwrap_or(state.config.local_runner_enable)
}

/// The embedded runner's actual runtime state — never the persisted
/// preference above. A preference can read "on" for one instant after boot,
/// before the auto-start task (`server.rs`) has actually run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    Stopped,
    Running,
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeStatus {
    pub state: RuntimeState,
    pub since: Option<DateTime<Utc>>,
}

/// What asking the runner's own configured provider for its model catalog
/// produced. Deliberately not the real provider crate's own catalog-status
/// type — this crate never depends on `tack-runner`; the concrete
/// [`LocalRunnerControl`] translates its real type into this one at the
/// boundary, the same way `AppState::local_runner` itself crosses it.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CatalogSnapshot {
    NotConfigured,
    SecretUnresolved,
    Unreachable {
        http_status: Option<u16>,
    },
    Configured {
        model_count: usize,
        checked_at: DateTime<Utc>,
    },
}

/// One stored secret's name and when it was last set — never its value.
#[derive(Debug, Clone, Serialize)]
pub struct SecretMeta {
    pub name: String,
    /// `None` when the name is present in the store but this process has no
    /// record of when it was set (e.g. it was written by `tack runner
    /// secret set` before this UI ever ran) — never a fabricated timestamp.
    pub set_at: Option<DateTime<Utc>>,
}

/// Every variant names an operation and, where the backend gave one, its own
/// reason — never a secret value. Safe to log or fold into an [`ApiError`].
#[derive(Debug, thiserror::Error)]
pub enum LocalRunnerControlError {
    #[error("embedded runner failed to start: {0}")]
    StartFailed(String),
    #[error("secret store operation failed: {0}")]
    SecretStore(String),
}

impl From<LocalRunnerControlError> for ApiError {
    fn from(error: LocalRunnerControlError) -> Self {
        ApiError::Internal(anyhow::anyhow!(error.to_string()))
    }
}

/// The seam this crate calls to control an embedded runner without ever
/// depending on `tack-runner` or learning where its secret store lives.
/// `crates/tack-cli/src/local_runner.rs` is the only implementation today —
/// see that module's doc comment for how it composes this with
/// `tack_runner::bootstrap`.
#[async_trait]
pub trait LocalRunnerControl: Send + Sync {
    /// Whether the embedded runner is actually alive right now, and since
    /// when. Never the persisted preference — see [`RuntimeStatus`]'s doc.
    async fn status(&self) -> RuntimeStatus;

    /// Starts the embedded runner if it is not already running. A no-op,
    /// not an error, if it is.
    async fn start(&self) -> Result<(), LocalRunnerControlError>;

    /// Stops the embedded runner if running. A no-op otherwise.
    async fn stop(&self);

    /// The enrolled runner's id once the runner has registered, `None` before.
    async fn runner_id(&self) -> Option<String>;

    /// Names and set-at timestamps of every stored secret. Never values.
    async fn list_secrets(&self) -> Vec<SecretMeta>;

    /// Stores `value` under `name`, overwriting any existing entry. A
    /// freshly configured provider's catalog is visible on the very next
    /// [`LocalRunnerControl::catalog`] call. If the runner is running and
    /// `name` is the entry a configured provider resolves its credential
    /// from, the runner is restarted before this returns: it received its
    /// configuration by value when it was spawned, so the only way the
    /// next dispatch can use this key is a runner that booted with it. A
    /// restart that fails leaves the runner stopped and returns the error,
    /// never a runner that still serves the old configuration.
    async fn set_secret(&self, name: &str, value: &str) -> Result<(), LocalRunnerControlError>;

    /// Removes the secret named `name`. Not an error if it was already
    /// absent (matches `rm -f`). Restarts a running runner under the same
    /// rule as [`LocalRunnerControl::set_secret`], in the other direction.
    async fn remove_secret(&self, name: &str) -> Result<(), LocalRunnerControlError>;

    /// What the configured provider's catalog looks like right now.
    /// Computed fresh on every call, never cached, so there is nothing to
    /// invalidate.
    async fn catalog(&self) -> CatalogSnapshot;

    /// Read-only resolution of a stored `secret_reference`
    /// (`store:<name>` or `env:<VARIABLE>`, see
    /// `tack_runner::secrets::SecretStore::resolve`) for the server's own
    /// outbound calls — e.g. a project's GitHub token
    /// (`github_sync::github_token_for_project`). The value never leaves
    /// the process in a response or a log; only the reference *name* may
    /// ever be logged by a caller.
    async fn resolve_secret(&self, reference: &str) -> Result<String, LocalRunnerControlError>;
}

fn require_control(state: &AppState) -> ApiResult<Arc<dyn LocalRunnerControl>> {
    state
        .local_runner
        .clone()
        .ok_or_else(|| ApiError::NotFound("local runner control is not available".into()))
}

fn runtime_state_str(state: RuntimeState) -> &'static str {
    match state {
        RuntimeState::Running => "running",
        RuntimeState::Stopped => "stopped",
    }
}

fn catalog_json(catalog: &CatalogSnapshot) -> Value {
    serde_json::to_value(catalog).unwrap_or(Value::Null)
}

/// GET /api/local-runner — the persisted preference, the live runtime
/// state, and the current provider catalog. Only mounted on a loopback bind
/// with a control actually wired in — see `router.rs`'s `local_runner_routes`.
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/local-runner",
    tag = "local-runner",
    responses(
        (status = 200, description = "Embedded-runner preference, runtime state, and provider catalog", body = serde_json::Value),
    ),
)]
pub async fn get_local_runner(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let control = require_control(&state)?;
    let enabled = effective_local_runner_enabled(&state).await;
    let status = control.status().await;
    let catalog = control.catalog().await;
    Ok(Json(json!({
        "enabled": enabled,
        "state": runtime_state_str(status.state),
        "since": status.since,
        "catalog": catalog_json(&catalog),
    })))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateLocalRunner {
    pub enabled: bool,
}

/// PUT /api/local-runner — save the preference and start/stop the embedded
/// runner to match, immediately, with no restart. Persist first, then
/// reconcile the runtime: a crash between the two still boots correctly
/// next time.
#[instrument(skip(state))]
#[utoipa::path(
    put,
    path = "/api/local-runner",
    tag = "local-runner",
    request_body = UpdateLocalRunner,
    responses((status = 204, description = "Preference saved and the runtime reconciled to match")),
)]
pub async fn put_local_runner(
    State(state): State<AppState>,
    Json(input): Json<UpdateLocalRunner>,
) -> ApiResult<StatusCode> {
    let control = require_control(&state)?;
    save(
        state.pool(),
        &LocalRunnerSettings {
            enabled: Some(input.enabled),
        },
    )
    .await?;

    if input.enabled {
        control.start().await?;
    } else {
        control.stop().await;
    }

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/local-runner/secrets — names and set-at timestamps only.
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/local-runner/secrets",
    tag = "local-runner",
    responses((status = 200, description = "Stored secret names and set-at timestamps, never values", body = serde_json::Value)),
)]
pub async fn list_local_runner_secrets(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let control = require_control(&state)?;
    let secrets = control.list_secrets().await;
    Ok(Json(json!({
        "data": secrets
            .into_iter()
            .map(|s| json!({ "name": s.name, "set_at": s.set_at }))
            .collect::<Vec<_>>(),
    })))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetLocalRunnerSecret {
    pub value: String,
}

/// PUT /api/local-runner/secrets/{name} — store a value. Never echoes it
/// back, not even as a hash: the response carries nothing but a status code.
#[instrument(skip(state, input))]
#[utoipa::path(
    put,
    path = "/api/local-runner/secrets/{name}",
    tag = "local-runner",
    request_body = SetLocalRunnerSecret,
    responses((status = 204, description = "Stored; the value is never echoed back")),
)]
pub async fn put_local_runner_secret(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(input): Json<SetLocalRunnerSecret>,
) -> ApiResult<StatusCode> {
    let control = require_control(&state)?;
    control.set_secret(&name, &input.value).await?;
    // `catalog()` always computes fresh (see its own doc comment), so
    // calling it here — even though this route discards the result — is
    // what makes "set a key" and "the catalog reflects it" happen inside
    // one request, rather than leaving it to whichever GET happens next.
    let _ = control.catalog().await;
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/local-runner/secrets/{name} — not an error if already absent.
#[instrument(skip(state))]
#[utoipa::path(
    delete,
    path = "/api/local-runner/secrets/{name}",
    tag = "local-runner",
    responses((status = 204, description = "Removed (or already absent)")),
)]
pub async fn delete_local_runner_secret(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> ApiResult<StatusCode> {
    let control = require_control(&state)?;
    control.remove_secret(&name).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct FolderPath {
    pub path: String,
}

/// What the folder check found at a path.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct FolderCheck {
    pub exists: bool,
    pub is_dir: bool,
    pub is_git: bool,
    pub branch: Option<String>,
    pub remote_url: Option<String>,
    pub dirty_files: usize,
}

type FolderError = (StatusCode, Json<Value>);

fn folder_error(status: StatusCode, code: &str, message: &str, details: Value) -> FolderError {
    (
        status,
        Json(json!({ "error": {
            "status": status.as_u16(),
            "code": code,
            "message": message,
            "details": details,
        }})),
    )
}

/// The folder routes run `git` in this process, so they only make sense when
/// the embedded runner (same machine) is configured, and only for absolute paths.
fn folder_path(state: &AppState, input: &FolderPath) -> Result<std::path::PathBuf, FolderError> {
    if state.local_runner.is_none() {
        return Err(folder_error(
            StatusCode::CONFLICT,
            "local_runner_unavailable",
            "The embedded runner is not configured",
            json!({}),
        ));
    }
    let path = std::path::PathBuf::from(&input.path);
    if !path.is_absolute() {
        return Err(folder_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "path must be absolute",
            json!({ "field": "path" }),
        ));
    }
    Ok(path)
}

/// Runs `git` in `dir`; `Some(stdout)` only on a zero exit within the timeout.
async fn run_git(dir: &std::path::Path, args: &[&str]) -> Option<String> {
    let mut command = tokio::process::Command::new("git");
    command
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(10), command.output())
        .await
        .ok()?
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// POST /api/local-runner/check-folder — what is at `path`; writes nothing.
#[instrument(skip(state))]
#[utoipa::path(
    post,
    path = "/api/local-runner/check-folder",
    tag = "local-runner",
    request_body = FolderPath,
    responses(
        (status = 200, description = "Whether the path exists, is a directory, and its git facts", body = FolderCheck),
        (status = 400, description = "The path is not absolute"),
        (status = 409, description = "The embedded runner is not configured"),
    ),
)]
pub async fn check_local_runner_folder(
    State(state): State<AppState>,
    Json(input): Json<FolderPath>,
) -> Result<Json<FolderCheck>, FolderError> {
    let path = folder_path(&state, &input)?;
    let exists = tokio::fs::try_exists(&path).await.unwrap_or(false);
    let is_dir = exists && tokio::fs::metadata(&path).await.is_ok_and(|m| m.is_dir());
    let is_git = is_dir
        && run_git(&path, &["rev-parse", "--is-inside-work-tree"])
            .await
            .is_some_and(|out| out == "true");
    let (mut branch, mut remote_url, mut dirty_files) = (None, None, 0);
    if is_git {
        branch = run_git(&path, &["rev-parse", "--abbrev-ref", "HEAD"]).await;
        remote_url = run_git(&path, &["remote", "get-url", "origin"]).await;
        dirty_files = run_git(&path, &["status", "--porcelain"])
            .await
            .map_or(0, |out| out.lines().count());
    }
    Ok(Json(FolderCheck {
        exists,
        is_dir,
        is_git,
        branch,
        remote_url,
        dirty_files,
    }))
}

/// POST /api/local-runner/init-folder — creates `path` and runs `git init`.
/// An existing non-empty directory (or a file) is a 409.
#[instrument(skip(state))]
#[utoipa::path(
    post,
    path = "/api/local-runner/init-folder",
    tag = "local-runner",
    request_body = FolderPath,
    responses(
        (status = 200, description = "Directory created and initialised as a git repository", body = serde_json::Value),
        (status = 400, description = "The path is not absolute"),
        (status = 409, description = "The path exists and is not an empty directory, or the embedded runner is not configured"),
    ),
)]
pub async fn init_local_runner_folder(
    State(state): State<AppState>,
    Json(input): Json<FolderPath>,
) -> Result<Json<Value>, FolderError> {
    let path = folder_path(&state, &input)?;
    let internal = |message: &str| {
        folder_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            message,
            json!({}),
        )
    };
    if tokio::fs::try_exists(&path).await.unwrap_or(false) {
        let empty_dir = match tokio::fs::read_dir(&path).await {
            Ok(mut entries) => entries.next_entry().await.is_ok_and(|e| e.is_none()),
            Err(_) => false,
        };
        if !empty_dir {
            return Err(folder_error(
                StatusCode::CONFLICT,
                "folder_not_empty",
                "The path exists and is not an empty directory",
                json!({ "field": "path" }),
            ));
        }
    }
    tokio::fs::create_dir_all(&path)
        .await
        .map_err(|_| internal("Could not create the directory"))?;
    run_git(&path, &["init"])
        .await
        .ok_or_else(|| internal("git init failed"))?;
    Ok(Json(json!({ "path": input.path })))
}

/// GET /api/local-runner/harness-verification — per harness kind, when its
/// latest succeeded attempt ended. A kind with no succeeded attempt is absent.
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/local-runner/harness-verification",
    tag = "local-runner",
    responses((status = 200, description = "Harness kind to the ended_at of its latest succeeded attempt", body = serde_json::Value)),
)]
pub async fn get_harness_verification(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT json_extract(actual_execution, '$.harness_kind') AS kind, MAX(ended_at) AS ended_at \
         FROM execution_attempts \
         WHERE state = 'succeeded' AND actual_execution IS NOT NULL AND ended_at IS NOT NULL \
         GROUP BY kind HAVING kind IS NOT NULL",
    )
    .fetch_all(state.pool())
    .await?;
    let harnesses: serde_json::Map<String, Value> = rows
        .into_iter()
        .map(|(k, t)| (k, Value::String(t)))
        .collect();
    Ok(Json(json!({ "harnesses": harnesses })))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct TestRun {
    pub harness_kind: String,
}

const TEST_PROJECT_NAME: &str = "Agent tests";

/// POST /api/local-runner/test-run — runs the harness once on this machine
/// from an empty scratch directory: no remote, no typed model. It files the
/// run under an archived `Agent tests` project (created on first use, so it
/// never shows in the project list) and answers the new request's id.
#[instrument(skip(state))]
#[utoipa::path(
    post,
    path = "/api/local-runner/test-run",
    tag = "local-runner",
    request_body = TestRun,
    responses(
        (status = 200, description = "The execution request that was queued", body = serde_json::Value),
        (status = 409, description = "The embedded runner is not configured, not running or not enrolled"),
    ),
)]
pub async fn post_test_run(
    State(state): State<AppState>,
    Json(input): Json<TestRun>,
) -> Result<Json<Value>, FolderError> {
    let unavailable = |message: &str| {
        folder_error(
            StatusCode::CONFLICT,
            "local_runner_unavailable",
            message,
            json!({}),
        )
    };
    let internal = |message: &str| {
        folder_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            message,
            json!({}),
        )
    };
    let Some(control) = state.local_runner.clone() else {
        return Err(unavailable("The embedded runner is not configured"));
    };
    if control.status().await.state != RuntimeState::Running {
        return Err(unavailable("The embedded runner is not running"));
    }
    let runner_id = control
        .runner_id()
        .await
        .ok_or_else(|| unavailable("The embedded runner is not enrolled"))?;
    let pool = state.pool();
    let profile: Option<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT id, name, instructions, tool_policy, limits FROM agent_profiles \
         WHERE kind = 'implementer'",
    )
    .fetch_optional(pool)
    .await
    .map_err(|_| internal("Could not look up the Implementer profile"))?;
    let (profile_id, name, instructions, tool_policy, limits) = profile.ok_or_else(|| {
        folder_error(
            StatusCode::NOT_FOUND,
            "not_found",
            "The Implementer profile does not exist",
            json!({}),
        )
    })?;

    let existing: Option<String> =
        sqlx::query_scalar("SELECT id FROM projects WHERE workspace_id = ? AND name = ?")
            .bind(state.workspace_id.to_string())
            .bind(TEST_PROJECT_NAME)
            .fetch_optional(pool)
            .await
            .map_err(|_| internal("Could not look up the test project"))?;
    let project = match existing.and_then(|id| id.parse::<uuid::Uuid>().ok()) {
        Some(id) => state
            .repo
            .get_project(id)
            .await
            .map_err(|_| internal("Could not load the test project"))?
            .ok_or_else(|| internal("Could not load the test project"))?,
        None => {
            let created = state
                .repo
                .create_project(
                    state.workspace_id,
                    tack_core::models::CreateProject {
                        name: TEST_PROJECT_NAME.to_owned(),
                        description: Some(
                            "Runs started from Settings to check an agent".to_owned(),
                        ),
                        project_type: tack_core::models::ProjectType::Software,
                        template: None,
                    },
                )
                .await
                .map_err(|_| internal("Could not create the test project"))?;
            state
                .repo
                .update_project(
                    created.id,
                    tack_core::models::UpdateProject {
                        archived: Some(true),
                        ..Default::default()
                    },
                )
                .await
                .map_err(|_| internal("Could not archive the test project"))?;
            created
        }
    };
    let initial_status = project
        .workflow
        .initial_status()
        .map_err(|_| internal("The test project has no initial status"))?;
    let now = Utc::now();
    let item = state
        .repo
        .create_item(
            project.id,
            &initial_status,
            tack_core::models::CreateItem {
                title: format!("Agent test {}", now.format("%Y-%m-%d %H:%M:%S")),
                ..Default::default()
            },
        )
        .await
        .map_err(|_| internal("Could not create the test item"))?;

    let parse = |raw: &str| serde_json::from_str::<Value>(raw).unwrap_or_else(|_| json!({}));
    let tool_policy = parse(&tool_policy);
    // The profile's tools for this harness, as the Run dialog sends them; none would leave
    // the agent unable to act.
    let tools = tool_policy["tools"][input.harness_kind.as_str()].clone();
    let tools = if tools.is_array() { tools } else { json!([]) };
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        "x-tack-principal",
        axum::http::HeaderValue::from_static("local-runner-test"),
    );
    let create = crate::handlers::executions::CreateExecution {
        item_id: item.id,
        idempotency_key: uuid::Uuid::new_v4().to_string(),
        selector_kind: "exact_runner".to_owned(),
        selector_id: runner_id,
        agent_profile_id: profile_id,
        requested_harness_kind: input.harness_kind,
        requested_model_provider: None,
        requested_model_id: None,
        agent_profile_snapshot: json!({
            "name": name,
            "instructions": instructions,
            "tool_policy": tool_policy,
            "timeout_seconds": 300,
            "budgets": parse(&limits),
        }),
        repository_snapshot: Some(json!({
            "kind": "scratch", "remote": "", "base_revision": "", "subdirectory": null,
        })),
        permission_policy: json!({ "tools": tools, "network": false }),
        budgets: json!({}),
        environment: json!({}),
        metadata: json!({}),
        timeout_seconds: 300,
        status_map_policy_id: None,
        verify: None,
        push_branch: None,
    };
    let operator = crate::handlers::executions::OperatorExecutionState::with_clock(
        state.repo.clone(),
        Arc::new(tack_db::repo::execution::SystemExecutionClock),
    );
    let Json(created) =
        crate::handlers::executions::create_execution(State(operator), headers, Json(create))
            .await?;
    Ok(Json(json!({ "request_id": created.request_id })))
}
