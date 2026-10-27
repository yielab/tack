use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use tracing::error;

use tack_core::CoreError;
use tack_db::StoreError;
use tack_db::repo::dependencies::DependencyError;
use tack_protocol::execution::StableErrorCode;

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

    /// A failure of a use case that keeps the code it was raised with: an invalid lifecycle
    /// transition, a stale lease, a run that was refused.
    #[error("{message}")]
    Coded {
        code: StableErrorCode,
        message: String,
    },

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

impl ApiError {
    /// The code every answer of this error carries in its body; its
    /// [`StableErrorCode::status`] is the HTTP status. A variant that names a kind of failure
    /// has the code of that kind, a rule of the core has its own, and [`ApiError::Coded`] keeps
    /// the one a use case raised.
    pub fn code(&self) -> StableErrorCode {
        match self {
            ApiError::NotFound(_) => StableErrorCode::NotFound,
            ApiError::BadRequest(_) => StableErrorCode::InvalidRequest,
            ApiError::Unprocessable(_) => StableErrorCode::Unprocessable,
            ApiError::Forbidden(_) => StableErrorCode::Forbidden,
            ApiError::Conflict(_) => StableErrorCode::Conflict,
            ApiError::MisdirectedHost(_) => StableErrorCode::MisdirectedHost,
            ApiError::CrossSiteRequest(_) => StableErrorCode::CrossSiteRequest,
            ApiError::UnsupportedMediaType(_) => StableErrorCode::UnsupportedMediaType,
            ApiError::UnreadableArchive(_) => StableErrorCode::UnreadableArchive,
            ApiError::Coded { code, .. } => *code,
            ApiError::SectionFailed { .. } => StableErrorCode::SectionFailed,
            ApiError::Internal(_) => StableErrorCode::InternalError,
            ApiError::Store(failure) => failure.code(),
            ApiError::Core(rule) => rule.code(),
            ApiError::Dependency(DependencyError::Core(rule)) => rule.code(),
            ApiError::Dependency(DependencyError::Db(failure)) => failure.code(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let code = self.code();
        let message = match &self {
            ApiError::NotFound(msg)
            | ApiError::BadRequest(msg)
            | ApiError::Unprocessable(msg)
            | ApiError::Forbidden(msg)
            | ApiError::Conflict(msg)
            | ApiError::MisdirectedHost(msg)
            | ApiError::CrossSiteRequest(msg)
            | ApiError::UnsupportedMediaType(msg)
            | ApiError::UnreadableArchive(msg)
            | ApiError::Coded { message: msg, .. } => msg.clone(),
            ApiError::Core(err) => err.to_string(),
            ApiError::Dependency(err @ DependencyError::Core(_)) => err.to_string(),
            ApiError::Dependency(DependencyError::Db(
                failure @ (StoreError::NotFound { .. } | StoreError::Conflict(_)),
            )) => failure.to_string(),
            ApiError::Dependency(DependencyError::Db(failure)) => {
                log_store_failure(failure);
                "Internal server error".into()
            }
            ApiError::SectionFailed {
                section,
                kept,
                source,
            } => {
                error!(error = %source, section, "A project file section could not be written");
                if *kept {
                    format!(
                        "The {section} section could not be written. What the file wrote before \
                         it stays, so importing it again finishes the job."
                    )
                } else {
                    format!("The {section} section could not be written, so nothing was created.")
                }
            }
            ApiError::Store(err @ (StoreError::NotFound { .. } | StoreError::Conflict(_))) => {
                err.to_string()
            }
            ApiError::Store(err) => {
                log_store_failure(err);
                "Internal server error".into()
            }
            ApiError::Internal(err) => {
                error!(error = %err, "Internal error");
                "Internal server error".into()
            }
        };

        let status =
            StatusCode::from_u16(code.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = json!({ "error": {
            "status": status.as_u16(),
            "message": message,
            "code": code,
        } });

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
            AppError::Store(error) => ApiError::Store(error),
            AppError::Core(error) => ApiError::Core(error),
            error @ (AppError::InvalidTransition { .. }
            | AppError::StaleLease { .. }
            | AppError::Refused(_)) => ApiError::Coded {
                code: error.code(),
                message: error.to_string(),
            },
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn body_of(error: ApiError) -> (StatusCode, serde_json::Value) {
        let response = error.into_response();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1 << 16).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    async fn message_of(error: ApiError) -> (StatusCode, String) {
        let (status, body) = body_of(error).await;
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
            StatusCode::CONFLICT
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

    #[tokio::test]
    async fn every_error_body_has_a_code_and_the_code_decides_the_status() {
        use tack_app::AppError;
        use uuid::Uuid;
        let cases: Vec<(ApiError, &str, StatusCode)> = vec![
            (
                ApiError::NotFound("x".into()),
                "not_found",
                StatusCode::NOT_FOUND,
            ),
            (
                ApiError::BadRequest("x".into()),
                "invalid_request",
                StatusCode::BAD_REQUEST,
            ),
            (
                ApiError::Unprocessable("x".into()),
                "unprocessable",
                StatusCode::UNPROCESSABLE_ENTITY,
            ),
            (
                ApiError::Forbidden("x".into()),
                "forbidden",
                StatusCode::FORBIDDEN,
            ),
            (
                ApiError::Conflict("x".into()),
                "conflict",
                StatusCode::CONFLICT,
            ),
            (
                ApiError::MisdirectedHost("x".into()),
                "misdirected_host",
                StatusCode::MISDIRECTED_REQUEST,
            ),
            (
                ApiError::Internal(anyhow::anyhow!("x")),
                "internal_error",
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            (
                StoreError::not_found("item").into(),
                "not_found",
                StatusCode::NOT_FOUND,
            ),
            (
                CoreError::WipLimitExceeded {
                    column: "doing".into(),
                    limit: 1,
                    current: 1,
                }
                .into(),
                "wip_limit_exceeded",
                StatusCode::BAD_REQUEST,
            ),
            (
                CoreError::DependencyCycle(Uuid::nil()).into(),
                "dependency_cycle",
                StatusCode::BAD_REQUEST,
            ),
            (
                CoreError::InvalidTransition {
                    from: "a".into(),
                    to: "b".into(),
                }
                .into(),
                "invalid_status_transition",
                StatusCode::BAD_REQUEST,
            ),
            (
                AppError::StaleLease {
                    attempt_id: "att".into(),
                    fence: 1,
                }
                .into(),
                "stale_lease",
                StatusCode::CONFLICT,
            ),
        ];
        for (error, code, status) in cases {
            let label = error.to_string();
            let (got, body) = body_of(error).await;
            assert_eq!(
                (got, body["error"]["code"].as_str()),
                (status, Some(code)),
                "{label}"
            );
            assert_eq!(body["error"]["status"], status.as_u16(), "{label}");
        }
    }
}
