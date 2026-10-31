//! The extractors every handler uses instead of axum's own.
//!
//! axum answers a body, a path or a query string it cannot read with a plain-text body and no
//! code. These wrappers read the same things the same way but refuse with [`ApiError`], so a
//! rejected request gets the standard error body: the `code` a client matches on, words for a
//! person, `details.field` when the failing field is known, and the request id.
//!
//! [`Json`] is also the response type, so a handler swaps one import and nothing else.

use axum::body::Bytes;
use axum::extract::path::ErrorKind;
use axum::extract::rejection::{BytesRejection, JsonRejection, PathRejection, QueryRejection};
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tack_protocol::execution::StableErrorCode;
use tracing::error;

use crate::error::ApiError;

/// A JSON request body, or a JSON response.
#[derive(Debug)]
pub struct Json<T>(pub T);

impl<T: Serialize> IntoResponse for Json<T> {
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

impl<T> std::ops::Deref for Json<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T, S> FromRequest<S> for Json<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, ApiError> {
        axum::Json::<T>::from_request(request, state)
            .await
            .map(|axum::Json(value)| Json(value))
            .map_err(json_rejection)
    }
}

/// The captured parameters of the route's path.
#[derive(Debug)]
pub struct Path<T>(pub T);

impl<T> std::ops::Deref for Path<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T, S> FromRequestParts<S> for Path<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiError> {
        axum::extract::Path::<T>::from_request_parts(parts, state)
            .await
            .map(|axum::extract::Path(value)| Path(value))
            .map_err(path_rejection)
    }
}

/// The query string, read as `T`.
#[derive(Debug)]
pub struct Query<T>(pub T);

impl<T> std::ops::Deref for Query<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T, S> FromRequestParts<S> for Query<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiError> {
        axum::extract::Query::<T>::from_request_parts(parts, state)
            .await
            .map(|axum::extract::Query(value)| Query(value))
            .map_err(query_rejection)
    }
}

/// The request body as bytes, for a handler that reads it itself.
#[derive(Debug)]
pub struct RawBody(pub Bytes);

impl<S: Send + Sync> FromRequest<S> for RawBody {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, ApiError> {
        Bytes::from_request(request, state)
            .await
            .map(RawBody)
            .map_err(bytes_rejection)
    }
}

/// A `multipart/form-data` body; one whose boundary is missing or invalid is refused like any
/// other unreadable request.
pub struct Multipart(pub axum::extract::Multipart);

impl<S: Send + Sync> FromRequest<S> for Multipart {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, ApiError> {
        axum::extract::Multipart::from_request(request, state)
            .await
            .map(Multipart)
            .map_err(|rejection| refusal(rejection.status(), rejection.body_text()))
    }
}

/// A request to switch to a WebSocket; a plain request to such a route is refused like any
/// other unreadable one.
pub struct Upgrade(pub WebSocketUpgrade);

impl<S: Send + Sync> FromRequestParts<S> for Upgrade {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiError> {
        WebSocketUpgrade::from_request_parts(parts, state)
            .await
            .map(Upgrade)
            .map_err(|rejection| refusal(rejection.status(), rejection.body_text()))
    }
}

fn json_rejection(rejection: JsonRejection) -> ApiError {
    let status = rejection.status();
    match rejection {
        JsonRejection::MissingJsonContentType(_) => ApiError::Coded {
            code: StableErrorCode::UnsupportedMediaType,
            message: "The request body must be sent as application/json.".into(),
            field: None,
        },
        JsonRejection::JsonSyntaxError(failure) => ApiError::Coded {
            code: StableErrorCode::InvalidRequest,
            message: format!(
                "The request body is not valid JSON ({}).",
                cause_of(&failure)
            ),
            field: None,
        },
        JsonRejection::JsonDataError(failure) => {
            let cause = without_position(cause_of(&failure));
            let field = field_named_in(&cause);
            ApiError::Coded {
                code: StableErrorCode::InvalidRequest,
                message: match &field {
                    Some(name) if cause.starts_with("missing field") => {
                        format!("The field `{name}` is required.")
                    }
                    _ => format!("The request body is not valid: {cause}."),
                },
                field,
            }
        }
        other => refusal(status, other.body_text()),
    }
}

fn path_rejection(rejection: PathRejection) -> ApiError {
    let status = rejection.status();
    match &rejection {
        PathRejection::FailedToDeserializePathParams(failure) => match failure.kind() {
            ErrorKind::ParseErrorAtKey { key, .. }
            | ErrorKind::DeserializeError { key, .. }
            | ErrorKind::InvalidUtf8InPathParam { key } => ApiError::field(
                StableErrorCode::InvalidRequest,
                key.clone(),
                format!("The `{key}` in the address is not valid."),
            ),
            ErrorKind::ParseErrorAtIndex { .. } | ErrorKind::ParseError { .. } => ApiError::Coded {
                code: StableErrorCode::InvalidRequest,
                message: "A value in the address is not valid.".into(),
                field: None,
            },
            _ => refusal(status, rejection.body_text()),
        },
        PathRejection::MissingPathParams(_) => refusal(status, rejection.body_text()),
        _ => refusal(status, rejection.body_text()),
    }
}

fn query_rejection(rejection: QueryRejection) -> ApiError {
    let cause = cause_of(&rejection);
    let field = field_named_in(&cause);
    ApiError::Coded {
        code: StableErrorCode::InvalidRequest,
        message: format!("The query string is not valid: {cause}."),
        field,
    }
}

fn bytes_rejection(rejection: BytesRejection) -> ApiError {
    refusal(rejection.status(), rejection.body_text())
}

/// The words for a rejection axum describes only by a status: the code of that status, and for
/// a 500 (a route declared wrongly, never a client's doing) the cause goes to the log only.
fn refusal(status: StatusCode, text: String) -> ApiError {
    let (code, message) = match status {
        StatusCode::PAYLOAD_TOO_LARGE => (
            StableErrorCode::PayloadTooLarge,
            "The request body is larger than this server accepts.".to_owned(),
        ),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => (
            StableErrorCode::UnsupportedMediaType,
            "The request body has a content type this route does not accept.".to_owned(),
        ),
        status if status.is_server_error() => {
            error!(%status, cause = %text, "A request could not be read because the route is declared wrongly");
            (
                StableErrorCode::InternalError,
                "Internal server error".to_owned(),
            )
        }
        _ => (StableErrorCode::InvalidRequest, format!("{text}.")),
    };
    ApiError::Coded {
        code,
        message,
        field: None,
    }
}

/// What serde (or the form decoder) said went wrong, without axum's fixed prefix.
fn cause_of(failure: &dyn std::error::Error) -> String {
    let mut text = failure.to_string();
    for prefix in [
        "Failed to deserialize the JSON body into the target type: ",
        "Failed to parse the request body as JSON: ",
        "Failed to deserialize query string: ",
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            text = rest.to_owned();
            break;
        }
    }
    text
}

/// A data error without where serde stopped ("... at line 1 column 28"): that means nothing to a
/// person filling a form, and the field is named separately.
fn without_position(cause: String) -> String {
    match cause.rsplit_once(" at line ") {
        Some((before, position))
            if position
                .split_once(" column ")
                .is_some_and(|(line, column)| {
                    line.bytes().all(|b| b.is_ascii_digit())
                        && column.bytes().all(|b| b.is_ascii_digit())
                }) =>
        {
            before.to_owned()
        }
        _ => cause,
    }
}

/// The field serde names in "missing field `x`" or "unknown field `x`", when it names one.
fn field_named_in(cause: &str) -> Option<String> {
    let rest = ["missing field `", "unknown field `"]
        .iter()
        .find_map(|marker| cause.split_once(marker).map(|(_, rest)| rest))?;
    let (name, _) = rest.split_once('`')?;
    Some(name.to_owned())
}
