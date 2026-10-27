use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use tracing::error;

use tack_core::CoreError;
use tack_db::StoreError;
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

    /// The `Host` header does not name this server (`misdirected_host`).
    #[error("{0}")]
    MisdirectedHost(String),

    /// A state-changing request that a page on another site sent
    /// (`cross_site_request`).
    #[error("{0}")]
    CrossSiteRequest(String),

    /// A request body whose `Content-Type` this route does not accept
    /// (`unsupported_media_type`).
    #[error("{0}")]
    UnsupportedMediaType(String),

    /// A restore's archive or manifest cannot be read (`unreadable_archive`): the stored bytes
    /// are not what a backup is, so retrying the same restore cannot succeed.
    #[error("{0}")]
    UnreadableArchive(String),

    /// One section of a project file could not be written. `kept` says
    /// whether what the file wrote before it stays (an existing project) or
    /// went with the project (a new one).
    #[error("the {section} section could not be written")]
    SectionFailed {
        section: &'static str,
        kept: bool,
        #[source]
        source: StoreError,
    },

    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),

    #[error("{0}")]
    Store(#[from] StoreError),

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
            ApiError::MisdirectedHost(msg) => (StatusCode::MISDIRECTED_REQUEST, msg.clone()),
            ApiError::CrossSiteRequest(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            ApiError::UnsupportedMediaType(msg) => {
                (StatusCode::UNSUPPORTED_MEDIA_TYPE, msg.clone())
            }
            ApiError::UnreadableArchive(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
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
            ApiError::SectionFailed {
                section,
                kept,
                source,
            } => {
                error!(error = %source, section, "A project file section could not be written");
                let message = if *kept {
                    format!(
                        "The {section} section could not be written. What the file wrote before \
                         it stays, so importing it again finishes the job."
                    )
                } else {
                    format!("The {section} section could not be written, so nothing was created.")
                };
                (StatusCode::INTERNAL_SERVER_ERROR, message)
            }
            ApiError::Store(err) => match err {
                StoreError::NotFound { .. } => (StatusCode::NOT_FOUND, err.to_string()),
                StoreError::Conflict(_) => (StatusCode::CONFLICT, err.to_string()),
                StoreError::Sql(_) | StoreError::Corrupt { .. } => {
                    log_store_failure(err);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Internal server error".into(),
                    )
                }
            },
            ApiError::Internal(err) => {
                error!(error = %err, "Internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".into(),
                )
            }
        };

        let code = match &self {
            ApiError::MisdirectedHost(_) => Some("misdirected_host"),
            ApiError::CrossSiteRequest(_) => Some("cross_site_request"),
            ApiError::UnsupportedMediaType(_) => Some("unsupported_media_type"),
            ApiError::UnreadableArchive(_) => Some("unreadable_archive"),
            _ => None,
        };
        let mut error_body = json!({
            "status": status.as_u16(),
            "message": message,
        });
        if let Some(code) = code {
            error_body["code"] = json!(code);
        }

        let body = json!({ "error": error_body });

        (status, axum::Json(body)).into_response()
    }
}

/// Logs what a store call failed with when the client is told only "Internal server error": the
/// database's own error, or the table and column of a stored value that could not be decoded with
/// the reason it failed. Never the value.
pub(crate) fn log_store_failure(failure: &StoreError) {
    match failure {
        StoreError::Corrupt {
            table,
            column,
            source,
        } => error!(table, column, error = %source, "A stored value could not be decoded"),
        StoreError::Sql(failure) => error!(error = %failure, "Database error"),
        StoreError::NotFound { .. } | StoreError::Conflict(_) => {}
    }
}

/// The status a handler without an error body answers a failed store call with: the one the
/// store raised for a row that is not there or a write that conflicts, and a logged 500 for
/// everything else, never a 404 guessed from a failure of another kind.
pub(crate) fn status_of_store_failure(failure: &StoreError) -> StatusCode {
    match failure {
        StoreError::NotFound { .. } => StatusCode::NOT_FOUND,
        StoreError::Conflict(_) => StatusCode::CONFLICT,
        StoreError::Sql(_) | StoreError::Corrupt { .. } => {
            log_store_failure(failure);
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// Logs a stored document that did not fit its type: where serde stopped and what kind of
/// failure it was, never its message, which quotes the offending value.
pub(crate) fn log_unreadable_document(what: &str, failure: &serde_json::Error) {
    error!(
        kind = ?failure.classify(),
        line = failure.line(),
        column = failure.column(),
        "{what}"
    );
}

impl From<tack_app::AppError> for ApiError {
    fn from(error: tack_app::AppError) -> Self {
        use tack_app::AppError;
        match error {
            error @ AppError::NotFound => ApiError::NotFound(error.to_string()),
            AppError::Conflict(message) => ApiError::Conflict(message),
            error @ AppError::InvalidTransition { .. } => ApiError::BadRequest(error.to_string()),
            error @ AppError::StaleLease { .. } => ApiError::Conflict(error.to_string()),
            AppError::Store(error) => ApiError::Store(error),
            AppError::Core(error) => ApiError::Core(error),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn message_of(error: ApiError) -> (StatusCode, String) {
        let response = error.into_response();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1 << 16).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        (
            status,
            body["error"]["message"].as_str().unwrap().to_string(),
        )
    }

    #[tokio::test]
    async fn a_failed_section_names_itself_and_says_what_stayed() {
        let failed = |kept| ApiError::SectionFailed {
            section: "roles",
            kept,
            source: StoreError::not_found("role"),
        };
        let (status, kept) = message_of(failed(true)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            kept,
            "The roles section could not be written. What the file wrote before it stays, so \
             importing it again finishes the job."
        );
        let (_, gone) = message_of(failed(false)).await;
        assert_eq!(
            gone,
            "The roles section could not be written, so nothing was created."
        );
    }

    #[tokio::test]
    async fn a_store_error_maps_to_the_status_the_store_raised() {
        let (status, message) = message_of(StoreError::not_found("item").into()).await;
        assert_eq!(
            (status, message.as_str()),
            (StatusCode::NOT_FOUND, "item not found")
        );
        let (status, message) =
            message_of(StoreError::Conflict("already_reviewed".into()).into()).await;
        assert_eq!(
            (status, message.as_str()),
            (StatusCode::CONFLICT, "conflict: already_reviewed")
        );
    }

    #[tokio::test]
    async fn a_corrupt_stored_value_is_an_internal_error_that_does_not_name_the_column() {
        let corrupt = StoreError::corrupt("items", "tags", "expected value at line 1 column 1");
        let (status, message) = message_of(corrupt.into()).await;
        assert_eq!(
            (status, message.as_str()),
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
        );
    }

    #[tokio::test]
    async fn an_app_error_maps_to_the_status_its_kind_names() {
        use tack_app::AppError;
        let status_of = |error: AppError| async move { message_of(error.into()).await.0 };
        assert_eq!(status_of(AppError::NotFound).await, StatusCode::NOT_FOUND);
        assert_eq!(
            status_of(AppError::Conflict("x".into())).await,
            StatusCode::CONFLICT
        );
        assert_eq!(
            status_of(AppError::InvalidTransition {
                from: "a".into(),
                to: "b".into()
            })
            .await,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            status_of(AppError::StaleLease {
                attempt_id: "att".into(),
                fence: 1
            })
            .await,
            StatusCode::CONFLICT
        );
    }
}
