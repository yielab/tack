//! III-C2 card-local runner-protocol handler modules. C5 owns global-router
//! wiring under `/api/runner/v1` (see `docs/contracts/runner-v1/protocol.json`
//! `base_path`). This router is deliberately unregistered in `handlers.rs`,
//! mirroring C1's `executions`/`runner_admin` cards.
//!
//! Route layout is this card's own choice: only the additive
//! `recovery-observation` operation has an explicit path in `protocol.json`
//! (`/attempts/{attempt_id}/recovery-observation`, preserved here verbatim,
//! relative to `base_path`); every other canonical exchange in
//! `docs/contracts/runner-v1/` fixes payload shapes, not URLs, so C5 may
//! rename these when it mounts the router.
//!
//! `runner_auth` lives at `handlers/runner_protocol/runner_auth.rs` (a
//! submodule of this file) rather than a sibling `handlers/runner_auth.rs`.
//! That nested layout is what keeps it unregistered in `handlers.rs` today —
//! Rust resolves `mod runner_auth;` relative to *this file's own path*
//! (`runner_protocol.rs` → `runner_protocol/`), so the submodule still
//! resolves correctly when a test pulls this file in via
//! `#[path = "../src/handlers/runner_protocol.rs"]`, exactly as the card
//! brief requires.
//!
//! Every write below validates attempt id + runner identity (from the
//! authenticated bearer credential, never a request-body field) + fencing
//! token before calling a B2 repository method, and every payload is checked
//! against `docs/contracts/runner-v1/limits.json` before any repository call
//! — so a rejected request writes nothing (see `payload_too_large` case
//! notes below and the acceptance tests in `tests/c2_handlers_test.rs`).

use std::sync::Arc;

use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode, header},
    routing::{post, put},
};
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use tack_db::{
    Repository,
    repo::execution::{
        ArtifactContentCommitResult, AttemptTransitionInput, AttemptTransitionPhase,
        AttemptTransitionResult, CancellationObservation, CancellationObservationInput,
        ClaimedExecution, Completion, CompletionResult, CredentialRotationResult, EventApplyResult,
        EventBatch, ExecutionClock, HeartbeatBatchResult, HeartbeatLease, NewArtifact, NewDecision,
        NewEvent, RecoveryDisposition as DbRecoveryDisposition,
        RecoveryObservation as DbRecoveryObservation, RecoveryObservationInput,
        RecoveryObservationResult, RedeemEnrollmentResult, RequestSelection,
    },
};
use tack_orch::execution::{
    ActualExecution, AttemptId, EmbeddedCapabilitySnapshot, ProtocolVersion,
    RecoveryDisposition as TackRecoveryDisposition, RecoveryKey,
    RecoveryObservation as TackRecoveryObservation, RecoveryObservationRequest,
    RecoveryObservationResponse, StableErrorCode, Usage,
};
use tack_orch::scheduler::{SchedulingPolicy, choose_request_for_runner};
use uuid::Uuid;

// An explicit `#[path]` (rather than relying on the implicit `foo.rs` ->
// `foo/` submodule-directory convention) keeps this resolution correct
// regardless of how `runner_protocol.rs` itself was included — in
// particular, when a test pulls this file in via
// `#[path = "../src/handlers/runner_protocol.rs"] mod runner_protocol;`,
// rustc's mod-directory inference for an explicitly-`#[path]`-loaded file is
// the file's own directory, not `<stem>/`, so an implicit `mod runner_auth;`
// here resolves to the wrong (nonexistent) sibling path. This attribute
// pins it to the actual nested file the card brief specifies.
#[path = "runner_protocol/runner_auth.rs"]
pub mod runner_auth;

// III-F2: nested the same way `runner_auth` is (see that `mod` line's own
// comment) — declaring these as submodules of this already-registered file
// keeps them reachable without touching `handlers/mod.rs`, which this card
// must not edit. `artifact_storage` is the pure storage module (safe paths,
// streaming write/read); `retention` is the pure event/artifact sweep logic
// (F5 owns the recurring background task that calls it); `artifact_download`
// is the operator-facing content-download handler, deliberately never merged
// into this file's own `routes()` (that router is runner-credential-only —
// see its own doc comment) — it is proven only via a locally-constructed
// router in this card's own test file, with its real mounting recorded as a
// wiring request in `docs/agent-handoffs/part-iii/III-F2.md`.
#[path = "runner_protocol/artifact_download.rs"]
pub mod artifact_download;
#[path = "runner_protocol/artifact_storage.rs"]
pub mod artifact_storage;
#[path = "runner_protocol/retention.rs"]
pub mod retention;

use artifact_storage::{ArtifactContentError, ArtifactStorage};
use runner_auth::{invalid_request, payload_too_large, protocol_error, stale_lease};

type HandlerResult = runner_auth::ProtocolResult<Json<Value>>;

/// Default artifact-content storage root, relative to the process's working
/// directory — mirrors `config.rs#default_storage_dir`'s own `"./storage"`
/// default, one level deeper so artifact blobs never collide with attachment
/// files that already live directly under `TACK_STORAGE_DIR`. This is only
/// ever used until the integrator wires the real, operator-configured
/// `TACK_STORAGE_DIR` through — see the F2 handoff's recorded wiring
/// request. `RunnerProtocolState::new`'s two-argument signature is
/// deliberately left unchanged (production's one call site,
/// `router.rs#runner_protocol_routes`, is off-limits to this card) so this
/// default is additive, never a breaking change.
const DEFAULT_ARTIFACT_STORAGE_ROOT: &str = "./storage/execution-artifacts";

/// State for this card's local router. C5 constructs this from the shared API
/// state when it performs the one permitted global-router integration.
#[derive(Clone)]
pub struct RunnerProtocolState {
    pub repo: Repository,
    pub clock: Arc<dyn ExecutionClock>,
    pub artifact_storage: Arc<ArtifactStorage>,
}

impl RunnerProtocolState {
    pub fn new(repo: Repository, clock: Arc<dyn ExecutionClock>) -> Self {
        Self {
            repo,
            clock,
            artifact_storage: Arc::new(ArtifactStorage::new(DEFAULT_ARTIFACT_STORAGE_ROOT)),
        }
    }

    /// Additive builder so the integrator can point artifact content storage
    /// at the operator-configured `TACK_STORAGE_DIR` (see the F2 handoff's
    /// wiring request) without changing `new`'s call signature. Used by this
    /// card's own `f2_artifact_events_test.rs`; `#[allow(dead_code)]` because
    /// the pre-existing, unrelated `c2_handlers_test.rs` also loads this file
    /// via `#[path]` (for its own auth non-substitution test) without
    /// calling this — see `artifact_download.rs`'s module-level allow for
    /// the fuller precedent.
    #[allow(dead_code)]
    pub fn with_artifact_storage_root(mut self, root: impl Into<std::path::PathBuf>) -> Self {
        self.artifact_storage = Arc::new(ArtifactStorage::new(root.into()));
        self
    }
}

/// Generous headroom above `limits.json`'s `json_body_bytes_max` (1 MiB) so
/// axum's own default body-limit layer never preempts our stable
/// `payload_too_large` envelope; every handler still enforces the exact
/// frozen byte caps itself before touching the database. This is a ceiling,
/// not a floor — see [`effective_body_limit_bytes`] and [`routes`]'s doc
/// comment for how an operator-configured global limit can tighten it
/// further but never loosen it.
const RUNNER_ROUTER_BODY_LIMIT_BYTES: usize = 4 * 1024 * 1024;

/// The precedence rule an integrator-authorized cross-card fix (III-C2 /
/// III-C5, see both handoffs' "Amendment: runner-v1 body limit respects the
/// operator-configured global limit" sections) closed: `min(configured,
/// RUNNER_ROUTER_BODY_LIMIT_BYTES)`. An operator who tightens
/// `TACK_MAX_BODY_SIZE`/`max_body_size_bytes` below the 4 MiB protocol
/// ceiling gets a genuinely smaller runner-v1 surface; a loose or unset
/// global limit (`router.rs`'s own default is 2 MiB, already below the
/// ceiling) can never widen it past what the protocol allows. `min`, not a
/// straight pass-through of `configured`, is what keeps the ceiling a true
/// upper bound in both directions.
fn effective_body_limit_bytes(configured_max_body_size_bytes: usize) -> usize {
    configured_max_body_size_bytes.min(RUNNER_ROUTER_BODY_LIMIT_BYTES)
}

/// `configured_max_body_size_bytes` is the operator's global
/// `max_body_size_bytes` (`TACK_MAX_BODY_SIZE`, `router.rs`'s
/// `state.config.max_body_size_bytes`) — the same value the outer router
/// layers as its own `DefaultBodyLimit`. Without threading it through here,
/// this router's own more-specific `DefaultBodyLimit` layer always wins over
/// the outer one (axum applies whichever `DefaultBodyLimit` layer is closest
/// to the handler), so a tightened global limit could never actually shrink
/// the runner-v1 surface below the fixed 4 MiB ceiling. See
/// [`effective_body_limit_bytes`].
pub fn routes(state: RunnerProtocolState, configured_max_body_size_bytes: usize) -> Router {
    Router::new()
        .route("/enroll", post(enroll))
        .route("/refresh", post(refresh))
        .route("/claim", post(claim))
        .route("/heartbeat", post(heartbeat))
        .route("/attempts/{attempt_id}/accept", post(accept_attempt))
        .route("/attempts/{attempt_id}/start", post(start_attempt))
        .route("/attempts/{attempt_id}/events", post(submit_events))
        .route("/attempts/{attempt_id}/decisions", post(create_decision))
        .route(
            "/attempts/{attempt_id}/decisions/poll",
            post(poll_decisions),
        )
        .route("/attempts/{attempt_id}/artifacts", post(submit_artifacts))
        // III-F2: artifact content upload. A distinct, more-specific
        // `DefaultBodyLimit` on this one route (mirroring
        // `attachments.rs`'s own fixed 50 MB ceiling, independent of the
        // operator-configured global limit) — the router-wide layer below
        // (a 4 MiB ceiling meant for JSON control-plane bodies) would
        // otherwise reject any real artifact upload before this handler's
        // own streaming size/checksum checks ever ran. Per axum's
        // closest-to-the-handler precedence (see `effective_body_limit_bytes`'s
        // doc comment above), this per-route layer wins over the router-wide
        // one that follows.
        .route(
            "/attempts/{attempt_id}/artifacts/{artifact_id}/content",
            put(put_artifact_content).layer(DefaultBodyLimit::max(
                LIMITS.artifact_content_bytes_max as usize,
            )),
        )
        .route("/attempts/{attempt_id}/completion", post(submit_completion))
        .route(
            "/attempts/{attempt_id}/cancellation-observation",
            post(observe_cancellation_report),
        )
        .route(
            "/attempts/{attempt_id}/recovery-observation",
            post(observe_recovery),
        )
        .layer(DefaultBodyLimit::max(effective_body_limit_bytes(
            configured_max_body_size_bytes,
        )))
        .with_state(state)
}

// ---------------------------------------------------------------------
// docs/contracts/runner-v1/limits.json, mirrored exactly and checked
// against the frozen fixture in this module's own test below.
// ---------------------------------------------------------------------

// Nine of the fields below (each individually annotated) are read only by
// `limits_constants_match_frozen_fixture_exactly`, never by a request-time
// check in this file. That is not an oversight: every one of them bounds a
// concern this card's handlers do not implement a code path for —
// `environment_*` bounds the execution *request's* environment entries
// (validated at enqueue time, owned by B2/C1, not by any runner-protocol
// handler here); `decision_answer_bytes_max` bounds an operator's decision
// *answer*, and decision resolution has no endpoint in this wave (see the
// handoff's known limitations — it is scoped to a later wave, F5);
// `request_timeout_seconds_max` bounds the execution request's timeout
// policy (enqueue-time, not runner-protocol); `retention_event_days_default`/
// `retention_artifact_days_default` are background-sweep/storage defaults,
// not a per-request check; and `heartbeat_grace_seconds` /
// `event_batch_bytes_max` are each documented no-ops at their own call sites
// (`heartbeat_grace_seconds` has no fixture response field to echo — see
// `enrollment.response.json` — and `event_batch_bytes_max` is numerically
// identical to `json_body_bytes_max`, which `parse_body` already enforces on
// every request body — see the comment in `submit_events`).
//
// The struct still carries all 27 fields, unconditionally, so the fixture
// test proves the *entire* frozen `docs/contracts/runner-v1/limits.json`
// round-trips with no drift — not just the subset this card's handlers
// happen to enforce today. Each `#[allow(dead_code)]` below is therefore
// scoped to exactly the field it excuses, not the whole struct or module —
// unlike the blanket `#[allow(dead_code)]` C5 had to add to the
// `pub mod runner_protocol;` line in its own `handlers.rs` to keep clippy
// green once this module was first compiled as a real lib target. C5 may
// remove that allow now that the dead fields are excused precisely, here,
// in the file that owns them.
struct Limits {
    json_body_bytes_max: u64,
    metadata_bytes_max: u64,
    #[allow(dead_code)]
    environment_entries_max: u64,
    #[allow(dead_code)]
    environment_name_bytes_max: u64,
    #[allow(dead_code)]
    environment_value_bytes_max: u64,
    labels_max: u64,
    label_name_bytes_max: u64,
    label_value_bytes_max: u64,
    heartbeat_interval_seconds: i64,
    #[allow(dead_code)]
    heartbeat_grace_seconds: i64,
    lease_duration_seconds: i64,
    claim_wait_ms_max: u64,
    active_attempts_per_heartbeat_max: u64,
    event_batch_count_max: u64,
    event_payload_bytes_max: u64,
    #[allow(dead_code)]
    event_batch_bytes_max: u64,
    decision_prompt_bytes_max: u64,
    decision_options_max: u64,
    #[allow(dead_code)]
    decision_answer_bytes_max: u64,
    artifact_manifest_count_max: u64,
    artifact_metadata_bytes_max: u64,
    artifact_content_bytes_max: u64,
    artifact_attempt_total_bytes_max: u64,
    capabilities_bytes_max: u64,
    #[allow(dead_code)]
    request_timeout_seconds_max: u64,
    #[allow(dead_code)]
    retention_event_days_default: u64,
    #[allow(dead_code)]
    retention_artifact_days_default: u64,
}

const LIMITS: Limits = Limits {
    json_body_bytes_max: 1_048_576,
    metadata_bytes_max: 65_536,
    environment_entries_max: 64,
    environment_name_bytes_max: 128,
    environment_value_bytes_max: 4_096,
    labels_max: 64,
    label_name_bytes_max: 64,
    label_value_bytes_max: 256,
    heartbeat_interval_seconds: 15,
    heartbeat_grace_seconds: 45,
    lease_duration_seconds: 60,
    claim_wait_ms_max: 30_000,
    active_attempts_per_heartbeat_max: 64,
    event_batch_count_max: 100,
    event_payload_bytes_max: 65_536,
    event_batch_bytes_max: 1_048_576,
    decision_prompt_bytes_max: 32_768,
    decision_options_max: 32,
    decision_answer_bytes_max: 32_768,
    artifact_manifest_count_max: 100,
    artifact_metadata_bytes_max: 32_768,
    artifact_content_bytes_max: 52_428_800,
    artifact_attempt_total_bytes_max: 524_288_000,
    capabilities_bytes_max: 262_144,
    request_timeout_seconds_max: 86_400,
    retention_event_days_default: 30,
    retention_artifact_days_default: 30,
};

/// Not fixed by `limits.json`: a reasonable, explicitly-chosen window for the
/// manifest-accepted upload URL returned by `submit_artifacts`. III-F2 adds
/// the real content-upload endpoint (`put_artifact_content`, below) this URL
/// points at; the endpoint itself does not enforce this window (there is no
/// fixture-defined field to check it against), so it is advisory to the
/// runner today, not yet a hard server-side expiry.
const ARTIFACT_UPLOAD_WINDOW_SECONDS: i64 = 600;
/// Header a runner sets on `PUT .../artifacts/{artifact_id}/content` to carry
/// its fencing token — the request body *is* the artifact's raw bytes, so
/// (unlike every other runner-protocol write) the fencing token cannot travel
/// inside a JSON body. Not part of any frozen fixture: this whole route is
/// this card's own addition (`docs/contracts/runner-v1/` only fixes the
/// manifest exchange's payload shape, not this URL — see this file's own
/// top-of-file doc comment on route-layout freedom).
const ARTIFACT_FENCING_TOKEN_HEADER: &str = "x-tack-fencing-token";
/// Not fixed by any fixture: a reasonable no-work poll backoff hint.
const NO_WORK_RETRY_AFTER_MS: u64 = 5_000;
/// Not fixed by any fixture: how long an issued/rotated runner credential
/// remains valid before a runner must call `/refresh` with
/// `rotate_credential: true`.
const CREDENTIAL_LIFETIME_DAYS: i64 = 90;

// ---------------------------------------------------------------------
// Shared request parsing helpers.
// ---------------------------------------------------------------------

fn parse_body(bytes: &[u8]) -> runner_auth::ProtocolResult<Value> {
    if bytes.len() as u64 > LIMITS.json_body_bytes_max {
        return Err(payload_too_large(
            "json_body_bytes_max",
            LIMITS.json_body_bytes_max,
        ));
    }
    serde_json::from_slice(bytes)
        .map_err(|_| invalid_request("body", "Request body is not valid JSON"))
}

/// `unsupported_protocol` is checked against the raw JSON value (not a typed
/// field) so a version mismatch always produces the exact stable envelope
/// (`errors/unsupported-protocol.json`) instead of a generic deserialize
/// rejection, per `protocol.json`'s `compatibility.unknown_enum_values` and
/// `semantic_changes` rules.
fn check_protocol_version(value: &Value) -> runner_auth::ProtocolResult<()> {
    match value.get("protocol_version") {
        Some(Value::Number(n)) if n.as_u64() == Some(1) => Ok(()),
        Some(Value::Number(n)) => Err(protocol_error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::UnsupportedProtocol,
            "The runner protocol version is not supported",
            json!({"received": n.clone(), "minimum_supported": 1, "maximum_supported": 1}),
        )),
        _ => Err(invalid_request(
            "protocol_version",
            "protocol_version must be the integer 1",
        )),
    }
}

fn field<'a>(value: &'a Value, name: &str) -> runner_auth::ProtocolResult<&'a Value> {
    value
        .get(name)
        .ok_or_else(|| invalid_request(name, &format!("{name} is required")))
}

fn as_str<'a>(value: &'a Value, name: &str) -> runner_auth::ProtocolResult<&'a str> {
    field(value, name)?
        .as_str()
        .ok_or_else(|| invalid_request(name, &format!("{name} must be a string")))
}

fn as_i64(value: &Value, name: &str) -> runner_auth::ProtocolResult<i64> {
    field(value, name)?
        .as_i64()
        .ok_or_else(|| invalid_request(name, &format!("{name} must be an integer")))
}

fn as_u64(value: &Value, name: &str) -> runner_auth::ProtocolResult<u64> {
    field(value, name)?
        .as_u64()
        .ok_or_else(|| invalid_request(name, &format!("{name} must be a non-negative integer")))
}

fn as_array<'a>(value: &'a Value, name: &str) -> runner_auth::ProtocolResult<&'a Vec<Value>> {
    field(value, name)?
        .as_array()
        .ok_or_else(|| invalid_request(name, &format!("{name} must be an array")))
}

fn as_datetime(value: &Value, name: &str) -> runner_auth::ProtocolResult<DateTime<Utc>> {
    let raw = as_str(value, name)?;
    DateTime::parse_from_rfc3339(raw)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|_| invalid_request(name, &format!("{name} must be an RFC3339 timestamp")))
}

/// Re-formats a client-supplied RFC3339 timestamp through chrono's own
/// `Utc` `to_rfc3339()` before it is ever used in a TEXT comparison against
/// stored timestamps. Every stored timestamp in this schema is written via
/// that same normalization; comparing an un-normalized `Z`-suffixed input
/// against a stored `+00:00`-suffixed value would sort incorrectly even
/// though both denote the same instant.
fn normalize_datetime(value: &Value, name: &str) -> runner_auth::ProtocolResult<String> {
    Ok(as_datetime(value, name)?.to_rfc3339())
}

fn json_byte_len(value: &Value) -> u64 {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len() as u64)
        .unwrap_or(u64::MAX)
}

/// III-F2: a deliberately permissive `type/subtype` shape check — this is
/// not a MIME registry validator (registered types/suffixes/parameters are
/// out of scope), just enough to reject garbage (empty, no slash, control
/// characters, absurd length) while accepting every real value this repo's
/// own fixtures and tests use (`text/x-diff`, `application/octet-stream`,
/// `text/plain`, ...).
fn is_plausible_media_type(value: &str) -> bool {
    const MAX_MEDIA_TYPE_BYTES: usize = 255;
    if value.is_empty() || value.len() > MAX_MEDIA_TYPE_BYTES {
        return false;
    }
    let Some((kind, subtype)) = value.split_once('/') else {
        return false;
    };
    let plausible_token = |token: &str| {
        !token.is_empty()
            && token
                .bytes()
                .all(|byte| byte.is_ascii_graphic() && byte != b'/')
    };
    plausible_token(kind) && plausible_token(subtype)
}

fn internal_error(message: &str) -> runner_auth::ProtocolErrorResponse {
    protocol_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        StableErrorCode::InternalError,
        message,
        json!({}),
    )
}

/// Whether a `sqlx::Error` is a unique-constraint violation, as opposed to
/// any other database-level fault — mirrors
/// `runner_admin.rs::is_unique_violation` (that copy is private to its own
/// module, so this is a local twin rather than a shared import) so a name
/// collision maps to `conflict` and everything else stays `internal_error`.
fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|db_error| db_error.is_unique_violation())
}

fn generate_raw_credential() -> String {
    format!("runner_credential_{}", Uuid::new_v4())
}

/// Authenticates the bearer credential, parses+validates the body against
/// the protocol envelope, and checks `runner_id`/`attempt_id` against the
/// authenticated principal and the path — the common prefix shared by every
/// attempt-scoped write below. Returns the authenticated principal, the
/// parsed body, and its required `fencing_token`.
async fn authenticate_attempt_request(
    state: &RunnerProtocolState,
    headers: &HeaderMap,
    attempt_id: &str,
    body: &[u8],
) -> runner_auth::ProtocolResult<(runner_auth::RunnerPrincipal, Value, i64)> {
    let now = state.clock.now();
    let principal = runner_auth::authenticate(&state.repo, headers, now).await?;
    let value = parse_body(body)?;
    check_protocol_version(&value)?;
    runner_auth::require_matching_runner(&value, &principal)?;
    runner_auth::require_matching_attempt(&value, attempt_id)?;
    let fencing_token = as_i64(&value, "fencing_token")?;
    Ok((principal, value, fencing_token))
}

/// Validates the embedded runner capability payload shared by `/enroll` and
/// `/refresh` against B1's [`EmbeddedCapabilitySnapshot`] — the type built
/// exactly for this wire shape (`crates/tack-orch/src/execution/capabilities.rs`),
/// rather than a hand-rolled, field-by-field walk of the raw `Value`.
///
/// **Adopted from B1's card brief** (III-C2 task 4): this card's original
/// hand-rolled validator only checked `concurrency`/`labels` structurally and
/// stored the rest of the payload (`harnesses`, `features`, `limits`,
/// `reported_at`) as an opaque, size-bounded JSON blob, because strict
/// `tack_orch::execution::RunnerCapabilities` parsing rejects
/// `refresh.request.json`'s sparse `features: {}`/`harnesses: []` example
/// (see the original handoff, "Capability payload shape ambiguity").
/// `EmbeddedCapabilitySnapshot` is B1's answer to exactly that gap: `features`
/// is opaque `serde_json::Value` there (so the sparse refresh fixture still
/// parses) while `harnesses`, `limits`, `reported_at` and `concurrency` are
/// now genuinely typed and structurally validated — a strict improvement
/// over the previous "everything but concurrency/labels is an untyped blob"
/// posture, closing that handoff ambiguity rather than merely working around
/// it. `embedded_capability_snapshot_parses_full_and_sparse_fixtures` in
/// `capabilities.rs` already proves both `enrollment.request.json` and
/// `refresh.request.json` parse; `full_capabilities()` in this card's own
/// `c2_handlers_test.rs` sends the same full shape, so no existing test
/// payload needed changing.
///
/// This function still owns exactly two things `EmbeddedCapabilitySnapshot`
/// does not and should not encode — a shape type has no opinion on protocol
/// *limits* or *business rules*: the pre-parse `capabilities_bytes_max` byte
/// cap (checked before the typed parse even runs, so an oversized payload
/// never pays JSON-deserialization cost) and the `available <= total`
/// business rule B1's brief explicitly called out as this card's own to
/// keep.
fn validate_capability_payload(
    capabilities: &Value,
) -> runner_auth::ProtocolResult<(String, i64, i64)> {
    if json_byte_len(capabilities) > LIMITS.capabilities_bytes_max {
        return Err(payload_too_large(
            "capabilities_bytes_max",
            LIMITS.capabilities_bytes_max,
        ));
    }
    let parsed: EmbeddedCapabilitySnapshot =
        serde_json::from_value(capabilities.clone()).map_err(|_| {
            invalid_request(
                "capabilities",
                "capabilities does not match the runner-v1 embedded capability shape",
            )
        })?;
    if parsed.concurrency.available > parsed.concurrency.total {
        return Err(invalid_request(
            "concurrency",
            "available capacity cannot exceed total capacity",
        ));
    }
    if parsed.labels.len() as u64 > LIMITS.labels_max {
        return Err(payload_too_large("labels_max", LIMITS.labels_max));
    }
    for (key, value) in &parsed.labels {
        if key.len() as u64 > LIMITS.label_name_bytes_max {
            return Err(payload_too_large(
                "label_name_bytes_max",
                LIMITS.label_name_bytes_max,
            ));
        }
        if value.len() as u64 > LIMITS.label_value_bytes_max {
            return Err(payload_too_large(
                "label_value_bytes_max",
                LIMITS.label_value_bytes_max,
            ));
        }
    }
    let labels_json = serde_json::to_string(&parsed.labels)
        .map_err(|_| internal_error("Could not encode labels"))?;
    Ok((
        labels_json,
        parsed.concurrency.total as i64,
        parsed.concurrency.available as i64,
    ))
}

// ---------------------------------------------------------------------
// Enrollment exchange. Authenticated by the single-use enrollment token in
// the body, never a bearer header — there is no runner identity yet.
// ---------------------------------------------------------------------

pub async fn enroll(State(state): State<RunnerProtocolState>, body: Bytes) -> HandlerResult {
    let value = parse_body(&body)?;
    check_protocol_version(&value)?;
    let enrollment_token = as_str(&value, "enrollment_token")?.to_owned();
    let runner_name = as_str(&value, "runner_name")?.to_owned();
    let runner_version = as_str(&value, "runner_version")?.to_owned();
    let capabilities = field(&value, "capabilities")?.clone();
    let (labels_json, total_capacity, available_capacity) =
        validate_capability_payload(&capabilities)?;
    let capability_snapshot_json = serde_json::to_string(&capabilities)
        .map_err(|_| internal_error("Could not encode capabilities"))?;

    let now = state.clock.now();
    let raw_credential = generate_raw_credential();
    let credential_hash_value = runner_auth::credential_hash(&raw_credential);
    let credential_expires_at = now + Duration::days(CREDENTIAL_LIFETIME_DAYS);
    let token_hash = runner_auth::credential_hash(&enrollment_token);

    let redeemed = state
        .repo
        .redeem_enrollment_token(
            &token_hash,
            &credential_hash_value,
            credential_expires_at,
            &runner_version,
            &runner_name,
            &labels_json,
            total_capacity,
            available_capacity,
            &capability_snapshot_json,
            1,
            state.clock.as_ref(),
        )
        .await
        .map_err(|err| {
            // III-H7: a collision on `agent_runners`'s `UNIQUE` `name` column
            // gets the frozen `conflict` outcome
            // (`docs/contracts/runner-v1/errors/conflict.json`), never an
            // unhandled 500 — the same classification
            // `runner_admin.rs::create_pending_runner` already applies to the
            // sibling INSERT. Nothing else this statement writes carries a
            // uniqueness constraint, so every other failure stays
            // `internal_error`. The `redeem_enrollment_token` UPDATE no
            // longer writes the self-reported `runner_name` into `name` (see
            // that function's doc comment), so this branch is not reachable
            // through today's public API; it is kept as the same
            // defense-in-depth the sibling admin route relies on, in case a
            // future migration adds another unique column this statement
            // touches.
            if is_unique_violation(&err) {
                tracing::warn!("enrollment rejected: runner name already in use");
                protocol_error(
                    StatusCode::CONFLICT,
                    StableErrorCode::Conflict,
                    "A runner with this name is already enrolled",
                    json!({}),
                )
            } else {
                internal_error("Could not redeem enrollment token")
            }
        })?;

    match redeemed {
        RedeemEnrollmentResult::Redeemed(runner_id) => {
            tracing::info!(runner_id = %runner_id, "runner enrolled");
            Ok(Json(json!({
                "protocol_version": 1,
                "runner_id": runner_id,
                "runner_credential": raw_credential,
                "credential_expires_at": credential_expires_at.to_rfc3339(),
                "heartbeat_interval_seconds": LIMITS.heartbeat_interval_seconds,
                "lease_duration_seconds": LIMITS.lease_duration_seconds,
                "server_time": now.to_rfc3339(),
            })))
        }
        RedeemEnrollmentResult::InvalidOrExpired => {
            tracing::warn!("enrollment token rejected: invalid, expired, or already used");
            Err(protocol_error(
                StatusCode::UNAUTHORIZED,
                StableErrorCode::Unauthorized,
                "The enrollment token is invalid, expired, or already used",
                json!({}),
            ))
        }
    }
}

// ---------------------------------------------------------------------
// Capability refresh. Runner-bearer authenticated; hashes and (optionally)
// rotates the runner credential.
//
// Credential rotation (`rotate_credential: true`) uses B2's
// `Repository::rotate_runner_credential` — a compare-and-set keyed on the
// *authenticated* credential hash (`principal.credential_hash`), added by
// B2's "Three-review fix-up" amendment specifically because this handler's
// prior direct-`pool()` write had no such predicate: two concurrent or
// retried rotations both authenticate against the same still-valid old
// hash and silently last-writer-wins, leaving the loser holding/caching a
// credential the server already discarded (recoverable only via a fresh
// operator-issued enrollment token). See
// docs/agent-handoffs/part-iii/III-B2.md, "Three-review fix-up... Defect 3".
//
// The capability/profile fields (`runner_version`, `name`, `labels`,
// `capability_snapshot`) still have no dedicated B2 repository method (only
// enrollment and revocation do), so they remain a direct `Repository::pool()`
// write — the same precedent C1's `runner_admin.rs` uses for tables with no
// wrapper — run *after* the credential CAS succeeds (or, on the
// non-rotating branch, unconditionally). Ordering the CAS first means a
// rejected rotation (`HashMismatch`) never touches the capability columns.
// ---------------------------------------------------------------------

pub async fn refresh(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    body: Bytes,
) -> HandlerResult {
    let now = state.clock.now();
    let principal = runner_auth::authenticate(&state.repo, &headers, now).await?;
    let value = parse_body(&body)?;
    check_protocol_version(&value)?;
    runner_auth::require_matching_runner(&value, &principal)?;
    let runner_name = as_str(&value, "runner_name")?.to_owned();
    let runner_version = as_str(&value, "runner_version")?.to_owned();
    let rotate_credential = value
        .get("rotate_credential")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let capabilities = field(&value, "capabilities")?.clone();
    let (labels_json, _total_capacity, _available_capacity) =
        validate_capability_payload(&capabilities)?;
    let capability_snapshot_json = serde_json::to_string(&capabilities)
        .map_err(|_| internal_error("Could not encode capabilities"))?;

    let now_s = now.to_rfc3339();
    let revoked_error = || {
        protocol_error(
            StatusCode::FORBIDDEN,
            StableErrorCode::RunnerRevoked,
            "The runner credential has been revoked",
            json!({"runner_id": principal.runner_id}),
        )
    };

    let (raw_credential, credential_expires_at) = if rotate_credential {
        let raw = generate_raw_credential();
        let hash = runner_auth::credential_hash(&raw);
        let expires = now + Duration::days(CREDENTIAL_LIFETIME_DAYS);
        let rotation = state
            .repo
            .rotate_runner_credential(
                &principal.runner_id,
                &principal.credential_hash,
                &hash,
                expires,
                state.clock.as_ref(),
            )
            .await
            .map_err(|_| internal_error("Could not rotate runner credential"))?;
        match rotation {
            CredentialRotationResult::Rotated(_) => {}
            CredentialRotationResult::HashMismatch => {
                // `HashMismatch` covers both "another rotation already won
                // the compare-and-set race" and "the runner is no longer
                // active/was revoked between authentication and this write"
                // (see `CredentialRotationResult`'s own doc comment in
                // tack-db) — the CAS predicate cannot distinguish which.
                // Mapped to `conflict`, not `runner_revoked`: the plain,
                // non-rotating branch below already has a distinct, more
                // precise `runner_revoked` error for the case it can
                // actually prove (its own row-affected check), and
                // collapsing a merely-lost race into "you were revoked"
                // would repeat the exact anti-pattern Task 1's
                // IdempotencyConflict/Conflict split eliminated elsewhere in
                // this file. The message below is a near-literal match for
                // docs/contracts/runner-v1/errors/conflict.json — "The
                // resource changed before this operation committed",
                // `retryable: true`. A caller that retries (rotating or
                // not) re-authenticates fresh against the current
                // credential and will itself surface `runner_revoked` if
                // that was the real cause, so no information is lost by not
                // disambiguating synchronously here.
                return Err(protocol_error(
                    StatusCode::CONFLICT,
                    StableErrorCode::Conflict,
                    "The runner credential changed before this rotation committed",
                    json!({"runner_id": principal.runner_id}),
                ));
            }
        }
        let updated = sqlx::query(
            "UPDATE agent_runners SET runner_version=?, name=?, labels=?, capability_snapshot=?, updated_at=? \
             WHERE id=? AND state='active' AND revoked_at IS NULL",
        )
        .bind(&runner_version)
        .bind(&runner_name)
        .bind(&labels_json)
        .bind(&capability_snapshot_json)
        .bind(&now_s)
        .bind(&principal.runner_id)
        .execute(state.repo.pool())
        .await
        .map_err(|_| internal_error("Could not update runner capabilities"))?;
        if updated.rows_affected() != 1 {
            // The credential CAS above already proved the row was active
            // and non-revoked at that instant; the only way this second,
            // narrower write (`state='active' AND revoked_at IS NULL`) can
            // now affect zero rows is a revoke landing in the brief window
            // between the two statements, so `revoked_error()` remains
            // precise here.
            return Err(revoked_error());
        }
        (Some(raw), expires)
    } else {
        let row = sqlx::query(
            "SELECT credential_expires_at FROM agent_runners WHERE id=? AND state='active' AND revoked_at IS NULL",
        )
        .bind(&principal.runner_id)
        .fetch_optional(state.repo.pool())
        .await
        .map_err(|_| internal_error("Could not read runner credential expiry"))?;
        let Some(row) = row else {
            return Err(revoked_error());
        };
        let expires_raw: Option<String> = row.get("credential_expires_at");
        let expires = match expires_raw {
            Some(raw) => DateTime::parse_from_rfc3339(&raw)
                .map(|value| value.with_timezone(&Utc))
                .map_err(|_| internal_error("Stored credential expiry is corrupt"))?,
            None => now,
        };
        let updated = sqlx::query(
            "UPDATE agent_runners SET runner_version=?, name=?, labels=?, capability_snapshot=?, updated_at=? \
             WHERE id=? AND state='active' AND revoked_at IS NULL",
        )
        .bind(&runner_version)
        .bind(&runner_name)
        .bind(&labels_json)
        .bind(&capability_snapshot_json)
        .bind(&now_s)
        .bind(&principal.runner_id)
        .execute(state.repo.pool())
        .await
        .map_err(|_| internal_error("Could not update runner capabilities"))?;
        if updated.rows_affected() != 1 {
            return Err(revoked_error());
        }
        (None, expires)
    };

    tracing::info!(runner_id = %principal.runner_id, rotated = rotate_credential, "runner capability refresh accepted");
    Ok(Json(json!({
        "protocol_version": 1,
        "runner_id": principal.runner_id,
        "runner_credential": raw_credential,
        "credential_expires_at": credential_expires_at.to_rfc3339(),
        "accepted_at": now.to_rfc3339(),
    })))
}

// ---------------------------------------------------------------------
// Claim.
// ---------------------------------------------------------------------

pub async fn claim(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    body: Bytes,
) -> HandlerResult {
    let now = state.clock.now();
    let principal = runner_auth::authenticate(&state.repo, &headers, now).await?;
    let value = parse_body(&body)?;
    check_protocol_version(&value)?;
    runner_auth::require_matching_runner(&value, &principal)?;
    let claim_request_id = as_str(&value, "claim_request_id")?.to_owned();
    // `available_capacity` is part of the wire contract (`claim.request.json`
    // requires it) but has no effect here: unlike `heartbeat`, whose
    // `heartbeat_batch` repository call cross-checks a runner's reported
    // capacity against its actual active reservations atomically (and can
    // return `Conflict`), `claim_execution_idempotent_with_snapshot` does
    // not accept a capacity argument at all. Previously this line
    // type-checked the field and discarded the result — validating a value
    // that is never used implies an enforcement that does not exist. A real
    // cross-check would have to run either inside that same atomic claim
    // transaction (out of this card's scope; `tack-db` is B2-owned) or as a
    // separate, non-atomic read of `agent_runners.available_capacity`
    // racing exactly the concurrent-claim hazard B2's `BEGIN IMMEDIATE` fix
    // (see docs/agent-handoffs/part-iii/III-B2.md) was written to close.
    // Deliberately dropped rather than kept as a vestigial shape check — an
    // independent verifier's finding; see the handoff.
    let wait_ms = as_u64(&value, "wait_ms")?;
    if wait_ms > LIMITS.claim_wait_ms_max {
        return Err(payload_too_large(
            "claim_wait_ms_max",
            LIMITS.claim_wait_ms_max,
        ));
    }

    // Card III-E6 (Wave 4 integrator): the pure `tack_orch::scheduler` decides
    // *which* selector-eligible queued request (if any) this runner should
    // attempt, against live capacity/heartbeat/label/harness/model data —
    // replacing the pre-Wave-4 naive `ORDER BY created_at LIMIT 1` match.
    // This is a separate, read-only query pass ahead of the fenced claim
    // transaction (`tack-db` cannot depend on `tack-orch`; see
    // `RequestSelection`'s doc comment in `crates/tack-db/src/repo/execution.rs`
    // for the full reasoning) — the claim transaction below still
    // re-validates the chosen id is genuinely `queued` and eligible before
    // leasing it, so a decision that raced against a concurrent claim simply
    // yields "no work" this round rather than a double lease.
    let scheduled_request_id = choose_request_for_runner(
        &state.repo,
        &principal.runner_id,
        now,
        &SchedulingPolicy::default(),
    )
    .await
    .map_err(|_| internal_error("Could not evaluate scheduling candidates"))?;

    let attempt_id = format!("att_{}", Uuid::new_v4());
    let claimed = state
        .repo
        .claim_execution_idempotent_with_snapshot(
            &principal.runner_id,
            &claim_request_id,
            &attempt_id,
            Duration::seconds(LIMITS.lease_duration_seconds),
            state.clock.as_ref(),
            RequestSelection::Scheduled(scheduled_request_id.as_deref()),
        )
        .await
        .map_err(|_| internal_error("Could not claim work"))?;

    match claimed {
        Some(ClaimedExecution {
            lease,
            request_snapshot,
        }) => {
            // `base_revision` is a required, immutable field on the frozen
            // request snapshot (TODO.md III.1.2). `unwrap_or_default()`
            // would silently turn "missing/unreadable" into `""` — a
            // structural zero standing in for unknown, which rule 7
            // forbids. Surface it as `internal_error` instead: B2's own
            // `claim_execution_idempotent_with_snapshot` already validates
            // the full snapshot shape before a request can be enqueued
            // (quarantining anything incomplete as `needs_operator` rather
            // than leasing it — see III-B2's "Request snapshot hardening"),
            // so reaching this branch with an absent/malformed
            // `base_revision` indicates a persistence-layer contract
            // violation, not a client input error.
            let base_revision = request_snapshot
                .get("repository")
                .and_then(|repository| repository.get("base_revision"))
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    internal_error(
                        "The claimed request snapshot is missing its required base_revision",
                    )
                })?
                .to_owned();
            Ok(Json(json!({
                "protocol_version": 1,
                "claim_request_id": claim_request_id,
                "lease": {
                    "attempt_id": lease.attempt_id,
                    "runner_id": lease.runner_id,
                    "fencing_token": lease.fencing_token,
                    "issued_at": lease.issued_at.to_rfc3339(),
                    "expires_at": lease.expires_at.to_rfc3339(),
                },
                "request": request_snapshot,
                "attempt": {
                    "attempt_id": lease.attempt_id,
                    "request_id": lease.request_id,
                    "attempt_number": lease.attempt_number,
                    "runner_id": lease.runner_id,
                    "fencing_token": lease.fencing_token,
                    "state": "leased",
                    "workspace_id": Value::Null,
                    "base_revision": base_revision,
                },
            })))
        }
        None => Ok(Json(json!({
            "protocol_version": 1,
            "claim_request_id": claim_request_id,
            "lease": Value::Null,
            "retry_after_ms": NO_WORK_RETRY_AFTER_MS,
            "reason": "no_eligible_work",
        }))),
    }
}

// ---------------------------------------------------------------------
// Heartbeat.
// ---------------------------------------------------------------------

struct PreparedLease {
    attempt_id: String,
    fencing_token: i64,
    state: String,
    journal_state: String,
    last_event_checkpoint: Option<String>,
}

pub async fn heartbeat(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    body: Bytes,
) -> HandlerResult {
    let now = state.clock.now();
    let principal = runner_auth::authenticate(&state.repo, &headers, now).await?;
    let value = parse_body(&body)?;
    check_protocol_version(&value)?;
    runner_auth::require_matching_runner(&value, &principal)?;
    let heartbeat_id = as_str(&value, "heartbeat_id")?.to_owned();
    let sent_at = as_datetime(&value, "sent_at")?;
    let available_capacity = as_i64(&value, "available_capacity")?;
    let active_attempts = as_array(&value, "active_attempts")?.clone();
    if active_attempts.len() as u64 > LIMITS.active_attempts_per_heartbeat_max {
        return Err(payload_too_large(
            "active_attempts_per_heartbeat_max",
            LIMITS.active_attempts_per_heartbeat_max,
        ));
    }
    let prepared: Vec<PreparedLease> = active_attempts
        .iter()
        .map(|entry| -> runner_auth::ProtocolResult<PreparedLease> {
            Ok(PreparedLease {
                attempt_id: as_str(entry, "attempt_id")?.to_owned(),
                fencing_token: as_i64(entry, "fencing_token")?,
                state: as_str(entry, "state")?.to_owned(),
                journal_state: as_str(entry, "journal_state")?.to_owned(),
                last_event_checkpoint: entry
                    .get("last_event_checkpoint")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        })
        .collect::<runner_auth::ProtocolResult<Vec<_>>>()?;
    let leases: Vec<HeartbeatLease> = prepared
        .iter()
        .map(|prepared| HeartbeatLease {
            attempt_id: &prepared.attempt_id,
            fencing_token: prepared.fencing_token,
            state: &prepared.state,
            journal_state: &prepared.journal_state,
            last_event_checkpoint: prepared.last_event_checkpoint.as_deref(),
        })
        .collect();

    let result = state
        .repo
        .heartbeat_batch(
            &principal.runner_id,
            &heartbeat_id,
            sent_at,
            available_capacity,
            &leases,
            Duration::seconds(LIMITS.lease_duration_seconds),
            state.clock.as_ref(),
        )
        .await
        .map_err(|_| internal_error("Could not record heartbeat"))?;

    match result {
        HeartbeatBatchResult::Accepted(resp) | HeartbeatBatchResult::Replayed(resp) => {
            Ok(Json(json!({
                "protocol_version": 1,
                "heartbeat_id": resp.heartbeat_id,
                "accepted_at": resp.accepted_at.to_rfc3339(),
                "lease_results": resp.leases.iter().map(|lease| json!({
                    "attempt_id": lease.attempt_id,
                    "fencing_token": lease.fencing_token,
                    "lease_expires_at": lease.lease_expires_at.to_rfc3339(),
                    "cancellation_requested": lease.cancellation_requested,
                })).collect::<Vec<_>>(),
            })))
        }
        HeartbeatBatchResult::Conflict => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::IdempotencyConflict,
            "The heartbeat_id was already used with different content",
            json!({"heartbeat_id": heartbeat_id}),
        )),
        HeartbeatBatchResult::StaleLease(id) => Err(stale_lease(&id)),
    }
}

// ---------------------------------------------------------------------
// Accept (preparing) / start (running). Not backed by a named
// request/response fixture pair — see the handoff's contract-ambiguity
// note. Backed directly by B2's `transition_attempt_with_facts`, which
// implements exactly the frozen `leased -> preparing -> running`
// (`lease_owner`) rule from `lifecycle-transitions.json`.
// ---------------------------------------------------------------------

pub async fn accept_attempt(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path(attempt_id): Path<String>,
    body: Bytes,
) -> HandlerResult {
    transition_attempt(
        state,
        headers,
        attempt_id,
        body,
        AttemptTransitionPhase::Preparing,
    )
    .await
}

pub async fn start_attempt(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path(attempt_id): Path<String>,
    body: Bytes,
) -> HandlerResult {
    transition_attempt(
        state,
        headers,
        attempt_id,
        body,
        AttemptTransitionPhase::Running,
    )
    .await
}

async fn transition_attempt(
    state: RunnerProtocolState,
    headers: HeaderMap,
    attempt_id: String,
    body: Bytes,
    phase: AttemptTransitionPhase,
) -> HandlerResult {
    let (principal, value, fencing_token) =
        authenticate_attempt_request(&state, &headers, &attempt_id, &body).await?;
    let workspace_id = as_str(&value, "workspace_id")?.to_owned();
    let base_revision = as_str(&value, "base_revision")?.to_owned();
    let process_id = value
        .get("process_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let phase_name = match phase {
        AttemptTransitionPhase::Preparing => "preparing",
        AttemptTransitionPhase::Running => "running",
    };
    if matches!(phase, AttemptTransitionPhase::Running)
        && process_id.as_deref().map(str::is_empty).unwrap_or(true)
    {
        return Err(invalid_request(
            "process_id",
            "process_id is required to report running",
        ));
    }

    let result = state
        .repo
        .transition_attempt_with_facts(
            AttemptTransitionInput {
                runner_id: &principal.runner_id,
                attempt_id: &attempt_id,
                fencing_token,
                phase,
                workspace_id: &workspace_id,
                base_revision: &base_revision,
                process_id: process_id.as_deref(),
            },
            state.clock.as_ref(),
        )
        .await
        .map_err(|_| internal_error("Could not record attempt transition"))?;

    match result {
        AttemptTransitionResult::Applied(resp) => Ok(Json(json!({
            "protocol_version": 1,
            "attempt_id": resp.attempt_id,
            "state": resp.state,
            "replayed": false,
            "committed_at": resp.committed_at,
        }))),
        AttemptTransitionResult::Replayed(resp) => Ok(Json(json!({
            "protocol_version": 1,
            "attempt_id": resp.attempt_id,
            "state": resp.state,
            "replayed": true,
            "committed_at": resp.committed_at,
        }))),
        AttemptTransitionResult::Stale => Err(stale_lease(&attempt_id)),
        AttemptTransitionResult::Conflict => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::InvalidTransition,
            "The attempt cannot report this phase from its current state",
            json!({"to": phase_name}),
        )),
    }
}

// ---------------------------------------------------------------------
// Event batch. Every limit is checked before any repository call, so a
// rejected batch — oversized or not — writes nothing.
// ---------------------------------------------------------------------

struct PreparedEvent {
    row_id: String,
    event_id: String,
    sequence: i64,
    source: String,
    kind: String,
    payload_json: String,
    occurred_at: DateTime<Utc>,
}

pub async fn submit_events(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path(attempt_id): Path<String>,
    body: Bytes,
) -> HandlerResult {
    // `event_batch_bytes_max` (1 MiB) equals `json_body_bytes_max`, so
    // `parse_body` inside `authenticate_attempt_request` already enforces
    // this endpoint's whole-batch byte cap; no separate check is needed.
    let (principal, value, fencing_token) =
        authenticate_attempt_request(&state, &headers, &attempt_id, &body).await?;
    let checkpoint = as_str(&value, "checkpoint")?.to_owned();
    let previous_checkpoint = value
        .get("previous_checkpoint")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let events = as_array(&value, "events")?.clone();
    if events.len() as u64 > LIMITS.event_batch_count_max {
        return Err(payload_too_large(
            "event_batch_count_max",
            LIMITS.event_batch_count_max,
        ));
    }
    let prepared: Vec<PreparedEvent> = events
        .iter()
        .map(|event| -> runner_auth::ProtocolResult<PreparedEvent> {
            let payload = field(event, "payload")?.clone();
            if json_byte_len(&payload) > LIMITS.event_payload_bytes_max {
                return Err(payload_too_large(
                    "event_payload_bytes_max",
                    LIMITS.event_payload_bytes_max,
                ));
            }
            let payload_json = serde_json::to_string(&payload)
                .map_err(|_| internal_error("Could not encode event payload"))?;
            Ok(PreparedEvent {
                row_id: format!("evt_row_{}", Uuid::new_v4()),
                event_id: as_str(event, "event_id")?.to_owned(),
                sequence: as_i64(event, "sequence")?,
                source: as_str(event, "source")?.to_owned(),
                kind: as_str(event, "kind")?.to_owned(),
                payload_json,
                occurred_at: as_datetime(event, "occurred_at")?,
            })
        })
        .collect::<runner_auth::ProtocolResult<Vec<_>>>()?;
    let owned: Vec<NewEvent> = prepared
        .iter()
        .map(|prepared| NewEvent {
            id: &prepared.row_id,
            event_id: &prepared.event_id,
            sequence: prepared.sequence,
            source: &prepared.source,
            kind: &prepared.kind,
            payload: &prepared.payload_json,
            occurred_at: prepared.occurred_at,
        })
        .collect();

    let result = state
        .repo
        .append_execution_events_result(
            EventBatch {
                runner_id: &principal.runner_id,
                attempt_id: &attempt_id,
                fencing_token,
                previous_checkpoint: previous_checkpoint.as_deref(),
                checkpoint: &checkpoint,
            },
            &owned,
            state.clock.as_ref(),
        )
        .await
        .map_err(|_| internal_error("Could not record events"))?;

    match result {
        EventApplyResult::Applied(batch) => Ok(Json(json!({
            "protocol_version": 1,
            "attempt_id": attempt_id,
            "accepted_event_ids": batch.accepted_event_ids,
            "duplicate_event_ids": batch.duplicate_event_ids,
            "committed_checkpoint": batch.committed_checkpoint,
        }))),
        // B2's "Three-review fix-up" amendment split the old, collapsed
        // `ReplayConflict` into two causes carried by distinct variants: the
        // same `(attempt_id, checkpoint)` idempotency-scoped key reused with
        // different content (this can never succeed by retrying — the
        // non-retryable `idempotency_conflict` code) versus a benign
        // out-of-order resync or a defensive lost compare-and-set (genuinely
        // retryable — the `conflict` code, unchanged from before this
        // amendment). See docs/agent-handoffs/part-iii/III-B2.md, "Three-review
        // fix-up: idempotency-conflict split...".
        EventApplyResult::IdempotencyConflict => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::IdempotencyConflict,
            "The event batch checkpoint was already used with different event content",
            json!({"attempt_id": attempt_id}),
        )),
        EventApplyResult::Conflict => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "The event batch checkpoint does not match the attempt's current stream position",
            json!({"attempt_id": attempt_id}),
        )),
        EventApplyResult::Stale => Err(stale_lease(&attempt_id)),
    }
}

// ---------------------------------------------------------------------
// Decision create + poll.
// ---------------------------------------------------------------------

pub async fn create_decision(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path(attempt_id): Path<String>,
    body: Bytes,
) -> HandlerResult {
    let (principal, value, fencing_token) =
        authenticate_attempt_request(&state, &headers, &attempt_id, &body).await?;
    let decision_id = as_str(&value, "decision_id")?.to_owned();
    let kind = as_str(&value, "kind")?.to_owned();
    let prompt = as_str(&value, "prompt")?.to_owned();
    if prompt.len() as u64 > LIMITS.decision_prompt_bytes_max {
        return Err(payload_too_large(
            "decision_prompt_bytes_max",
            LIMITS.decision_prompt_bytes_max,
        ));
    }
    let options = as_array(&value, "options")?.clone();
    if options.len() as u64 > LIMITS.decision_options_max {
        return Err(payload_too_large(
            "decision_options_max",
            LIMITS.decision_options_max,
        ));
    }
    for option in &options {
        as_str(option, "option_id")?;
        as_str(option, "label")?;
    }
    let metadata = value.get("metadata").cloned().unwrap_or_else(|| json!({}));
    if !metadata.is_object() {
        return Err(invalid_request("metadata", "metadata must be an object"));
    }
    if json_byte_len(&metadata) > LIMITS.metadata_bytes_max {
        return Err(payload_too_large(
            "metadata_bytes_max",
            LIMITS.metadata_bytes_max,
        ));
    }
    let expires_at = match value.get("expires_at") {
        None | Some(Value::Null) => None,
        Some(_) => Some(as_datetime(&value, "expires_at")?),
    };

    let now = state.clock.now();
    // Precise precondition check before calling B2, so a wrong runner/fence
    // combo (`stale_lease`) is distinguishable from an ineligible attempt
    // state (`conflict`) — both collapse to a single `bool` in
    // `create_execution_decision`.
    let row = sqlx::query(
        "SELECT state, lease_expires_at FROM execution_attempts WHERE id=? AND runner_id=? AND fencing_token=?",
    )
    .bind(&attempt_id)
    .bind(&principal.runner_id)
    .bind(fencing_token)
    .fetch_optional(state.repo.pool())
    .await
    .map_err(|_| internal_error("Could not verify attempt"))?;
    let Some(row) = row else {
        return Err(stale_lease(&attempt_id));
    };
    let attempt_state: String = row.get("state");
    let lease_expires_at: String = row.get("lease_expires_at");
    if lease_expires_at <= now.to_rfc3339() {
        return Err(stale_lease(&attempt_id));
    }
    if !matches!(attempt_state.as_str(), "running" | "waiting_decision") {
        return Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "Decisions can only be created while the attempt is running or awaiting a decision",
            json!({"state": attempt_state}),
        ));
    }

    let row_id = format!("dec_row_{}", Uuid::new_v4());
    let options_json = serde_json::to_string(&Value::Array(options.clone()))
        .map_err(|_| internal_error("Could not encode options"))?;
    let metadata_json = serde_json::to_string(&metadata)
        .map_err(|_| internal_error("Could not encode metadata"))?;
    let written = state
        .repo
        .create_execution_decision(
            &principal.runner_id,
            &attempt_id,
            fencing_token,
            NewDecision {
                id: &row_id,
                decision_id: &decision_id,
                kind: &kind,
                prompt: &prompt,
                options: &options_json,
                metadata: &metadata_json,
                expires_at,
            },
            state.clock.as_ref(),
        )
        .await
        .map_err(|_| internal_error("Could not record decision"))?;
    if !written {
        return Err(stale_lease(&attempt_id));
    }

    // B2 inserts decisions with `ON CONFLICT (attempt_id, decision_id) DO
    // NOTHING`, so a reused `decision_id` with different content would
    // otherwise silently report success without writing the new content.
    // Read the committed row back and treat a mismatch as
    // `idempotency_conflict` — an exact replay (fresh insert or identical
    // resubmission) returns the row's own `created_at`/`state`.
    let stored = sqlx::query(
        "SELECT state, kind, prompt, options, metadata, expires_at, created_at FROM execution_decisions WHERE attempt_id=? AND decision_id=?",
    )
    .bind(&attempt_id)
    .bind(&decision_id)
    .fetch_one(state.repo.pool())
    .await
    .map_err(|_| internal_error("Could not verify decision"))?;
    let stored_state: String = stored.get("state");
    let stored_kind: String = stored.get("kind");
    let stored_prompt: String = stored.get("prompt");
    let stored_options: String = stored.get("options");
    let stored_metadata: String = stored.get("metadata");
    let stored_expires_at: Option<String> = stored.get("expires_at");
    let stored_created_at: String = stored.get("created_at");
    let stored_options_value: Value = serde_json::from_str(&stored_options).unwrap_or(Value::Null);
    let stored_metadata_value: Value =
        serde_json::from_str(&stored_metadata).unwrap_or(Value::Null);
    let requested_expires_at = expires_at.map(|value| value.to_rfc3339());
    if stored_kind != kind
        || stored_prompt != prompt
        || stored_options_value != Value::Array(options)
        || stored_metadata_value != metadata
        || stored_expires_at != requested_expires_at
    {
        return Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::IdempotencyConflict,
            "The decision_id was already used with different content",
            json!({"decision_id": decision_id}),
        ));
    }

    Ok(Json(json!({
        "protocol_version": 1,
        "decision_id": decision_id,
        "state": stored_state,
        "created_at": stored_created_at,
    })))
}

pub async fn poll_decisions(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path(attempt_id): Path<String>,
    body: Bytes,
) -> HandlerResult {
    let (principal, value, fencing_token) =
        authenticate_attempt_request(&state, &headers, &attempt_id, &body).await?;
    let after = normalize_datetime(&value, "after")?;

    let exists = sqlx::query(
        "SELECT 1 FROM execution_attempts WHERE id=? AND runner_id=? AND fencing_token=?",
    )
    .bind(&attempt_id)
    .bind(&principal.runner_id)
    .bind(fencing_token)
    .fetch_optional(state.repo.pool())
    .await
    .map_err(|_| internal_error("Could not verify attempt"))?;
    if exists.is_none() {
        return Err(stale_lease(&attempt_id));
    }

    let rows = sqlx::query(
        "SELECT decision_id, state, answer, resolved_at, resolved_by, updated_at FROM execution_decisions \
         WHERE attempt_id=? AND updated_at > ? ORDER BY updated_at ASC",
    )
    .bind(&attempt_id)
    .bind(&after)
    .fetch_all(state.repo.pool())
    .await
    .map_err(|_| internal_error("Could not poll decisions"))?;

    let mut next_after = after.clone();
    let decisions: Vec<Value> = rows
        .iter()
        .map(|row| {
            let answer: Option<String> = row.get("answer");
            let resolved_by: Option<String> = row.get("resolved_by");
            let updated_at: String = row.get("updated_at");
            if updated_at > next_after {
                next_after = updated_at.clone();
            }
            json!({
                "decision_id": row.get::<String, _>("decision_id"),
                "state": row.get::<String, _>("state"),
                "answer": answer.and_then(|raw| serde_json::from_str::<Value>(&raw).ok()),
                "resolved_at": row.get::<Option<String>, _>("resolved_at"),
                "resolved_by": resolved_by.and_then(|raw| serde_json::from_str::<Value>(&raw).ok()),
            })
        })
        .collect();

    Ok(Json(json!({
        "protocol_version": 1,
        "decisions": decisions,
        "next_after": next_after,
    })))
}

// ---------------------------------------------------------------------
// Artifact manifest. Manifest-only: content upload/download is out of this
// card's scope (see the handoff).
// ---------------------------------------------------------------------

struct PreparedArtifact {
    artifact_id: String,
    kind: String,
    name: String,
    media_type: Option<String>,
    size_bytes: i64,
    sha256: String,
    content_disposition: Option<String>,
    metadata: Value,
}

pub async fn submit_artifacts(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path(attempt_id): Path<String>,
    body: Bytes,
) -> HandlerResult {
    let (principal, value, fencing_token) =
        authenticate_attempt_request(&state, &headers, &attempt_id, &body).await?;
    let artifacts = as_array(&value, "artifacts")?.clone();
    if artifacts.len() as u64 > LIMITS.artifact_manifest_count_max {
        return Err(payload_too_large(
            "artifact_manifest_count_max",
            LIMITS.artifact_manifest_count_max,
        ));
    }
    let mut batch_total: u64 = 0;
    let prepared: Vec<PreparedArtifact> = artifacts
        .iter()
        .map(
            |artifact| -> runner_auth::ProtocolResult<PreparedArtifact> {
                let size_bytes = as_i64(artifact, "size_bytes")?;
                if size_bytes < 0 || size_bytes as u64 > LIMITS.artifact_content_bytes_max {
                    return Err(payload_too_large(
                        "artifact_content_bytes_max",
                        LIMITS.artifact_content_bytes_max,
                    ));
                }
                batch_total = batch_total.saturating_add(size_bytes as u64);
                let sha256 = as_str(artifact, "sha256")?.to_owned();
                if sha256.len() != 64
                    || !sha256.bytes().all(|byte| {
                        byte.is_ascii_hexdigit() && !(byte as char).is_ascii_uppercase()
                    })
                {
                    return Err(invalid_request(
                        "sha256",
                        "sha256 must be 64 lowercase hex characters",
                    ));
                }
                let metadata = artifact
                    .get("metadata")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if !metadata.is_object() {
                    return Err(invalid_request("metadata", "metadata must be an object"));
                }
                if json_byte_len(&metadata) > LIMITS.artifact_metadata_bytes_max {
                    return Err(payload_too_large(
                        "artifact_metadata_bytes_max",
                        LIMITS.artifact_metadata_bytes_max,
                    ));
                }
                // III-F2: `media_type` had no shape check at all before this
                // card — any string, of any length, was accepted verbatim.
                // Not bounded by any `limits.json` field (unlike `sha256`,
                // which the fixture's own value shape fixes), so this is a
                // reasonable, explicitly-chosen check this card introduces:
                // a plausible `type/subtype` MIME shape and a modest length
                // cap, catching both garbage values and an unbounded-length
                // field slipping through unmeasured.
                if let Some(media_type) = artifact.get("media_type").and_then(Value::as_str)
                    && !is_plausible_media_type(media_type)
                {
                    return Err(invalid_request(
                        "media_type",
                        "media_type must look like type/subtype and be at most 255 bytes",
                    ));
                }
                Ok(PreparedArtifact {
                    artifact_id: as_str(artifact, "artifact_id")?.to_owned(),
                    kind: as_str(artifact, "kind")?.to_owned(),
                    name: as_str(artifact, "name")?.to_owned(),
                    media_type: artifact
                        .get("media_type")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    size_bytes,
                    sha256,
                    content_disposition: artifact
                        .get("content_disposition")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    metadata,
                })
            },
        )
        .collect::<runner_auth::ProtocolResult<Vec<_>>>()?;

    let now = state.clock.now();
    let row = sqlx::query(
        "SELECT state, lease_expires_at FROM execution_attempts WHERE id=? AND runner_id=? AND fencing_token=?",
    )
    .bind(&attempt_id)
    .bind(&principal.runner_id)
    .bind(fencing_token)
    .fetch_optional(state.repo.pool())
    .await
    .map_err(|_| internal_error("Could not verify attempt"))?;
    let Some(row) = row else {
        return Err(stale_lease(&attempt_id));
    };
    let attempt_state: String = row.get("state");
    let lease_expires_at: String = row.get("lease_expires_at");
    if lease_expires_at <= now.to_rfc3339() {
        return Err(stale_lease(&attempt_id));
    }
    // Whitelist form, aligned with `create_decision`'s state gate: artifacts
    // may only land while the attempt is actively running or paused for a
    // decision. The prior blacklist form (`succeeded|failed|cancelled`) let
    // `lost` and `needs_operator` still accept artifact writes, even though
    // those states exist precisely to mean "stop trusting this runner's
    // reports" (TODO.md III.1.1) — an independent verifier's finding.
    if !matches!(attempt_state.as_str(), "running" | "waiting_decision") {
        return Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "Artifacts can only be recorded while the attempt is running or awaiting a decision",
            json!({"state": attempt_state}),
        ));
    }

    let existing_total: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(size_bytes),0) FROM execution_artifacts WHERE attempt_id=?",
    )
    .bind(&attempt_id)
    .fetch_one(state.repo.pool())
    .await
    .map_err(|_| internal_error("Could not verify artifact totals"))?;
    if (existing_total as u64).saturating_add(batch_total) > LIMITS.artifact_attempt_total_bytes_max
    {
        return Err(payload_too_large(
            "artifact_attempt_total_bytes_max",
            LIMITS.artifact_attempt_total_bytes_max,
        ));
    }

    let upload_expires_at = (now + Duration::seconds(ARTIFACT_UPLOAD_WINDOW_SECONDS)).to_rfc3339();
    let mut accepted = Vec::with_capacity(prepared.len());
    for item in prepared {
        let row_id = format!("art_row_{}", Uuid::new_v4());
        let metadata_json = serde_json::to_string(&item.metadata)
            .map_err(|_| internal_error("Could not encode metadata"))?;
        let written = state
            .repo
            .record_execution_artifact(
                &principal.runner_id,
                &attempt_id,
                fencing_token,
                NewArtifact {
                    id: &row_id,
                    artifact_id: &item.artifact_id,
                    kind: &item.kind,
                    name: &item.name,
                    media_type: item.media_type.as_deref(),
                    size_bytes: item.size_bytes,
                    sha256: &item.sha256,
                    content_disposition: item.content_disposition.as_deref(),
                    content_reference: None,
                    metadata: &metadata_json,
                },
                state.clock.as_ref(),
            )
            .await
            .map_err(|_| internal_error("Could not record artifact"))?;
        if !written {
            return Err(stale_lease(&attempt_id));
        }

        // Same `ON CONFLICT ... DO NOTHING` gap as decisions: read back and
        // compare before reporting success for a reused `artifact_id`.
        let stored = sqlx::query(
            "SELECT kind, name, media_type, size_bytes, sha256, content_disposition, metadata FROM execution_artifacts WHERE attempt_id=? AND artifact_id=?",
        )
        .bind(&attempt_id)
        .bind(&item.artifact_id)
        .fetch_one(state.repo.pool())
        .await
        .map_err(|_| internal_error("Could not verify artifact"))?;
        let stored_metadata: String = stored.get("metadata");
        let stored_metadata_value: Value =
            serde_json::from_str(&stored_metadata).unwrap_or(Value::Null);
        let mismatch = stored.get::<String, _>("kind") != item.kind
            || stored.get::<String, _>("name") != item.name
            || stored.get::<Option<String>, _>("media_type") != item.media_type
            || stored.get::<i64, _>("size_bytes") != item.size_bytes
            || stored.get::<String, _>("sha256") != item.sha256
            || stored.get::<Option<String>, _>("content_disposition") != item.content_disposition
            || stored_metadata_value != item.metadata;
        if mismatch {
            return Err(protocol_error(
                StatusCode::CONFLICT,
                StableErrorCode::IdempotencyConflict,
                "The artifact_id was already used with different content",
                json!({"artifact_id": item.artifact_id}),
            ));
        }

        accepted.push(json!({
            "artifact_id": item.artifact_id,
            "state": "manifest_accepted",
            "upload": {
                "method": "PUT",
                // III-F2: attempt-scoped (unlike the pre-F2 placeholder path
                // this replaces) — `artifact_id` alone cannot disambiguate
                // between two different attempts that happen to choose the
                // same runner-supplied id, and the real endpoint needs the
                // attempt to authenticate/fence against.
                "path": format!(
                    "/api/runner/v1/attempts/{attempt_id}/artifacts/{}/content",
                    item.artifact_id
                ),
                "expires_at": upload_expires_at,
            },
        }));
    }

    Ok(Json(json!({
        "protocol_version": 1,
        "attempt_id": attempt_id,
        "artifacts": accepted,
    })))
}

// ---------------------------------------------------------------------
// III-F2: artifact content upload. Streams the request body straight to
// `ArtifactStorage` (never buffers it whole — see that module's own doc
// comment) and only commits `content_reference` once both the total byte
// count and the SHA-256 exactly match the manifest `submit_artifacts`
// already recorded. Every failure path (oversize, short, checksum mismatch,
// stream error) stages nothing: no blob survives on disk, and
// `content_reference` is never written.
// ---------------------------------------------------------------------

pub async fn put_artifact_content(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path((attempt_id, artifact_id)): Path<(String, String)>,
    body: Body,
) -> HandlerResult {
    let now = state.clock.now();
    let principal = runner_auth::authenticate(&state.repo, &headers, now).await?;

    let fencing_token = headers
        .get(ARTIFACT_FENCING_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<i64>().ok())
        .ok_or_else(|| {
            invalid_request(
                "fencing_token",
                "The X-Tack-Fencing-Token header is required and must be an integer",
            )
        })?;

    // One query resolves both eligibility facts this handler needs: the
    // manifest row (sha256/size/media_type/content_reference) and the
    // owning attempt's fencing/lease/state — mirrors `submit_artifacts`'s
    // own two-part check (lease/state gate + artifact lookup) collapsed into
    // one round trip via a join, since (unlike `submit_artifacts`) there is
    // no batch of rows to loop over here.
    let row = sqlx::query(
        "SELECT ea.sha256 AS sha256, ea.size_bytes AS size_bytes, ea.media_type AS media_type, \
         ea.content_reference AS content_reference, eat.state AS state, \
         eat.lease_expires_at AS lease_expires_at \
         FROM execution_artifacts ea \
         JOIN execution_attempts eat ON eat.id = ea.attempt_id \
         WHERE ea.attempt_id = ? AND ea.artifact_id = ? AND eat.runner_id = ? AND eat.fencing_token = ?",
    )
    .bind(&attempt_id)
    .bind(&artifact_id)
    .bind(&principal.runner_id)
    .bind(fencing_token)
    .fetch_optional(state.repo.pool())
    .await
    .map_err(|_| internal_error("Could not verify artifact"))?;
    let Some(row) = row else {
        // Covers every "this write is not currently valid" case in one
        // stable code, matching `submit_artifacts`'s own precedent: an
        // unknown artifact_id, a fencing mismatch, or a runner_id mismatch
        // are all indistinguishable from a stale lease to an unauthenticated
        // caller, and must stay that way — a more specific error here would
        // let a caller probe for which of the three is true.
        return Err(stale_lease(&attempt_id));
    };
    let attempt_state: String = row.get("state");
    let lease_expires_at: String = row.get("lease_expires_at");
    if lease_expires_at <= now.to_rfc3339() {
        return Err(stale_lease(&attempt_id));
    }
    if !matches!(attempt_state.as_str(), "running" | "waiting_decision") {
        return Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "Artifact content can only be recorded while the attempt is running or awaiting a decision",
            json!({"state": attempt_state}),
        ));
    }
    let existing_content_reference: Option<String> = row.get("content_reference");
    if existing_content_reference.is_some() {
        // Content is immutable once verified (see
        // `set_execution_artifact_content_reference`'s own doc comment) — a
        // second PUT for the same artifact_id is refused before consuming
        // any of its body, rather than re-verified and silently discarded.
        return Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "Artifact content has already been recorded and is immutable",
            json!({"artifact_id": artifact_id}),
        ));
    }
    let declared_media_type: Option<String> = row.get("media_type");
    if let Some(declared) = declared_media_type.as_deref()
        && let Some(provided) = headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
        && !content_type_matches(declared, provided)
    {
        return Err(invalid_request(
            "content-type",
            "The upload Content-Type does not match the artifact manifest's declared media_type",
        ));
    }
    let declared_size_bytes: i64 = row.get("size_bytes");
    let declared_sha256: String = row.get("sha256");

    let stored = state
        .artifact_storage
        .store_streaming(
            &attempt_id,
            &artifact_id,
            declared_size_bytes as u64,
            &declared_sha256,
            Box::pin(body.into_data_stream()),
        )
        .await;
    let stored = match stored {
        Ok(stored) => stored,
        Err(ArtifactContentError::ChecksumMismatch) => {
            return Err(protocol_error(
                StatusCode::CONFLICT,
                StableErrorCode::ArtifactChecksumMismatch,
                "The uploaded artifact does not match its manifest",
                json!({"artifact_id": artifact_id}),
            ));
        }
        Err(ArtifactContentError::OversizeStream) | Err(ArtifactContentError::SizeMismatch) => {
            return Err(payload_too_large(
                "artifact_content_bytes_max",
                LIMITS.artifact_content_bytes_max,
            ));
        }
        Err(ArtifactContentError::StreamRead) => {
            return Err(invalid_request(
                "content",
                "The upload stream ended before it could be verified",
            ));
        }
        Err(ArtifactContentError::UnsafeStorageLocation) | Err(ArtifactContentError::Io) => {
            return Err(internal_error("Could not store artifact content"));
        }
    };

    let commit = state
        .repo
        .set_execution_artifact_content_reference(
            &principal.runner_id,
            &attempt_id,
            &artifact_id,
            fencing_token,
            &stored.content_reference,
            state.clock.as_ref(),
        )
        .await
        .map_err(|_| internal_error("Could not record artifact content"))?;
    match commit {
        ArtifactContentCommitResult::Committed => Ok(Json(json!({
            "protocol_version": 1,
            "attempt_id": attempt_id,
            "artifact_id": artifact_id,
            "state": "content_verified",
            "size_bytes": stored.bytes_written,
            "sha256": declared_sha256,
        }))),
        ArtifactContentCommitResult::AlreadySet => {
            // Lost a race to a concurrent upload of the same artifact_id
            // after fully streaming and verifying our own copy: the bytes we
            // just wrote are correct but orphaned (the DB row is now owned
            // by whichever request committed first), so they are removed
            // rather than left as an unreferenced blob.
            state
                .artifact_storage
                .remove_blob(&stored.content_reference)
                .await;
            Err(protocol_error(
                StatusCode::CONFLICT,
                StableErrorCode::Conflict,
                "Artifact content has already been recorded and is immutable",
                json!({"artifact_id": artifact_id}),
            ))
        }
        ArtifactContentCommitResult::Stale => {
            state
                .artifact_storage
                .remove_blob(&stored.content_reference)
                .await;
            Err(stale_lease(&attempt_id))
        }
    }
}

/// Case-insensitive, parameter-stripped comparison — `text/x-diff` matches
/// `text/x-diff` and `text/x-diff; charset=utf-8` alike, since HTTP clients
/// routinely append a `charset` parameter this endpoint has no use for.
fn content_type_matches(declared: &str, provided: &str) -> bool {
    let essence = |value: &str| {
        value
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase()
    };
    essence(declared) == essence(provided)
}

// ---------------------------------------------------------------------
// Completion.
// ---------------------------------------------------------------------

pub async fn submit_completion(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path(attempt_id): Path<String>,
    body: Bytes,
) -> HandlerResult {
    let (principal, value, fencing_token) =
        authenticate_attempt_request(&state, &headers, &attempt_id, &body).await?;
    let completion_id = as_str(&value, "completion_id")?.to_owned();
    let terminal_state = as_str(&value, "terminal_state")?.to_owned();
    if !matches!(
        terminal_state.as_str(),
        "succeeded" | "failed" | "cancelled"
    ) {
        return Err(invalid_request(
            "terminal_state",
            "terminal_state must be succeeded, failed, or cancelled",
        ));
    }
    let terminal_reason = field(&value, "terminal_reason")?.clone();
    if !terminal_reason.is_object() {
        return Err(invalid_request(
            "terminal_reason",
            "terminal_reason must be an object",
        ));
    }
    // Reused directly from B1's frozen domain (`tack_orch::execution`) —
    // both shapes match `completion.request.json`'s `actual_execution` and
    // `usage` exactly, so parsing into these typed values (rather than a
    // parallel DTO) is both the card brief's instruction and the strongest
    // available guarantee that this handler matches the frozen fixture.
    let actual_execution: ActualExecution =
        serde_json::from_value(field(&value, "actual_execution")?.clone()).map_err(|_| {
            invalid_request(
                "actual_execution",
                "actual_execution does not match the runner-v1 shape",
            )
        })?;
    let usage: Usage = serde_json::from_value(field(&value, "usage")?.clone())
        .map_err(|_| invalid_request("usage", "usage does not match the runner-v1 shape"))?;
    let final_event_checkpoint = value
        .get("final_event_checkpoint")
        .and_then(Value::as_str)
        .map(str::to_owned);

    let terminal_reason_json = serde_json::to_string(&terminal_reason)
        .map_err(|_| internal_error("Could not encode terminal_reason"))?;
    let actual_execution_json = serde_json::to_string(&actual_execution)
        .map_err(|_| internal_error("Could not encode actual_execution"))?;
    let usage_json =
        serde_json::to_string(&usage).map_err(|_| internal_error("Could not encode usage"))?;

    let result = state
        .repo
        .complete_execution_result(
            Completion {
                runner_id: &principal.runner_id,
                attempt_id: &attempt_id,
                fencing_token,
                completion_id: &completion_id,
                final_event_checkpoint: final_event_checkpoint.as_deref(),
                terminal_state: &terminal_state,
                terminal_reason: &terminal_reason_json,
                actual_execution: &actual_execution_json,
                usage: &usage_json,
            },
            state.clock.as_ref(),
        )
        .await
        .map_err(|_| internal_error("Could not record completion"))?;

    match result {
        CompletionResult::Committed(resp) => Ok(Json(json!({
            "protocol_version": 1,
            "attempt_id": attempt_id,
            "completion_id": resp.completion_id,
            "state": resp.terminal_state,
            "replayed": false,
            "committed_at": resp.committed_at,
        }))),
        CompletionResult::Replayed(resp) => Ok(Json(json!({
            "protocol_version": 1,
            "attempt_id": attempt_id,
            "completion_id": resp.completion_id,
            "state": resp.terminal_state,
            "replayed": true,
            "committed_at": resp.committed_at,
        }))),
        // Same split as `submit_events` above, from the same B2 amendment:
        // the same `(attempt_id, completion_id)` idempotency-scoped key
        // reused with different content can never succeed by retrying
        // (`idempotency_conflict`), while a distinct completion_id racing a
        // concurrent terminal write, or a pre-M055 terminal attempt with no
        // authoritative historical response, is a benign, retryable
        // `conflict` — unchanged from before this amendment.
        CompletionResult::IdempotencyConflict => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::IdempotencyConflict,
            "The completion_id was already used with different content",
            json!({"attempt_id": attempt_id}),
        )),
        CompletionResult::Conflict => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "The completion could not be committed",
            json!({"attempt_id": attempt_id}),
        )),
        CompletionResult::Stale => Err(stale_lease(&attempt_id)),
    }
}

// ---------------------------------------------------------------------
// Cancellation observation.
// ---------------------------------------------------------------------

pub async fn observe_cancellation_report(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path(attempt_id): Path<String>,
    body: Bytes,
) -> HandlerResult {
    let (principal, value, fencing_token) =
        authenticate_attempt_request(&state, &headers, &attempt_id, &body).await?;
    let cancellation_request_id = as_str(&value, "cancellation_request_id")?.to_owned();
    let observation = as_str(&value, "observation")?;
    if observation != "process_stopped" {
        return Err(invalid_request(
            "observation",
            "observation must be process_stopped",
        ));
    }
    let observed_at = as_datetime(&value, "observed_at")?;
    let details = field(&value, "details")?.clone();
    if !details.is_object() {
        return Err(invalid_request("details", "details must be an object"));
    }
    let details_json =
        serde_json::to_string(&details).map_err(|_| internal_error("Could not encode details"))?;
    let observation_json = serde_json::to_string(&Value::String("process_stopped".into()))
        .expect("a static string literal always serializes");

    let result = state
        .repo
        .observe_cancellation(
            CancellationObservationInput {
                runner_id: &principal.runner_id,
                attempt_id: &attempt_id,
                fencing_token,
                cancellation_request_id: &cancellation_request_id,
                observed_at,
                details: &details_json,
                observation: &observation_json,
            },
            state.clock.as_ref(),
        )
        .await
        .map_err(|_| internal_error("Could not record cancellation observation"))?;

    match result {
        CancellationObservation::Cancelled(resp) => Ok(Json(json!({
            "protocol_version": 1,
            "attempt_id": attempt_id,
            "cancellation_request_id": resp.cancellation_request_id,
            "state": resp.state,
            "replayed": false,
            "committed_at": resp.committed_at,
        }))),
        CancellationObservation::Replayed(resp) => Ok(Json(json!({
            "protocol_version": 1,
            "attempt_id": attempt_id,
            "cancellation_request_id": resp.cancellation_request_id,
            "state": resp.state,
            "replayed": true,
            "committed_at": resp.committed_at,
        }))),
        CancellationObservation::Conflict => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::IdempotencyConflict,
            "The cancellation_request_id was already used with different content",
            json!({"cancellation_request_id": cancellation_request_id}),
        )),
        CancellationObservation::Stale => Err(stale_lease(&attempt_id)),
        CancellationObservation::AlreadyTerminal { state } => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::InvalidTransition,
            "The attempt already reached a terminal state",
            json!({"from": state, "to": "cancelled"}),
        )),
        CancellationObservation::Ambiguous { state } => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "Cancellation was not requested for this attempt or its state is not recognized",
            json!({"attempt_id": attempt_id, "state": state}),
        )),
    }
}

// ---------------------------------------------------------------------
// Recovery observation (additive v1 operation; exact path from
// `protocol.json`, relative to `base_path`). Uses B1's typed
// `RecoveryObservationRequest`/`RecoveryObservationResponse` directly, so
// this handler's wire shape is exactly what B1 already fixture-tested.
// ---------------------------------------------------------------------

pub async fn observe_recovery(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path(attempt_id): Path<String>,
    body: Bytes,
) -> HandlerResult {
    let now = state.clock.now();
    let principal = runner_auth::authenticate(&state.repo, &headers, now).await?;
    let value = parse_body(&body)?;
    check_protocol_version(&value)?;
    runner_auth::require_matching_runner(&value, &principal)?;
    runner_auth::require_matching_attempt(&value, &attempt_id)?;
    let request: RecoveryObservationRequest = serde_json::from_value(value).map_err(|_| {
        invalid_request(
            "body",
            "The recovery-observation request does not match the runner-v1 shape",
        )
    })?;
    if request.attempt_id.as_str() != attempt_id {
        return Err(invalid_request(
            "attempt_id",
            "attempt_id in the body must match the path",
        ));
    }
    let fencing_token = request.fencing_token.0 as i64;
    let details_json = serde_json::to_string(&request.details)
        .map_err(|_| internal_error("Could not encode details"))?;
    let observation = match request.observation {
        TackRecoveryObservation::ProcessStopped => DbRecoveryObservation::ProcessStopped,
        TackRecoveryObservation::ProcessRunning => DbRecoveryObservation::ProcessRunning,
        TackRecoveryObservation::Ambiguous => DbRecoveryObservation::Ambiguous,
    };

    let result = state
        .repo
        .recover_attempt(
            RecoveryObservationInput {
                runner_id: &principal.runner_id,
                attempt_id: request.attempt_id.as_str(),
                fencing_token,
                recovery_key: request.recovery_key.as_str(),
                observation,
                details: &details_json,
            },
            state.clock.as_ref(),
        )
        .await
        .map_err(|_| internal_error("Could not record recovery observation"))?;

    let (resp, replayed) = match result {
        RecoveryObservationResult::Applied(resp) => (resp, false),
        RecoveryObservationResult::Replayed(resp) => (resp, true),
        RecoveryObservationResult::Conflict => {
            return Err(protocol_error(
                StatusCode::CONFLICT,
                StableErrorCode::IdempotencyConflict,
                "The recovery_key was already used with different content",
                json!({"recovery_key": request.recovery_key.as_str()}),
            ));
        }
        RecoveryObservationResult::Stale => return Err(stale_lease(&attempt_id)),
    };
    let disposition = match resp.disposition {
        DbRecoveryDisposition::SafePreSpawnRequeue => TackRecoveryDisposition::SafePreSpawnRequeue,
        DbRecoveryDisposition::NeedsOperator => TackRecoveryDisposition::NeedsOperator,
        DbRecoveryDisposition::AlreadyTerminal => TackRecoveryDisposition::AlreadyTerminal,
    };
    let committed_at = DateTime::parse_from_rfc3339(&resp.committed_at)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| internal_error("Stored committed_at is corrupt"))?;
    let response = RecoveryObservationResponse {
        protocol_version: ProtocolVersion::v1(),
        attempt_id: AttemptId::new(resp.attempt_id),
        recovery_key: RecoveryKey::new(resp.recovery_key),
        disposition,
        replayed,
        committed_at,
        additional: Default::default(),
    };
    Ok(Json(serde_json::to_value(response).map_err(|_| {
        internal_error("Could not encode response")
    })?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_body_limit_bytes_is_the_lesser_of_configured_and_ceiling() {
        // Direction 1: a tighter operator-configured limit wins.
        assert_eq!(effective_body_limit_bytes(2 * 1024), 2 * 1024);
        // Direction 2: a looser (or default 2 MiB, or a very large) configured
        // limit never loosens the surface past the 4 MiB protocol ceiling.
        assert_eq!(
            effective_body_limit_bytes(10 * 1024 * 1024),
            RUNNER_ROUTER_BODY_LIMIT_BYTES
        );
        assert_eq!(
            effective_body_limit_bytes(usize::MAX),
            RUNNER_ROUTER_BODY_LIMIT_BYTES
        );
        // Exactly at the ceiling: still the ceiling, not a fencepost.
        assert_eq!(
            effective_body_limit_bytes(RUNNER_ROUTER_BODY_LIMIT_BYTES),
            RUNNER_ROUTER_BODY_LIMIT_BYTES
        );
    }

    #[test]
    fn limits_constants_match_frozen_fixture_exactly() {
        let raw = include_str!("../../../../docs/contracts/runner-v1/limits.json");
        let fixture: Value = serde_json::from_str(raw).expect("limits fixture");
        let mine = json!({
            "protocol_version": 1,
            "json_body_bytes_max": LIMITS.json_body_bytes_max,
            "metadata_bytes_max": LIMITS.metadata_bytes_max,
            "environment_entries_max": LIMITS.environment_entries_max,
            "environment_name_bytes_max": LIMITS.environment_name_bytes_max,
            "environment_value_bytes_max": LIMITS.environment_value_bytes_max,
            "labels_max": LIMITS.labels_max,
            "label_name_bytes_max": LIMITS.label_name_bytes_max,
            "label_value_bytes_max": LIMITS.label_value_bytes_max,
            "heartbeat_interval_seconds": LIMITS.heartbeat_interval_seconds,
            "heartbeat_grace_seconds": LIMITS.heartbeat_grace_seconds,
            "lease_duration_seconds": LIMITS.lease_duration_seconds,
            "claim_wait_ms_max": LIMITS.claim_wait_ms_max,
            "active_attempts_per_heartbeat_max": LIMITS.active_attempts_per_heartbeat_max,
            "event_batch_count_max": LIMITS.event_batch_count_max,
            "event_payload_bytes_max": LIMITS.event_payload_bytes_max,
            "event_batch_bytes_max": LIMITS.event_batch_bytes_max,
            "decision_prompt_bytes_max": LIMITS.decision_prompt_bytes_max,
            "decision_options_max": LIMITS.decision_options_max,
            "decision_answer_bytes_max": LIMITS.decision_answer_bytes_max,
            "artifact_manifest_count_max": LIMITS.artifact_manifest_count_max,
            "artifact_metadata_bytes_max": LIMITS.artifact_metadata_bytes_max,
            "artifact_content_bytes_max": LIMITS.artifact_content_bytes_max,
            "artifact_attempt_total_bytes_max": LIMITS.artifact_attempt_total_bytes_max,
            "capabilities_bytes_max": LIMITS.capabilities_bytes_max,
            "request_timeout_seconds_max": LIMITS.request_timeout_seconds_max,
            "retention_event_days_default": LIMITS.retention_event_days_default,
            "retention_artifact_days_default": LIMITS.retention_artifact_days_default,
        });
        assert_eq!(mine, fixture);
    }

    #[test]
    fn check_protocol_version_reports_the_stable_envelope() {
        assert!(check_protocol_version(&json!({"protocol_version": 1})).is_ok());
        let (status, body) = check_protocol_version(&json!({"protocol_version": 2})).unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body.0["error"]["code"], "unsupported_protocol");
        assert_eq!(body.0["error"]["details"]["received"], 2);
        assert_eq!(body.0["error"]["details"]["minimum_supported"], 1);
        assert_eq!(body.0["error"]["details"]["maximum_supported"], 1);
        let (status, body) = check_protocol_version(&json!({})).unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body.0["error"]["code"], "invalid_request");
    }

    #[test]
    fn normalize_datetime_makes_z_and_offset_suffixes_comparable() {
        let z = normalize_datetime(&json!({"after": "2026-08-06T12:20:59Z"}), "after").unwrap();
        let offset =
            normalize_datetime(&json!({"after": "2026-08-06T12:20:59+00:00"}), "after").unwrap();
        assert_eq!(z, offset);
    }

    #[test]
    fn validate_capability_payload_rejects_available_over_total_and_oversized_labels() {
        // Every case below is otherwise a complete `EmbeddedCapabilitySnapshot`
        // shape (`reported_at`/`limits`/`concurrency`/`labels`/`harnesses`/
        // `features` all present) — since Task 4's adoption of B1's typed
        // parse, an incomplete shape is rejected before these business rules
        // ever run (see `validate_capability_payload_rejects_incomplete_shape`
        // below).
        let snapshot = |total: i64, available: i64, labels: Value| {
            json!({
                "reported_at": "2026-08-06T12:00:00Z",
                "labels": labels,
                "concurrency": {"total": total, "available": available},
                "harnesses": [],
                "features": {},
                "limits": {"event_payload_bytes_max": 1, "artifact_content_bytes_max": 1},
            })
        };
        let ok = snapshot(2, 1, json!({"os": "linux"}));
        assert!(validate_capability_payload(&ok).is_ok());
        let over = snapshot(1, 2, json!({}));
        assert!(validate_capability_payload(&over).is_err());
        let mut labels = serde_json::Map::new();
        for i in 0..(LIMITS.labels_max + 1) {
            labels.insert(format!("k{i}"), json!("v"));
        }
        let too_many = snapshot(1, 1, Value::Object(labels));
        let (status, body) = validate_capability_payload(&too_many).unwrap_err();
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(body.0["error"]["code"], "payload_too_large");
        assert_eq!(body.0["error"]["details"]["limit"], "labels_max");
    }

    /// Task 4's typed-parse adoption means a payload missing a required
    /// `EmbeddedCapabilitySnapshot` field (here, `limits`) is now rejected as
    /// `invalid_request` before any business rule runs — this is strictly
    /// more validation than the pre-amendment hand-rolled check, which
    /// ignored `limits`/`reported_at`/`harnesses` entirely.
    #[test]
    fn validate_capability_payload_rejects_incomplete_shape() {
        let missing_limits = json!({
            "reported_at": "2026-08-06T12:00:00Z",
            "labels": {},
            "concurrency": {"total": 1, "available": 1},
            "harnesses": [],
            "features": {},
        });
        let (status, body) = validate_capability_payload(&missing_limits).unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body.0["error"]["code"], "invalid_request");
    }

    /// The exact sparse shape `refresh.request.json` uses (`harnesses: []`,
    /// `features: {}`) — the fixture this card's original handoff flagged as
    /// unable to satisfy strict `RunnerCapabilities` parsing — must still
    /// validate under `EmbeddedCapabilitySnapshot`, closing that ambiguity.
    #[test]
    fn validate_capability_payload_accepts_refresh_fixtures_sparse_shape() {
        let raw = include_str!("../../../../docs/contracts/runner-v1/refresh.request.json");
        let value: Value = serde_json::from_str(raw).expect("refresh fixture");
        let capabilities = &value["capabilities"];
        assert!(validate_capability_payload(capabilities).is_ok());
    }
}
