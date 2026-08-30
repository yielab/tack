//! Operator-facing verified-artifact content download.
//!
//! This handler is **not** part of `runner_protocol`'s own `routes()` — that
//! router is deliberately runner-credential-only and sits structurally
//! outside `require_token` (see `router.rs#runner_protocol_routes`'s own doc
//! comment). An operator download must instead live under the operator
//! `/api` surface, which means the actual mounting touches `router.rs`/
//! `handlers/mod.rs` — both off-limits to this card. So, per this card's own
//! brief ("If a route must be mounted ... write the handler in your own
//! module and record the wiring request in your handoff"), this module was
//! written self-contained, and III-F2 proved it only via [`routes`]'s own
//! locally-constructed router. The recorded wiring request (path, auth
//! expectations) is in `docs/agent-handoffs/part-iii/III-F2.md`.
//!
//! **Amendment (III-F6/F6a):** that request has since been granted — the
//! Wave 5 integrator mounted [`routes`] under the real operator surface in
//! `router.rs#operator_execution_routes` as
//! `GET /api/executions/{request_id}/attempts/{attempt_number}/artifacts/{artifact_id}/content`,
//! sharing the `TACK_STORAGE_DIR`-derived artifact root with
//! `runner_protocol_routes`. `crates/tack-api/tests/f6a_artifact_wiring_test.rs`
//! proves the mount through the real `build_router` and was verified
//! load-bearing by unmounting it and watching the test 404.
//!
//! Nested under `runner_protocol/` (a submodule of an already-registered
//! file) purely so it is reachable without touching `handlers/mod.rs` — see
//! `runner_protocol.rs`'s own `mod artifact_download;` comment. It is not
//! part of the runner protocol itself; `principal()` below reads
//! `x-tack-principal`, the *operator* auth header (mirroring
//! `executions.rs`'s own `principal()`), never a runner bearer credential.
//!
//! Streams the file back chunk-by-chunk (`futures::stream::unfold` over a
//! `tokio::fs::File`, no whole-file read into memory) — the read-side half of
//! this card's "streaming content" charter, complementing
//! `artifact_storage.rs`'s write-side streaming.
//!
//! Module-level `dead_code` allow: every item here is reachable and
//! exercised by this card's own `f2_artifact_events_test.rs` (which loads
//! this file the same way), but `crates/tack-api/tests/c2_handlers_test.rs`
//! — a pre-existing, unrelated test binary this card does not own — also
//! pulls in `runner_protocol.rs` via `#[path]` for its own auth
//! non-substitution test, and never calls into this module. Dead-code
//! analysis is per compiled binary, so *that* binary alone would otherwise
//! flag every item here as unused. This mirrors the exact, already-documented
//! precedent in `runner_protocol.rs` itself (`RunnerV1ErrorEnvelope` in
//! `executions.rs`, and the individually-annotated `Limits` fields) — an
//! honest reflection of "not yet wired into production" (see this module's
//! own recorded wiring request), not a mask over a genuine bug.
#![allow(dead_code)]

use std::sync::Arc;

use axum::{
    Router,
    body::{Body, Bytes},
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use futures::Stream;
use serde_json::{Value, json};
use tack_db::Repository;
use tack_orch::execution::{ProtocolErrorEnvelope, StableErrorCode};
use tokio::io::AsyncReadExt;

use super::artifact_storage::ArtifactStorage;

#[derive(Clone)]
pub struct ArtifactDownloadState {
    pub repo: Repository,
    pub artifact_storage: Arc<ArtifactStorage>,
}

/// Placeholder request-correlation id, matching `executions.rs`'s own
/// `OPERATOR_REQUEST_ID` convention — the real integrator replaces this with
/// the request's actual correlation id once mounted for real.
const REQUEST_ID: &str = "req_operator_artifact_download";

fn error(status: StatusCode, code: StableErrorCode, message: &str, details: Value) -> Response {
    let envelope = ProtocolErrorEnvelope::new(code, message, REQUEST_ID, details);
    (
        status,
        axum::Json(serde_json::to_value(envelope).expect("envelope serializes")),
    )
        .into_response()
}

/// Mirrors `executions.rs#principal` exactly: the real router's
/// `inject_operator_principal` middleware overwrites this header from
/// server-verified config, never from an untrusted client value (see
/// CLAUDE.md's own note on `x-tack-principal`). This handler only ever reads
/// it, never trusts a value it did not put there itself.
fn principal(headers: &HeaderMap) -> Result<String, Box<Response>> {
    headers
        .get("x-tack-principal")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            Box::new(error(
                StatusCode::UNAUTHORIZED,
                StableErrorCode::Unauthorized,
                "An authenticated operator principal is required",
                json!({}),
            ))
        })
}

/// Operator-facing artifact-download router. Mounted in production by
/// `router.rs#operator_execution_routes`, inside the `require_token`
/// operator surface — never under `runner_protocol_routes`. III-F2 authored
/// this constructor before the mount existed; see this module's doc comment.
pub fn routes(state: ArtifactDownloadState) -> Router {
    Router::new()
        .route(
            "/executions/{request_id}/attempts/{attempt_number}/artifacts/{artifact_id}/content",
            get(download_artifact_content),
        )
        .with_state(state)
}

pub async fn download_artifact_content(
    State(state): State<ArtifactDownloadState>,
    headers: HeaderMap,
    Path((request_id, attempt_number, artifact_id)): Path<(String, i64, String)>,
) -> Response {
    if let Err(response) = principal(&headers) {
        return *response;
    }
    let row = match state
        .repo
        .get_execution_artifact_by_attempt_number(&request_id, attempt_number, &artifact_id)
        .await
    {
        Ok(row) => row,
        Err(_) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not look up artifact",
                json!({}),
            );
        }
    };
    let Some(row) = row else {
        return error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Artifact not found",
            json!({"artifact_id": artifact_id}),
        );
    };
    let Some(content_reference) = row.content_reference else {
        // Honest, distinct state (rule 7): the manifest genuinely exists —
        // this is not a 404 — but no verified content has landed yet. A
        // caller must not read this as "gone" or silently get zero bytes.
        return error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "Artifact content has not been verified yet",
            json!({"artifact_id": artifact_id}),
        );
    };
    let file = match state
        .artifact_storage
        .open_for_read(&content_reference)
        .await
    {
        Ok(file) => file,
        Err(_) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not open artifact content",
                json!({}),
            );
        }
    };

    let body = Body::from_stream(chunked_read_stream(file));
    let mut response = Response::new(body);
    let content_type = row
        .media_type
        .as_deref()
        .and_then(|value| HeaderValue::from_str(value).ok())
        .unwrap_or_else(|| HeaderValue::from_static("application/octet-stream"));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, content_type);
    if let Ok(length) = HeaderValue::from_str(&row.size_bytes.to_string()) {
        response
            .headers_mut()
            .insert(header::CONTENT_LENGTH, length);
    }
    // `HeaderValue::from_str` rejects control characters (including CR/LF),
    // so a runner-controlled `name` containing header-injection bytes simply
    // fails to construct and this falls back to a generic disposition
    // instead of ever emitting a malformed or injected header.
    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", row.name))
        .unwrap_or_else(|_| HeaderValue::from_static("attachment"));
    response
        .headers_mut()
        .insert(header::CONTENT_DISPOSITION, disposition);
    response
}

/// Chunk-by-chunk file read, never the whole file at once. `unfold`'s state
/// carries a `done` flag so a read error is yielded exactly once and then
/// terminates the stream, rather than retrying a broken file handle forever.
fn chunked_read_stream(file: tokio::fs::File) -> impl Stream<Item = Result<Bytes, std::io::Error>> {
    const CHUNK_BYTES: usize = 64 * 1024;
    futures::stream::unfold((file, false), |(mut file, done)| async move {
        if done {
            return None;
        }
        let mut buf = vec![0u8; CHUNK_BYTES];
        match file.read(&mut buf).await {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                Some((Ok(Bytes::from(buf)), (file, false)))
            }
            Err(error) => Some((Err(error), (file, true))),
        }
    })
}
