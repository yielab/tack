//! Turns the embedded runner on/off from the UI, when the operator and the runner share a
//! machine. The rules live in `tack_app::local_runner`; these are the routes.
//!
//! `AppState::local_runner` is `None` for any caller that never wired a control in (a bare
//! `tack_api::serve()`, or a test): every route here then treats it exactly like a non-loopback
//! bind, absent rather than refusing (see `router.rs`'s `local_runner_routes`).

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tack_app::local_runner::{
    self as app, FolderError, LocalRunnerControl, LocalRunnerControlError, TestRunError,
};
use tracing::instrument;

use crate::error::{ApiError, ApiResult};
use crate::router::AppState;

impl From<LocalRunnerControlError> for ApiError {
    fn from(error: LocalRunnerControlError) -> Self {
        ApiError::Internal(anyhow::anyhow!(error.to_string()))
    }
}

fn require_control(state: &AppState) -> ApiResult<&dyn LocalRunnerControl> {
    state
        .local_runner
        .as_deref()
        .ok_or_else(|| ApiError::NotFound("local runner control is not available".into()))
}

/// GET /api/local-runner — the persisted preference, the live runtime state. Only mounted on a
/// loopback bind with a control actually wired in — see `router.rs`'s `local_runner_routes`.
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/local-runner",
    tag = "local-runner",
    responses(
        (status = 200, description = "Embedded-runner preference and runtime state", body = serde_json::Value),
    ),
)]
pub async fn get_local_runner(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let control = require_control(&state)?;
    let enabled = app::effective_enabled(&state.repo, state.config.local_runner_enable).await;
    let status = control.status().await;
    Ok(Json(json!({
        "enabled": enabled,
        "state": status.state,
        "since": status.since,
        "max_concurrent_attempts": status.max_concurrent_attempts,
        "active_attempts": status.active_attempts,
        "config_file": control.config_file(),
    })))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateLocalRunner {
    pub enabled: bool,
}

/// PUT /api/local-runner — save the preference and start/stop the embedded runner to match,
/// immediately, with no restart. Persist first, then reconcile the runtime: a crash between the
/// two still boots correctly next time.
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
    app::set_enabled(&state.repo, input.enabled).await?;
    if input.enabled {
        control.start().await?;
    } else {
        control.stop().await;
    }
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
    /// The install command the checked-out commit's lockfile implies, when it has one.
    pub install: Option<DetectedInstall>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct DetectedInstall {
    pub command: String,
    pub lockfile: String,
}

impl From<app::FolderFacts> for FolderCheck {
    fn from(facts: app::FolderFacts) -> Self {
        Self {
            exists: facts.exists,
            is_dir: facts.is_dir,
            is_git: facts.is_git,
            branch: facts.branch,
            remote_url: facts.remote_url,
            dirty_files: facts.dirty_files,
            install: facts.install.map(|found| DetectedInstall {
                command: found.command,
                lockfile: found.lockfile,
            }),
        }
    }
}

type FolderReply = (StatusCode, Json<Value>);

fn folder_error(status: StatusCode, code: &str, message: &str, details: Value) -> FolderReply {
    let error = json!({
        "status": status.as_u16(), "code": code, "message": message, "details": details,
    });
    (status, Json(json!({ "error": error })))
}

fn folder_reply(error: FolderError) -> FolderReply {
    let (status, code, details) = match error {
        FolderError::Unavailable => (StatusCode::CONFLICT, "local_runner_unavailable", json!({})),
        FolderError::NotAbsolute => (
            StatusCode::BAD_REQUEST,
            "invalid_request",
            json!({ "field": "path" }),
        ),
        FolderError::NotEmpty => (
            StatusCode::CONFLICT,
            "folder_not_empty",
            json!({ "field": "path" }),
        ),
        FolderError::CouldNotCreate | FolderError::GitInitFailed => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            json!({}),
        ),
    };
    folder_error(status, code, &error.to_string(), details)
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
) -> Result<Json<FolderCheck>, FolderReply> {
    let path = app::folder_path(state.local_runner.is_some(), &input.path).map_err(folder_reply)?;
    Ok(Json(app::check_folder(&path).await.into()))
}

/// POST /api/local-runner/init-folder — creates `path` and runs `git init`. An existing
/// non-empty directory (or a file) is a 409.
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
) -> Result<Json<Value>, FolderReply> {
    let path = app::folder_path(state.local_runner.is_some(), &input.path).map_err(folder_reply)?;
    app::init_folder(&path).await.map_err(folder_reply)?;
    Ok(Json(json!({ "path": input.path })))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct TestRun {
    pub harness_kind: String,
}

fn test_run_reply(error: TestRunError) -> FolderReply {
    let message = error.to_string();
    let (status, code) = match error {
        TestRunError::NotConfigured | TestRunError::NotRunning | TestRunError::NotEnrolled => {
            (StatusCode::CONFLICT, "local_runner_unavailable")
        }
        TestRunError::NoImplementerProfile => (StatusCode::NOT_FOUND, "not_found"),
        TestRunError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        TestRunError::Refused(refusal) => return super::executions::refused(refusal),
    };
    folder_error(status, code, &message, json!({}))
}

/// POST /api/local-runner/test-run — runs the harness once on this machine from an empty
/// scratch directory: no remote, no typed model. It files the run under an archived
/// `Agent tests` project (created on first use, so it never shows in the project list) and
/// answers the new request's id.
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
) -> Result<Json<Value>, FolderReply> {
    let request_id = app::start_test_run(
        &state.repo,
        &tack_protocol::SystemClock,
        state.local_runner.as_deref(),
        state.workspace_id,
        input.harness_kind,
    )
    .await
    .map_err(test_run_reply)?;
    Ok(Json(json!({ "request_id": request_id })))
}
