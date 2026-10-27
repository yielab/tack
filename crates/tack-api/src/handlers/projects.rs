use crate::extract::{Json, Path};
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use tack_protocol::execution::StableErrorCode;
use tracing::instrument;
use uuid::Uuid;
use validator::Validate;

use tack_core::models::{CodeOrigin, CreateProject, Project, UpdateProject};

use crate::error::{ApiError, ApiResult};
use crate::openapi::ErrorEnvelope;
use crate::router::AppState;

#[utoipa::path(
    post,
    path = "/api/projects",
    tag = "projects",
    request_body = CreateProject,
    responses(
        (status = 200, description = "Project created", body = Project),
        (status = 400, description = "Validation error", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn create_project(
    State(state): State<AppState>,
    Json(input): Json<CreateProject>,
) -> Response {
    if let Err(e) = input.validate() {
        return ApiError::BadRequest(e.to_string()).into_response();
    }
    let code_origin = input.code_origin.unwrap_or_default();
    if let Some(message) = repository_problem(code_origin, input.repository.as_deref()) {
        return repository_rejected(message);
    }
    match state.repo.create_project(state.workspace_id, input).await {
        Ok(project) => Json(serde_json::to_value(project).unwrap()).into_response(),
        Err(e) => ApiError::from(e).into_response(),
    }
}

/// Why a `code_origin`/`repository` pair cannot be stored, or `None` when it
/// can: a folder origin needs an absolute path, a url origin a URL. Shared by
/// creation and update so both reject exactly the same combinations.
fn repository_problem(code_origin: CodeOrigin, repository: Option<&str>) -> Option<&'static str> {
    match (code_origin, repository) {
        (CodeOrigin::Folder | CodeOrigin::NewFolder, Some(path)) if !path.starts_with('/') => {
            Some("repository must be an absolute path for a folder code origin")
        }
        (CodeOrigin::Url, Some(url))
            if !url
                .split_once("://")
                .is_some_and(|(scheme, rest)| !scheme.is_empty() && !rest.is_empty()) =>
        {
            Some("repository must be a URL for a url code origin")
        }
        _ => None,
    }
}

/// The 400 for a rejected `repository`: the protocol envelope naming the
/// field, so a form can show the message under that one input.
fn repository_rejected(message: &str) -> Response {
    ApiError::field(StableErrorCode::InvalidRequest, "repository", message).into_response()
}

#[utoipa::path(
    get,
    path = "/api/projects",
    tag = "projects",
    responses((status = 200, description = "All projects in the workspace", body = Vec<Project>)),
)]
#[instrument(skip(state))]
pub async fn list_projects(State(state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let projects = state.repo.list_projects(state.workspace_id).await?;
    Ok(Json(serde_json::to_value(projects).unwrap()))
}

#[utoipa::path(
    get,
    path = "/api/projects/{id}",
    tag = "projects",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "The project", body = Project),
        (status = 404, description = "Project not found", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn get_project(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let project = state
        .repo
        .get_project(id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {id} not found")))?;
    Ok(Json(serde_json::to_value(project).unwrap()))
}

#[utoipa::path(
    patch,
    path = "/api/projects/{id}",
    tag = "projects",
    params(("id" = Uuid, Path, description = "Project ID")),
    request_body = UpdateProject,
    responses(
        (status = 200, description = "Updated project", body = Project),
        (status = 400, description = "Validation error", body = ErrorEnvelope),
        (status = 404, description = "Project not found", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn update_project(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(mut input): Json<UpdateProject>,
) -> Response {
    if let Err(e) = input.validate() {
        return ApiError::BadRequest(e.to_string()).into_response();
    }
    if let Some(Some(command)) = &mut input.prepare_command {
        match tack_protocol::prepare::clean_command(command) {
            Ok(clean) => *command = clean,
            Err(message) => return ApiError::BadRequest(message.to_owned()).into_response(),
        }
    }
    let current = match state.repo.get_project(id).await {
        Ok(Some(project)) => project,
        Ok(None) => return ApiError::NotFound(format!("Project {id} not found")).into_response(),
        Err(e) => return ApiError::from(e).into_response(),
    };
    let code_origin = input.code_origin.unwrap_or(current.code_origin);
    let repository = match &input.repository {
        Some(value) => value.as_deref(),
        None => current.repository.as_deref(),
    };
    if let Some(message) = repository_problem(code_origin, repository) {
        return repository_rejected(message);
    }
    if let Some(Some(status)) = &input.auto_start_status {
        let workflow = input.workflow.as_ref().unwrap_or(&current.workflow);
        if !workflow.statuses.iter().any(|s| &s.name == status) {
            return ApiError::BadRequest(format!(
                "auto_start_status {status:?} is not a status of the project's workflow"
            ))
            .into_response();
        }
    }
    match state.repo.update_project(id, input).await {
        Ok(Some(project)) => Json(serde_json::to_value(project).unwrap()).into_response(),
        Ok(None) => ApiError::NotFound(format!("Project {id} not found")).into_response(),
        Err(e) => ApiError::from(e).into_response(),
    }
}

#[utoipa::path(
    delete,
    path = "/api/projects/{id}",
    tag = "projects",
    params(("id" = Uuid, Path, description = "Project ID")),
    responses(
        (status = 200, description = "Deleted", body = serde_json::Value),
        (status = 404, description = "Project not found", body = ErrorEnvelope),
    ),
)]
#[instrument(skip(state))]
pub async fn delete_project(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<serde_json::Value>> {
    let deleted = state.repo.delete_project(id).await?;
    if !deleted {
        return Err(ApiError::NotFound(format!("Project {id} not found")));
    }
    Ok(Json(serde_json::json!({"deleted": true})))
}
