//! The id every request carries.
//!
//! A client may send its own in `x-request-id`; the board keeps it when it is short and safe to
//! put in a log line, and makes one otherwise. Every error body built while the request is served
//! names it, and the response echoes it in the same header, so one failure can be followed from
//! the client's log to the board's.

use axum::extract::Request;
use axum::http::{HeaderMap, HeaderValue};
use axum::middleware::Next;
use axum::response::Response;
use tracing::Instrument;

/// The header a client puts a correlation id in, and the one the board answers with.
pub const REQUEST_ID_HEADER: &str = "x-request-id";

tokio::task_local! {
    /// The id of the request being served, set by [`with_request_id`] around every handler so an
    /// error body names the request it answers.
    static REQUEST_ID: String;
}

/// The id of the request being served, or a fresh one outside a served request (a unit test
/// calling a helper directly).
pub fn current_request_id() -> String {
    REQUEST_ID
        .try_with(String::clone)
        .unwrap_or_else(|_| new_request_id())
}

fn new_request_id() -> String {
    format!("req_{}", uuid::Uuid::new_v4().simple())
}

/// A client's own id is kept when it is short and made of characters that are safe in a log
/// line and a header; anything else is replaced by one the board makes.
fn accepted_request_id(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(REQUEST_ID_HEADER)?.to_str().ok()?;
    let plausible = !raw.is_empty()
        && raw.len() <= 64
        && raw
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':'));
    plausible.then(|| raw.to_owned())
}

/// Middleware: gives the request the id its client sent (or one of the board's own), logs it,
/// lets every error body built while serving it carry it, and echoes it in the response header.
/// A request that already has an id (the router and a sub-router both carry this layer) keeps
/// the outer one.
pub async fn with_request_id(request: Request, next: Next) -> Response {
    if REQUEST_ID.try_with(|_| ()).is_ok() {
        return next.run(request).await;
    }
    let request_id = accepted_request_id(request.headers()).unwrap_or_else(new_request_id);
    let span = tracing::info_span!(
        "request",
        request_id = %request_id,
        method = %request.method(),
        path = %request.uri().path(),
    );
    let mut response = REQUEST_ID
        .scope(request_id.clone(), next.run(request).instrument(span))
        .await;
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    response
}
