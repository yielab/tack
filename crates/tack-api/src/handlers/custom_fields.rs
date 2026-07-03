use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use tack_core::models::*;
use tack_db::repo;
use tracing::instrument;
use uuid::Uuid;

use crate::AppState;

// ─── Field Definitions ───────────────────────────────────────

/// POST /api/projects/:id/custom-fields - Create a custom field for a project
#[instrument(skip(state))]
#[utoipa::path(
    post,
    path = "/api/projects/{project_id}/custom-fields",
    tag = "custom-fields",
    params(
        ("project_id" = Uuid, Path, description = "Project ID"),
    ),
    request_body = tack_core::models::CreateCustomField,
    responses(
        (status = 200, description = "Field created", body = tack_core::models::CustomFieldDefinition),
        (status = 404, description = "Project not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn create_field(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(data): Json<CreateCustomField>,
) -> Result<Json<CustomFieldDefinition>, StatusCode> {
    // Verify project exists
    state
        .repo
        .get_project(project_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    repo::custom_fields::create_field(state.pool(), project_id, data)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, project_id = %project_id, "Failed to create custom field");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// GET /api/projects/:id/custom-fields - List all custom fields for a project
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/projects/{project_id}/custom-fields",
    tag = "custom-fields",
    params(
        ("project_id" = Uuid, Path, description = "Project ID"),
    ),
    responses(
        (status = 200, description = "Custom field definitions", body = Vec<tack_core::models::CustomFieldDefinition>),
    ),
)]
pub async fn list_fields(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Vec<CustomFieldDefinition>>, StatusCode> {
    repo::custom_fields::list_fields_for_project(state.pool(), project_id)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, project_id = %project_id, "Failed to list custom fields");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// GET /api/custom-fields/:id - Get a specific custom field
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/custom-fields/{id}",
    tag = "custom-fields",
    params(
        ("id" = Uuid, Path, description = "Custom field ID"),
    ),
    responses(
        (status = 200, description = "The field definition", body = tack_core::models::CustomFieldDefinition),
        (status = 404, description = "Field not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn get_field(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CustomFieldDefinition>, StatusCode> {
    repo::custom_fields::get_field(state.pool(), id)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, field_id = %id, "Failed to get custom field");
            StatusCode::NOT_FOUND
        })
}

/// PATCH /api/custom-fields/:id - Update a custom field
#[instrument(skip(state))]
#[utoipa::path(
    patch,
    path = "/api/custom-fields/{id}",
    tag = "custom-fields",
    params(
        ("id" = Uuid, Path, description = "Custom field ID"),
    ),
    request_body = tack_core::models::UpdateCustomField,
    responses(
        (status = 200, description = "Updated field", body = tack_core::models::CustomFieldDefinition),
        (status = 500, description = "Update failed", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn update_field(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(data): Json<UpdateCustomField>,
) -> Result<Json<CustomFieldDefinition>, StatusCode> {
    repo::custom_fields::update_field(state.pool(), id, data)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, field_id = %id, "Failed to update custom field");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// DELETE /api/custom-fields/:id - Delete a custom field
#[instrument(skip(state))]
#[utoipa::path(
    delete,
    path = "/api/custom-fields/{id}",
    tag = "custom-fields",
    params(
        ("id" = Uuid, Path, description = "Custom field ID"),
    ),
    responses(
        (status = 204, description = "Field deleted"),
    ),
)]
pub async fn delete_field(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    repo::custom_fields::delete_field(state.pool(), id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| {
            tracing::error!(error = %e, field_id = %id, "Failed to delete custom field");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

// ─── Field Values ────────────────────────────────────────────

/// PUT /api/items/:item_id/custom-fields/:field_id - Set a custom field value for an item
#[instrument(skip(state))]
#[utoipa::path(
    put,
    path = "/api/items/{item_id}/custom-fields/{field_id}",
    tag = "custom-fields",
    params(
        ("item_id" = Uuid, Path, description = "Item ID"),
        ("field_id" = Uuid, Path, description = "Custom field ID"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Value set", body = tack_core::models::CustomFieldValue),
        (status = 404, description = "Item or field not found", body = crate::openapi::ErrorEnvelope),
        (status = 422, description = "Value failed field validation", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn set_field_value(
    State(state): State<AppState>,
    Path((item_id, field_id)): Path<(Uuid, Uuid)>,
    Json(data): Json<serde_json::Value>,
) -> Result<Json<CustomFieldValue>, StatusCode> {
    // Verify item exists
    state
        .repo
        .get_item(item_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Verify field exists and validate value against its type
    let field = repo::custom_fields::get_field(state.pool(), field_id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    if let Err(msg) = field.validate_value(&data) {
        tracing::warn!(field_id = %field_id, error = %msg, "Custom field value validation failed");
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    repo::custom_fields::set_field_value(state.pool(), item_id, field_id, data)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, item_id = %item_id, field_id = %field_id, "Failed to set custom field value");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// GET /api/items/:id/custom-fields - Get all custom field values for an item
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/items/{item_id}/custom-fields",
    tag = "custom-fields",
    params(
        ("item_id" = Uuid, Path, description = "Item ID"),
    ),
    responses(
        (status = 200, description = "All custom field values for the item", body = Vec<tack_core::models::CustomFieldValue>),
    ),
)]
pub async fn get_all_field_values(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
) -> Result<Json<Vec<CustomFieldValue>>, StatusCode> {
    repo::custom_fields::get_all_field_values_for_item(state.pool(), item_id)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, item_id = %item_id, "Failed to get custom field values");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}

/// GET /api/items/:item_id/custom-fields/:field_id - Get a specific custom field value
#[instrument(skip(state))]
#[utoipa::path(
    get,
    path = "/api/items/{item_id}/custom-fields/{field_id}",
    tag = "custom-fields",
    params(
        ("item_id" = Uuid, Path, description = "Item ID"),
        ("field_id" = Uuid, Path, description = "Custom field ID"),
    ),
    responses(
        (status = 200, description = "The field value", body = tack_core::models::CustomFieldValue),
        (status = 404, description = "Value not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn get_field_value(
    State(state): State<AppState>,
    Path((item_id, field_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<CustomFieldValue>, StatusCode> {
    repo::custom_fields::get_field_value(state.pool(), item_id, field_id)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, item_id = %item_id, field_id = %field_id, "Custom field value not found");
            StatusCode::NOT_FOUND
        })
}

/// DELETE /api/items/:item_id/custom-fields/:field_id - Delete a custom field value
#[instrument(skip(state))]
#[utoipa::path(
    delete,
    path = "/api/items/{item_id}/custom-fields/{field_id}",
    tag = "custom-fields",
    params(
        ("item_id" = Uuid, Path, description = "Item ID"),
        ("field_id" = Uuid, Path, description = "Custom field ID"),
    ),
    responses(
        (status = 204, description = "Value deleted"),
    ),
)]
pub async fn delete_field_value(
    State(state): State<AppState>,
    Path((item_id, field_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, StatusCode> {
    repo::custom_fields::delete_field_value(state.pool(), item_id, field_id)
        .await
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| {
            tracing::error!(error = %e, item_id = %item_id, field_id = %field_id, "Failed to delete custom field value");
            StatusCode::INTERNAL_SERVER_ERROR
        })
}
