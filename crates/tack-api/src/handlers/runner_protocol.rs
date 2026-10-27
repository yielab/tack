//! Runner-protocol handler modules, mounted under `/api/runner/v1` (see
//! `docs/contracts/runner-v1/protocol.json`'s `base_path`) by `router.rs`.
//!
//! Only the additive `recovery-observation` operation has an explicit path in
//! `protocol.json` (`/attempts/{attempt_id}/recovery-observation`, preserved
//! here verbatim, relative to `base_path`); every other canonical exchange in
//! `docs/contracts/runner-v1/` fixes payload shapes, not URLs.
//!
//! `runner_auth` lives at `handlers/runner_protocol/runner_auth.rs` (a
//! submodule of this file) rather than a sibling `handlers/runner_auth.rs`.
//! Rust resolves `mod runner_auth;` relative to *this file's own path*
//! (`runner_protocol.rs` → `runner_protocol/`).
//!
//! Every write below validates attempt id + runner identity (from the
//! authenticated bearer credential, never a request-body field) + fencing
//! token before calling a B2 repository method, and every payload is checked
//! against `docs/contracts/runner-v1/limits.json` before any repository call
//! — so a rejected request writes nothing (see `payload_too_large` case
//! notes below and the acceptance tests in
//! `tests/runner_protocol/lifecycle.rs`).

use std::collections::BTreeMap;
use std::sync::Arc;
use tack_protocol::Clock;

use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode, header},
    routing::{post, put},
};
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use tack_app::AppError;
use tack_app::runs::complete::{Completed, CompletionReport};
use tack_app::runs::create_decision::{DecisionCreated, DecisionRequest};
use tack_app::runs::enroll::{Enrolled, Enrollment};
use tack_app::runs::heartbeat::{Beat, Heartbeat};
use tack_app::runs::observe_recovery::{Observation, RecoveryObserved};
use tack_app::runs::refresh::{Refresh, Refreshed};
use tack_app::runs::report_events::{EventReport, EventsReported};
use tack_app::runs::submit_artifacts::{
    ArtifactManifest, ContentUpload, ManifestSubmission, ManifestSubmitted, UploadChecked,
};
use tack_db::StoreError;
use tack_db::repo::execution::state as execution_state;
use tack_db::{
    Repository, queries,
    repo::execution::{
        ArtifactContentCommitResult, AttemptTransitionInput, AttemptTransitionPhase,
        AttemptTransitionResult, CancellationObservation, CancellationObservationInput,
        ClaimedExecution,
    },
};
use tack_protocol::execution::{
    ArtifactGrant, ArtifactManifestRequest, ArtifactManifestResponse, ArtifactUpload, AttemptId,
    AttemptSnapshot, AttemptTransitionRequest, AttemptTransitionResponse, CancelObservation,
    CancellationObservationRequest, CancellationObservationResponse, ClaimGranted, ClaimIdle,
    ClaimLease, ClaimRequest, ClaimResponse, CompletionRequest, CompletionResponse, DecisionAnswer,
    DecisionCreateRequest, DecisionCreateResponse, DecisionOption, DecisionPollRequest,
    DecisionPollResponse, DecisionRecommendation, DecisionUpdate, EmbeddedCapabilitySnapshot,
    EnrollmentRequest, EnrollmentResponse, EventBatchRequest, EventBatchResponse,
    ExecutionRequestSnapshot, ExecutionState, FencingToken, HeartbeatRequest, HeartbeatResponse,
    LeaseResult, ProtocolVersion, RecoveryObservationRequest, RecoveryObservationResponse,
    RefreshRequest, RefreshResponse, RunnerId, StableErrorCode,
};
use uuid::Uuid;

pub mod runner_auth;

// Declared as submodules of this already-registered file, keeping them
// reachable without touching `handlers/mod.rs`. `artifact_storage` is the
// pure storage module (safe paths, streaming write/read); `retention` is
// the pure event/artifact sweep logic; `artifact_download` is the
// operator-facing content-download handler, deliberately never merged into
// this file's own `routes()` (that router is runner-credential-only — see
// its own doc comment) — it is mounted separately in `router.rs`.
pub mod artifact_download;
pub mod artifact_storage;
pub mod retention;

use artifact_storage::{ArtifactContentError, ArtifactStorage};
use runner_auth::{invalid_request, payload_too_large, protocol_error, stale_lease};

type HandlerResult = runner_auth::ProtocolResult<Json<Value>>;

/// Default artifact-content storage root, relative to the process's working
/// directory — mirrors `config.rs#default_storage_dir`'s own `"./storage"`
/// default, one level deeper so artifact blobs never collide with attachment
/// files that already live directly under `TACK_STORAGE_DIR`. Production
/// (`router.rs#runner_protocol_routes`) always overrides this via
/// [`RunnerProtocolState::with_artifact_storage_root`] with the
/// operator-configured `TACK_STORAGE_DIR`; this default is only actually
/// reachable from a caller that constructs `RunnerProtocolState::new`
/// directly and skips that call (e.g. a test that doesn't care where
/// artifacts land).
const DEFAULT_ARTIFACT_STORAGE_ROOT: &str = "./storage/execution-artifacts";

/// State for this module's local router, constructed from the shared API
/// state in `router.rs`.
#[derive(Clone)]
pub struct RunnerProtocolState {
    pub repo: Repository,
    pub clock: Arc<dyn Clock>,
    pub artifact_storage: Arc<ArtifactStorage>,
    /// The shared API state, when production wires it in: a succeeded
    /// completion that pushed a branch uses it to open the pull request.
    pub app: Option<crate::router::AppState>,
}

impl RunnerProtocolState {
    pub fn new(repo: Repository, clock: Arc<dyn Clock>) -> Self {
        Self {
            repo,
            clock,
            artifact_storage: Arc::new(ArtifactStorage::new(DEFAULT_ARTIFACT_STORAGE_ROOT)),
            app: None,
        }
    }

    /// Hands the shared API state to the completion handler.
    pub fn with_app_state(mut self, app: crate::router::AppState) -> Self {
        self.app = Some(app);
        self
    }

    /// Builder that points artifact content storage at a caller-chosen
    /// root without changing `new`'s call signature. Production
    /// (`router.rs#runner_protocol_routes`) calls this to swap in the
    /// operator-configured `TACK_STORAGE_DIR`. Also used directly by
    /// `runner_protocol/artifact_events.rs`.
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

/// The precedence rule: `min(configured,
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
        // Artifact content upload. A distinct, more-specific
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
        .layer(axum::middleware::from_fn(runner_auth::with_request_id))
        .with_state(state)
}

// ---------------------------------------------------------------------
// docs/contracts/runner-v1/limits.json, mirrored exactly and checked
// against the frozen fixture in this module's own test below.
// ---------------------------------------------------------------------

// Only the limits this file enforces at request time are mirrored. The other
// nine keys of `limits.json` bound concerns owned elsewhere (the execution
// request's environment and timeout at enqueue time, the operator's decision
// answer in `decisions.rs`, the retention sweeps) or are documented no-ops at
// their call sites (`heartbeat_grace_seconds`, `event_batch_bytes_max`); the
// fixture test below names them, so a key added to or dropped from the
// frozen file still fails it.
struct Limits {
    json_body_bytes_max: u64,
    metadata_bytes_max: u64,
    labels_max: u64,
    label_name_bytes_max: u64,
    label_value_bytes_max: u64,
    heartbeat_interval_seconds: i64,
    lease_duration_seconds: i64,
    claim_wait_ms_max: u64,
    active_attempts_per_heartbeat_max: u64,
    event_batch_count_max: u64,
    event_payload_bytes_max: u64,
    decision_prompt_bytes_max: u64,
    decision_options_max: u64,
    artifact_manifest_count_max: u64,
    artifact_metadata_bytes_max: u64,
    artifact_content_bytes_max: u64,
    artifact_attempt_total_bytes_max: u64,
    capabilities_bytes_max: u64,
}

const LIMITS: Limits = Limits {
    json_body_bytes_max: 1_048_576,
    metadata_bytes_max: 65_536,
    labels_max: 64,
    label_name_bytes_max: 64,
    label_value_bytes_max: 256,
    heartbeat_interval_seconds: 15,
    lease_duration_seconds: 60,
    claim_wait_ms_max: 30_000,
    active_attempts_per_heartbeat_max: 64,
    event_batch_count_max: 100,
    event_payload_bytes_max: 65_536,
    decision_prompt_bytes_max: 32_768,
    decision_options_max: 32,
    artifact_manifest_count_max: 100,
    artifact_metadata_bytes_max: 32_768,
    artifact_content_bytes_max: 52_428_800,
    artifact_attempt_total_bytes_max: 524_288_000,
    capabilities_bytes_max: 262_144,
};

/// Not fixed by `limits.json`: a reasonable, explicitly-chosen window for the
/// manifest-accepted upload URL returned by `submit_artifacts`. The real
/// content-upload endpoint (`put_artifact_content`, below) is what this URL
/// points at; the endpoint itself does not enforce this window (there is no
/// fixture-defined field to check it against), so it is advisory to the
/// runner today, not yet a hard server-side expiry.
const ARTIFACT_UPLOAD_WINDOW_SECONDS: i64 = 600;
/// Header a runner sets on `PUT .../artifacts/{artifact_id}/content` to carry
/// its fencing token — the request body *is* the artifact's raw bytes, so
/// (unlike every other runner-protocol write) the fencing token cannot travel
/// inside a JSON body. Not part of any frozen fixture: `docs/contracts/
/// runner-v1/` only fixes the manifest exchange's payload shape, not this
/// URL.
const ARTIFACT_FENCING_TOKEN_HEADER: &str = "x-tack-fencing-token";
/// Not fixed by any fixture: a reasonable no-work poll backoff hint.
const NO_WORK_RETRY_AFTER_MS: u64 = 5_000;

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

/// Reads a request body as one of the protocol's message structs. A body that does not fit
/// is `invalid_request`, naming the field when serde can; the message never repeats what the
/// body held, since a field's value can be a credential or a prompt.
fn parse_message<T: serde::de::DeserializeOwned>(value: Value) -> runner_auth::ProtocolResult<T> {
    serde_path_to_error::deserialize(value).map_err(|error| {
        // The path says where serde stopped (`options[1].label`); a missing field is reported
        // against its parent, so the field's own name is added.
        let mut path = error.path().to_string();
        if let Some(missing) = error
            .inner()
            .to_string()
            .strip_prefix("missing field `")
            .and_then(|rest| rest.split('`').next())
        {
            path = if path == "." {
                missing.to_owned()
            } else {
                format!("{path}.{missing}")
            };
        }
        let field = if path == "." { "body" } else { path.as_str() };
        invalid_request(
            field,
            "The request does not match the runner-v1 message shape",
        )
    })
}

/// A response as the message struct that defines it.
fn message_json<T: serde::Serialize>(message: &T) -> HandlerResult {
    serde_json::to_value(message)
        .map(Json)
        .map_err(|_| internal_error("Could not encode the response"))
}

/// A fencing token, which is unsigned on the wire; one the store's integer column cannot hold
/// is as malformed as a negative one.
fn fencing_token_from_wire(token: u64) -> Option<FencingToken> {
    i64::try_from(token).ok().map(|_| FencingToken::new(token))
}

/// The fence every attempt-scoped request carries, as the one field the common prefix reads
/// before the handler parses its own message.
#[derive(serde::Deserialize)]
struct Fenced {
    fencing_token: FencingToken,
}

/// Re-formats a client-supplied instant through chrono's own `Utc` `to_rfc3339()` before it is
/// ever used in a TEXT comparison against stored timestamps. Every stored timestamp in this
/// schema is written via that same normalization; comparing an un-normalized `Z`-suffixed input
/// against a stored `+00:00`-suffixed value would sort incorrectly even though both denote the
/// same instant.
fn normalize_datetime(instant: DateTime<Utc>) -> String {
    instant.to_rfc3339()
}

fn json_byte_len(value: &impl serde::Serialize) -> u64 {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len() as u64)
        .unwrap_or(u64::MAX)
}

/// A deliberately permissive `type/subtype` shape check — this is
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
    attempt_id: &AttemptId,
    body: &[u8],
) -> runner_auth::ProtocolResult<(runner_auth::RunnerPrincipal, Value, FencingToken)> {
    let now = state.clock.now();
    let principal = runner_auth::authenticate(&state.repo, headers, now).await?;
    let value = parse_body(body)?;
    check_protocol_version(&value)?;
    runner_auth::require_matching_runner(&value, &principal)?;
    runner_auth::require_matching_attempt(&value, attempt_id)?;
    let Fenced { fencing_token } = parse_message(value.clone())?;
    let fencing_token = fencing_token_from_wire(fencing_token.get()).ok_or_else(|| {
        invalid_request(
            "fencing_token",
            "fencing_token must be a non-negative integer",
        )
    })?;
    Ok((principal, value, fencing_token))
}

/// Validates the embedded runner capability payload shared by `/enroll` and
/// `/refresh` against [`EmbeddedCapabilitySnapshot`] — the type built for
/// exactly this wire shape (`tack-orch`'s `execution/capabilities.rs`),
/// rather than a hand-rolled walk of the raw `Value`.
///
/// Strict `RunnerCapabilities` parsing rejects `refresh.request.json`'s
/// sparse `features: {}`/`harnesses: []` example, so
/// `EmbeddedCapabilitySnapshot` keeps `features` as an opaque
/// `serde_json::Value` while `harnesses`, `limits`, `reported_at` and
/// `concurrency` stay genuinely typed and validated.
///
/// Owns exactly two things the shape type does not: the pre-parse
/// `capabilities_bytes_max` byte cap (checked before the typed parse runs,
/// so an oversized payload never pays deserialization cost) and the
/// `available <= total` business rule.
fn validate_capability_payload(
    parsed: &EmbeddedCapabilitySnapshot,
) -> runner_auth::ProtocolResult<(String, i64, i64)> {
    let encoded = serde_json::to_value(parsed)
        .map_err(|_| internal_error("Could not encode capabilities"))?;
    if json_byte_len(&encoded) > LIMITS.capabilities_bytes_max {
        return Err(payload_too_large(
            "capabilities_bytes_max",
            LIMITS.capabilities_bytes_max,
        ));
    }
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
    let EnrollmentRequest {
        enrollment_token,
        runner_name,
        runner_version,
        capabilities,
        ..
    } = parse_message(value)?;
    let (labels_json, total_capacity, available_capacity) =
        validate_capability_payload(&capabilities)?;
    let capability_snapshot_json = serde_json::to_string(&capabilities)
        .map_err(|_| internal_error("Could not encode capabilities"))?;

    let raw_credential = generate_raw_credential();
    let credential_hash_value = runner_auth::credential_hash(&raw_credential);
    let token_hash = runner_auth::credential_hash(&enrollment_token);

    let enrolled = tack_app::runs::enroll::enroll(
        &state.repo,
        state.clock.now(),
        Enrollment {
            token_hash: &token_hash,
            credential_hash: &credential_hash_value,
            runner_version: &runner_version,
            runner_name: &runner_name,
            labels_json: &labels_json,
            total_capacity,
            available_capacity,
            capability_snapshot_json: &capability_snapshot_json,
        },
    )
    .await
    .map_err(|failure| match failure {
        // A collision on a unique column of `agent_runners` gets the frozen `conflict`
        // outcome (`docs/contracts/runner-v1/errors/conflict.json`), never an unhandled 500.
        // Redeeming no longer writes the self-reported `runner_name`, so this is defense in
        // depth against a future unique column the statement touches.
        AppError::Conflict(_) | AppError::Store(StoreError::Conflict(_)) => {
            tracing::warn!("enrollment rejected: runner name already in use");
            protocol_error(
                StatusCode::CONFLICT,
                StableErrorCode::Conflict,
                "A runner with this name is already enrolled",
                json!({}),
            )
        }
        _ => internal_error("Could not redeem enrollment token"),
    })?;

    match enrolled {
        Enrolled::Runner {
            runner_id,
            credential_expires_at,
            server_time,
        } => {
            tracing::info!(runner_id = %runner_id, "runner enrolled");
            message_json(&EnrollmentResponse {
                protocol_version: ProtocolVersion::v1(),
                runner_id: RunnerId::new(runner_id),
                runner_credential: raw_credential,
                credential_expires_at,
                heartbeat_interval_seconds: LIMITS.heartbeat_interval_seconds.unsigned_abs(),
                lease_duration_seconds: LIMITS.lease_duration_seconds.unsigned_abs(),
                server_time,
            })
        }
        Enrolled::InvalidToken => {
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
// Credential rotation (`rotate_credential: true`) is a compare-and-set keyed on the
// *authenticated* credential hash (`principal.credential_hash`). Without that predicate, two
// concurrent or retried rotations both authenticate against the same still-valid old hash and
// silently last-writer-wins, leaving the loser holding/caching a credential the server already
// discarded (recoverable only via a fresh operator-issued enrollment token). The rotation and
// the capability write live in `tack_app::runs::refresh`; a rejected rotation never touches the
// capability columns.
//
// A losing rotation can fail at *either* of two points, and both
// must answer the same retryable `conflict`, not `unauthorized`. The later
// point — the CAS itself losing (`CredentialRotationResult::HashMismatch`
// below) — already does. The earlier point does not:
// `runner_auth::authenticate` above runs a plain `SELECT ... WHERE
// credential_hash=?` first, and if the *other* concurrent rotation already
// committed its new hash by the time this request's `authenticate` runs,
// that `SELECT` finds no row at all and fails "not recognized" (401) before
// the CAS — the check that would have classified it correctly — ever runs.
// See `reclassify_refresh_auth_error` immediately below.
// ---------------------------------------------------------------------

/// Remaps `authenticate`'s "credential not recognized" failure to the same
/// retryable `conflict` outcome `rotate_runner_credential`'s CAS returns
/// for `HashMismatch`, but only when the request body says
/// `rotate_credential: true` — only a request itself trying to rotate has
/// the genuine ambiguity between "you lost the race" and "this credential
/// was always bogus" (see `is_credential_not_recognized`'s doc comment). A
/// non-rotating refresh, or any other `authenticate` failure, is returned
/// unchanged.
///
/// Peeks `rotate_credential` from the raw, not-yet-validated body — cheap,
/// side-effect-free, used only to pick an error code; the real, validated
/// parse still runs unchanged on the authenticated success path below.
fn reclassify_refresh_auth_error(
    error: runner_auth::ProtocolErrorResponse,
    raw_body: &[u8],
) -> runner_auth::ProtocolErrorResponse {
    let (status, body) = error;
    if status != StatusCode::UNAUTHORIZED || !runner_auth::is_credential_not_recognized(&body.0) {
        return (status, body);
    }
    let intends_rotation = serde_json::from_slice::<Value>(raw_body)
        .ok()
        .and_then(|value| value.get("rotate_credential").and_then(Value::as_bool))
        .unwrap_or(false);
    if !intends_rotation {
        return (status, body);
    }
    runner_auth::protocol_error(
        StatusCode::CONFLICT,
        StableErrorCode::Conflict,
        "The runner credential changed before this rotation committed",
        json!({}),
    )
}

pub async fn refresh(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    body: Bytes,
) -> HandlerResult {
    let now = state.clock.now();
    let principal = runner_auth::authenticate(&state.repo, &headers, now)
        .await
        .map_err(|error| reclassify_refresh_auth_error(error, &body))?;
    let value = parse_body(&body)?;
    check_protocol_version(&value)?;
    runner_auth::require_matching_runner(&value, &principal)?;
    // The name is validated as present but never written: the board's row owns it
    // (see `redeem_enrollment_token`), and a runner's self-report must not rename it.
    let RefreshRequest {
        runner_version,
        rotate_credential,
        capabilities,
        ..
    } = parse_message(value)?;
    let (labels_json, _total_capacity, _available_capacity) =
        validate_capability_payload(&capabilities)?;
    let capability_snapshot_json = serde_json::to_string(&capabilities)
        .map_err(|_| internal_error("Could not encode capabilities"))?;

    let raw_credential = rotate_credential.then(generate_raw_credential);
    let new_credential_hash = raw_credential.as_deref().map(runner_auth::credential_hash);
    let refreshed = tack_app::runs::refresh::refresh(
        &state.repo,
        state.clock.now(),
        Refresh {
            runner_id: principal.runner_id.as_str(),
            presented_credential_hash: &principal.credential_hash,
            runner_version: &runner_version,
            labels_json: &labels_json,
            capability_snapshot_json: &capability_snapshot_json,
            new_credential_hash: new_credential_hash.as_deref(),
        },
    )
    .await
    .map_err(|failure| match failure {
        // A unique-constraint violation is a `conflict` naming the column the database
        // reports, never a bare 500.
        AppError::Store(StoreError::Conflict(message)) => {
            tracing::warn!(error = %message, "runner refresh rejected: unique column");
            protocol_error(
                StatusCode::CONFLICT,
                StableErrorCode::Conflict,
                &format!("Another runner already holds this value ({message})"),
                json!({}),
            )
        }
        _ => internal_error("Could not update runner capabilities"),
    })?;

    match refreshed {
        Refreshed::Accepted {
            credential_expires_at,
            accepted_at,
        } => {
            tracing::info!(runner_id = %principal.runner_id, rotated = rotate_credential, "runner capability refresh accepted");
            message_json(&RefreshResponse {
                protocol_version: ProtocolVersion::v1(),
                runner_id: principal.runner_id,
                runner_credential: raw_credential,
                credential_expires_at,
                accepted_at,
            })
        }
        // The compare-and-set cannot tell a lost rotation race from a runner revoked between
        // authentication and the write, so both answer the retryable `conflict`; a retry
        // re-authenticates and surfaces `runner_revoked` itself if that was the cause.
        Refreshed::CredentialChanged => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "The runner credential changed before this rotation committed",
            json!({"runner_id": principal.runner_id}),
        )),
        Refreshed::Revoked => Err(protocol_error(
            StatusCode::FORBIDDEN,
            StableErrorCode::RunnerRevoked,
            "The runner credential has been revoked",
            json!({"runner_id": principal.runner_id}),
        )),
    }
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
    let request: ClaimRequest = parse_message(value)?;
    // `available_capacity` is part of the wire contract (`claim.request.json`
    // requires it) but has no effect here: unlike `heartbeat`, whose
    // `heartbeat_batch` repository call cross-checks a runner's reported
    // capacity against its actual active reservations atomically (and can
    // return `Conflict`), `claim_execution_idempotent_with_snapshot` does
    // not accept a capacity argument at all. A real cross-check would have to
    // run inside that same atomic claim transaction, or as a separate,
    // non-atomic read racing exactly the concurrent-claim hazard the
    // `BEGIN IMMEDIATE` fix closes; the field is read for the shape and not used.
    let claim_request_id = request.claim_request_id;
    if request.wait_ms > LIMITS.claim_wait_ms_max {
        return Err(payload_too_large(
            "claim_wait_ms_max",
            LIMITS.claim_wait_ms_max,
        ));
    }

    // `tack_app::runs::claim` asks the pure scheduler which queued request this runner should
    // attempt, then takes the fenced claim, which re-checks that choice before leasing it.
    let claimed = tack_app::runs::claim::claim(
        &state.repo,
        state.clock.now(),
        &principal.runner_id,
        &claim_request_id,
        Duration::seconds(LIMITS.lease_duration_seconds),
    )
    .await
    .map_err(|_| internal_error("Could not claim work"))?;

    match claimed {
        Some(ClaimedExecution {
            lease,
            request_snapshot,
        }) => {
            // `claim_execution_idempotent_with_snapshot` validates the snapshot's shape
            // before a request can be enqueued (quarantining anything incomplete as
            // `needs_operator` rather than leasing it), so a stored snapshot that does not
            // parse here is a persistence-layer contract violation, not a client error.
            let request: ExecutionRequestSnapshot = serde_json::from_value(request_snapshot)
                .map_err(|_| internal_error("The claimed request snapshot is not readable"))?;
            let attempt_number = u32::try_from(lease.attempt_number)
                .map_err(|_| internal_error("The claimed attempt number is out of range"))?;
            message_json(&ClaimResponse::Work(Box::new(ClaimGranted {
                protocol_version: ProtocolVersion::v1(),
                claim_request_id,
                lease: ClaimLease {
                    attempt_id: lease.attempt_id.clone(),
                    runner_id: lease.runner_id.clone(),
                    fencing_token: lease.fencing_token,
                    issued_at: lease.issued_at,
                    expires_at: lease.expires_at,
                },
                attempt: AttemptSnapshot {
                    attempt_id: lease.attempt_id,
                    request_id: lease.request_id,
                    attempt_number,
                    runner_id: lease.runner_id,
                    fencing_token: lease.fencing_token,
                    state: ExecutionState::Leased,
                    workspace_id: None,
                    base_revision: request.repository.base_revision.clone(),
                    lease_issued_at: None,
                    lease_expires_at: None,
                    last_heartbeat_at: None,
                    additional: BTreeMap::new(),
                },
                request,
            })))
        }
        None => message_json(&ClaimResponse::Idle(ClaimIdle {
            protocol_version: ProtocolVersion::v1(),
            claim_request_id,
            lease: (),
            retry_after_ms: NO_WORK_RETRY_AFTER_MS,
            reason: "no_eligible_work".to_owned(),
        })),
    }
}

// ---------------------------------------------------------------------
// Heartbeat.
// ---------------------------------------------------------------------

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
    let request: HeartbeatRequest = parse_message(value)?;
    if request.active_attempts.len() as u64 > LIMITS.active_attempts_per_heartbeat_max {
        return Err(payload_too_large(
            "active_attempts_per_heartbeat_max",
            LIMITS.active_attempts_per_heartbeat_max,
        ));
    }
    if request
        .active_attempts
        .iter()
        .any(|attempt| fencing_token_from_wire(attempt.fencing_token.get()).is_none())
    {
        return Err(invalid_request(
            "fencing_token",
            "fencing_token must fit the store",
        ));
    }
    let heartbeat_id = request.heartbeat_id;

    let result = tack_app::runs::heartbeat::heartbeat(
        &state.repo,
        state.clock.now(),
        Beat {
            runner_id: &principal.runner_id,
            heartbeat_id: &heartbeat_id,
            sent_at: request.sent_at,
            available_capacity: i64::from(request.available_capacity),
            leases: &request.active_attempts,
            lease_duration: Duration::seconds(LIMITS.lease_duration_seconds),
        },
    )
    .await
    .map_err(|_| internal_error("Could not record heartbeat"))?;

    match result {
        Heartbeat::Accepted(resp) | Heartbeat::Replayed(resp) => message_json(&HeartbeatResponse {
            protocol_version: ProtocolVersion::v1(),
            heartbeat_id: resp.heartbeat_id,
            accepted_at: resp.accepted_at,
            lease_results: resp
                .leases
                .into_iter()
                .map(|lease| LeaseResult {
                    attempt_id: lease.attempt_id,
                    fencing_token: lease.fencing_token,
                    lease_expires_at: lease.lease_expires_at,
                    cancellation_requested: lease.cancellation_requested,
                })
                .collect(),
        }),
        Heartbeat::IdempotencyConflict => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::IdempotencyConflict,
            "The heartbeat_id was already used with different content",
            json!({"heartbeat_id": heartbeat_id}),
        )),
        Heartbeat::StaleLease(id) => Err(stale_lease(id.as_str())),
        Heartbeat::RunnerNotActive => Err(stale_lease(principal.runner_id.as_str())),
    }
}

// ---------------------------------------------------------------------
// Accept (preparing) / start (running). Not backed by a named
// request/response fixture pair. Backed directly by
// `transition_attempt_with_facts`, which
// implements exactly the frozen `leased -> preparing -> running`
// (`lease_owner`) rule from `lifecycle-transitions.json`.
// ---------------------------------------------------------------------

pub async fn accept_attempt(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path(attempt_id): Path<AttemptId>,
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
    Path(attempt_id): Path<AttemptId>,
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

/// The part of a running-phase start report the server keeps: variable names
/// (strings of at most 128 characters, 256 at most) and the sandbox, which is
/// `bwrap` or `none` and nothing else.
fn start_report(request: &AttemptTransitionRequest) -> Value {
    let names: Vec<&str> = request
        .environment_names
        .iter()
        .map(String::as_str)
        .filter(|name| name.chars().count() <= 128)
        .take(256)
        .collect();
    let sandbox = request
        .sandbox
        .as_deref()
        .filter(|sandbox| matches!(*sandbox, "bwrap" | "harness" | "none"));
    json!({"environment_names": names, "sandbox": sandbox})
}

/// An instant read back from a stored RFC 3339 column; one that does not parse is a corrupt row.
fn stored_instant(raw: &str, column: &str) -> runner_auth::ProtocolResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| internal_error(&format!("Stored {column} is corrupt")))
}

/// The answer to an accepted or started attempt, from the stored response.
fn transition_json(
    resp: tack_db::repo::execution::AttemptTransitionResponse,
    replayed: bool,
) -> HandlerResult {
    let committed_at = DateTime::parse_from_rfc3339(&resp.committed_at)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| internal_error("Stored committed_at is corrupt"))?;
    message_json(&AttemptTransitionResponse {
        protocol_version: ProtocolVersion::v1(),
        attempt_id: resp.attempt_id,
        state: resp.state,
        replayed,
        committed_at,
    })
}

async fn transition_attempt(
    state: RunnerProtocolState,
    headers: HeaderMap,
    attempt_id: AttemptId,
    body: Bytes,
    phase: AttemptTransitionPhase,
) -> HandlerResult {
    let (principal, value, fencing_token) =
        authenticate_attempt_request(&state, &headers, &attempt_id, &body).await?;
    let request: AttemptTransitionRequest = parse_message(value)?;
    let AttemptTransitionRequest {
        workspace_id,
        base_revision,
        process_id,
        ..
    } = request.clone();
    let phase_name = match phase {
        AttemptTransitionPhase::Preparing => execution_state::text(ExecutionState::Preparing),
        AttemptTransitionPhase::Running => execution_state::text(ExecutionState::Running),
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
            state.clock.now(),
        )
        .await
        .map_err(|_| internal_error("Could not record attempt transition"))?;

    if matches!(phase, AttemptTransitionPhase::Running)
        && matches!(result, AttemptTransitionResult::Applied(_))
    {
        state
            .repo
            .record_start_report(
                &principal.runner_id,
                &attempt_id,
                fencing_token,
                &start_report(&request).to_string(),
            )
            .await
            .map_err(|_| internal_error("Could not record the start report"))?;
    }

    match result {
        AttemptTransitionResult::Applied(resp) => transition_json(resp, false),
        AttemptTransitionResult::Replayed(resp) => transition_json(resp, true),
        AttemptTransitionResult::Stale => Err(stale_lease(attempt_id.as_str())),
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

pub async fn submit_events(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path(attempt_id): Path<AttemptId>,
    body: Bytes,
) -> HandlerResult {
    // `event_batch_bytes_max` (1 MiB) equals `json_body_bytes_max`, so
    // `parse_body` inside `authenticate_attempt_request` already enforces
    // this endpoint's whole-batch byte cap; no separate check is needed.
    let (principal, value, fencing_token) =
        authenticate_attempt_request(&state, &headers, &attempt_id, &body).await?;
    let request: EventBatchRequest = parse_message(value)?;
    let checkpoint = request.checkpoint;
    let previous_checkpoint = request.previous_checkpoint;
    if request.events.len() as u64 > LIMITS.event_batch_count_max {
        return Err(payload_too_large(
            "event_batch_count_max",
            LIMITS.event_batch_count_max,
        ));
    }
    if request
        .events
        .iter()
        .any(|event| json_byte_len(&event.payload) > LIMITS.event_payload_bytes_max)
    {
        return Err(payload_too_large(
            "event_payload_bytes_max",
            LIMITS.event_payload_bytes_max,
        ));
    }

    let result = tack_app::runs::report_events::report_events(
        &state.repo,
        state.clock.now(),
        EventReport {
            runner_id: &principal.runner_id,
            attempt_id: &attempt_id,
            fencing_token,
            previous_checkpoint: previous_checkpoint.as_ref(),
            checkpoint: &checkpoint,
            events: &request.events,
        },
    )
    .await
    .map_err(|_| internal_error("Could not record events"))?;

    match result {
        EventsReported::Applied(batch) => message_json(&EventBatchResponse {
            protocol_version: ProtocolVersion::v1(),
            attempt_id,
            accepted_event_ids: batch.accepted_event_ids,
            duplicate_event_ids: batch.duplicate_event_ids,
            committed_checkpoint: batch.committed_checkpoint,
        }),
        // Two causes, carried by distinct variants: the
        // same `(attempt_id, checkpoint)` idempotency-scoped key reused with
        // different content (this can never succeed by retrying — the
        // non-retryable `idempotency_conflict` code) versus a benign
        // out-of-order resync or a defensive lost compare-and-set (genuinely
        // retryable — the `conflict` code).
        EventsReported::IdempotencyConflict => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::IdempotencyConflict,
            "The event batch checkpoint was already used with different event content",
            json!({"attempt_id": attempt_id}),
        )),
        EventsReported::Conflict => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "The event batch checkpoint does not match the attempt's current stream position",
            json!({"attempt_id": attempt_id}),
        )),
        EventsReported::Stale => Err(stale_lease(attempt_id.as_str())),
    }
}

// ---------------------------------------------------------------------
// Decision create + poll.
// ---------------------------------------------------------------------

/// A text a runner supplies is bounded by `decision_prompt_bytes_max`.
fn bounded_text(text: &str) -> runner_auth::ProtocolResult<()> {
    if text.len() as u64 > LIMITS.decision_prompt_bytes_max {
        return Err(payload_too_large(
            "decision_prompt_bytes_max",
            LIMITS.decision_prompt_bytes_max,
        ));
    }
    Ok(())
}

/// A list of texts is bounded in length by `decision_options_max` and each text as above.
fn bounded_string_list(items: &[String]) -> runner_auth::ProtocolResult<()> {
    if items.len() as u64 > LIMITS.decision_options_max {
        return Err(payload_too_large(
            "decision_options_max",
            LIMITS.decision_options_max,
        ));
    }
    items.iter().try_for_each(|item| bounded_text(item))
}

/// Optional per-option fields are bounded: text by `decision_prompt_bytes_max`, list length by
/// `decision_options_max`.
fn validate_decision_option_extras(option: &DecisionOption) -> runner_auth::ProtocolResult<()> {
    option
        .description
        .iter()
        .try_for_each(|text| bounded_text(text))?;
    option
        .risks
        .iter()
        .try_for_each(|risks| bounded_string_list(risks))
}

/// `recommendation.option_id` must name one of the request's `options`.
fn validate_decision_recommendation(
    recommendation: &DecisionRecommendation,
    options: &[DecisionOption],
) -> runner_auth::ProtocolResult<()> {
    if !options
        .iter()
        .any(|option| option.option_id == recommendation.option_id)
    {
        return Err(invalid_request(
            "recommendation.option_id",
            "recommendation.option_id must name one of options",
        ));
    }
    recommendation
        .rationale
        .iter()
        .try_for_each(|text| bounded_text(text))?;
    recommendation
        .evidence_refs
        .iter()
        .try_for_each(|refs| bounded_string_list(refs))
}

pub async fn create_decision(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path(attempt_id): Path<AttemptId>,
    body: Bytes,
) -> HandlerResult {
    let (principal, value, fencing_token) =
        authenticate_attempt_request(&state, &headers, &attempt_id, &body).await?;
    let request: DecisionCreateRequest = parse_message(value)?;
    let DecisionCreateRequest {
        decision_id,
        kind,
        prompt,
        options,
        recommendation,
        expires_at,
        metadata,
        ..
    } = request;
    bounded_text(&prompt)?;
    if options.len() as u64 > LIMITS.decision_options_max {
        return Err(payload_too_large(
            "decision_options_max",
            LIMITS.decision_options_max,
        ));
    }
    options
        .iter()
        .try_for_each(validate_decision_option_extras)?;
    if let Some(recommendation) = &recommendation {
        validate_decision_recommendation(recommendation, &options)?;
    }
    if json_byte_len(&metadata) > LIMITS.metadata_bytes_max {
        return Err(payload_too_large(
            "metadata_bytes_max",
            LIMITS.metadata_bytes_max,
        ));
    }
    let created = tack_app::runs::create_decision::create_decision(
        &state.repo,
        state.clock.now(),
        DecisionRequest {
            runner_id: &principal.runner_id,
            attempt_id: &attempt_id,
            fencing_token,
            decision_id: &decision_id,
            kind: &kind,
            prompt: &prompt,
            options: &options,
            metadata: &metadata,
            recommendation: recommendation.as_ref(),
            expires_at,
        },
    )
    .await
    .map_err(|_| internal_error("Could not record decision"))?;

    match created {
        DecisionCreated::Created {
            state: decision_state,
            created_at,
        } => message_json(&DecisionCreateResponse {
            protocol_version: ProtocolVersion::v1(),
            decision_id,
            state: decision_state,
            created_at: stored_instant(&created_at, "created_at")?,
        }),
        DecisionCreated::Stale => Err(stale_lease(attempt_id.as_str())),
        DecisionCreated::Ineligible {
            state: attempt_state,
        } => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "Decisions can only be created while the attempt is running or awaiting a decision",
            json!({"state": attempt_state.as_str()}),
        )),
        DecisionCreated::IdempotencyConflict => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::IdempotencyConflict,
            "The decision_id was already used with different content",
            json!({"decision_id": decision_id}),
        )),
    }
}

pub async fn poll_decisions(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path(attempt_id): Path<AttemptId>,
    body: Bytes,
) -> HandlerResult {
    let (principal, value, fencing_token) =
        authenticate_attempt_request(&state, &headers, &attempt_id, &body).await?;
    let DecisionPollRequest { after, .. } = parse_message(value)?;
    let after = normalize_datetime(after);

    let holds_fence = queries::execution_attempts::holds_fence(
        &state.repo,
        &attempt_id,
        &principal.runner_id,
        fencing_token,
    )
    .await
    .map_err(|_| internal_error("Could not verify attempt"))?;
    if !holds_fence {
        return Err(stale_lease(attempt_id.as_str()));
    }

    let rows = queries::execution_decisions::decisions_changed_since(
        &state.repo,
        attempt_id.as_str(),
        &after,
    )
    .await
    .map_err(|_| internal_error("Could not poll decisions"))?;

    let mut next_after = after.clone();
    let decisions = rows
        .into_iter()
        .map(|row| -> runner_auth::ProtocolResult<DecisionUpdate> {
            if row.updated_at > next_after {
                next_after = row.updated_at.clone();
            }
            Ok(DecisionUpdate {
                decision_id: row.decision_id,
                state: row.state,
                answer: row
                    .answer
                    .and_then(|raw| serde_json::from_str::<DecisionAnswer>(&raw).ok()),
                resolved_at: row
                    .resolved_at
                    .as_deref()
                    .map(|raw| stored_instant(raw, "resolved_at"))
                    .transpose()?,
                resolved_by: row
                    .resolved_by
                    .and_then(|raw| serde_json::from_str::<Value>(&raw).ok()),
            })
        })
        .collect::<runner_auth::ProtocolResult<Vec<_>>>()?;

    message_json(&DecisionPollResponse {
        protocol_version: ProtocolVersion::v1(),
        decisions,
        next_after: stored_instant(&next_after, "next_after")?,
    })
}

// ---------------------------------------------------------------------
// Artifact manifest. Manifest-only: content upload is
// [`put_artifact_content`] below, content download is
// `artifact_download.rs`.
// ---------------------------------------------------------------------

pub async fn submit_artifacts(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path(attempt_id): Path<AttemptId>,
    body: Bytes,
) -> HandlerResult {
    let (principal, value, fencing_token) =
        authenticate_attempt_request(&state, &headers, &attempt_id, &body).await?;
    let ArtifactManifestRequest { artifacts, .. } = parse_message(value)?;
    if artifacts.len() as u64 > LIMITS.artifact_manifest_count_max {
        return Err(payload_too_large(
            "artifact_manifest_count_max",
            LIMITS.artifact_manifest_count_max,
        ));
    }
    let manifest: Vec<ArtifactManifest> = artifacts
        .into_iter()
        .map(
            |artifact| -> runner_auth::ProtocolResult<ArtifactManifest> {
                if artifact.size_bytes > LIMITS.artifact_content_bytes_max {
                    return Err(payload_too_large(
                        "artifact_content_bytes_max",
                        LIMITS.artifact_content_bytes_max,
                    ));
                }
                let size_bytes = i64::try_from(artifact.size_bytes).map_err(|_| {
                    payload_too_large(
                        "artifact_content_bytes_max",
                        LIMITS.artifact_content_bytes_max,
                    )
                })?;
                if artifact.sha256.len() != 64
                    || !artifact.sha256.bytes().all(|byte| {
                        byte.is_ascii_hexdigit() && !(byte as char).is_ascii_uppercase()
                    })
                {
                    return Err(invalid_request(
                        "sha256",
                        "sha256 must be 64 lowercase hex characters",
                    ));
                }
                let metadata = Value::Object(artifact.metadata);
                if json_byte_len(&metadata) > LIMITS.artifact_metadata_bytes_max {
                    return Err(payload_too_large(
                        "artifact_metadata_bytes_max",
                        LIMITS.artifact_metadata_bytes_max,
                    ));
                }
                // `media_type` is not bounded by any `limits.json` field
                // (unlike `sha256`, which the fixture's own value shape fixes),
                // so this is a reasonable, explicitly-chosen check:
                // a plausible `type/subtype` MIME shape and a modest length
                // cap, catching both garbage values and an unbounded-length
                // field slipping through unmeasured.
                if let Some(media_type) = artifact.media_type.as_deref()
                    && !is_plausible_media_type(media_type)
                {
                    return Err(invalid_request(
                        "media_type",
                        "media_type must look like type/subtype and be at most 255 bytes",
                    ));
                }
                Ok(ArtifactManifest {
                    artifact_id: artifact.artifact_id,
                    kind: artifact.kind,
                    name: artifact.name,
                    media_type: artifact.media_type,
                    size_bytes,
                    sha256: artifact.sha256,
                    content_disposition: artifact.content_disposition,
                    metadata,
                })
            },
        )
        .collect::<runner_auth::ProtocolResult<Vec<_>>>()?;

    let submitted = tack_app::runs::submit_artifacts::submit_artifacts(
        &state.repo,
        state.clock.now(),
        ManifestSubmission {
            runner_id: &principal.runner_id,
            attempt_id: &attempt_id,
            fencing_token,
            artifacts: &manifest,
            attempt_total_bytes_max: LIMITS.artifact_attempt_total_bytes_max,
            upload_window: Duration::seconds(ARTIFACT_UPLOAD_WINDOW_SECONDS),
        },
    )
    .await
    .map_err(|_| internal_error("Could not record artifact"))?;

    match submitted {
        ManifestSubmitted::Accepted { upload_expires_at } => {
            let expires_at = stored_instant(&upload_expires_at, "upload_expires_at")?;
            let granted = manifest
                .iter()
                .map(|item| ArtifactGrant {
                    artifact_id: item.artifact_id.clone(),
                    state: "manifest_accepted".to_owned(),
                    upload: ArtifactUpload {
                        method: "PUT".to_owned(),
                        // Attempt-scoped — `artifact_id` alone cannot disambiguate
                        // between two different attempts that happen to choose the
                        // same runner-supplied id, and the real endpoint needs the
                        // attempt to authenticate/fence against.
                        path: format!(
                            "/api/runner/v1/attempts/{attempt_id}/artifacts/{}/content",
                            item.artifact_id
                        ),
                        expires_at: Some(expires_at),
                    },
                })
                .collect();
            message_json(&ArtifactManifestResponse {
                protocol_version: ProtocolVersion::v1(),
                attempt_id,
                artifacts: granted,
            })
        }
        ManifestSubmitted::Stale => Err(stale_lease(attempt_id.as_str())),
        ManifestSubmitted::Ineligible {
            state: attempt_state,
        } => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "Artifacts can only be recorded while the attempt is running or awaiting a decision",
            json!({"state": attempt_state.as_str()}),
        )),
        ManifestSubmitted::TooLarge => Err(payload_too_large(
            "artifact_attempt_total_bytes_max",
            LIMITS.artifact_attempt_total_bytes_max,
        )),
        ManifestSubmitted::IdempotencyConflict { artifact_id } => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::IdempotencyConflict,
            "The artifact_id was already used with different content",
            json!({"artifact_id": artifact_id}),
        )),
    }
}

// ---------------------------------------------------------------------
// Artifact content upload. Streams the request body straight to
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
    Path((attempt_id, artifact_id)): Path<(AttemptId, String)>,
    body: Body,
) -> HandlerResult {
    let now = state.clock.now();
    let principal = runner_auth::authenticate(&state.repo, &headers, now).await?;

    let fencing_token = headers
        .get(ARTIFACT_FENCING_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .and_then(fencing_token_from_wire)
        .ok_or_else(|| {
            invalid_request(
                "fencing_token",
                "The X-Tack-Fencing-Token header is required and must be a non-negative integer",
            )
        })?;

    let upload = ContentUpload {
        runner_id: &principal.runner_id,
        attempt_id: &attempt_id,
        artifact_id: &artifact_id,
        fencing_token,
        content_type: headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
    };
    let target = match tack_app::runs::submit_artifacts::check_artifact_upload(
        &state.repo,
        state.clock.now(),
        upload,
    )
    .await
    .map_err(|_| internal_error("Could not verify artifact"))?
    {
        UploadChecked::Ready(target) => target,
        // Covers every "this write is not currently valid" case in one stable code, matching
        // `submit_artifacts`'s own precedent: an unknown artifact_id, a fencing mismatch, or a
        // runner_id mismatch are all indistinguishable from a stale lease to an unauthenticated
        // caller, and must stay that way — a more specific error here would let a caller probe
        // for which of the three is true.
        UploadChecked::Stale => return Err(stale_lease(attempt_id.as_str())),
        UploadChecked::Ineligible {
            state: attempt_state,
        } => {
            return Err(protocol_error(
                StatusCode::CONFLICT,
                StableErrorCode::Conflict,
                "Artifact content can only be recorded while the attempt is running or awaiting a decision",
                json!({"state": attempt_state.as_str()}),
            ));
        }
        // Content is immutable once verified: a second PUT for the same artifact_id is refused
        // before consuming any of its body, rather than re-verified and silently discarded.
        UploadChecked::AlreadyRecorded => {
            return Err(protocol_error(
                StatusCode::CONFLICT,
                StableErrorCode::Conflict,
                "Artifact content has already been recorded and is immutable",
                json!({"artifact_id": artifact_id}),
            ));
        }
        UploadChecked::ContentTypeMismatch => {
            return Err(invalid_request(
                "content-type",
                "The upload Content-Type does not match the artifact manifest's declared media_type",
            ));
        }
    };

    let stored = state
        .artifact_storage
        .store_streaming(
            attempt_id.as_str(),
            &artifact_id,
            target.size_bytes,
            &target.sha256,
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

    let commit = tack_app::runs::submit_artifacts::commit_artifact_content(
        &state.repo,
        state.clock.now(),
        upload,
        &stored.content_reference,
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
            "sha256": target.sha256,
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
            Err(stale_lease(attempt_id.as_str()))
        }
    }
}

// ---------------------------------------------------------------------
// Completion.
// ---------------------------------------------------------------------

pub async fn submit_completion(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path(attempt_id): Path<AttemptId>,
    body: Bytes,
) -> HandlerResult {
    let (principal, value, fencing_token) =
        authenticate_attempt_request(&state, &headers, &attempt_id, &body).await?;
    let request: CompletionRequest = parse_message(value)?;
    let completion_id = request.completion_id;
    let terminal_state = Some(request.terminal_state)
        .filter(|state| {
            matches!(
                state,
                ExecutionState::Succeeded | ExecutionState::Failed | ExecutionState::Cancelled
            )
        })
        .ok_or_else(|| {
            invalid_request(
                "terminal_state",
                "terminal_state must be succeeded, failed, or cancelled",
            )
        })?;
    let Value::Object(terminal_reason) = request.terminal_reason else {
        return Err(invalid_request(
            "terminal_reason",
            "terminal_reason must be an object",
        ));
    };
    let actual_execution = request.actual_execution;
    let usage = request.usage;
    let final_event_checkpoint = request.final_event_checkpoint;

    let storage = state.artifact_storage.as_ref();
    let completed = tack_app::runs::complete::complete(
        &state.repo,
        state.clock.now(),
        CompletionReport {
            runner_id: &principal.runner_id,
            attempt_id: &attempt_id,
            fencing_token,
            completion_id: &completion_id,
            final_event_checkpoint: final_event_checkpoint.as_ref(),
            terminal_state,
            terminal_reason: &terminal_reason,
            actual_execution: &actual_execution,
            usage: &usage,
        },
        |reference: String| async move { read_pack(storage, &reference).await },
    )
    .await
    .map_err(|_| internal_error("Could not record completion"))?;

    if let Completed::Committed { item_moved, .. } = &completed {
        if let (Some(app), Some(moved)) = (state.app.as_ref(), item_moved) {
            // The board learns about the move the same way as from an edit.
            crate::handlers::websocket::broadcast_event(
                app,
                crate::handlers::websocket::BoardEvent::ItemUpdated {
                    project_id: moved.project_id,
                    item_id: moved.item_id,
                    old_status: Some(moved.old_status.clone()),
                    new_status: moved.new_status.clone(),
                },
            );
        }
        if let Some(app) = state.app.clone() {
            // The pack goes to prill's store; the runner does not wait for it, and a failure
            // is retried when the pull request is next followed.
            let attempt_id = attempt_id.clone();
            app.tasks.clone().spawn(async move {
                crate::prill_store::record_mrp(&app, attempt_id.as_str()).await
            });
        }
    }

    match completed {
        Completed::Committed { response, .. } => completion_json(attempt_id, response, false),
        Completed::Replayed(response) => completion_json(attempt_id, response, true),
        // Same split as `submit_events` above: the same
        // `(attempt_id, completion_id)` idempotency-scoped key
        // reused with different content can never succeed by retrying
        // (`idempotency_conflict`), while a distinct completion_id racing a
        // concurrent terminal write, or a pre-M055 terminal attempt with no
        // authoritative historical response, is a benign, retryable
        // `conflict`.
        Completed::IdempotencyConflict => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::IdempotencyConflict,
            "The completion_id was already used with different content",
            json!({"attempt_id": attempt_id}),
        )),
        Completed::Conflict => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "The completion could not be committed",
            json!({"attempt_id": attempt_id}),
        )),
        Completed::Stale => Err(stale_lease(attempt_id.as_str())),
    }
}

/// The answer to a completion, from the response the store kept.
fn completion_json(
    attempt_id: AttemptId,
    response: tack_db::repo::execution::CompletionResponse,
    replayed: bool,
) -> HandlerResult {
    let committed_at = DateTime::parse_from_rfc3339(&response.committed_at)
        .map_err(|_| internal_error("The stored completion time is not readable"))?
        .with_timezone(&Utc);
    message_json(&CompletionResponse {
        protocol_version: ProtocolVersion::v1(),
        attempt_id,
        completion_id: response.completion_id,
        state: response.terminal_state,
        replayed,
        committed_at,
    })
}

/// The merge-readiness pack stored at `reference`, when it can be read and parses.
async fn read_pack(
    storage: &ArtifactStorage,
    reference: &str,
) -> Option<tack_protocol::mrp::MergeReadinessPack> {
    use tokio::io::AsyncReadExt;
    let mut file = storage.open_for_read(reference).await.ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

// ---------------------------------------------------------------------
// Cancellation observation.
// ---------------------------------------------------------------------

fn cancellation_json(
    attempt_id: AttemptId,
    resp: tack_db::repo::execution::CancellationResponse,
    replayed: bool,
) -> HandlerResult {
    message_json(&CancellationObservationResponse {
        protocol_version: ProtocolVersion::v1(),
        attempt_id,
        cancellation_request_id: resp.cancellation_request_id,
        state: resp.state,
        replayed,
        committed_at: stored_instant(&resp.committed_at, "committed_at")?,
    })
}

pub async fn observe_cancellation_report(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path(attempt_id): Path<AttemptId>,
    body: Bytes,
) -> HandlerResult {
    let (principal, value, fencing_token) =
        authenticate_attempt_request(&state, &headers, &attempt_id, &body).await?;
    let CancellationObservationRequest {
        cancellation_request_id,
        observation,
        observed_at,
        details,
        ..
    } = parse_message(value)?;
    if observation != CancelObservation::ProcessStopped {
        return Err(invalid_request(
            "observation",
            "observation must be process_stopped",
        ));
    }
    let details_json =
        serde_json::to_string(&details).map_err(|_| internal_error("Could not encode details"))?;
    let observation_json = serde_json::to_string(&observation)
        .map_err(|_| internal_error("Could not encode observation"))?;

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
            state.clock.now(),
        )
        .await
        .map_err(|_| internal_error("Could not record cancellation observation"))?;

    match result {
        CancellationObservation::Cancelled(resp) => cancellation_json(attempt_id, resp, false),
        CancellationObservation::Replayed(resp) => cancellation_json(attempt_id, resp, true),
        CancellationObservation::Conflict => Err(protocol_error(
            StatusCode::CONFLICT,
            StableErrorCode::IdempotencyConflict,
            "The cancellation_request_id was already used with different content",
            json!({"cancellation_request_id": cancellation_request_id}),
        )),
        CancellationObservation::Stale => Err(stale_lease(attempt_id.as_str())),
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
// `protocol.json`, relative to `base_path`). Uses the typed
// `RecoveryObservationRequest`/`RecoveryObservationResponse` directly, so
// this handler's wire shape is exactly what is already fixture-tested.
// ---------------------------------------------------------------------

pub async fn observe_recovery(
    State(state): State<RunnerProtocolState>,
    headers: HeaderMap,
    Path(attempt_id): Path<AttemptId>,
    body: Bytes,
) -> HandlerResult {
    let now = state.clock.now();
    let principal = runner_auth::authenticate(&state.repo, &headers, now).await?;
    let value = parse_body(&body)?;
    check_protocol_version(&value)?;
    runner_auth::require_matching_runner(&value, &principal)?;
    runner_auth::require_matching_attempt(&value, &attempt_id)?;
    let request: RecoveryObservationRequest = parse_message(value)?;
    if request.attempt_id != attempt_id {
        return Err(invalid_request(
            "attempt_id",
            "attempt_id in the body must match the path",
        ));
    }
    let fencing_token = fencing_token_from_wire(request.fencing_token.get()).ok_or_else(|| {
        invalid_request(
            "fencing_token",
            "fencing_token must be a non-negative integer",
        )
    })?;
    let observed = tack_app::runs::observe_recovery::observe_recovery(
        &state.repo,
        state.clock.now(),
        Observation {
            runner_id: &principal.runner_id,
            attempt_id: &request.attempt_id,
            fencing_token,
            recovery_key: &request.recovery_key,
            observation: request.observation,
            details: &request.details,
        },
    )
    .await
    .map_err(|_| internal_error("Could not record recovery observation"))?;

    let recovered = match observed {
        RecoveryObserved::Recorded(recovered) => recovered,
        RecoveryObserved::IdempotencyConflict => {
            return Err(protocol_error(
                StatusCode::CONFLICT,
                StableErrorCode::IdempotencyConflict,
                "The recovery_key was already used with different content",
                json!({"recovery_key": request.recovery_key.as_str()}),
            ));
        }
        RecoveryObserved::Stale => return Err(stale_lease(attempt_id.as_str())),
    };
    let committed_at = DateTime::parse_from_rfc3339(&recovered.committed_at)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| internal_error("Stored committed_at is corrupt"))?;
    let response = RecoveryObservationResponse {
        protocol_version: ProtocolVersion::v1(),
        attempt_id: recovered.attempt_id,
        recovery_key: recovered.recovery_key,
        disposition: recovered.disposition,
        replayed: recovered.replayed,
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
    fn effective_body_limit_clamps_to_protocol_ceiling() {
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
            "labels_max": LIMITS.labels_max,
            "label_name_bytes_max": LIMITS.label_name_bytes_max,
            "label_value_bytes_max": LIMITS.label_value_bytes_max,
            "heartbeat_interval_seconds": LIMITS.heartbeat_interval_seconds,
            "lease_duration_seconds": LIMITS.lease_duration_seconds,
            "claim_wait_ms_max": LIMITS.claim_wait_ms_max,
            "active_attempts_per_heartbeat_max": LIMITS.active_attempts_per_heartbeat_max,
            "event_batch_count_max": LIMITS.event_batch_count_max,
            "event_payload_bytes_max": LIMITS.event_payload_bytes_max,
            "decision_prompt_bytes_max": LIMITS.decision_prompt_bytes_max,
            "decision_options_max": LIMITS.decision_options_max,
            "artifact_manifest_count_max": LIMITS.artifact_manifest_count_max,
            "artifact_metadata_bytes_max": LIMITS.artifact_metadata_bytes_max,
            "artifact_content_bytes_max": LIMITS.artifact_content_bytes_max,
            "artifact_attempt_total_bytes_max": LIMITS.artifact_attempt_total_bytes_max,
            "capabilities_bytes_max": LIMITS.capabilities_bytes_max,
        });
        let mut expected = fixture.clone();
        for unmirrored in [
            "environment_entries_max",
            "environment_name_bytes_max",
            "environment_value_bytes_max",
            "heartbeat_grace_seconds",
            "event_batch_bytes_max",
            "decision_answer_bytes_max",
            "request_timeout_seconds_max",
            "retention_event_days_default",
            "retention_artifact_days_default",
        ] {
            expected
                .as_object_mut()
                .expect("limits fixture is an object")
                .remove(unmirrored)
                .unwrap_or_else(|| panic!("limits fixture lost `{unmirrored}`"));
        }
        assert_eq!(mine, expected);
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

    /// What the handlers do with a capabilities value: read it as the protocol's snapshot,
    /// then apply the business rules.
    fn validate_capability_payload(
        capabilities: &Value,
    ) -> runner_auth::ProtocolResult<(String, i64, i64)> {
        let parsed: EmbeddedCapabilitySnapshot = parse_message(capabilities.clone())?;
        super::validate_capability_payload(&parsed)
    }

    #[test]
    fn normalize_datetime_makes_z_and_offset_suffixes_comparable() {
        let instant = |raw: &str| {
            DateTime::parse_from_rfc3339(raw)
                .unwrap()
                .with_timezone(&Utc)
        };
        let z = normalize_datetime(instant("2026-08-06T12:20:59Z"));
        let offset = normalize_datetime(instant("2026-08-06T12:20:59+00:00"));
        assert_eq!(z, offset);
    }

    #[test]
    fn a_message_that_does_not_fit_names_the_field_serde_stopped_at() {
        let (status, body) = parse_message::<DecisionCreateRequest>(json!({
            "protocol_version": 1,
            "runner_id": "r",
            "attempt_id": "a",
            "fencing_token": 7,
            "decision_id": "d",
            "kind": "k",
            "prompt": "p",
            "options": [{"option_id": "x"}],
        }))
        .unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body.0["error"]["code"], "invalid_request");
        assert_eq!(body.0["error"]["details"]["field"], "options[0].label");
        let (_, body) =
            parse_message::<EnrollmentRequest>(json!({"protocol_version": 1})).unwrap_err();
        assert_eq!(body.0["error"]["details"]["field"], "enrollment_token");
        let (_, body) = parse_message::<DecisionPollRequest>(json!({
            "protocol_version": 1,
            "runner_id": "r",
            "attempt_id": "a",
            "fencing_token": -1,
            "after": "2026-08-06T12:20:59Z",
        }))
        .unwrap_err();
        assert_eq!(body.0["error"]["details"]["field"], "fencing_token");
    }

    #[test]
    fn validate_capability_rejects_over_total_or_oversized_label() {
        // Every case below is otherwise a complete `EmbeddedCapabilitySnapshot`
        // shape (`reported_at`/`limits`/`concurrency`/`labels`/`harnesses`/
        // `features` all present) — the typed parse this handler validates
        // through rejects an incomplete shape before these business rules
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

    /// The typed parse means a payload missing a required
    /// `EmbeddedCapabilitySnapshot` field (here, `limits`) is rejected as
    /// `invalid_request` before any business rule runs — stricter than a
    /// hand-rolled check that ignored `limits`/`reported_at`/`harnesses`
    /// entirely would be.
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
    /// `features: {}`), which fails strict `RunnerCapabilities` parsing, must
    /// still validate under `EmbeddedCapabilitySnapshot`.
    #[test]
    fn validate_capability_accepts_sparse_refresh_fixtures_shape() {
        let raw = include_str!("../../../../docs/contracts/runner-v1/refresh.request.json");
        let value: Value = serde_json::from_str(raw).expect("refresh fixture");
        let capabilities = &value["capabilities"];
        assert!(validate_capability_payload(capabilities).is_ok());
    }
}
