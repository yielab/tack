use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use tracing::error;

use tack_core::CoreError;
use tack_db::repo::dependencies::DependencyError;

/// Unified API error type that maps to HTTP status codes.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    BadRequest(String),

    #[error("{0}")]
    Forbidden(String),

    #[error("{0}")]
    Conflict(String),

    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("{0}")]
    Core(#[from] CoreError),

    #[error("{0}")]
    Dependency(#[from] DependencyError),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            ApiError::Core(err) => match err {
                CoreError::ItemNotFound(_)
                | CoreError::ProjectNotFound(_)
                | CoreError::SprintNotFound(_)
                | CoreError::RoleNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
                CoreError::InvalidTransition { .. }
                | CoreError::WipLimitExceeded { .. }
                | CoreError::DependencyCycle(_)
                | CoreError::DuplicateDependency { .. }
                | CoreError::InvalidVocabularyKey(_)
                | CoreError::EmptyWorkflow
                | CoreError::HasChildren(_, _)
                | CoreError::Validation(_)
                | CoreError::InvalidWorkflow(_) => (StatusCode::BAD_REQUEST, err.to_string()),
            },
            ApiError::Dependency(err) => match err {
                DependencyError::Core(_) => (StatusCode::BAD_REQUEST, err.to_string()),
                DependencyError::Db(_) => {
                    error!(error = %err, "Database error in dependency operation");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Internal server error".into(),
                    )
                }
            },
            ApiError::Database(err) => {
                error!(error = %err, "Database error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".into(),
                )
            }
            ApiError::Internal(err) => {
                error!(error = %err, "Internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".into(),
                )
            }
        };

        let body = json!({
            "error": {
                "status": status.as_u16(),
                "message": message,
            }
        });

        (status, axum::Json(body)).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
