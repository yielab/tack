//! Runner-credential authentication for `/api/runner/v1`.
//!
//! This module is the single seam every runner-protocol write passes through
//! before touching any B2 repository method. It resolves a `runner_bearer_credential`
//! (an `Authorization: Bearer <credential>` header, per `protocol.json`) to
//! exactly one active, non-revoked, non-expired runner identity.
//!
//! Deliberately narrow: this reads only the `Authorization` header. It never
//! reads `x-tack-principal` (the operator-auth header C1/C5 use), which is
//! what makes operator auth structurally unable to substitute for runner auth
//! and vice versa — the two auth paths do not share a header, a table
//! lookup, or an error path.
//!
//! Credentials are hashed with SHA-256 before every comparison; the raw
//! bearer value is never logged, never echoed in an error body, and never
//! passed to `tracing`. Only the resolved `runner_id` is ever logged.

use axum::Json;
use axum::http::{HeaderMap, StatusCode, header};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use tack_db::Repository;
use tack_orch::execution::{ProtocolErrorEnvelope, StableErrorCode};

/// A runner identity resolved from an authenticated bearer credential. This
/// type can only be constructed by [`authenticate`]; handlers must never
/// build one from an unauthenticated request-body `runner_id` field.
///
/// `credential_hash` is the SHA-256 hash of the bearer credential that was
/// actually authenticated (never the raw credential) — it lets a handler
/// perform a compare-and-set against `agent_runners.credential_hash` (see
/// `rotate_runner_credential` in `crates/tack-db/src/repo/execution.rs`)
/// keyed on the identity it just verified, rather than re-parsing the
/// `Authorization` header a second time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerPrincipal {
    pub runner_id: String,
    pub credential_hash: String,
}

pub type ProtocolErrorResponse = (StatusCode, Json<Value>);
pub type ProtocolResult<T> = Result<T, ProtocolErrorResponse>;

/// C5 replaces this non-secret sentinel with the request correlation ID once
/// it mounts these card-local routes in the global API router (mirrors C1's
/// `OPERATOR_REQUEST_ID` convention for its own card-local router).
pub const RUNNER_REQUEST_ID: &str = "req_runner";

/// Builds the stable v1 error envelope via B1's `ProtocolErrorEnvelope::new`,
/// which derives `retryable` from `code` (`StableErrorCode::retryable`) so it
/// can never drift from `docs/contracts/runner-v1/errors/*.json` — B1 is the
/// single authority for that classification (see the "Retryability-authority
/// amendment" in `docs/agent-handoffs/part-iii/III-B1.md` and rule III.1.6:
/// hand-written feature DTOs are not another authority). `details` must
/// follow the per-code shape documented in `docs/contracts/runner-v1/README.md`.
pub fn protocol_error(
    status: StatusCode,
    code: StableErrorCode,
    message: &str,
    details: Value,
) -> ProtocolErrorResponse {
    let envelope = ProtocolErrorEnvelope::new(code, message, RUNNER_REQUEST_ID, details);
    (
        status,
        Json(serde_json::to_value(envelope).expect("envelope serializes")),
    )
}

pub fn invalid_request(field: &str, message: &str) -> ProtocolErrorResponse {
    protocol_error(
        StatusCode::BAD_REQUEST,
        StableErrorCode::InvalidRequest,
        message,
        json!({"field": field}),
    )
}

pub fn payload_too_large(limit: &str, maximum: u64) -> ProtocolErrorResponse {
    protocol_error(
        StatusCode::PAYLOAD_TOO_LARGE,
        StableErrorCode::PayloadTooLarge,
        "The payload exceeds a protocol limit",
        json!({"limit": limit, "maximum": maximum}),
    )
}

pub fn stale_lease(attempt_id: &str) -> ProtocolErrorResponse {
    protocol_error(
        StatusCode::CONFLICT,
        StableErrorCode::StaleLease,
        "The lease is no longer valid",
        json!({"attempt_id": attempt_id}),
    )
}

pub fn forbidden(message: &str) -> ProtocolErrorResponse {
    protocol_error(
        StatusCode::FORBIDDEN,
        StableErrorCode::Forbidden,
        message,
        json!({}),
    )
}

/// SHA-256 hex digest. Used both for enrollment-token and runner-credential
/// comparisons; only ever called on a value that is about to be discarded or
/// is already a fixture-declared `example_*` placeholder in tests.
pub fn credential_hash(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

/// The exact message `authenticate` emits when no row's `credential_hash`
/// matches the presented bearer at all — as opposed to matching a row that
/// is merely revoked, inactive, or expired. Shared with
/// [`is_credential_not_recognized`] so the two call sites (the emission
/// here and the classification there) cannot drift apart; see that
/// function's doc comment for why this distinction exists (III-H4).
const CREDENTIAL_NOT_RECOGNIZED_MESSAGE: &str = "The runner credential is not recognized";

/// Authenticates `Authorization: Bearer <runner_credential>` against the
/// stored, hashed runner credential. Returns `unauthorized` for a missing,
/// unrecognized, inactive, or expired credential, and the stable
/// `runner_revoked` code (matching `errors/runner-revoked.json`) once a
/// credential resolves to a revoked runner — distinct from an unrecognized
/// one so a runner can tell "you were removed" from "you were never valid".
pub async fn authenticate(
    repo: &Repository,
    headers: &HeaderMap,
    now: DateTime<Utc>,
) -> ProtocolResult<RunnerPrincipal> {
    let provided = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|token| !token.is_empty());
    let Some(token) = provided else {
        return Err(protocol_error(
            StatusCode::UNAUTHORIZED,
            StableErrorCode::Unauthorized,
            "A runner bearer credential is required",
            json!({}),
        ));
    };
    let hash = credential_hash(token);
    let row = sqlx::query(
        "SELECT id, state, revoked_at, credential_expires_at FROM agent_runners WHERE credential_hash = ? LIMIT 1",
    )
    .bind(&hash)
    .fetch_optional(repo.pool())
    .await
    .map_err(|_| {
        protocol_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not verify runner credential",
            json!({}),
        )
    })?;
    let Some(row) = row else {
        return Err(protocol_error(
            StatusCode::UNAUTHORIZED,
            StableErrorCode::Unauthorized,
            CREDENTIAL_NOT_RECOGNIZED_MESSAGE,
            json!({}),
        ));
    };
    let runner_id: String = row.get("id");
    let state: String = row.get("state");
    let revoked_at: Option<String> = row.get("revoked_at");
    if revoked_at.is_some() || state == "revoked" {
        return Err(protocol_error(
            StatusCode::FORBIDDEN,
            StableErrorCode::RunnerRevoked,
            "The runner credential has been revoked",
            json!({"runner_id": runner_id}),
        ));
    }
    if state != "active" {
        return Err(protocol_error(
            StatusCode::UNAUTHORIZED,
            StableErrorCode::Unauthorized,
            "The runner is not active",
            json!({}),
        ));
    }
    let expires_at: Option<String> = row.get("credential_expires_at");
    if let Some(expires_at) = expires_at {
        let expires_at = DateTime::parse_from_rfc3339(&expires_at)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|_| {
                protocol_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    StableErrorCode::InternalError,
                    "Stored credential expiry is corrupt",
                    json!({}),
                )
            })?;
        if expires_at <= now {
            return Err(protocol_error(
                StatusCode::UNAUTHORIZED,
                StableErrorCode::Unauthorized,
                "The runner credential has expired",
                json!({}),
            ));
        }
    }
    tracing::debug!(runner_id = %runner_id, "runner protocol v1 request authenticated");
    Ok(RunnerPrincipal {
        runner_id,
        credential_hash: hash,
    })
}

/// True exactly when an `authenticate` error is the "no row's `credential_hash`
/// matched this bearer at all" case, as opposed to a revoked, inactive,
/// expired, or missing-header credential.
///
/// This distinction exists for one caller: `/refresh`'s rotation-race
/// handling (III-H4, see `runner_protocol.rs::reclassify_refresh_auth_error`).
/// SQLite has no history of a credential once it is rotated away — the
/// `UPDATE ... SET credential_hash=?` in `rotate_runner_credential` (B2)
/// overwrites the old hash in place — so a `/refresh` request that loses a
/// concurrent rotation race and a `/refresh` request carrying a genuinely
/// bogus credential are indistinguishable by a second query: both hit this
/// exact "not recognized" branch, for the same reason (no row currently
/// carries the presented hash). `/refresh`'s rotating branch already accepts
/// that same ambiguity one step later, when the CAS itself loses a race
/// (`CredentialRotationResult::HashMismatch`, mapped to `conflict` rather
/// than disambiguated further — see that match arm's own comment). This
/// function lets `/refresh` apply the identical policy to the *earlier* of
/// the two indistinguishable failure points, so a losing rotation gets the
/// same retryable answer regardless of which point it failed at, instead of
/// a policy that depends on load-sensitive scheduling of two DB statements.
///
/// Deliberately does **not** change `authenticate`'s own return value or
/// behavior for its other 16 call sites in `runner_protocol.rs` — this only
/// classifies an error already produced, for one caller to decide whether to
/// remap it.
pub fn is_credential_not_recognized(error_body: &Value) -> bool {
    error_body
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        == Some(CREDENTIAL_NOT_RECOGNIZED_MESSAGE)
}

/// Cross-checks a request body's `runner_id` field against the authenticated
/// principal. The authenticated bearer credential is always the identity of
/// record; a body `runner_id` that disagrees with it is a confused-deputy
/// attempt, not a routing detail, so it is rejected as `forbidden` rather
/// than silently overridden or treated as `invalid_request`.
pub fn require_matching_runner(body: &Value, principal: &RunnerPrincipal) -> ProtocolResult<()> {
    match body.get("runner_id").and_then(Value::as_str) {
        Some(id) if id == principal.runner_id => Ok(()),
        Some(_) => Err(forbidden(
            "The authenticated runner may not act as a different runner_id",
        )),
        None => Err(invalid_request("runner_id", "runner_id is required")),
    }
}

/// Cross-checks a request body's `attempt_id` field (when present) against
/// the path's `attempt_id`. Absent is allowed — the path is authoritative —
/// but a present, differing value indicates a client bug worth surfacing.
pub fn require_matching_attempt(body: &Value, attempt_id: &str) -> ProtocolResult<()> {
    match body.get("attempt_id").and_then(Value::as_str) {
        Some(id) if id == attempt_id => Ok(()),
        Some(_) => Err(invalid_request(
            "attempt_id",
            "attempt_id in the body must match the path",
        )),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_hash_is_sha256_hex_and_never_the_raw_value() {
        let raw = "example_runner_credential_returned_once";
        let hashed = credential_hash(raw);
        assert_ne!(hashed, raw);
        assert_eq!(hashed.len(), 64);
        assert!(hashed.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn require_matching_runner_rejects_impersonation_and_missing_field() {
        let principal = RunnerPrincipal {
            runner_id: "runr_self".into(),
            credential_hash: "irrelevant-for-this-test".into(),
        };
        assert!(require_matching_runner(&json!({"runner_id": "runr_self"}), &principal).is_ok());
        let (status, body) =
            require_matching_runner(&json!({"runner_id": "runr_other"}), &principal).unwrap_err();
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body.0["error"]["code"], "forbidden");
        let (status, body) = require_matching_runner(&json!({}), &principal).unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body.0["error"]["code"], "invalid_request");
    }

    #[test]
    fn is_credential_not_recognized_matches_only_the_not_recognized_message() {
        let (_, not_recognized) = protocol_error(
            StatusCode::UNAUTHORIZED,
            StableErrorCode::Unauthorized,
            CREDENTIAL_NOT_RECOGNIZED_MESSAGE,
            json!({}),
        );
        assert!(is_credential_not_recognized(&not_recognized.0));

        let (_, expired) = protocol_error(
            StatusCode::UNAUTHORIZED,
            StableErrorCode::Unauthorized,
            "The runner credential has expired",
            json!({}),
        );
        assert!(!is_credential_not_recognized(&expired.0));

        let (_, revoked) = protocol_error(
            StatusCode::FORBIDDEN,
            StableErrorCode::RunnerRevoked,
            "The runner credential has been revoked",
            json!({}),
        );
        assert!(!is_credential_not_recognized(&revoked.0));
    }

    #[test]
    fn require_matching_attempt_allows_absent_but_rejects_mismatch() {
        assert!(require_matching_attempt(&json!({}), "att_1").is_ok());
        assert!(require_matching_attempt(&json!({"attempt_id": "att_1"}), "att_1").is_ok());
        let (status, body) =
            require_matching_attempt(&json!({"attempt_id": "att_2"}), "att_1").unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body.0["error"]["code"], "invalid_request");
    }
}
