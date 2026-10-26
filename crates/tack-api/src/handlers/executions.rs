//! Operator execution handlers, mounted into the global router.

use std::collections::BTreeMap;
use std::sync::Arc;
use tack_protocol::Clock;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    http::StatusCode,
    routing::{get, post},
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tack_db::queries::{
    agent_profiles, attempt_reviews, autonomy, budgets, execution_attempts, execution_requests,
    pull_requests,
};
use tack_db::{Repository, StoreError, repo::execution::RunnerListingRow};
use tack_orch::scheduler::select::SchedulingPolicy;
use tack_protocol::execution::{ProtocolErrorEnvelope, StableErrorCode};
use tack_protocol::usage_provenance::{ModelProvenance, UsageEconomics, derive_attempt_facts};
use utoipa::ToSchema;
use uuid::Uuid;

use tack_core::run_defaults::Resolved;

use crate::AppState;
use tack_app::AppError;
use tack_app::runs::cancel_run::{Cancellation, cancel_run};
use tack_app::runs::requeue::requeue;
use tack_app::runs::start_run::{
    self, LimitSource, Refusal, RunRequest, Started, compose_instructions, profile_source_label,
    start_run, unset_tokens,
};

/// Documents `tack_protocol::execution::ProtocolErrorEnvelope`'s real wire shape
/// (`docs/contracts/runner-v1/errors/*.json`) for every operator
/// execution/fleet/runner/profile route — not `crate::openapi::ErrorEnvelope`,
/// a different, incompatible shape. A doc-only mirror, not a second runtime
/// authority: `tack-orch` must stay free of an OpenAPI dependency, so the
/// real type cannot derive `ToSchema` itself. Defined here rather than in
/// `crate::openapi` so this stays colocated with the routes it documents.
/// `code` is a free string, not an enum, for the same reason — its fifteen
/// frozen values are enumerated in `docs/contracts/runner-v1/README.md`.
/// Referenced only by *type* (utoipa's `ToSchema`), never constructed as a
/// value — real responses build the runtime type instead.
#[derive(Debug, Serialize, ToSchema)]
pub struct RunnerV1ErrorEnvelope {
    pub error: RunnerV1Error,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RunnerV1Error {
    /// One of the fifteen frozen `docs/contracts/runner-v1/errors/*.json`
    /// codes (e.g. `not_found`, `conflict`, `idempotency_conflict`,
    /// `invalid_transition`, `stale_lease`, `runner_revoked`).
    #[schema(example = "not_found")]
    pub code: String,
    pub message: String,
    pub request_id: String,
    /// Whether a conformant client may safely retry — derived from `code`
    /// alone (`StableErrorCode::retryable`), never set independently.
    pub retryable: bool,
    /// Per-code structured detail — shape documented per operation in
    /// `docs/contracts/runner-v1/README.md` (e.g. `invalid_transition`'s
    /// `{from, to}`, `stale_lease`'s `{attempt_id, current_fencing_token}`).
    pub details: Value,
}

/// State for the operator execution router — constructed from the shared
/// API state's repository and clock when mounted in `router.rs`.
#[derive(Clone)]
pub struct OperatorExecutionState {
    pub repo: Repository,
    pub clock: Arc<dyn Clock>,
    /// The shared API state, when production wires it in: accepting an attempt
    /// that pushed a branch uses it to open the pull request.
    pub app: Option<crate::router::AppState>,
}

impl OperatorExecutionState {
    pub fn with_clock(repo: Repository, clock: Arc<dyn Clock>) -> Self {
        Self {
            repo,
            clock,
            app: None,
        }
    }

    /// Hands the shared API state to the review handler.
    pub fn with_app_state(mut self, app: crate::router::AppState) -> Self {
        self.app = Some(app);
        self
    }
}

/// Static request-correlation id placeholder. No per-request correlation
/// id is wired into these error envelopes yet.
const OPERATOR_REQUEST_ID: &str = "req_operator";

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
    let envelope = ProtocolErrorEnvelope::new(code, message, OPERATOR_REQUEST_ID, details);
    (
        status,
        Json(serde_json::to_value(envelope).expect("envelope serializes")),
    )
}

fn principal(headers: &HeaderMap) -> Result<String, (StatusCode, Json<Value>)> {
    headers
        .get("x-tack-principal")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            error(
                StatusCode::UNAUTHORIZED,
                StableErrorCode::Unauthorized,
                "An authenticated operator principal is required",
                json!({}),
            )
        })
}

fn parse_rfc3339(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

/// Deserializes a key's value, `null` included, so its presence is detectable.
fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct CreateExecution {
    pub item_id: Uuid,
    pub idempotency_key: String,
    /// `exact_runner` or `fleet`. Omitted (with `selector_id`), the agent profile's fleet,
    /// else the only active runner; with neither, `select_where_this_runs`.
    #[serde(default)]
    pub selector_kind: Option<String>,
    #[serde(default)]
    pub selector_id: Option<String>,
    /// Omitted, the server picks one: run settings, the item's role, the project's
    /// default, the implementer, and records which in `metadata.profile_source`.
    #[serde(default)]
    pub agent_profile_id: Option<String>,
    /// Omitted, the resolved profile's agent (`tool_policy.agent`), else the
    /// project's default agent; with neither, `select_an_agent`.
    #[serde(default)]
    pub requested_harness_kind: Option<String>,
    /// The one model string the agent is asked to use, passed verbatim.
    /// Absent, the project's default model applies when this run's agent is
    /// the project's default agent; otherwise the agent chooses.
    #[serde(default)]
    pub requested_model: Option<String>,
    /// Removed: the server records the profile it loads by `agent_profile_id`.
    /// A request that still carries the key is refused (`field_removed`).
    #[serde(default, deserialize_with = "present", skip_serializing)]
    #[schema(ignore)]
    pub agent_profile_snapshot: Option<Value>,
    /// `tack_protocol::execution::RepositorySnapshot` (`{kind, remote,
    /// base_revision, subdirectory}`). Omitted (or `{}`) the server fills it
    /// from the item's project. An explicit one must name the project's own
    /// repository (`details.reason = "repository_differs_from_project"`) and
    /// may only add `base_revision` and `subdirectory`; a project with no code
    /// is `invalid_request` (`details.reason = "project_has_no_code"`).
    #[serde(default)]
    pub repository_snapshot: Option<Value>,
    /// `tack_protocol::execution::PermissionPolicy` (`{tools, network,
    /// approvals}`). `network` absent: the resolved profile's `tool_policy.network`, else
    /// off; `true` or `false` is kept as sent. `approvals` is `"auto"` or `"ask"`; absent means
    /// `"auto"`. `"ask"` is rejected at scheduling time for a runner whose
    /// matched harness does not attest `decisions: supported`.
    pub permission_policy: Value,
    /// A run's limits. `tokens` absent: the profile's `limits.budgets.tokens`, else no limit;
    /// `null`: no limit for this run even when the profile has one; a positive integer: that
    /// many. `0`, a negative number or a non-number is refused (`invalid_request`, field
    /// `budgets.tokens`). `cost_usd` was removed and is refused (`field_removed`).
    pub budgets: Value,
    pub environment: Value,
    pub metadata: Value,
    /// Absent: the profile's `limits.timeout_seconds`, else the default time limit.
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    /// `done_on_success` or `done_on_mrp_accepted`; any other id is rejected
    /// with `invalid_request`. Omitted leaves the item's status untouched.
    #[serde(default)]
    pub status_map_policy_id: Option<String>,
    /// `false` declines the runner's verifier for this run. The runner's own
    /// configuration decides whether a verifier exists; `true` never enables one.
    #[serde(default)]
    pub verify: Option<bool>,
    /// `false` declines the runner's branch push for this run. The runner's own
    /// configuration decides whether a push exists; `true` never enables one.
    #[serde(default)]
    pub push_branch: Option<bool>,
    /// `mcp` marks a run an agent started through `tack mcp`: it is never merged
    /// automatically or by a move to Done, only by the Merge button. Any other
    /// value, or none, means a person started it.
    #[serde(default)]
    pub origin: Option<String>,
}

/// Response body for `POST /api/executions` — a newly created request or an
/// idempotent replay of an existing one (`replayed` distinguishes the two).
#[derive(Debug, Serialize, ToSchema)]
pub struct CreateExecutionResponse {
    pub protocol_version: u32,
    pub request_id: String,
    /// Always `"queued"` on success — a request enters the runner-v1
    /// lifecycle (`docs/contracts/runner-v1/lifecycle-transitions.json`)
    /// only once created.
    pub state: String,
    pub replayed: bool,
}

/// One row of `GET /api/executions` / the `GET /api/executions/{id}`
/// detail. Deliberately five scalar columns today — attempt/event detail
/// is not carried here; it is available separately via `GET
/// /api/executions/{id}/attempts`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ExecutionSummary {
    pub request_id: String,
    pub item_id: String,
    pub state: String,
    pub cancellation_requested_at: Option<String>,
    pub created_at: String,
    /// Why no runner has claimed this request yet, as a plain sentence; null once claimed.
    pub waiting_reason: Option<String>,
    /// When a runner's poll first found the request unclaimable; null once claimed.
    pub waiting_since: Option<String>,
    /// The run's token limit and who applies it.
    pub limit: RunLimit,
}

/// A run's token limit, who set it and who applies it, with the one sentence every screen prints.
#[derive(Debug, Serialize, ToSchema)]
pub struct RunLimit {
    /// Absent when the run has no limit set in Tack.
    pub tokens: Option<u64>,
    /// `profile`, `provisional`, `run`, `day` or `none`.
    pub source: String,
    /// Who stops the run at the limit: `docket` stops the task itself; `claude-code` and
    /// `opencode`: the runner stops the agent when its counted tokens reach it; `measured`: Tack
    /// reads the agent's tokens and cannot stop it.
    pub enforced_by: String,
    pub sentence: String,
    /// What the run's latest attempt has counted so far, live while it runs; absent before any attempt.
    pub used: Option<u64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExecutionListResponse {
    pub protocol_version: u32,
    pub data: Vec<ExecutionSummary>,
}

/// Query parameters for `GET /api/executions`. Three read shapes, in
/// precedence order: `item_ids` (comma-separated) asks for the single
/// latest execution per item, for a batch of items — the shape a
/// Board/Sprint screen with many cards needs, one request regardless of
/// how many cards are on screen, and wins over `item_id`/`limit` when
/// present; else `item_id` scopes every request for one item; else the
/// unscoped, install-wide list, bounded by `limit`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListExecutionsQuery {
    pub item_id: Option<Uuid>,
    /// Comma-separated item ids — returns exactly one row per id, its own
    /// most recent execution. Takes precedence over `item_id`/`limit` when
    /// present.
    //
    // A plain string field, not `Vec<Uuid>`: axum's `Query` extractor
    // deserializes query parameters via `serde_urlencoded`, which cannot
    // parse a repeated same-named key into a sequence field (confirmed
    // against this exact version — `serde_urlencoded::from_str` on a
    // repeated key errors "invalid type: string ..., expected a
    // sequence"). Parsed by `ListExecutionsQuery::parsed_item_ids`, not at
    // deserialize time, so a malformed id surfaces as this route's own
    // contract-shaped `invalid_request` error instead of axum's generic
    // query-rejection response.
    #[param(
        value_type = Option<String>,
        example = "11111111-1111-1111-1111-111111111111,22222222-2222-2222-2222-222222222222"
    )]
    #[serde(default)]
    pub item_ids: Option<String>,
    pub limit: Option<u32>,
}

impl ListExecutionsQuery {
    /// Applied when `limit` is omitted.
    pub const DEFAULT_LIMIT: u32 = 200;
    /// Hard cap even when a caller asks for more — also the ceiling on how
    /// many ids `item_ids` may carry, so a pathologically long batch can't
    /// force an unbounded `IN (...)`.
    pub const MAX_LIMIT: u32 = 2000;

    fn effective_limit(&self) -> i64 {
        i64::from(
            self.limit
                .unwrap_or(Self::DEFAULT_LIMIT)
                .min(Self::MAX_LIMIT),
        )
    }

    /// Parses `item_ids` into a list of `Uuid`s, `None` when the parameter
    /// was absent or empty. `Err` names the malformed fragment verbatim
    /// (never silently dropped) or, when the list is longer than
    /// `MAX_LIMIT`, a message naming the count — both are surfaced by the
    /// caller as `invalid_request`.
    fn parsed_item_ids(&self) -> Result<Option<Vec<Uuid>>, String> {
        let Some(raw) = self.item_ids.as_deref().map(str::trim) else {
            return Ok(None);
        };
        if raw.is_empty() {
            return Ok(None);
        }
        let ids = raw
            .split(',')
            .map(|part| {
                let part = part.trim();
                Uuid::parse_str(part).map_err(|_| part.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        if ids.len() > Self::MAX_LIMIT as usize {
            return Err(format!(
                "item_ids carries {} ids, more than the {} allowed",
                ids.len(),
                Self::MAX_LIMIT
            ));
        }
        Ok(Some(ids))
    }
}

/// Terminal reason from a runner attempt: `code`, `message`, and optional
/// `artifact`/`artifacts` on success; `workspace_kept_at` after a completed
/// recovery. The wire shape is fully documented by runner contracts; this
/// struct mirrors that contract, with a flattened `additional` field for
/// extensibility. All fields are optional; a fresh attempt carries no
/// `terminal_reason` until it reports one.
#[derive(Debug, Default, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct TerminalReason {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_kept_at: Option<String>,
    /// Additional fields not explicitly typed, preserved from the wire
    /// (extensibility for future runner protocol versions).
    #[serde(flatten)]
    pub additional: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ExecutionDetailResponse {
    pub protocol_version: u32,
    pub request_id: String,
    pub item_id: String,
    pub state: String,
    pub cancellation_requested_at: Option<String>,
    pub created_at: String,
    /// The composed prompt the agent is given, as stored with the request; null when absent.
    pub instructions: Option<String>,
    /// Why no runner has claimed this request yet, as a plain sentence; null once claimed.
    pub waiting_reason: Option<String>,
    /// When a runner's poll first found the request unclaimable; null once claimed.
    pub waiting_since: Option<String>,
    /// The run's token limit and who applies it.
    pub limit: RunLimit,
}

/// One attempt as reported by `GET /api/executions/{id}/attempts` — every
/// column `execution_attempts` carries (migration 045).
#[derive(Debug, Serialize, ToSchema)]
pub struct AttemptSummary {
    pub attempt_id: String,
    pub request_id: String,
    pub attempt_number: i64,
    pub runner_id: String,
    pub fencing_token: i64,
    pub state: String,
    pub lease_issued_at: String,
    pub lease_expires_at: String,
    pub last_heartbeat_at: Option<String>,
    pub event_checkpoint: Option<String>,
    pub completion_id: Option<String>,
    pub workspace_id: Option<String>,
    pub base_revision: Option<String>,
    /// Names (never values) of the environment variables the runner gave the
    /// harness, as it reported at start; empty when it reported none.
    pub environment_names: Vec<String>,
    /// What the harness ran inside, as the runner reported at start: `bwrap`
    /// or `none`; `null` when it did not say.
    pub sandbox: Option<String>,
    /// `tack_protocol::execution::ActualExecution` once the attempt has
    /// reported one, else `null`.
    pub actual_execution: Option<Value>,
    pub terminal_reason: Option<TerminalReason>,
    /// `tack_protocol::execution::Usage` once reported, else `null` — never a
    /// fabricated zero.
    pub usage: Option<Value>,
    /// The tokens counted against a limit: new input plus output, cache reads
    /// left out (`Usage::counted`). `null` when nothing was measured, which is
    /// not zero.
    pub counted_tokens: Option<u64>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// `tack_protocol::usage_provenance::ModelProvenance` (`matched` /
    /// `auto_select_observed` / `mismatched`), or `null` when the attempt
    /// has not yet reported `actual_execution` — populated by
    /// `derive_attempt_facts`. Runtime type stays untyped `Value`;
    /// `#[schema(...)]` below points the *generated OpenAPI document* at the
    /// real type instead of the untyped-`Value` default, which would
    /// generate an empty `{}` schema.
    #[schema(value_type = Option<ModelProvenance>, nullable)]
    pub model_provenance: Option<Value>,
    /// `tack_protocol::usage_provenance::UsageEconomics` — two
    /// independently-provenanced dollar dimensions, never summed. Absent
    /// usage/timestamps serialize as `{"value": null, "source":
    /// "not_measured"}`, never a structural zero; no runner infra cost-rate
    /// is stored anywhere in this schema today, so
    /// `runner_time_cost.cost_usd_estimated` is always `not_measured` in
    /// every real response from this endpoint. `#[schema(...)]` below
    /// points the generated document at the real type, the same fix as
    /// `model_provenance` above.
    #[schema(value_type = UsageEconomics)]
    pub usage_economics: Value,
    /// The pull request this attempt opened on GitHub, or `null`.
    pub pull_request: Option<AttemptPullRequest>,
    /// The operator's verdict on this attempt, or `null` while unreviewed.
    pub review: Option<AttemptReview>,
    /// Whether the Merge button may be pressed, and why not.
    pub merge: AttemptMerge,
}

/// The Merge button's state: `reason` says why it is disabled, `null` when enabled.
#[derive(Debug, Serialize, ToSchema)]
pub struct AttemptMerge {
    pub enabled: bool,
    /// Why the button is disabled; when it is enabled, what held its last press
    /// (CI is read on the press, not to draw the button).
    pub reason: Option<String>,
    /// Who merged the attempt and under which policy, or `null` while it has not merged.
    pub merged: Option<MergedBy>,
}

/// The merge that happened for an attempt: `trigger` is what asked for it
/// (`button`, `done`, `auto`), `merge_sha` the commit it landed on.
#[derive(Debug, Serialize, ToSchema)]
pub struct MergedBy {
    pub trigger: String,
    pub level: Option<String>,
    pub domain: Option<String>,
    pub merged_at: String,
    pub merge_sha: Option<String>,
    pub policy_snapshot: Option<serde_json::Value>,
}

/// An operator's verdict on one attempt: `verdict` is `accepted` or `rejected`.
#[derive(Debug, Serialize, ToSchema)]
pub struct AttemptReview {
    pub verdict: String,
    pub note: Option<String>,
    pub reviewed_at: String,
    /// The pull request an accepted attempt opened, when it opened one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pull_request_url: Option<String>,
}

/// The pull request an attempt opened: `state` is `open`, `merged`, `closed` or
/// `reverted`.
#[derive(Debug, Serialize, ToSchema)]
pub struct AttemptPullRequest {
    pub number: i64,
    pub url: String,
    pub state: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AttemptListResponse {
    pub protocol_version: u32,
    pub data: Vec<AttemptSummary>,
}

/// One event as reported by `GET
/// /api/executions/{id}/attempts/{attempt_number}/events`.
#[derive(Debug, Serialize, ToSchema)]
pub struct EventSummary {
    pub event_id: String,
    pub sequence: i64,
    pub source: String,
    pub kind: String,
    pub payload: Value,
    pub occurred_at: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EventListResponse {
    pub protocol_version: u32,
    pub data: Vec<EventSummary>,
}

/// Response body for `POST /api/executions/{id}/cancel`. `state` is
/// `cancelled` when nobody held the run (its attempt was waiting on a person,
/// or its lease had expired) and the request ended at once. Otherwise it is
/// `cancellation_requested`, deliberately **not** a
/// `docs/contracts/runner-v1/lifecycle-transitions.json` value — the request
/// is recorded (`cancellation_requested_at`) and the runner holding the run
/// reports the outcome, visible via `GET /api/executions/{id}`.
#[derive(Debug, Serialize, ToSchema)]
pub struct CancellationRequestedResponse {
    pub protocol_version: u32,
    pub request_id: String,
    #[schema(example = "cancellation_requested")]
    pub state: String,
}

/// Response body for `POST /api/executions/{id}/requeue`.
#[derive(Debug, Serialize, ToSchema)]
pub struct RequeueResponse {
    pub protocol_version: u32,
    pub request_id: String,
    #[schema(example = "queued")]
    pub state: String,
    #[schema(example = "needs_operator")]
    pub recovered_from: String,
    pub replayed: bool,
}

pub fn routes(state: OperatorExecutionState) -> Router {
    Router::new()
        .route("/executions", post(create_execution).get(list_executions))
        .route("/executions/{request_id}", get(get_execution))
        .route(
            "/executions/{request_id}/attempts",
            get(list_execution_attempts),
        )
        .route(
            "/executions/{request_id}/attempts/{attempt_number}/events",
            get(list_execution_attempt_events),
        )
        .route("/attempts/{attempt_id}/merge", post(merge_attempt))
        .route(
            "/executions/{request_id}/cancel",
            post(request_cancellation),
        )
        .route(
            "/executions/{request_id}/requeue",
            post(requeue_needs_operator),
        )
        .with_state(state)
}

#[utoipa::path(
    post,
    path = "/api/executions",
    tag = "execution-operator",
    request_body = CreateExecution,
    responses(
        (status = 200, description = "Execution request created or idempotently replayed", body = CreateExecutionResponse),
        (status = 400, description = "invalid_request", body = RunnerV1ErrorEnvelope),
        (status = 404, description = "not_found (item does not exist)", body = RunnerV1ErrorEnvelope),
        (status = 409, description = "conflict / idempotency_conflict / runner_revoked", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn create_execution(
    State(state): State<OperatorExecutionState>,
    headers: HeaderMap,
    Json(input): Json<CreateExecution>,
) -> Result<Json<CreateExecutionResponse>, (StatusCode, Json<Value>)> {
    create_as(state, principal(&headers)?, input).await
}

/// The limit a stored run carries, with who applies it and the sentence every screen prints. Reads
/// the snapshot (`budgets.tokens`, `metadata.limit_source`, the profile's name) and the agent that
/// ran it: the latest attempt's, else the one requested. docket stops its own task at the limit and
/// the runner stops Claude Code and opencode when their counted tokens reach it; every other agent
/// is measured, and says so. `day_room` is what is left of the project's daily token limit, when it
/// has one: a run with no limit of its own gets that when it is claimed (`Repository::limit_request_to_day_room`).
fn limit_source(snapshot: &Value, harness_kind: Option<&str>, day_room: Option<i64>) -> RunLimit {
    let tokens = snapshot["budgets"]["tokens"].as_u64().filter(|n| *n > 0);
    let source = match (tokens, snapshot["metadata"]["limit_source"].as_str()) {
        (None, _) => LimitSource::None,
        (Some(_), Some("profile")) => LimitSource::Profile,
        (Some(_), Some("provisional")) => LimitSource::Provisional,
        (Some(_), Some("day")) => LimitSource::Day,
        (Some(_), _) => LimitSource::Run,
    };
    let docket = harness_kind == Some("docket");
    let enforced_by = match harness_kind {
        Some("docket") => "docket",
        Some("claude-code") => "claude-code",
        Some("opencode") => "opencode",
        _ => "measured",
    };
    let agent = match harness_kind {
        Some("claude-code") => "Claude Code",
        Some("codex") => "Codex",
        Some("opencode") => "OpenCode",
        _ => "the agent",
    };
    // Who stops the run at its limit, as the sentence names it.
    let who = |source: &LimitSource| {
        if docket {
            "docket stops the task there".to_owned()
        } else if enforced_by != "measured" {
            format!("Tack stops {agent}'s run there")
        } else if matches!(source, LimitSource::Day) {
            format!(
                "measured, not enforced (Tack reads {agent}'s tokens but cannot stop it at a limit yet, so this run can go past today's limit)"
            )
        } else {
            format!(
                "measured, not enforced (Tack reads {agent}'s tokens but cannot stop it at a limit yet)"
            )
        }
    };
    let sentence = match (tokens, day_room) {
        (None, Some(room)) => format!(
            "No limit of its own · gets what is left of today's limit ({} tokens left) · {}",
            tack_core::run_defaults::token_count(room.max(0)),
            who(&LimitSource::Day)
        ),
        (None, None) if docket => {
            "No limit set in Tack · docket stops each step at its own limit (docket's setting)"
                .to_owned()
        }
        (None, None) => "No limit set in Tack".to_owned(),
        (Some(n), _) => {
            let spaced = tack_core::run_defaults::token_count(i64::try_from(n).unwrap_or(i64::MAX));
            let from = match source {
                LimitSource::Profile => {
                    let name = snapshot["resolved_agent_profile"]["name"]
                        .as_str()
                        .unwrap_or("");
                    format!("from the profile {name}")
                }
                LimitSource::Day => "what was left of today's limit".to_owned(),
                _ => "set for this run".to_owned(),
            };
            if matches!(source, LimitSource::Provisional) {
                format!(
                    "Provisional limit: {spaced} tokens (no measured runs yet) · set in Settings → Automation · {}",
                    who(&source)
                )
            } else {
                format!("{spaced} tokens · {from} · {}", who(&source))
            }
        }
    };
    RunLimit {
        tokens,
        source: source.as_str().to_owned(),
        enforced_by: enforced_by.to_owned(),
        sentence,
        used: None,
    }
}

/// The limit of each listed request, read from its stored snapshot and the agent that ran it, and
/// what is left of its project's daily limit (for a run with no limit of its own).
async fn run_limits_of(
    repo: &Repository,
    ids: &[String],
    now: DateTime<Utc>,
) -> Result<std::collections::HashMap<String, RunLimit>, StoreError> {
    let rows = execution_requests::run_limit_inputs(repo, ids).await?;
    let used = budgets::counted_by_request(repo, ids).await?;
    let mut rooms: std::collections::HashMap<String, Option<i64>> =
        std::collections::HashMap::new();
    for row in &rows {
        if let std::collections::hash_map::Entry::Vacant(slot) = rooms.entry(row.project_id.clone())
        {
            let room = budgets::day_room(repo, &row.project_id, now)
                .await?
                .map(|day| day.room);
            slot.insert(room);
        }
    }
    Ok(rows
        .iter()
        .map(|row| {
            let snapshot = json!({
                "budgets": row.budgets.as_deref().and_then(|b| serde_json::from_str::<Value>(b).ok()).unwrap_or(Value::Null),
                "metadata": {"limit_source": row.source},
                "resolved_agent_profile": {"name": row.profile},
            });
            let day_room = rooms.get(&row.project_id).copied().flatten();
            let mut limit = limit_source(&snapshot, row.harness_kind.as_deref(), day_room);
            limit.used = used.get(&row.id).and_then(|n| u64::try_from(*n).ok());
            (row.id.clone(), limit)
        })
        .collect())
}

/// A refusal of the run use cases, as the operator API's error envelope.
pub(crate) fn refused(refusal: Refusal) -> (StatusCode, Json<Value>) {
    error(
        StatusCode::from_u16(refusal.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        refusal.code,
        &refusal.message,
        refusal.details,
    )
}

impl From<CreateExecution> for RunRequest {
    fn from(input: CreateExecution) -> Self {
        RunRequest {
            item_id: input.item_id,
            idempotency_key: input.idempotency_key,
            selector_kind: input.selector_kind,
            selector_id: input.selector_id,
            agent_profile_id: input.agent_profile_id,
            requested_harness_kind: input.requested_harness_kind,
            requested_model: input.requested_model,
            agent_profile_snapshot: input.agent_profile_snapshot,
            repository_snapshot: input.repository_snapshot,
            permission_policy: input.permission_policy,
            budgets: input.budgets,
            environment: input.environment,
            metadata: input.metadata,
            timeout_seconds: input.timeout_seconds,
            status_map_policy_id: input.status_map_policy_id,
            verify: input.verify,
            push_branch: input.push_branch,
            origin: input.origin,
        }
    }
}

fn created(started: Started) -> Json<CreateExecutionResponse> {
    Json(CreateExecutionResponse {
        protocol_version: 1,
        request_id: started.request_id,
        state: "queued".into(),
        replayed: started.replayed,
    })
}

/// `create_execution` for a caller that is the server itself: the principal is a string,
/// not a header, and the profile, agent and machine are resolved as for any other run.
pub(crate) async fn create_as(
    state: OperatorExecutionState,
    principal: String,
    input: CreateExecution,
) -> Result<Json<CreateExecutionResponse>, (StatusCode, Json<Value>)> {
    start_run(&state.repo, state.clock.as_ref(), &principal, input.into())
        .await
        .map(created)
        .map_err(refused)
}

/// The profile a run with no named profile gets, and the fleet that profile names.
pub(crate) async fn resolve_profile(
    repo: &Repository,
    item: &tack_core::models::Item,
    project: &tack_core::models::Project,
    explicit: Option<&str>,
) -> Result<Option<Resolved>, (StatusCode, Json<Value>)> {
    start_run::resolve_profile(repo, item, project, explicit)
        .await
        .map_err(refused)
}

#[utoipa::path(
    get,
    path = "/api/executions",
    tag = "execution-operator",
    params(ListExecutionsQuery),
    responses(
        (status = 200, description = "Execution requests, newest first. `item_ids` present: exactly one row per id that has at least one execution, the batch's own most recent one. Else scoped to one item when `item_id` is given, otherwise every request the install has recorded up to `limit`", body = ExecutionListResponse),
        (status = 400, description = "invalid_request (a malformed `item_ids` entry, or more ids than the route's cap)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn list_executions(
    State(state): State<OperatorExecutionState>,
    Query(query): Query<ListExecutionsQuery>,
) -> Result<Json<ExecutionListResponse>, (StatusCode, Json<Value>)> {
    let item_ids = query.parsed_item_ids().map_err(|bad_value| {
        error(
            StatusCode::BAD_REQUEST,
            StableErrorCode::InvalidRequest,
            "item_ids must be a comma-separated list of UUIDs",
            json!({"field": "item_ids", "value": bad_value}),
        )
    })?;
    let rows = if let Some(item_ids) = item_ids {
        let item_ids: Vec<String> = item_ids.iter().map(Uuid::to_string).collect();
        state.repo.list_latest_executions_for_items(&item_ids).await
    } else {
        let limit = query.effective_limit();
        let item_id = query.item_id.map(|id| id.to_string());
        state.repo.list_executions(item_id.as_deref(), limit).await
    }
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not list executions",
            json!({}),
        )
    })?;
    let now = state.clock.now();
    let runners = queued_runners(&state.repo, rows.iter().any(|row| row.state == "queued")).await?;
    let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
    let mut limits = run_limits_of(&state.repo, &ids, now).await.map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not read the runs' limits",
            json!({}),
        )
    })?;
    let data: Vec<ExecutionSummary> = rows
        .into_iter()
        .map(|row| ExecutionSummary {
            limit: limits
                .remove(&row.id)
                .unwrap_or_else(|| limit_source(&Value::Null, None, None)),
            waiting_reason: queued_reason(
                &row.state,
                row.waiting_reason,
                &row.selector_kind,
                &row.selector_id,
                &runners,
                now,
            ),
            request_id: row.id,
            item_id: row.item_id,
            state: row.state,
            cancellation_requested_at: row.cancellation_requested_at,
            created_at: row.created_at,
            waiting_since: row.waiting_since,
        })
        .collect();
    Ok(Json(ExecutionListResponse {
        protocol_version: 1,
        data,
    }))
}

/// What a queued request no poll has explained reads when no runner is
/// online: no active runner has sent a heartbeat within the scheduler's
/// window. Decided at read time; never stored, never grounds for failing it.
const NO_RUNNER_ONLINE: &str = "no runner is online";

/// Whether a listed runner is online now: [`SchedulingPolicy::is_online`] is the one rule,
/// this only reads the stored row into it.
pub(crate) fn runner_is_online(runner: &RunnerListingRow, now: DateTime<Utc>) -> bool {
    SchedulingPolicy::default().is_online(
        runner.state == "active" && runner.revoked_at.is_none(),
        runner
            .last_heartbeat_at
            .as_deref()
            .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
            .map(|at| at.with_timezone(&Utc)),
        now,
    )
}

/// Every runner, read only when a listed run is queued: the queue's reasons need them.
async fn queued_runners(
    repo: &Repository,
    any_queued: bool,
) -> Result<Vec<RunnerListingRow>, (StatusCode, Json<Value>)> {
    if !any_queued {
        return Ok(Vec::new());
    }
    repo.list_runners().await.map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not list runners",
            json!({}),
        )
    })
}

/// When the computer a queued run asked for last answered, if it has stopped answering: the newest
/// heartbeat of the named runner, or of a group's (fleet's) members, is past the liveness window.
/// `None` while one answers, and when none ever did (there is then no time to name).
fn silent_since(
    selector_kind: &str,
    selector_id: &str,
    runners: &[RunnerListingRow],
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let asked: Vec<&RunnerListingRow> = match selector_kind {
        "exact_runner" => runners.iter().filter(|r| r.id == selector_id).collect(),
        "fleet" => runners
            .iter()
            .filter(|r| r.fleet_ids.iter().any(|fleet| fleet == selector_id))
            .collect(),
        _ => return None,
    };
    let last = asked
        .iter()
        .filter_map(|runner| runner.last_heartbeat_at.as_deref().and_then(parse_rfc3339))
        .max()?;
    // The scheduler's own rule for "answering": past its window, nothing in the group answers.
    (!SchedulingPolicy::default().is_online(true, Some(last), now)).then_some(last)
}

/// A listed run's reason. A queued run waits for the computer it asked for when that has stopped
/// answering, and names when: a poll's stored reason is from before that, so it gives way. Otherwise
/// it shows what a poll stored, and with nothing stored, "no runner is online" when none is. Other
/// states show what is stored. Read at read time; never stored.
fn queued_reason(
    state: &str,
    stored: Option<String>,
    selector_kind: &str,
    selector_id: &str,
    runners: &[RunnerListingRow],
    now: DateTime<Utc>,
) -> Option<String> {
    if state != "queued" {
        return stored;
    }
    if let Some(since) = silent_since(selector_kind, selector_id, runners, now) {
        return Some(format!(
            "The computer this run asked for has not answered since {}",
            since.to_rfc3339_opts(SecondsFormat::Secs, true)
        ));
    }
    stored.or_else(|| {
        (!runners.iter().any(|runner| runner_is_online(runner, now)))
            .then(|| NO_RUNNER_ONLINE.to_string())
    })
}

#[utoipa::path(
    get,
    path = "/api/executions/{request_id}",
    tag = "execution-operator",
    params(("request_id" = String, Path, description = "Execution request ID (opaque)")),
    responses(
        (status = 200, description = "Execution request detail", body = ExecutionDetailResponse),
        (status = 404, description = "not_found", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn get_execution(
    State(state): State<OperatorExecutionState>,
    Path(request_id): Path<String>,
) -> Result<Json<ExecutionDetailResponse>, (StatusCode, Json<Value>)> {
    let row = execution_requests::request_detail(&state.repo, &request_id)
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not load execution",
                json!({}),
            )
        })?;
    let Some(row) = row else {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Execution request does not exist",
            json!({"resource": "execution_request"}),
        ));
    };
    let now = state.clock.now();
    let request_state = row.state;
    let runners = queued_runners(&state.repo, request_state == "queued").await?;
    let request_id = row.id;
    let limit = run_limits_of(&state.repo, std::slice::from_ref(&request_id), now)
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not read the run's limit",
                json!({}),
            )
        })?
        .remove(&request_id)
        .unwrap_or_else(|| limit_source(&Value::Null, None, None));
    Ok(Json(ExecutionDetailResponse {
        protocol_version: 1,
        request_id,
        limit,
        item_id: row.item_id,
        state: request_state.clone(),
        cancellation_requested_at: row.cancellation_requested_at,
        created_at: row.created_at,
        instructions: row.instructions,
        waiting_reason: queued_reason(
            &request_state,
            row.waiting_reason,
            &row.selector_kind,
            &row.selector_id,
            &runners,
            now,
        ),
        waiting_since: row.waiting_since,
    }))
}

/// `GET /api/executions/{request_id}/attempts` — the operator read path
/// for attempt data. `execution_attempts` (migration 045) is written by
/// the runner-v1 protocol, but `GET /executions/{id}` returns only 5
/// scalar columns (`request_id, item_id, state, cancellation_requested_at,
/// created_at`), never attempt data; this endpoint closes that gap. An
/// empty list here is a real, honest "no attempt yet" (the request is
/// still `queued`), not a placeholder — distinct from the 404 an unknown
/// `request_id` gets.
#[utoipa::path(
    get,
    path = "/api/executions/{request_id}/attempts",
    tag = "execution-operator",
    params(("request_id" = String, Path, description = "Execution request ID (opaque)")),
    responses(
        (status = 200, description = "Every attempt made against this request, oldest first (may be empty)", body = AttemptListResponse),
        (status = 404, description = "not_found", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn list_execution_attempts(
    State(state): State<OperatorExecutionState>,
    Path(request_id): Path<String>,
) -> Result<Json<AttemptListResponse>, (StatusCode, Json<Value>)> {
    // Also carries `requested_model` —
    // `derive_attempt_facts` needs the *requested* side of provenance,
    // which lives on `execution_requests`, not on any one attempt row.
    // Same not-found semantics as a plain `EXISTS(...)` check, one query
    // instead of two.
    let requested_model = execution_requests::requested_model_of_request(&state.repo, &request_id)
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not verify execution",
                json!({}),
            )
        })?;
    let Some(requested_model) = requested_model else {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Execution request does not exist",
            json!({"resource": "execution_request"}),
        ));
    };
    let attempts = state
        .repo
        .list_attempts_for_request(&request_id)
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not list attempts",
                json!({}),
            )
        })?;
    let mut pull_requests = std::collections::HashMap::new();
    for attempt in &attempts {
        if let Ok(Some(pr)) = pull_requests::pull_request_of_attempt(&state.repo, &attempt.id).await
        {
            pull_requests.insert(
                attempt.id.clone(),
                AttemptPullRequest {
                    number: pr.number,
                    url: pr.url,
                    state: pr.state,
                },
            );
        }
    }
    let mut reviews = std::collections::HashMap::new();
    for attempt in &attempts {
        let row = attempt_reviews::review_of_attempt(&state.repo, &attempt.id)
            .await
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    StableErrorCode::InternalError,
                    "Could not list attempt reviews",
                    json!({}),
                )
            })?;
        if let Some(review) = row {
            reviews.insert(
                attempt.id.clone(),
                AttemptReview {
                    verdict: review.verdict,
                    note: review.note,
                    reviewed_at: review.reviewed_at,
                    pull_request_url: None,
                },
            );
        }
    }
    let mut merges = std::collections::HashMap::new();
    for attempt in &attempts {
        let (enabled, reason) = match &state.app {
            Some(app) => tack_app::merge::button(&app.repo, &app.host(), &attempt.id).await,
            None => (
                false,
                Some("Merging is not available on this board".to_owned()),
            ),
        };
        let merged = match autonomy::merge_of_attempt(&state.repo, &attempt.id)
            .await
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    StableErrorCode::InternalError,
                    "Could not read the attempt's merge",
                    json!({}),
                )
            })? {
            Some(row) => row.merged_at.map(|merged_at| MergedBy {
                trigger: row.trigger,
                level: row.level,
                domain: row.domain,
                merged_at,
                merge_sha: row.merge_sha,
                policy_snapshot: row
                    .policy_snapshot
                    .and_then(|raw| serde_json::from_str(&raw).ok()),
            }),
            None => None,
        };
        merges.insert(
            attempt.id.clone(),
            AttemptMerge {
                enabled,
                reason,
                merged,
            },
        );
    }
    let data: Vec<AttemptSummary> = attempts
        .into_iter()
        .map(|attempt| {
            // `derive_attempt_facts` is a convenience "service handler" —
            // takes the exact raw column shapes `AttemptListingRow` already
            // carries, so nothing here re-derives provenance/economics
            // logic. No runner infra cost-rate is stored anywhere in this
            // schema today, so `runner_rate_usd_per_hour` is always `None`
            // here — `runner_time_cost.cost_usd_estimated` stays honestly
            // `not_measured`, never a fabricated rate.
            let facts = derive_attempt_facts(
                requested_model.as_deref(),
                attempt.actual_execution.as_deref(),
                attempt.usage.as_deref(),
                attempt.started_at.as_deref().and_then(parse_rfc3339),
                attempt.ended_at.as_deref().and_then(parse_rfc3339),
                None,
            );
            let counted_tokens = attempt
                .usage
                .as_deref()
                .and_then(|raw| serde_json::from_str::<tack_protocol::execution::Usage>(raw).ok())
                .and_then(|usage| usage.counted());
            let start_report = attempt
                .start_report
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok());
            let environment_names = start_report
                .as_ref()
                .and_then(|report| report.get("environment_names"))
                .and_then(Value::as_array)
                .map(|names| {
                    names
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let sandbox = start_report
                .as_ref()
                .and_then(|report| report.get("sandbox"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            AttemptSummary {
                environment_names,
                sandbox,
                pull_request: pull_requests.remove(&attempt.id),
                review: reviews.remove(&attempt.id),
                merge: merges
                    .remove(&attempt.id)
                    .expect("one merge state per attempt"),
                attempt_id: attempt.id,
                request_id: attempt.request_id,
                attempt_number: attempt.attempt_number,
                runner_id: attempt.runner_id,
                fencing_token: attempt.fencing_token,
                state: attempt.state,
                lease_issued_at: attempt.lease_issued_at,
                lease_expires_at: attempt.lease_expires_at,
                last_heartbeat_at: attempt.last_heartbeat_at,
                event_checkpoint: attempt.event_checkpoint,
                completion_id: attempt.completion_id,
                workspace_id: attempt.workspace_id,
                base_revision: attempt.base_revision,
                actual_execution: attempt
                    .actual_execution
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok()),
                terminal_reason: attempt
                    .terminal_reason
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<TerminalReason>(raw).ok()),
                usage: attempt
                    .usage
                    .as_deref()
                    .and_then(|raw| serde_json::from_str::<Value>(raw).ok()),
                counted_tokens,
                started_at: attempt.started_at,
                ended_at: attempt.ended_at,
                created_at: attempt.created_at,
                updated_at: attempt.updated_at,
                model_provenance: facts.model_provenance.map(|provenance| {
                    serde_json::to_value(provenance).expect("ModelProvenance serializes")
                }),
                usage_economics: serde_json::to_value(facts.usage_economics)
                    .expect("UsageEconomics serializes"),
            }
        })
        .collect();
    Ok(Json(AttemptListResponse {
        protocol_version: 1,
        data,
    }))
}

/// `POST /api/attempts/{attempt_id}/merge` — the Merge button. Merges the
/// attempt's pull request at its head commit, opening the pull request first when
/// it is missing; a refusal is a `409` carrying the reason, already recorded on
/// the item.
#[utoipa::path(
    post,
    path = "/api/attempts/{attempt_id}/merge",
    tag = "execution-operator",
    params(("attempt_id" = String, Path, description = "Execution attempt ID (opaque)")),
    responses(
        (status = 200, description = "Merged; carries the merge commit's sha", body = serde_json::Value),
        (status = 404, description = "not_found", body = RunnerV1ErrorEnvelope),
        (status = 409, description = "conflict: the merge was held, the message says why", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn merge_attempt(
    State(state): State<OperatorExecutionState>,
    Path(attempt_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let exists = execution_attempts::attempt_exists(&state.repo, &attempt_id)
        .await
        .unwrap_or(false);
    let (true, Some(app)) = (exists, state.app.as_ref()) else {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Execution attempt does not exist",
            json!({"resource": "execution_attempt"}),
        ));
    };
    match tack_app::merge::attempt(
        &app.repo,
        state.clock.as_ref(),
        &app.host(),
        &attempt_id,
        tack_core::autonomy::Trigger::Button,
        tack_app::merge::FactsExtra::default(),
    )
    .await
    {
        Ok(sha) => Ok(Json(json!({"merged": true, "merge_sha": sha}))),
        Err(reason) => Err(error(
            StatusCode::CONFLICT,
            StableErrorCode::Conflict,
            &reason,
            json!({"resource": "execution_attempt"}),
        )),
    }
}

/// `GET /api/executions/{request_id}/attempts/{attempt_number}/events` —
/// the other half of the attempts/events read path above. Returns
/// `404` naming which resource is missing (`execution_request` vs
/// `execution_attempt`) rather than a single ambiguous not-found, since a
/// client can otherwise not distinguish "wrong request id" from "this
/// attempt number never existed."
#[utoipa::path(
    get,
    path = "/api/executions/{request_id}/attempts/{attempt_number}/events",
    tag = "execution-operator",
    params(
        ("request_id" = String, Path, description = "Execution request ID (opaque)"),
        ("attempt_number" = i64, Path, description = "1-based attempt number"),
    ),
    responses(
        (status = 200, description = "Every event this attempt has reported, oldest first (may be empty)", body = EventListResponse),
        (status = 404, description = "not_found (execution_request or execution_attempt)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn list_execution_attempt_events(
    State(state): State<OperatorExecutionState>,
    Path((request_id, attempt_number)): Path<(String, i64)>,
) -> Result<Json<EventListResponse>, (StatusCode, Json<Value>)> {
    let exists = execution_requests::execution_request_exists(&state.repo, &request_id)
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not verify execution",
                json!({}),
            )
        })?;
    if !exists {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Execution request does not exist",
            json!({"resource": "execution_request"}),
        ));
    }
    let events = state
        .repo
        .list_events_for_attempt_number(&request_id, attempt_number)
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not list events",
                json!({}),
            )
        })?;
    let Some(events) = events else {
        return Err(error(
            StatusCode::NOT_FOUND,
            StableErrorCode::NotFound,
            "Attempt does not exist",
            json!({"resource": "execution_attempt"}),
        ));
    };
    let data: Vec<EventSummary> = events
        .into_iter()
        .map(|event| EventSummary {
            event_id: event.event_id,
            sequence: event.sequence,
            source: event.source,
            kind: event.kind,
            payload: serde_json::from_str::<Value>(&event.payload).unwrap_or(Value::Null),
            occurred_at: event.occurred_at,
            created_at: event.created_at,
        })
        .collect();
    Ok(Json(EventListResponse {
        protocol_version: 1,
        data,
    }))
}

#[utoipa::path(
    post,
    path = "/api/executions/{request_id}/cancel",
    tag = "execution-operator",
    params(("request_id" = String, Path, description = "Execution request ID (opaque)")),
    responses(
        (status = 200, description = "Cancelled at once when nobody holds the run; otherwise cancellation requested and not yet terminal", body = CancellationRequestedResponse),
        (status = 404, description = "not_found", body = RunnerV1ErrorEnvelope),
        (status = 409, description = "conflict (already terminal)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn request_cancellation(
    State(state): State<OperatorExecutionState>,
    Path(request_id): Path<String>,
) -> Result<Json<CancellationRequestedResponse>, (StatusCode, Json<Value>)> {
    let outcome = cancel_run(&state.repo, state.clock.as_ref(), &request_id)
        .await
        .map_err(|failure| match failure {
            AppError::NotFound => error(
                StatusCode::NOT_FOUND,
                StableErrorCode::NotFound,
                "Execution request does not exist",
                json!({"resource": "execution_request"}),
            ),
            AppError::Conflict(_) => error(
                StatusCode::CONFLICT,
                StableErrorCode::Conflict,
                "Execution already reached a terminal state before cancellation could apply",
                json!({}),
            ),
            _ => error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not request cancellation",
                json!({}),
            ),
        })?;
    Ok(Json(CancellationRequestedResponse {
        protocol_version: 1,
        request_id,
        state: match outcome {
            Cancellation::Ended => "cancelled",
            Cancellation::Requested => "cancellation_requested",
        }
        .into(),
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RecoveryConfirmation {
    pub recovery_key: String,
    pub reason: String,
}

#[utoipa::path(
    post,
    path = "/api/executions/{request_id}/requeue",
    tag = "execution-operator",
    params(("request_id" = String, Path, description = "Execution request ID (opaque)")),
    request_body = RecoveryConfirmation,
    responses(
        (status = 200, description = "Requeued (or replayed) after an audited recovery decision", body = RequeueResponse),
        (status = 409, description = "conflict / idempotency_conflict / invalid_transition", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn requeue_needs_operator(
    State(state): State<OperatorExecutionState>,
    headers: HeaderMap,
    Path(request_id): Path<String>,
    Json(input): Json<RecoveryConfirmation>,
) -> Result<Json<RequeueResponse>, (StatusCode, Json<Value>)> {
    let actor = principal(&headers)?;
    let requeued = requeue(
        &state.repo,
        state.clock.as_ref(),
        &request_id,
        &input.recovery_key,
        &input.reason,
        &actor,
    )
    .await
    .map_err(|failure| match failure {
        AppError::Conflict(_) => error(
            StatusCode::CONFLICT,
            StableErrorCode::IdempotencyConflict,
            "The recovery key was used with a different confirmation",
            json!({"idempotency_key": input.recovery_key}),
        ),
        AppError::InvalidTransition { from, to } => error(
            StatusCode::CONFLICT,
            StableErrorCode::InvalidTransition,
            "Only authoritatively recovered needs_operator attempts may be requeued",
            json!({"from": from, "to": to}),
        ),
        _ => error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not requeue execution",
            json!({}),
        ),
    })?;
    Ok(Json(RequeueResponse {
        protocol_version: 1,
        request_id,
        state: "queued".into(),
        recovered_from: "needs_operator".into(),
        replayed: requeued.replayed,
    }))
}

/// Response body for `GET /api/items/{id}/agent-context`.
#[derive(Debug, Serialize, ToSchema)]
pub struct AgentContextResponse {
    pub text: String,
    /// The profile the server resolved for this item (the one named, else the role's, the
    /// project's default or the implementer); `null` when none could be chosen.
    pub profile_id: Option<String>,
    /// Where that profile came from: `role:{name}`, `project_default`, `run_settings`,
    /// `implementer` or `ambiguous`; `null` when `profile_id` was named in the query.
    pub profile_source: Option<String>,
    /// That profile's `limits` (`budgets.tokens`, `timeout_seconds`), `{}` when it sets none; the
    /// run dialog pre-fills its limits from it.
    pub limits: Value,
    /// The limit a run with that profile would have, who applies it and the sentence every screen
    /// prints, from the same rule as a stored run's `limit`.
    pub limit: RunLimit,
}

/// Query parameters for `GET /api/items/{id}/agent-context`.
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AgentContextQuery {
    /// The agent profile to compose with; the project's default when absent.
    pub profile_id: Option<String>,
    /// The agent the run dialog has chosen (`claude-code`, `codex`, `docket`, `opencode`); it
    /// replaces the profile's agent in the limit's sentence. Absent: the profile's own.
    pub harness: Option<String>,
    /// The token limit typed in the run dialog, a positive whole number. Equal to the profile's it
    /// is still the profile's; different it is "set for this run". Zero or not a number is a 400
    /// naming `tokens`. Absent: the profile's limit.
    #[param(value_type = Option<u64>)]
    pub tokens: Option<String>,
    /// `true` when the person cleared the dialog's token limit to run without one; wins over
    /// `tokens`. Absent or `false`: the limit above applies.
    pub no_limit: Option<bool>,
}

/// `GET /api/items/{id}/agent-context` — what the agent will read for a given item:
/// the profile named by `profile_id` (else the project's default profile)'s instructions, the item's title and description,
/// the item's brief when it has one, and the project's definition_of_done when set.
#[utoipa::path(
    get,
    path = "/api/items/{id}/agent-context",
    tag = "items",
    params(("id" = Uuid, Path, description = "Item ID"), AgentContextQuery),
    responses(
        (status = 200, description = "The composed instructions for the agent", body = AgentContextResponse),
        (status = 404, description = "Item or project not found", body = crate::openapi::ErrorEnvelope),
    ),
)]
pub async fn get_item_agent_context(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Query(query): Query<AgentContextQuery>,
) -> Result<Json<AgentContextResponse>, (StatusCode, Json<Value>)> {
    let item = state
        .repo
        .get_item(item_id)
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not load item",
                json!({}),
            )
        })?
        .ok_or_else(|| {
            error(
                StatusCode::NOT_FOUND,
                StableErrorCode::NotFound,
                "Item does not exist",
                json!({"resource": "item"}),
            )
        })?;

    let project = state
        .repo
        .get_project(item.project_id)
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not load project",
                json!({}),
            )
        })?
        .ok_or_else(|| {
            error(
                StatusCode::NOT_FOUND,
                StableErrorCode::NotFound,
                "Project does not exist",
                json!({"resource": "project"}),
            )
        })?;

    // The profile the run would get: the one named, else run settings, the item's role,
    // the project's default, then the implementer.
    let resolved =
        resolve_profile(&state.repo, &item, &project, query.profile_id.as_deref()).await?;
    let answered_profile_id = resolved.as_ref().map(|r| r.profile_id.clone());
    let answered_source = resolved
        .as_ref()
        .and_then(|r| profile_source_label(&r.source));
    let profile = if let Some(resolved) = resolved {
        let profile = agent_profiles::profile_by_id(&state.repo, &resolved.profile_id)
            .await
            .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not load agent profile",
                json!({}),
            )
        })?;
        profile.map(|profile| {
            serde_json::json!({
                "agent_profile_id": profile.id,
                "name": profile.name,
                "instructions": profile.instructions,
                "tool_policy": serde_json::from_str::<serde_json::Value>(&profile.tool_policy).unwrap_or_default(),
                "limits": serde_json::from_str::<serde_json::Value>(&profile.limits).unwrap_or_else(|_| json!({})),
            })
        })
    } else {
        None
    }
    .unwrap_or_else(|| {
        serde_json::json!({
            "agent_profile_id": "",
            "name": "Default",
            "instructions": "",
            "tool_policy": serde_json::json!({}),
            "limits": serde_json::json!({}),
        })
    });

    let profile_instructions = profile
        .get("instructions")
        .and_then(Value::as_str)
        .unwrap_or_default();

    // Load the brief
    let brief = state.repo.get_item_brief(item_id).await.map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not load item brief",
            json!({}),
        )
    })?;

    let comments = state.repo.list_comments(item_id).await.map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not load item comments",
            json!({}),
        )
    })?;

    // Compose the instructions with the definition_of_done
    let composed = compose_instructions(
        profile_instructions,
        &item,
        &comments,
        brief.as_ref(),
        project.definition_of_done.as_deref(),
    );

    // The profile's limit, unless the dialog has its own: a different number is the run's, none is
    // "no limit". The sentence is still `limit_source`'s, over a snapshot of the same shape.
    let profile_tokens = profile["limits"]["budgets"]["tokens"].as_u64();
    let provisional = budgets::provisional_tokens(&state.repo, &project.id.to_string())
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not load the project's limits",
                json!({}),
            )
        })?
        .and_then(|n| u64::try_from(n).ok());
    let (tokens, limit_from) = if query.no_limit == Some(true) {
        let (tokens, source) = unset_tokens(&profile["limits"], provisional);
        (json!(tokens), source.as_str())
    } else if let Some(typed) = query.tokens.as_deref() {
        let typed = typed
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| {
                error(
                    StatusCode::BAD_REQUEST,
                    StableErrorCode::InvalidRequest,
                    "The token limit must be a positive whole number",
                    json!({"field": "tokens", "reason": "not_a_positive_whole_number"}),
                )
            })?;
        let from = if Some(typed) == profile_tokens {
            "profile"
        } else {
            "run"
        };
        (json!(typed), from)
    } else {
        let (tokens, source) = unset_tokens(&profile["limits"], provisional);
        (json!(tokens), source.as_str())
    };
    let harness = query
        .harness
        .as_deref()
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .or_else(|| profile["tool_policy"]["agent"].as_str());
    let day_room = budgets::day_room(&state.repo, &item.project_id.to_string(), Utc::now())
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not load the project's limits",
                json!({}),
            )
        })?
        .map(|day| day.room);
    let limit = limit_source(
        &json!({
            "budgets": {"tokens": tokens},
            "metadata": {"limit_source": limit_from},
            "resolved_agent_profile": {"name": profile["name"]},
        }),
        harness,
        day_room,
    );

    Ok(Json(AgentContextResponse {
        text: composed,
        profile_id: answered_profile_id,
        profile_source: answered_source,
        limits: profile["limits"].clone(),
        limit,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // (stored snapshot, agent, tokens, source, enforced_by, the sentence every screen prints)
    #[test]
    fn a_runs_limit_says_who_set_it_and_who_applies_it() {
        let profile = json!({"budgets": {"tokens": 300000}, "metadata": {"limit_source": "profile"},
            "resolved_agent_profile": {"name": "Implementer"}});
        let run = json!({"budgets": {"tokens": 300000}, "metadata": {"limit_source": "run"}});
        let none = json!({"budgets": {}, "metadata": {"limit_source": "none"}});
        let rows = [
            (
                &profile,
                "docket",
                Some(300_000),
                "profile",
                "docket",
                "300 000 tokens · from the profile Implementer · docket stops the task there",
            ),
            (
                &run,
                "claude-code",
                Some(300_000),
                "run",
                "claude-code",
                "300 000 tokens · set for this run · Tack stops Claude Code's run there",
            ),
            (
                &run,
                "opencode",
                Some(300_000),
                "run",
                "opencode",
                "300 000 tokens · set for this run · Tack stops OpenCode's run there",
            ),
            (
                &run,
                "codex",
                Some(300_000),
                "run",
                "measured",
                "300 000 tokens · set for this run · measured, not enforced (Tack reads Codex's tokens but cannot stop it at a limit yet)",
            ),
            (
                &none,
                "docket",
                None,
                "none",
                "docket",
                "No limit set in Tack · docket stops each step at its own limit (docket's setting)",
            ),
            (
                &none,
                "opencode",
                None,
                "none",
                "opencode",
                "No limit set in Tack",
            ),
            (
                &none,
                "codex",
                None,
                "none",
                "measured",
                "No limit set in Tack",
            ),
            (
                &json!({"budgets": {"tokens": 2000000}, "metadata": {"limit_source": "provisional"}}),
                "claude-code",
                Some(2_000_000),
                "provisional",
                "claude-code",
                "Provisional limit: 2 000 000 tokens (no measured runs yet) · set in Settings → Automation · Tack stops Claude Code's run there",
            ),
            (
                &json!({"budgets": {"tokens": 400000}, "metadata": {"limit_source": "day"}}),
                "docket",
                Some(400_000),
                "day",
                "docket",
                "400 000 tokens · what was left of today's limit · docket stops the task there",
            ),
            // An agent Tack stops stops at what is left of the day; a measured one can go past it.
            (
                &json!({"budgets": {"tokens": 400000}, "metadata": {"limit_source": "day"}}),
                "claude-code",
                Some(400_000),
                "day",
                "claude-code",
                "400 000 tokens · what was left of today's limit · Tack stops Claude Code's run there",
            ),
            (
                &json!({"budgets": {"tokens": 400000}, "metadata": {"limit_source": "day"}}),
                "codex",
                Some(400_000),
                "day",
                "measured",
                "400 000 tokens · what was left of today's limit · measured, not enforced (Tack reads Codex's tokens but cannot stop it at a limit yet, so this run can go past today's limit)",
            ),
            // An agent the runner does not know is measured.
            (
                &run,
                "mystery",
                Some(300_000),
                "run",
                "measured",
                "300 000 tokens · set for this run · measured, not enforced (Tack reads the agent's tokens but cannot stop it at a limit yet)",
            ),
            // A run stored before the source was recorded still names a limit it carries.
            (
                &json!({"budgets": {"tokens": 1000}}),
                "docket",
                Some(1000),
                "run",
                "docket",
                "1 000 tokens · set for this run · docket stops the task there",
            ),
        ];
        for (snapshot, agent, tokens, source, enforced_by, sentence) in rows {
            let limit = limit_source(snapshot, Some(agent), None);
            assert_eq!(
                (
                    limit.tokens,
                    limit.source.as_str(),
                    limit.enforced_by.as_str(),
                    limit.sentence.as_str()
                ),
                (tokens, source, enforced_by, sentence),
                "{snapshot} on {agent}"
            );
        }
    }
}
