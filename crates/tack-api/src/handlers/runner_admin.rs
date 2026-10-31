//! Operator fleet, runner, and profile handlers, mounted into the global router.

use crate::extract::{Json, Path, Query};
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    routing::{delete, get, patch, post},
};
use chrono::Duration;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tack_db::queries::{agent_fleets, agent_profiles};
use tack_db::repo::execution::{
    AddFleetMemberOutcome, EnrollmentToken, NewAgentProfile, NewRunner,
};
use tack_db::repo::execution::{AgentProfilePatch, pending_runner_input_is_valid};
use tack_db::{Repository, StoreError};
use tack_protocol::execution::{RunnerId, StableErrorCode};
use tack_protocol::{Clock, SystemClock};
use utoipa::ToSchema;
use uuid::Uuid;

use super::executions::{OperatorExecutionState, RunnerV1ErrorEnvelope, runner_is_online};
use crate::error::log_store_failure;

/// Builds the stable v1 error envelope via `ProtocolErrorEnvelope::new`,
/// which derives `retryable` from `code` (`StableErrorCode::retryable`) so it
/// can never drift from `docs/contracts/runner-v1/errors/*.json`. `details`
/// must follow the per-code shape documented in
/// `docs/contracts/runner-v1/README.md`.
fn error(
    status: StatusCode,
    code: StableErrorCode,
    message: &str,
    details: Value,
) -> (StatusCode, Json<Value>) {
    crate::handlers::runner_protocol::runner_auth::protocol_error(status, code, message, details)
}

pub fn routes(state: OperatorExecutionState) -> Router {
    Router::new()
        .route("/runner-fleets", post(create_fleet).get(list_fleets))
        .route("/runner-fleets/{fleet_id}/members", post(add_fleet_member))
        .route(
            "/runner-fleets/{fleet_id}/members/{runner_id}",
            delete(remove_fleet_member),
        )
        .route("/runners", get(list_runners))
        .route("/runners/enrollment", post(create_pending_runner))
        .route(
            "/runners/{runner_id}/enrollment-tokens/{token_id}/revoke",
            post(revoke_enrollment_token),
        )
        .route("/runners/{runner_id}/revoke", post(revoke_runner))
        .route("/agent-profiles", post(create_profile).get(list_profiles))
        .route(
            "/agent-profiles/{id}",
            patch(update_profile).delete(delete_profile),
        )
        .route("/agent-profiles/{id}/history", get(profile_history))
        .with_state(state)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateFleet {
    pub name: String,
    #[serde(default)]
    pub concurrency_limit: Option<i64>,
    #[serde(default = "empty")]
    pub default_policy: Value,
}
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateProfile {
    pub name: String,
    pub instructions: String,
    #[serde(default = "empty")]
    pub tool_policy: Value,
    #[serde(default = "empty")]
    pub limits: Value,
    /// The fleet a run with this profile goes to when it names no selector.
    #[serde(default)]
    pub fleet_id: Option<String>,
    /// One line on what the profile delivers; empty or absent is no line.
    #[serde(default)]
    pub delivers: Option<String>,
}
fn empty() -> Value {
    json!({})
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePendingRunner {
    pub name: String,
    #[serde(default = "empty")]
    pub labels: Value,
    pub total_capacity: i64,
    pub available_capacity: i64,
    #[serde(default = "empty")]
    pub capability_snapshot: Value,
    #[serde(default = "protocol_v1")]
    pub protocol_version: i64,
    #[serde(default = "default_enrollment_lifetime_seconds")]
    pub enrollment_lifetime_seconds: i64,
}

fn protocol_v1() -> i64 {
    1
}

fn default_enrollment_lifetime_seconds() -> i64 {
    60 * 60
}

fn token_hash(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateFleetResponse {
    pub protocol_version: u32,
    pub fleet_id: String,
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FleetSummary {
    pub fleet_id: String,
    pub name: String,
    pub concurrency_limit: Option<i64>,
    pub default_policy: Value,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FleetListResponse {
    pub protocol_version: u32,
    pub data: Vec<FleetSummary>,
}

/// One row of `GET /api/runners` — every column `agent_runners` carries
/// (minus credential material — see `RunnerListingRow`'s own doc comment in
/// `tack-db`), plus its current fleet roster.
#[derive(Debug, Serialize, ToSchema)]
pub struct RunnerSummary {
    pub runner_id: String,
    pub name: String,
    #[schema(example = "active")]
    pub state: String,
    /// Parsed `labels` — `null` only if the stored value is somehow not
    /// valid JSON (`labels_raw` always carries the raw stored string, so no
    /// information is lost even then).
    pub labels: Option<Value>,
    pub labels_raw: String,
    pub total_capacity: i64,
    pub available_capacity: i64,
    /// Parsed `capability_snapshot` — see `labels`' doc comment for the
    /// same "raw string always present" guarantee.
    pub capability_snapshot: Option<Value>,
    pub capability_snapshot_raw: String,
    pub protocol_version: i64,
    pub runner_version: Option<String>,
    pub last_heartbeat_at: Option<String>,
    /// Active, with a heartbeat inside the scheduler's window, as of this read.
    /// Decided with `SchedulingPolicy::is_online`; never stored.
    pub online: bool,
    pub revoked_at: Option<String>,
    pub fleet_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunnerListResponse {
    pub protocol_version: u32,
    pub data: Vec<RunnerSummary>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RevokeRunnerResponse {
    pub protocol_version: u32,
    pub runner_id: String,
    #[schema(example = "revoked")]
    pub state: String,
}

/// Response body for `POST /api/runners/enrollment`. `enrollment_token` is
/// the raw, one-time secret — never persisted, never returned again by any
/// later response.
#[derive(Debug, Serialize, ToSchema)]
pub struct CreatePendingRunnerResponse {
    pub protocol_version: u32,
    pub runner_id: String,
    pub token_id: String,
    pub enrollment_token: String,
    pub expires_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RevokeEnrollmentTokenResponse {
    pub protocol_version: u32,
    pub runner_id: String,
    pub token_id: String,
    #[schema(example = "revoked")]
    pub state: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateProfileResponse {
    pub protocol_version: u32,
    pub agent_profile_id: String,
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AgentProfileSummary {
    pub agent_profile_id: String,
    pub name: String,
    pub instructions: String,
    pub tool_policy: Value,
    pub limits: Value,
    pub kind: String,
    pub builtin: bool,
    pub summary: Option<String>,
    pub fleet_id: Option<String>,
    /// One line on what the profile delivers: "a change on a branch", "a document in a folder", "an answer".
    pub delivers: Option<String>,
    /// Read only: whether its tools can change files, derived from `tool_policy`.
    #[schema(read_only)]
    pub changes_files: bool,
    /// Read only: whether its tools can run commands, derived from `tool_policy`.
    #[schema(read_only)]
    pub runs_commands: bool,
    /// Read only: whether its tools or its network flag reach the network, derived from `tool_policy`.
    #[schema(read_only)]
    pub uses_network: bool,
}

/// Keeps a present `null` apart from an absent key: `Some(None)` clears, `None` leaves alone.
fn set_or_clear<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateProfile {
    pub name: Option<String>,
    pub instructions: Option<String>,
    pub tool_policy: Option<Value>,
    pub limits: Option<Value>,
    pub summary: Option<String>,
    /// Omitted leaves the fleet untouched; JSON `null` clears it.
    #[serde(default, deserialize_with = "set_or_clear")]
    #[schema(value_type = Option<String>)]
    pub fleet_id: Option<Option<String>>,
    /// Omitted leaves the line untouched; JSON `null` or an empty string clears it.
    #[serde(default, deserialize_with = "set_or_clear")]
    #[schema(value_type = Option<String>)]
    pub delivers: Option<Option<String>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AgentProfileListResponse {
    pub protocol_version: u32,
    pub data: Vec<AgentProfileSummary>,
}

#[utoipa::path(
    post,
    path = "/api/runner-fleets",
    tag = "execution-operator",
    request_body = CreateFleet,
    responses(
        (status = 200, description = "Fleet created", body = CreateFleetResponse),
        (status = 409, description = "conflict (name already exists)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn create_fleet(
    State(state): State<OperatorExecutionState>,
    Json(input): Json<CreateFleet>,
) -> Result<Json<CreateFleetResponse>, (StatusCode, Json<Value>)> {
    let id = format!("fleet_{}", Uuid::new_v4());
    let inserted = state
        .repo
        .create_agent_fleet(
            &id,
            &input.name,
            input.concurrency_limit,
            &input.default_policy.to_string(),
            state.clock.now(),
        )
        .await;
    match inserted {
        Ok(_) => Ok(Json(CreateFleetResponse {
            protocol_version: 1,
            fleet_id: id,
            name: input.name,
        })),
        Err(StoreError::Conflict(_)) => Err(error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "Fleet name already exists",
            json!({}),
        )),
        Err(_) => Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not create fleet",
            json!({}),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/api/runner-fleets",
    tag = "execution-operator",
    responses(
        (status = 200, description = "Every runner fleet, by name", body = FleetListResponse),
    ),
)]
pub async fn list_fleets(
    State(state): State<OperatorExecutionState>,
) -> Result<Json<FleetListResponse>, (StatusCode, Json<Value>)> {
    let rows = agent_fleets::fleets_by_name(&state.repo)
        .await
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not list fleets");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not list fleets",
                json!({}),
            )
        })?;
    let data: Vec<FleetSummary> = rows
        .into_iter()
        .map(|r| -> Result<FleetSummary, (StatusCode, Json<Value>)> {
            let policy = serde_json::from_str::<Value>(&r.default_policy).map_err(|failure| {
                tracing::error!(error = %failure, "Fleet policy is corrupt");
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    StableErrorCode::InternalError,
                    "Fleet policy is corrupt",
                    json!({}),
                )
            })?;
            Ok(FleetSummary {
                fleet_id: r.id,
                name: r.name,
                concurrency_limit: r.concurrency_limit,
                default_policy: policy,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(Json(FleetListResponse {
        protocol_version: 1,
        data,
    }))
}

/// Request body for `POST /api/runner-fleets/{fleet_id}/members`.
/// `agent_fleet_members` (migration 041) is a live scheduling *read* input
/// (`fetch_runner_scheduling_snapshot`, `fetch_fleet_concurrency`, the
/// fleet-selector claim query); this endpoint is the write path that
/// populates a fleet's roster, since an execution request can select
/// either an exact runner or a fleet.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddFleetMember {
    pub runner_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FleetMemberResponse {
    pub protocol_version: u32,
    pub fleet_id: String,
    pub runner_id: String,
    /// One of `"added"`, `"already_member"` (idempotent re-add) or
    /// `"removed"`.
    #[schema(example = "added")]
    pub state: String,
}

#[utoipa::path(
    post,
    path = "/api/runner-fleets/{fleet_id}/members",
    tag = "execution-operator",
    params(("fleet_id" = String, Path, description = "Fleet ID (opaque)")),
    request_body = AddFleetMember,
    responses(
        (status = 200, description = "Runner is now (or already was) a member of the fleet", body = FleetMemberResponse),
        (status = 404, description = "not_found (fleet or runner does not exist)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn add_fleet_member(
    State(state): State<OperatorExecutionState>,
    Path(fleet_id): Path<String>,
    Json(input): Json<AddFleetMember>,
) -> Result<Json<FleetMemberResponse>, (StatusCode, Json<Value>)> {
    let outcome = state
        .repo
        .add_fleet_member(&fleet_id, &input.runner_id, state.clock.now())
        .await
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not add fleet member");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not add fleet member",
                json!({}),
            )
        })?;
    match outcome {
        AddFleetMemberOutcome::Added => Ok(Json(FleetMemberResponse {
            protocol_version: 1,
            fleet_id,
            runner_id: input.runner_id,
            state: "added".into(),
        })),
        AddFleetMemberOutcome::AlreadyMember => Ok(Json(FleetMemberResponse {
            protocol_version: 1,
            fleet_id,
            runner_id: input.runner_id,
            state: "already_member".into(),
        })),
        AddFleetMemberOutcome::FleetNotFound => Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Fleet does not exist",
            json!({"resource": "fleet"}),
        )),
        AddFleetMemberOutcome::RunnerNotFound => Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Runner does not exist",
            json!({"resource": "runner"}),
        )),
    }
}

#[utoipa::path(
    delete,
    path = "/api/runner-fleets/{fleet_id}/members/{runner_id}",
    tag = "execution-operator",
    params(
        ("fleet_id" = String, Path, description = "Fleet ID (opaque)"),
        ("runner_id" = String, Path, description = "Runner ID (opaque)"),
    ),
    responses(
        (status = 200, description = "Runner removed from the fleet", body = FleetMemberResponse),
        (status = 404, description = "not_found (runner was not a member of this fleet)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn remove_fleet_member(
    State(state): State<OperatorExecutionState>,
    Path((fleet_id, runner_id)): Path<(String, String)>,
) -> Result<Json<FleetMemberResponse>, (StatusCode, Json<Value>)> {
    let removed = state
        .repo
        .remove_fleet_member(&fleet_id, &runner_id)
        .await
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not remove fleet member");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not remove fleet member",
                json!({}),
            )
        })?;
    if !removed {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Runner is not a member of this fleet",
            json!({"resource": "fleet_member"}),
        ));
    }
    Ok(Json(FleetMemberResponse {
        protocol_version: 1,
        fleet_id,
        runner_id,
        state: "removed".into(),
    }))
}

#[derive(Debug, Deserialize, Default, utoipa::IntoParams)]
pub struct ListRunnersQuery {
    /// Optional roster filter — a runner is included only if it is a
    /// current member of this fleet (`agent_fleet_members`).
    #[serde(default)]
    pub fleet_id: Option<String>,
}

/// `GET /api/runners[?fleet_id=]` — the read path for `agent_runners`
/// (migration 040) capacity/health/capability data. `capability_snapshot`/
/// `labels` are parsed JSON (matching every other list handler in this
/// file); a corrupt/unparseable value is reported per-runner as `null`
/// with the raw string preserved separately, rather than failing the whole
/// list for one bad row — an operator inspecting runner health must not
/// lose visibility into every *other* runner because one has a malformed
/// blob.
#[utoipa::path(
    get,
    path = "/api/runners",
    tag = "execution-operator",
    params(ListRunnersQuery),
    responses(
        (status = 200, description = "Every enrolled runner (optionally filtered to one fleet's roster)", body = RunnerListResponse),
    ),
)]
pub async fn list_runners(
    State(state): State<OperatorExecutionState>,
    Query(query): Query<ListRunnersQuery>,
) -> Result<Json<RunnerListResponse>, (StatusCode, Json<Value>)> {
    let runners = state.repo.list_runners().await.map_err(|failure| {
        tracing::error!(error = %failure, "Could not list runners");
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not list runners",
            json!({}),
        )
    })?;
    let now = state.clock.now();
    let data: Vec<RunnerSummary> = runners
        .into_iter()
        .filter(|runner| match &query.fleet_id {
            Some(fleet_id) => runner.fleet_ids.contains(fleet_id),
            None => true,
        })
        .map(|runner| {
            let labels = serde_json::from_str::<Value>(&runner.labels).ok();
            let capability_snapshot =
                serde_json::from_str::<Value>(&runner.capability_snapshot).ok();
            RunnerSummary {
                online: runner_is_online(&runner, now),
                runner_id: runner.id,
                name: runner.name,
                state: runner.state,
                labels,
                labels_raw: runner.labels,
                total_capacity: runner.total_capacity,
                available_capacity: runner.available_capacity,
                capability_snapshot,
                capability_snapshot_raw: runner.capability_snapshot,
                protocol_version: runner.protocol_version,
                runner_version: runner.runner_version,
                last_heartbeat_at: runner.last_heartbeat_at,
                revoked_at: runner.revoked_at,
                fleet_ids: runner.fleet_ids,
                created_at: runner.created_at,
                updated_at: runner.updated_at,
            }
        })
        .collect();
    Ok(Json(RunnerListResponse {
        protocol_version: 1,
        data,
    }))
}

#[utoipa::path(
    post,
    path = "/api/runners/{runner_id}/revoke",
    tag = "execution-operator",
    params(("runner_id" = String, Path, description = "Runner ID (opaque)")),
    responses(
        (status = 200, description = "Runner revoked", body = RevokeRunnerResponse),
        (status = 404, description = "not_found", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn revoke_runner(
    State(state): State<OperatorExecutionState>,
    Path(runner_id): Path<String>,
) -> Result<Json<RevokeRunnerResponse>, (StatusCode, Json<Value>)> {
    let changed = state
        .repo
        .revoke_runner(&runner_id, state.clock.now())
        .await
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not revoke runner");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not revoke runner",
                json!({}),
            )
        })?;
    if !changed {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Runner does not exist",
            json!({"resource": "runner"}),
        ));
    }
    Ok(Json(RevokeRunnerResponse {
        protocol_version: 1,
        runner_id,
        state: "revoked".into(),
    }))
}

/// Creates a pending runner and stores only a SHA-256 enrollment-token hash.
/// The raw token is deliberately emitted once here and is never readable from
/// metadata, list, revocation, or runner responses.
#[utoipa::path(
    post,
    path = "/api/runners/enrollment",
    tag = "execution-operator",
    request_body = CreatePendingRunner,
    responses(
        (status = 200, description = "Pending runner created; the raw enrollment token is returned exactly once", body = CreatePendingRunnerResponse),
        (status = 400, description = "invalid_request", body = RunnerV1ErrorEnvelope),
        (status = 409, description = "conflict (name already exists)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn create_pending_runner(
    State(state): State<OperatorExecutionState>,
    Json(input): Json<CreatePendingRunner>,
) -> Result<Json<CreatePendingRunnerResponse>, (StatusCode, Json<Value>)> {
    provision_pending_runner(&state.repo, state.clock.as_ref(), input)
        .await
        .map(Json)
        .map_err(|err| match err {
            ProvisionError::InvalidLifetime => error(
                StatusCode::BAD_REQUEST,
                StableErrorCode::InvalidRequest,
                "enrollment_lifetime_seconds must be positive",
                json!({"field": "enrollment_lifetime_seconds"}),
            ),
            ProvisionError::LifetimeOutOfRange => error(
                StatusCode::BAD_REQUEST,
                StableErrorCode::InvalidRequest,
                "enrollment_lifetime_seconds is out of range",
                json!({"field": "enrollment_lifetime_seconds"}),
            ),
            ProvisionError::LabelsNotSerializable => error(
                StatusCode::BAD_REQUEST,
                StableErrorCode::InvalidRequest,
                "labels cannot be serialized",
                json!({"field": "labels"}),
            ),
            ProvisionError::CapabilitySnapshotNotSerializable => error(
                StatusCode::BAD_REQUEST,
                StableErrorCode::InvalidRequest,
                "capability_snapshot cannot be serialized",
                json!({"field": "capability_snapshot"}),
            ),
            ProvisionError::InvalidCapacityOrWindow => error(
                StatusCode::BAD_REQUEST,
                StableErrorCode::InvalidRequest,
                "Pending runner capacity or enrollment window is invalid",
                json!({"field": "total_capacity"}),
            ),
            ProvisionError::NameConflict => error(
                StatusCode::CONFLICT,
                StableErrorCode::Conflict,
                "Runner name already exists",
                json!({}),
            ),
            ProvisionError::Internal => error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not create pending runner",
                json!({}),
            ),
        })
}

/// Failure from [`provision_pending_runner`], independent of any transport.
/// [`create_pending_runner`] maps each variant to the exact status code and
/// error envelope the HTTP route has always returned; [`provision_local_runner`]
/// surfaces the same variants to its own in-process caller instead.
#[derive(Debug, thiserror::Error)]
pub enum ProvisionError {
    #[error("enrollment_lifetime_seconds must be positive")]
    InvalidLifetime,
    #[error("enrollment_lifetime_seconds is out of range")]
    LifetimeOutOfRange,
    #[error("labels cannot be serialized")]
    LabelsNotSerializable,
    #[error("capability_snapshot cannot be serialized")]
    CapabilitySnapshotNotSerializable,
    #[error("pending runner capacity or enrollment window is invalid")]
    InvalidCapacityOrWindow,
    #[error("runner name already exists")]
    NameConflict,
    #[error("could not create pending runner")]
    Internal,
}

/// Validates `input`, generates the runner/token ids and the raw one-time
/// token, hashes it, and persists the pending runner via
/// `create_pending_runner_and_issue_token`. The raw token is returned only in
/// [`CreatePendingRunnerResponse::enrollment_token`] and is never stored —
/// only its hash is. Shared by the HTTP route (`create_pending_runner`) and
/// by in-process local provisioning (`provision_local_runner`); neither the
/// validation nor the storage differs between the two callers, only how the
/// result is reported back.
pub async fn provision_pending_runner(
    repo: &Repository,
    clock: &dyn Clock,
    input: CreatePendingRunner,
) -> Result<CreatePendingRunnerResponse, ProvisionError> {
    if input.enrollment_lifetime_seconds <= 0 {
        return Err(ProvisionError::InvalidLifetime);
    }
    let runner_id = format!("runr_{}", Uuid::new_v4());
    let token_id = format!("ent_{}", Uuid::new_v4());
    let raw_token = format!("enr_{}", Uuid::new_v4());
    let now = clock.now();
    let enrollment_lifetime = Duration::try_seconds(input.enrollment_lifetime_seconds)
        .ok_or(ProvisionError::LifetimeOutOfRange)?;
    let expires_at = now
        .checked_add_signed(enrollment_lifetime)
        .ok_or(ProvisionError::LifetimeOutOfRange)?;
    let labels = serde_json::to_string(&input.labels).map_err(|failure| {
        tracing::warn!(error = %failure, "Could not encode the runner provisioning");
        ProvisionError::LabelsNotSerializable
    })?;
    let capabilities = serde_json::to_string(&input.capability_snapshot).map_err(|failure| {
        tracing::warn!(error = %failure, "Could not encode the runner provisioning");
        ProvisionError::CapabilitySnapshotNotSerializable
    })?;
    let enrollment_token_hash = token_hash(&raw_token);
    let runner = NewRunner {
        id: &runner_id,
        name: &input.name,
        credential_hash: "pending:no-credential",
        labels: &labels,
        total_capacity: input.total_capacity,
        available_capacity: input.available_capacity,
        capability_snapshot: &capabilities,
        protocol_version: input.protocol_version,
    };
    let token = EnrollmentToken {
        id: &token_id,
        runner_id: &runner_id,
        token_hash: &enrollment_token_hash,
        expires_at,
    };
    // Out-of-bounds capacity or expiry is a genuine client input problem.
    if !pending_runner_input_is_valid(&runner, &token, clock.now()) {
        return Err(ProvisionError::InvalidCapacityOrWindow);
    }
    repo.create_pending_runner_and_issue_token(runner, token, clock.now())
        .await
        .map_err(|err| match err {
            StoreError::Conflict(_) => ProvisionError::NameConflict,
            _ => ProvisionError::Internal,
        })?;
    Ok(CreatePendingRunnerResponse {
        protocol_version: 1,
        runner_id,
        token_id,
        enrollment_token: raw_token,
        expires_at: expires_at.to_rfc3339(),
    })
}

/// Failure from [`provision_local_runner`]: either the short-lived pool it
/// opens against `database_url` could not be established, or the
/// provisioning call itself failed.
#[derive(Debug, thiserror::Error)]
pub enum ProvisionLocalRunnerError {
    #[error("could not open the database: {0}")]
    Pool(#[from] StoreError),
    #[error(transparent)]
    Provision(#[from] ProvisionError),
}

/// How many names ("This computer (host)", then "... 2", "... 3") are tried
/// before a taken name is reported as a conflict.
const LOCAL_NAME_ATTEMPTS: usize = 20;

/// The name of the runner the board itself hosts: "This computer (host)", or
/// "This computer" when the host has no name. `n` > 1 is the suffix a taken
/// name gets (a revoked runner keeps its row, and names are unique).
pub fn this_computer_name(host: &str, n: usize) -> String {
    let base = match host.trim() {
        "" => "This computer".to_owned(),
        host => format!("This computer ({host})"),
    };
    if n > 1 { format!("{base} {n}") } else { base }
}

/// Self-provisions a single local runner by calling
/// [`provision_pending_runner`] in-process against `database_url` — the same
/// URL `tack_api::server`'s boot sequence opens — instead of over HTTP.
/// Exists for `tack serve --with-runner`'s zero-touch local case: creating
/// the pending runner and minting its one-time token is an admin/bootstrap
/// concern here, not the runner protocol — the returned token is still
/// *redeemed* over real runner-v1 HTTP, unchanged. The runner is named after
/// this computer ([`this_computer_name`], `host` being its host name); a name
/// already taken (an earlier pending runner left by a crash, a revoked one)
/// gets the next number, so a retry cannot collide.
///
/// Opens and drops its own connection pool rather than reusing the
/// server's: SQLite's WAL mode makes a second, short-lived pool against the
/// same file safe — one more connection, not a competing writer.
pub async fn provision_local_runner(
    database_url: &str,
    host: &str,
) -> Result<CreatePendingRunnerResponse, ProvisionLocalRunnerError> {
    let pool = tack_db::init_pool(database_url)
        .await
        .map_err(StoreError::from)?;
    let repo = Repository::new(pool);
    let mut attempt = 1;
    loop {
        let input = CreatePendingRunner {
            name: this_computer_name(host, attempt),
            labels: empty(),
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: empty(),
            protocol_version: protocol_v1(),
            enrollment_lifetime_seconds: default_enrollment_lifetime_seconds(),
        };
        match provision_pending_runner(&repo, &SystemClock, input).await {
            Err(ProvisionError::NameConflict) if attempt < LOCAL_NAME_ATTEMPTS => attempt += 1,
            result => return Ok(result?),
        }
    }
}

/// Gives a local runner that still carries the name the board used to
/// generate for it (`local-` and a UUID) the name [`provision_local_runner`]
/// would give it now, so a runner enrolled earlier keeps its identity and
/// still reads as this computer. A runner with any other name was named by a
/// person and is left alone, as is one whose id is unknown. Returns whether a
/// runner was renamed.
pub async fn name_local_runner_after_host(
    database_url: &str,
    runner_id: &str,
    host: &str,
) -> Result<bool, ProvisionLocalRunnerError> {
    let pool = tack_db::init_pool(database_url)
        .await
        .map_err(StoreError::from)?;
    let repo = Repository::new(pool);
    for attempt in 1..=LOCAL_NAME_ATTEMPTS {
        match repo
            .rename_generated_local_runner(
                runner_id,
                &this_computer_name(host, attempt),
                SystemClock.now(),
            )
            .await
        {
            Err(StoreError::Conflict(_)) => {}
            result => return Ok(result?),
        }
    }
    Ok(false)
}

/// Whether `runner_id` still exists at `database_url` — opens and drops its
/// own short-lived pool exactly like [`provision_local_runner`] itself does,
/// against the same URL the caller's own server already opened successfully.
/// That is what makes this check authoritative rather than a guess: a `false`
/// here means this specific database has no such row, not merely that it was
/// unreachable just now.
pub async fn local_runner_id_exists(
    database_url: &str,
    runner_id: &str,
) -> Result<bool, ProvisionLocalRunnerError> {
    let pool = tack_db::init_pool(database_url)
        .await
        .map_err(StoreError::from)?;
    let repo = Repository::new(pool);
    Ok(repo
        .fetch_runner_scheduling_snapshot(&RunnerId::new(runner_id))
        .await?
        .is_some())
}

#[utoipa::path(
    post,
    path = "/api/runners/{runner_id}/enrollment-tokens/{token_id}/revoke",
    tag = "execution-operator",
    params(
        ("runner_id" = String, Path, description = "Runner ID (opaque)"),
        ("token_id" = String, Path, description = "Enrollment token ID (opaque)"),
    ),
    responses(
        (status = 200, description = "Token revoked", body = RevokeEnrollmentTokenResponse),
        (status = 404, description = "not_found", body = RunnerV1ErrorEnvelope),
        (status = 409, description = "conflict (already consumed)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn revoke_enrollment_token(
    State(state): State<OperatorExecutionState>,
    Path((runner_id, token_id)): Path<(String, String)>,
) -> Result<Json<RevokeEnrollmentTokenResponse>, (StatusCode, Json<Value>)> {
    state
        .repo
        .revoke_runner_enrollment_token(&runner_id, &token_id, state.clock.now())
        .await
        .map_err(|err| match err {
            StoreError::Conflict(_) => error(
                StatusCode::CONFLICT,
                StableErrorCode::Conflict,
                "Enrollment token was already consumed",
                json!({}),
            ),
            StoreError::NotFound { .. } => error(
                StatusCode::NOT_FOUND,
                StableErrorCode::NotFound,
                "Enrollment token does not exist",
                json!({"resource": "enrollment_token"}),
            ),
            err @ (StoreError::Sql(_) | StoreError::Corrupt { .. }) => {
                log_store_failure(&err);
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    StableErrorCode::InternalError,
                    "Could not revoke enrollment token",
                    json!({}),
                )
            }
        })?;
    Ok(Json(RevokeEnrollmentTokenResponse {
        protocol_version: 1,
        runner_id,
        token_id,
        state: "revoked".into(),
    }))
}

#[utoipa::path(
    post,
    path = "/api/agent-profiles",
    tag = "execution-operator",
    request_body = CreateProfile,
    responses(
        (status = 200, description = "Agent profile created", body = CreateProfileResponse),
        (status = 409, description = "conflict (name already exists)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn create_profile(
    State(state): State<OperatorExecutionState>,
    Json(input): Json<CreateProfile>,
) -> Result<Json<CreateProfileResponse>, (StatusCode, Json<Value>)> {
    check_profile_fields(
        &state.repo,
        Some(&input.tool_policy),
        input.fleet_id.as_deref(),
        input.delivers.as_deref(),
    )
    .await?;
    check_limits(&input.limits)?;
    let id = format!("ap_{}", Uuid::new_v4());
    let tool_policy = serde_json::to_string(&input.tool_policy).map_err(|failure| {
        tracing::warn!(error = %failure, "tool_policy cannot be serialized");
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "tool_policy cannot be serialized",
            json!({"field": "tool_policy"}),
        )
    })?;
    let limits = serde_json::to_string(&input.limits).map_err(|failure| {
        tracing::warn!(error = %failure, "limits cannot be serialized");
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "limits cannot be serialized",
            json!({"field": "limits"}),
        )
    })?;
    let delivers = input
        .delivers
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty());
    let result = state
        .repo
        .create_agent_profile_placed(
            NewAgentProfile {
                id: &id,
                name: &input.name,
                instructions: &input.instructions,
                tool_policy: &tool_policy,
                limits: &limits,
            },
            input.fleet_id.as_deref(),
            delivers,
            state.clock.now(),
        )
        .await;
    match result {
        Ok(_) => Ok(Json(CreateProfileResponse {
            protocol_version: 1,
            agent_profile_id: id,
            name: input.name,
        })),
        Err(StoreError::Conflict(_)) => Err(error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            "Agent profile name already exists",
            json!({}),
        )),
        Err(_) => Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not create agent profile",
            json!({}),
        )),
    }
}

#[utoipa::path(
    get,
    path = "/api/agent-profiles",
    tag = "execution-operator",
    responses(
        (status = 200, description = "Every agent profile, by name", body = AgentProfileListResponse),
    ),
)]
pub async fn list_profiles(
    State(state): State<OperatorExecutionState>,
) -> Result<Json<AgentProfileListResponse>, (StatusCode, Json<Value>)> {
    let rows = agent_profiles::profiles_by_name(&state.repo)
        .await
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not list agent profiles");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not list agent profiles",
                json!({}),
            )
        })?;
    let data = rows
        .into_iter()
        .map(profile_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(AgentProfileListResponse {
        protocol_version: 1,
        data,
    }))
}

fn profile_from_row(
    r: agent_profiles::AgentProfileRow,
) -> Result<AgentProfileSummary, (StatusCode, Json<Value>)> {
    let corrupt = |what: &str| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            what,
            json!({}),
        )
    };
    let tool_policy: Value = serde_json::from_str(&r.tool_policy).map_err(|failure| {
        tracing::error!(error = %failure, "Agent profile policy is corrupt");
        corrupt("Agent profile policy is corrupt")
    })?;
    let caps = tack_core::run_defaults::profile_capabilities(&tool_policy);
    Ok(AgentProfileSummary {
        agent_profile_id: r.id,
        name: r.name,
        instructions: r.instructions,
        tool_policy,
        limits: serde_json::from_str(&r.limits).map_err(|failure| {
            crate::error::log_unreadable_document("Agent profile limits are corrupt", &failure);
            corrupt("Agent profile limits are corrupt")
        })?,
        kind: r.kind,
        builtin: r.builtin != 0,
        summary: r.summary,
        fleet_id: r.fleet_id,
        delivers: r.delivers,
        changes_files: caps.changes_files,
        runs_commands: caps.runs_commands,
        uses_network: caps.uses_network,
    })
}

/// The agent kinds a runner can run: what `tool_policy.agent` may name.
pub(crate) const HARNESS_KINDS: [&str; 4] = ["claude-code", "codex", "opencode", "docket"];

/// A profile's `limits` through the one rule (`tack_core::run_defaults::check_limits`).
fn check_limits(limits: &Value) -> Result<(), (StatusCode, Json<Value>)> {
    tack_core::run_defaults::check_limits(limits).map_err(|message| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            &message,
            json!({"field": "limits"}),
        )
    })
}

/// 400 when `delivers` is not one short line, `tool_policy.agent` is not a known agent kind, `tool_policy.network` is not a boolean, `tool_policy.docket.team` is not docket's project id, or `fleet_id` names no fleet.
async fn check_profile_fields(
    repo: &Repository,
    tool_policy: Option<&Value>,
    fleet_id: Option<&str>,
    delivers: Option<&str>,
) -> Result<(), (StatusCode, Json<Value>)> {
    if let Some(Err(message)) = delivers.map(tack_core::run_defaults::check_delivers) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            &message,
            json!({"field": "delivers"}),
        ));
    }
    if let Some(agent) = tool_policy.and_then(|p| p.get("agent"))
        && !agent.as_str().is_some_and(|a| HARNESS_KINDS.contains(&a))
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            &format!(
                "tool_policy.agent must be one of {}",
                HARNESS_KINDS.join(", ")
            ),
            json!({"field": "tool_policy.agent"}),
        ));
    }
    if let Some(policy) = tool_policy
        && let Err(message) = tack_core::run_defaults::check_network(policy)
    {
        return Err(error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            &message,
            json!({"field": "tool_policy.network"}),
        ));
    }
    if let Some(team) = tool_policy.and_then(|p| p.get("docket")?.get("team")) {
        let valid = team.as_str().is_some_and(|t| {
            t.len() <= 64
                && t.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
                && t.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        });
        if !valid {
            return Err(error(
                StatusCode::BAD_REQUEST,
                StableErrorCode::InvalidRequest,
                "tool_policy.docket.team must match ^[a-z0-9][a-z0-9-]{0,63}$",
                json!({"field": "tool_policy.docket.team"}),
            ));
        }
    }
    if let Some(fleet_id) = fleet_id {
        let exists = agent_fleets::fleet_exists(repo, fleet_id)
            .await
            .map_err(|failure| {
                tracing::error!(error = %failure, "Could not verify fleet");
                profile_db_error("Could not verify fleet")
            })?;
        if !exists {
            return Err(error(
                StatusCode::BAD_REQUEST,
                StableErrorCode::InvalidRequest,
                "fleet_id names no fleet",
                json!({"field": "fleet_id"}),
            ));
        }
    }
    Ok(())
}

fn profile_not_found() -> (StatusCode, Json<Value>) {
    error(
        StatusCode::NOT_FOUND,
        StableErrorCode::NotFound,
        "Agent profile does not exist",
        json!({"resource": "agent_profile"}),
    )
}

fn profile_db_error(what: &str) -> (StatusCode, Json<Value>) {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        StableErrorCode::InternalError,
        what,
        json!({}),
    )
}

#[utoipa::path(
    patch,
    path = "/api/agent-profiles/{id}",
    tag = "execution-operator",
    params(("id" = String, Path, description = "Agent profile id")),
    request_body = UpdateProfile,
    responses(
        (status = 200, description = "Agent profile updated", body = AgentProfileSummary),
        (status = 404, description = "not_found", body = RunnerV1ErrorEnvelope),
        (status = 409, description = "conflict (name already exists)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn update_profile(
    State(state): State<OperatorExecutionState>,
    Path(id): Path<String>,
    Json(input): Json<UpdateProfile>,
) -> Result<Json<AgentProfileSummary>, (StatusCode, Json<Value>)> {
    check_profile_fields(
        &state.repo,
        input.tool_policy.as_ref(),
        input.fleet_id.as_ref().and_then(|f| f.as_deref()),
        input.delivers.as_ref().and_then(|d| d.as_deref()),
    )
    .await?;
    if let Some(limits) = &input.limits {
        check_limits(limits)?;
    }
    let tool_policy = input.tool_policy.map(|v| v.to_string());
    let limits = input.limits.map(|v| v.to_string());
    let delivers = input
        .delivers
        .map(|value| value.map(|d| d.trim().to_owned()).filter(|d| !d.is_empty()));
    let patch = AgentProfilePatch {
        name: input.name.as_deref(),
        instructions: input.instructions.as_deref(),
        tool_policy: tool_policy.as_deref(),
        limits: limits.as_deref(),
        summary: input.summary.as_deref(),
        fleet_id: input.fleet_id.as_ref().map(|f| f.as_deref()),
        delivers: delivers.as_ref().map(|d| d.as_deref()),
    };
    state
        .repo
        .update_agent_profile(&id, patch, state.clock.now())
        .await
        .map_err(|err| match err {
            StoreError::NotFound { .. } => profile_not_found(),
            StoreError::Conflict(_) => error(
                StatusCode::CONFLICT,
                StableErrorCode::Conflict,
                "Agent profile name already exists",
                json!({}),
            ),
            err @ (StoreError::Sql(_) | StoreError::Corrupt { .. }) => {
                log_store_failure(&err);
                profile_db_error("Could not update agent profile")
            }
        })?;
    let row = agent_profiles::profile_by_id(&state.repo, &id)
        .await
        .ok()
        .flatten()
        .ok_or_else(|| profile_db_error("Could not read agent profile"))?;
    Ok(Json(profile_from_row(row)?))
}

/// What a profile's last successful runs counted, and the limit proposed from them.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProfileHistory {
    /// Successful runs measured, at most the last 20.
    pub n: usize,
    pub p50: Option<i64>,
    pub p90: Option<i64>,
    pub min: Option<i64>,
    pub max: Option<i64>,
    /// 1.5 times the 90th percentile, rounded up to two figures; `null` under 5 runs.
    pub proposal: Option<i64>,
    /// How many successful runs it takes before a limit is proposed.
    pub runs_needed: usize,
}

#[utoipa::path(
    get,
    path = "/api/agent-profiles/{id}/history",
    tag = "execution-operator",
    params(("id" = String, Path, description = "Agent profile id")),
    responses(
        (status = 200, description = "The counted tokens of the profile's last successful runs", body = ProfileHistory),
        (status = 404, description = "not_found", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn profile_history(
    State(state): State<OperatorExecutionState>,
    Path(id): Path<String>,
) -> Result<Json<ProfileHistory>, (StatusCode, Json<Value>)> {
    let exists = agent_profiles::profile_exists(&state.repo, &id)
        .await
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not read agent profile");
            profile_db_error("Could not read agent profile")
        })?;
    if !exists {
        return Err(profile_not_found());
    }
    let mut runs = agent_profiles::successful_run_tokens(&state.repo, &id, 20)
        .await
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not read the profile's runs");
            profile_db_error("Could not read the profile's runs")
        })?;
    runs.sort_unstable();
    let p90 = tack_core::run_defaults::percentile(&runs, 90);
    Ok(Json(ProfileHistory {
        n: runs.len(),
        p50: tack_core::run_defaults::percentile(&runs, 50),
        p90,
        min: runs.first().copied(),
        max: runs.last().copied(),
        proposal: p90
            .filter(|_| runs.len() >= tack_core::run_defaults::RUNS_BEFORE_PROPOSAL)
            .map(tack_core::run_defaults::proposed_limit),
        runs_needed: tack_core::run_defaults::RUNS_BEFORE_PROPOSAL,
    }))
}

#[utoipa::path(
    delete,
    path = "/api/agent-profiles/{id}",
    tag = "execution-operator",
    params(("id" = String, Path, description = "Agent profile id")),
    responses(
        (status = 200, description = "Agent profile deleted"),
        (status = 404, description = "not_found", body = RunnerV1ErrorEnvelope),
        (status = 409, description = "conflict (built-in profile, details.reason = builtin_profile)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn delete_profile(
    State(state): State<OperatorExecutionState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    state
        .repo
        .delete_agent_profile(&id)
        .await
        .map_err(|err| match err {
            StoreError::NotFound { .. } => profile_not_found(),
            StoreError::Conflict(_) => error(
                StatusCode::CONFLICT,
                StableErrorCode::Conflict,
                "A built-in agent profile cannot be deleted",
                json!({"reason": "builtin_profile"}),
            ),
            err @ (StoreError::Sql(_) | StoreError::Corrupt { .. }) => {
                log_store_failure(&err);
                profile_db_error("Could not delete agent profile")
            }
        })?;
    Ok(Json(json!({"agent_profile_id": id, "deleted": true})))
}
