use axum::Json;
use axum::extract::{Path, State};
use tracing::instrument;
use uuid::Uuid;
use validator::Validate;

use tack_core::models::{CreateProject, Project, UpdateProject};

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
) -> ApiResult<Json<serde_json::Value>> {
    input
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    let project = state
        .repo
        .update_project(id, input)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {id} not found")))?;
    Ok(Json(serde_json::to_value(project).unwrap()))
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
