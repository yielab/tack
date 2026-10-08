use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use tack_orch::execution::{ProtocolErrorEnvelope, StableErrorCode};
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
) -> ApiResult<Json<serde_json::Value>> {
    input
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let project = state.repo.create_project(state.workspace_id, input).await?;
    Ok(Json(serde_json::to_value(project).unwrap()))
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
    Json(input): Json<UpdateProject>,
) -> Response {
    if let Err(e) = input.validate() {
        return ApiError::BadRequest(e.to_string()).into_response();
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
    let problem = match (code_origin, repository) {
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
    };
    if let Some(message) = problem {
        let envelope = ProtocolErrorEnvelope::new(
            StableErrorCode::InvalidRequest,
            message,
            "req_operator_projects",
            json!({"field": "repository"}),
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::to_value(envelope).expect("envelope serializes")),
        )
            .into_response();
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
