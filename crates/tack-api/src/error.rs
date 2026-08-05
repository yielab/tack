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
    Unprocessable(String),

    #[error("{0}")]
    Forbidden(String),

    #[error("{0}")]
    Conflict(String),

    /// A route/feature exists but is currently switched off (e.g.
    /// orchestration when `TACK_ORCH_ENABLE`/the `app_meta`-stored setting
    /// both say "disabled"). Distinct from a plain [`ApiError::Conflict`]
    /// because the response also carries a stable, machine-readable `code`
    /// — see `handlers::orch::require_orch_enabled` — so a caller can tell
    /// "this feature is off" from an ordinary write conflict
    /// programmatically, not just by parsing the human message.
    #[error("{message}")]
    FeatureDisabled { message: String, code: &'static str },

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
            ApiError::Unprocessable(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            ApiError::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            ApiError::FeatureDisabled { message, .. } => (StatusCode::CONFLICT, message.clone()),
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

        // Every variant gets the plain {status, message} envelope; only
        // `FeatureDisabled` adds the stable `code` a caller can match on
        // without parsing `message`. Kept as an addition (not a field every
        // error carries) so existing envelopes are unchanged.
        let mut error_body = json!({
            "status": status.as_u16(),
            "message": message,
        });
        if let ApiError::FeatureDisabled { code, .. } = &self {
            error_body["code"] = json!(code);
        }

        let body = json!({ "error": error_body });

        (status, axum::Json(body)).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
