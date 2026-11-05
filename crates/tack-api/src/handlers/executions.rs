//! Operator execution handlers, mounted into the global router.

use crate::extract::{Json, Path, Query};
use std::collections::BTreeMap;
use std::sync::Arc;
use tack_protocol::Clock;

use axum::{
    Router,
    extract::State,
    http::HeaderMap,
    http::StatusCode,
    routing::{get, post},
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tack_db::queries::execution_attempts::{AttemptWithOutcomes, OpenAttemptRow};
use tack_db::queries::{
    agent_profiles, budgets, execution_attempts, execution_events, execution_requests,
};
use tack_db::repo::execution::state as execution_state;
use tack_db::{
    Repository, StoreError,
    repo::execution::{AttemptListingRow, RunnerListingRow},
};
use tack_protocol::event_kind;
use tack_protocol::execution::{ExecutionRequestId, ExecutionState, RecoveryKey, StableErrorCode};
use tack_protocol::usage_provenance::{ModelProvenance, UsageEconomics, derive_attempt_facts};
use utoipa::ToSchema;
use uuid::Uuid;

use tack_core::run_defaults::Resolved;

use crate::AppState;
use tack_app::AppError;
use tack_app::runs::cancel_run::{Cancellation, cancel_run};
use tack_app::runs::continue_run::continue_run;
use tack_app::runs::requeue::requeue;
use tack_app::runs::start_run::{
    self, LimitSource, Refusal, RunRequest, Started, kind_of, profile_source_label,
    render_instructions, start_run, task_document, unset_tokens,
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
    /// Removed: a run never moves a task. A request that still carries the key is accepted, the
    /// value is ignored, and the answer's `notice` says so.
    #[serde(default, deserialize_with = "present", skip_serializing)]
    #[schema(ignore)]
    pub status_map_policy_id: Option<Value>,
    /// `false` declines the runner's verifier for this run. The runner's own
    /// configuration decides whether a verifier exists; `true` never enables one.
    #[serde(default)]
    pub verify: Option<bool>,
    /// `false` declines the runner's branch push for this run. The runner's own
    /// configuration decides whether a push exists; `true` never enables one.
    #[serde(default)]
    pub push_branch: Option<bool>,
    /// The autonomy level a docket run is asked for: `L3`, `L4` or `L5`, sent to docket as it is.
    /// Absent, docket decides from the team's profile; the project's level is never sent. `L3` is
    /// "Run again at L3". Any other value, or a level for an agent other than docket, is
    /// `invalid_request`.
    #[serde(default)]
    pub autonomy: Option<String>,
    /// `mcp` marks a run an agent started through `tack mcp`: it is never merged
    /// automatically or by a move to Done, only by the Merge button. Any other
    /// value, or none, means a person started it.
    #[serde(default)]
    pub origin: Option<String>,
    /// A previous execution request's id, to start this run from the work it kept instead of the
    /// project's base commit. The server reads that request's newest evidence
    /// (`actual_execution.git.branch`) and sets `metadata.rework_base` itself, the same key a
    /// rework already reads — a request never carries the branch directly. A name that does not
    /// exist is `not_found` (`details.reason = "from_run_not_found"`); one that belongs to
    /// another project, or left no branch to continue from, is `unprocessable`
    /// (`details.reason` is `"from_run_different_project"` or `"from_run_no_kept_work"`). Absent,
    /// `metadata.rework_base` is read only when the caller set it directly, as before.
    #[serde(default)]
    pub from_run: Option<String>,
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
    /// Present when the request named a setting that was removed: "ignored: the status is set by
    /// a person".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

/// One row of `GET /api/executions` / the `GET /api/executions/{id}`
/// detail. Deliberately five scalar columns today — attempt/event detail
/// is not carried here; it is available separately via `GET
/// /api/executions/{id}/attempts`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ExecutionSummary {
    pub request_id: String,
    pub item_id: String,
    #[schema(value_type = String)]
    pub state: ExecutionState,
    pub cancellation_requested_at: Option<String>,
    pub created_at: String,
    /// Why no runner has claimed this request yet, as a plain sentence; null once claimed.
    pub waiting_reason: Option<String>,
    /// When a runner's poll first found the request unclaimable; null once claimed.
    pub waiting_since: Option<String>,
    /// The runs that hold every slot of the computers this request may run on, when no poll has
    /// given a reason because those computers are full; empty otherwise. Read at read time.
    pub waiting_behind: Vec<WaitingBehind>,
    /// The run's token limit and who applies it.
    pub limit: RunLimit,
    /// The profile the run was made with, so a screen finds the run's team and agent from the
    /// run and not from the item's current setting; null when the profile has since been deleted.
    pub agent_profile_id: Option<String>,
    /// The agent the run asked for (`claude-code`, `codex`, `opencode`, `docket`); null when it
    /// left the choice to the computer.
    pub requested_harness_kind: Option<String>,
    /// The latest hop the run's newest attempt reported; null when the agent reports no hops
    /// (every agent but docket) or the attempt has not reached its first one.
    pub latest_hop: Option<LatestHop>,
    /// The run's newest attempt ended without ever starting work: the computer refused it, or the
    /// team's ceiling did. A screen reads "Not started" from this, never a failure of the work.
    pub never_started: bool,
    /// What the verifier's pack for the run's newest attempt recommends (`merge`, `review` or
    /// `reject`); null while that attempt has not succeeded, or when it left no readable pack.
    #[schema(value_type = Option<tack_protocol::mrp::MergeDecision>)]
    pub verifier_recommendation: Option<tack_protocol::mrp::MergeDecision>,
}

/// Whether a hop's latest event says it began or ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HopKind {
    Started,
    Finished,
}

/// The step of a docket run an attempt is in, read from the text of its latest hop event so a
/// screen finds the step on the team's list without fetching the events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct LatestHop {
    /// The role as the event names it (its first letter is upper case); match it to a team's
    /// steps without regard to case.
    pub role: String,
    pub kind: HopKind,
    /// How the hop ended, in the event's words (`done` when it reported no verdict); null for a
    /// hop that has started.
    pub verdict: Option<String>,
    /// The cycle a finished hop names; null when it names none (the first pass is not named).
    pub cycle: Option<u64>,
}

impl LatestHop {
    /// Reads the line the runner writes for a hop: `Role (model) started`, and for a finished one
    /// `Role (model): verdict[, cycle N][, gate G][, N tokens in ...]`. `None` for a line of
    /// another shape, so an event this reader does not know is not guessed at.
    fn from_event(kind: &str, text: &str) -> Option<Self> {
        let (role, rest) = text.trim().split_once(" (")?;
        if role.is_empty() {
            return None;
        }
        if kind == event_kind::AGENT_HOP_STARTED {
            rest.strip_suffix(") started")?;
            return Some(Self {
                role: role.to_owned(),
                kind: HopKind::Started,
                verdict: None,
                cycle: None,
            });
        }
        if kind != event_kind::AGENT_HOP_FINISHED {
            return None;
        }
        let (_model, parts) = rest.split_once("): ")?;
        let parts: Vec<&str> = parts.split(", ").collect();
        let cycle_of = |part: &str| part.strip_prefix("cycle ")?.parse::<u64>().ok();
        let is_trailer = |part: &str| {
            cycle_of(part).is_some()
                || part.starts_with("gate ")
                || part.split_once(" tokens in").is_some_and(|(count, tail)| {
                    count.parse::<u64>().is_ok() && (tail.is_empty() || tail.starts_with(" ("))
                })
        };
        // A failed hop's error is free text and may hold ", ": the verdict runs up to the first
        // part that is one of the line's own trailers.
        let end = parts.iter().position(|part| is_trailer(part));
        let verdict = parts[..end.unwrap_or(parts.len())].join(", ");
        Some(Self {
            role: role.to_owned(),
            kind: HopKind::Finished,
            verdict: Some(verdict).filter(|v| !v.is_empty()),
            cycle: parts.iter().find_map(|part| cycle_of(part)),
        })
    }
}

/// One run that holds a slot of a full computer a queued request waits for.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WaitingBehind {
    pub runner_id: String,
    pub runner_name: String,
    /// How many runs the computer takes at a time.
    pub runs_at_a_time: i64,
    pub request_id: String,
    pub attempt_number: i64,
    pub item_id: String,
    pub project_id: String,
    pub item_title: String,
    /// When the run started, or was taken by the computer if it has not started (RFC 3339).
    pub started_at: String,
    /// The question the run waits for a person to answer; null when it waits for no one.
    pub question: Option<String>,
    /// When that question was asked (RFC 3339).
    pub question_since: Option<String>,
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

/// Query parameters for `GET /api/executions`. Four read shapes, in
/// precedence order: `item_ids` (comma-separated) asks for the single
/// latest execution per item, for a batch of items — the shape a
/// Board/Sprint screen with many cards needs, one request regardless of
/// how many cards are on screen, and wins over `project_id`/`item_id`/
/// `limit` when present; else `project_id` asks for the runs a project is
/// working on now and those it ended in the last day, unbounded by `limit`;
/// else `item_id` scopes every request for one item; else the unscoped,
/// install-wide list, bounded by `limit`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListExecutionsQuery {
    pub item_id: Option<Uuid>,
    /// Every open run of the project (queued, running, waiting on a person) and those that ended
    /// in the last 24 hours, one row per run. An unknown project is a 404. Takes precedence over
    /// `item_id`/`limit`; `item_ids` takes precedence over it.
    pub project_id: Option<Uuid>,
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
                Uuid::parse_str(part).map_err(|failure| {
                    tracing::warn!(error = %failure, "Request refused");
                    part.to_string()
                })
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
    #[schema(value_type = String)]
    pub state: ExecutionState,
    pub cancellation_requested_at: Option<String>,
    pub created_at: String,
    /// The composed prompt the agent is given, as stored with the request; null when absent.
    pub instructions: Option<String>,
    /// Why no runner has claimed this request yet, as a plain sentence; null once claimed.
    pub waiting_reason: Option<String>,
    /// When a runner's poll first found the request unclaimable; null once claimed.
    pub waiting_since: Option<String>,
    /// The runs that hold every slot of the computers this request may run on, when no poll has
    /// given a reason because those computers are full; empty otherwise. Read at read time.
    pub waiting_behind: Vec<WaitingBehind>,
    /// The run's token limit and who applies it.
    pub limit: RunLimit,
    /// The profile the run was made with; null when the profile has since been deleted.
    pub agent_profile_id: Option<String>,
    /// The agent the run asked for; null when it left the choice to the computer.
    pub requested_harness_kind: Option<String>,
    /// The run's newest attempt ended without ever starting work, the same flag the run list
    /// carries (read through `tack_app::runs::run_facts`, the one rule).
    pub never_started: bool,
    /// What the verifier's pack for the run's newest attempt recommends (`merge`, `review` or
    /// `reject`); null while that attempt has not succeeded, or when it left no readable pack.
    #[schema(value_type = Option<tack_protocol::mrp::MergeDecision>)]
    pub verifier_recommendation: Option<tack_protocol::mrp::MergeDecision>,
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
    #[schema(value_type = String)]
    pub state: ExecutionState,
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
    /// The attempt ended without ever starting work: the computer refused it, or the team's
    /// ceiling did (`tack_core::run_outcome::never_started`, the one rule).
    pub never_started: bool,
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
    /// The "Run again at L3" button's state; present only on an attempt that ended asking for a
    /// person (`terminal_reason.code` of `needs_human`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerun_at_l3: Option<AttemptRerun>,
}

/// The "Run again at L3" button's state: `reason` says why it is disabled, `null` when enabled.
#[derive(Debug, Serialize, ToSchema)]
pub struct AttemptRerun {
    pub enabled: bool,
    pub reason: Option<String>,
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

/// Response body for `POST /api/executions/{id}/continue` — a new request that follows the same
/// docket task the named run left stopped at its own step budget (Phase 87, decision 6), or an
/// idempotent replay of one already made.
#[derive(Debug, Serialize, ToSchema)]
pub struct ContinueResponse {
    pub protocol_version: u32,
    pub request_id: String,
    #[schema(example = "queued")]
    pub state: String,
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
        .route(
            "/executions/{request_id}/continue",
            post(continue_execution),
        )
        .with_state(state)
}

/// `POST /api/executions/{id}/continue`: a new request that follows the same docket task `id`'s
/// run left stopped at its own step budget, instead of posting a new one (Phase 87, decision 6).
/// Refused `not_found` when `id` names no request, `unprocessable` when its own ending never
/// offered Continue (decision 5's table) or its token limit is fully spent.
#[utoipa::path(
    post,
    path = "/api/executions/{request_id}/continue",
    tag = "execution-operator",
    params(("request_id" = String, Path, description = "The id of the run to continue")),
    responses(
        (status = 200, description = "Continuation created or idempotently replayed", body = ContinueResponse),
        (status = 404, description = "not_found (the named run does not exist)", body = RunnerV1ErrorEnvelope),
        (status = 409, description = "idempotency_conflict", body = RunnerV1ErrorEnvelope),
        (status = 422, description = "unprocessable (never offered, or no tokens left)", body = RunnerV1ErrorEnvelope),
    ),
)]
pub async fn continue_execution(
    State(state): State<OperatorExecutionState>,
    Path(request_id): Path<ExecutionRequestId>,
) -> Result<Json<ContinueResponse>, (StatusCode, Json<Value>)> {
    let started = continue_run(&state.repo, state.clock.now(), request_id.as_str())
        .await
        .map_err(refused)?;
    Ok(Json(ContinueResponse {
        protocol_version: 1,
        request_id: started.request_id,
        state: "queued".into(),
        replayed: started.replayed,
    }))
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
        (status = 409, description = "conflict / idempotency_conflict / runner_revoked / group_not_runnable (an item that holds work is run only with a profile that plans or researches)", body = RunnerV1ErrorEnvelope),
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
        StatusCode::from_u16(refusal.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        refusal.code,
        &refusal.message,
        refusal.details,
    )
}

impl TryFrom<CreateExecution> for RunRequest {
    type Error = Refusal;

    fn try_from(input: CreateExecution) -> Result<Self, Refusal> {
        Ok(RunRequest {
            item_id: input.item_id,
            idempotency_key: input.idempotency_key,
            selector: start_run::selector_from(input.selector_kind, input.selector_id)?,
            agent_profile_id: input.agent_profile_id,
            requested_harness_kind: input.requested_harness_kind,
            requested_model: input.requested_model,
            agent_profile_snapshot: input.agent_profile_snapshot,
            repository_snapshot: input.repository_snapshot,
            permission_policy: start_run::PermissionRequest::from_value(input.permission_policy)?,
            budgets: input.budgets,
            environment: input.environment,
            metadata: input.metadata,
            timeout_seconds: input.timeout_seconds,
            verify: input.verify,
            push_branch: input.push_branch,
            autonomy: start_run::level_from(input.autonomy)?,
            origin: input.origin,
            from_run: input.from_run,
        })
    }
}

fn created(started: Started, named_removed_setting: bool) -> Json<CreateExecutionResponse> {
    Json(CreateExecutionResponse {
        protocol_version: 1,
        request_id: started.request_id,
        state: "queued".into(),
        replayed: started.replayed,
        notice: named_removed_setting.then(|| tack_core::models::STATUS_SET_BY_A_PERSON.to_owned()),
    })
}

/// `create_execution` for a caller that is the server itself: the principal is a string,
/// not a header, and the profile, agent and machine are resolved as for any other run.
pub(crate) async fn create_as(
    state: OperatorExecutionState,
    principal: String,
    input: CreateExecution,
) -> Result<Json<CreateExecutionResponse>, (StatusCode, Json<Value>)> {
    let named_removed_setting = input.status_map_policy_id.is_some();
    let input = RunRequest::try_from(input).map_err(refused)?;
    start_run(&state.repo, state.clock.now(), &principal, input)
        .await
        .map(|started| created(started, named_removed_setting))
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

/// How long a run that has ended stays in a project's list.
const ENDED_RUNS_LISTED_FOR: chrono::Duration = chrono::Duration::hours(24);

#[utoipa::path(
    get,
    path = "/api/executions",
    tag = "execution-operator",
    params(ListExecutionsQuery),
    responses(
        (status = 200, description = "Execution requests, newest first. `item_ids` present: exactly one row per id that has at least one execution, the batch's own most recent one. Else `project_id` present: every run of the project that is not over, and those that ended in the last 24 hours. Else scoped to one item when `item_id` is given, otherwise every request the install has recorded up to `limit`", body = ExecutionListResponse),
        (status = 400, description = "invalid_request (a malformed `item_ids` entry, or more ids than the route's cap)", body = RunnerV1ErrorEnvelope),
        (status = 404, description = "not_found (`project_id` names no project)", body = RunnerV1ErrorEnvelope),
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
    let now = state.clock.now();
    let rows = if let Some(item_ids) = item_ids {
        let item_ids: Vec<String> = item_ids.iter().map(Uuid::to_string).collect();
        state.repo.list_latest_executions_for_items(&item_ids).await
    } else if let Some(project_id) = query.project_id {
        match state.repo.get_project(project_id).await {
            Ok(Some(_)) => {}
            Ok(None) => {
                return Err(error(
                    StatusCode::NOT_FOUND,
                    StableErrorCode::NotFound,
                    "Project does not exist",
                    json!({"resource": "project"}),
                ));
            }
            Err(failure) => {
                tracing::error!(error = %failure, "Could not load project");
                return Err(error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    StableErrorCode::InternalError,
                    "Could not load project",
                    json!({}),
                ));
            }
        }
        state
            .repo
            .list_project_executions(&project_id.to_string(), now - ENDED_RUNS_LISTED_FOR)
            .await
    } else {
        let limit = query.effective_limit();
        let item_id = query.item_id.map(|id| id.to_string());
        state.repo.list_executions(item_id.as_deref(), limit).await
    }
    .map_err(|failure| {
        tracing::error!(error = %failure, "Could not list executions");
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not list executions",
            json!({}),
        )
    })?;
    let runners = queued_runners(
        &state.repo,
        rows.iter().any(|row| row.state == ExecutionState::Queued),
    )
    .await?;
    let holdings = full_runners(
        &state.repo,
        &runners,
        rows.iter()
            .any(|row| row.state == ExecutionState::Queued && row.waiting_reason.is_none()),
    )
    .await?;
    let ids: Vec<String> = rows.iter().map(|row| row.id.clone()).collect();
    let mut limits = run_limits_of(&state.repo, &ids, now)
        .await
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not read the runs' limits");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not read the runs' limits",
                json!({}),
            )
        })?;
    // One query for every listed run's newest attempt, never one per run.
    let mut hops: std::collections::HashMap<String, LatestHop> =
        execution_events::latest_hops_of_requests(&state.repo, &ids)
            .await
            .map_err(|failure| {
                tracing::error!(error = %failure, "Could not read the runs' latest hops");
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    StableErrorCode::InternalError,
                    "Could not read the runs' latest hops",
                    json!({}),
                )
            })?
            .into_iter()
            .filter_map(|hop| {
                let found = LatestHop::from_event(&hop.kind, hop.text.as_deref()?)?;
                Some((hop.request_id, found))
            })
            .collect();
    let ports = state.app.as_ref().map(|app| app.ports());
    let mut facts = tack_app::runs::run_facts::run_facts(
        &state.repo,
        ports.as_ref().map(|ports| ports.artifacts.as_ref()),
        &ids,
    )
    .await
    .map_err(|failure| {
        tracing::error!(error = %failure, "Could not read the runs' newest attempts");
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not read the runs' newest attempts",
            json!({}),
        )
    })?;
    let data: Vec<ExecutionSummary> = rows
        .into_iter()
        .map(|row| {
            let run_facts = facts.remove(&row.id).unwrap_or_default();
            let (behind, waiting_behind) =
                if row.state == ExecutionState::Queued && row.waiting_reason.is_none() {
                    waiting_behind(
                        &row.selector_kind,
                        &row.selector_id,
                        &runners,
                        &holdings,
                        now,
                    )
                } else {
                    (None, Vec::new())
                };
            ExecutionSummary {
                never_started: run_facts.never_started,
                verifier_recommendation: run_facts.verifier_recommendation,
                latest_hop: hops.remove(&row.id),
                limit: limits
                    .remove(&row.id)
                    .unwrap_or_else(|| limit_source(&Value::Null, None, None)),
                waiting_reason: queued_reason(
                    row.state,
                    row.waiting_reason,
                    behind,
                    &row.selector_kind,
                    &row.selector_id,
                    &runners,
                    now,
                ),
                waiting_behind,
                request_id: row.id,
                item_id: row.item_id,
                state: row.state,
                cancellation_requested_at: row.cancellation_requested_at,
                created_at: row.created_at,
                waiting_since: row.waiting_since,
                agent_profile_id: row.agent_profile_id,
                requested_harness_kind: row.requested_harness_kind,
            }
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

/// Whether a listed runner is online now: [`tack_app::scheduler_wiring::runner_is_online`] is the one rule,
/// this only reads the stored row into it.
pub(crate) fn runner_is_online(runner: &RunnerListingRow, now: DateTime<Utc>) -> bool {
    tack_app::scheduler_wiring::runner_is_online(
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
    repo.list_runners().await.map_err(|failure| {
        tracing::error!(error = %failure, "Could not list runners");
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not list runners",
            json!({}),
        )
    })
}

/// The runners a request's selector names: the one runner, or the members of the group (fleet).
fn named_runners<'a>(
    selector_kind: &str,
    selector_id: &str,
    runners: &'a [RunnerListingRow],
) -> Vec<&'a RunnerListingRow> {
    match selector_kind {
        "exact_runner" => runners.iter().filter(|r| r.id == selector_id).collect(),
        "fleet" => runners
            .iter()
            .filter(|r| r.fleet_ids.iter().any(|fleet| fleet == selector_id))
            .collect(),
        _ => Vec::new(),
    }
}

/// The open attempts of every runner with no free slot, by runner id. Read only when a queued
/// request has no reason from a poll: a full runner never polls, so nothing else explains it.
async fn full_runners(
    repo: &Repository,
    runners: &[RunnerListingRow],
    needed: bool,
) -> Result<BTreeMap<String, Vec<OpenAttemptRow>>, (StatusCode, Json<Value>)> {
    let mut held = BTreeMap::new();
    if !needed {
        return Ok(held);
    }
    for runner in runners.iter().filter(|r| is_full(r)) {
        let open = execution_attempts::open_attempts_of_runner(repo, &runner.id)
            .await
            .map_err(|failure| {
                tracing::error!(error = %failure, "Could not read what a runner is busy with");
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    StableErrorCode::InternalError,
                    "Could not read what a runner is busy with",
                    json!({}),
                )
            })?;
        held.insert(runner.id.clone(), open);
    }
    Ok(held)
}

/// Whether a runner can take work but has no free slot.
fn is_full(runner: &RunnerListingRow) -> bool {
    runner.state == "active"
        && runner.revoked_at.is_none()
        && runner.total_capacity > 0
        && runner.available_capacity <= 0
}

/// What a queued request with no reason from a poll waits behind: the sentence, and the runs it
/// is made from. Only when every computer the request may run on is full, since one with room
/// would take it. Nothing is stored.
fn waiting_behind(
    selector_kind: &str,
    selector_id: &str,
    runners: &[RunnerListingRow],
    full: &BTreeMap<String, Vec<OpenAttemptRow>>,
    now: DateTime<Utc>,
) -> (Option<String>, Vec<WaitingBehind>) {
    let named: Vec<&RunnerListingRow> = named_runners(selector_kind, selector_id, runners)
        .into_iter()
        .filter(|r| r.state == "active" && r.revoked_at.is_none())
        .collect();
    let parts: Option<Vec<(&str, i64, &[OpenAttemptRow])>> = named
        .iter()
        .map(|r| {
            full.get(&r.id)
                .map(|open| (r.name.as_str(), r.total_capacity, open.as_slice()))
        })
        .collect();
    let Some(parts) = parts.filter(|parts| !parts.is_empty()) else {
        return (None, Vec::new());
    };
    let Some(sentence) =
        tack_app::scheduler_wiring::busy_note(&parts, now, *chrono::Local::now().offset())
    else {
        return (None, Vec::new());
    };
    let behind = named
        .iter()
        .flat_map(|runner| {
            full[&runner.id].iter().map(|attempt| WaitingBehind {
                runner_id: runner.id.clone(),
                runner_name: runner.name.clone(),
                runs_at_a_time: runner.total_capacity,
                request_id: attempt.request_id.clone(),
                attempt_number: attempt.attempt_number,
                item_id: attempt.item_id.clone(),
                project_id: attempt.project_id.clone(),
                item_title: attempt.item_title.clone(),
                started_at: attempt.since.clone(),
                question: attempt.question.clone(),
                question_since: attempt.question_since.clone(),
            })
        })
        .collect();
    (Some(sentence), behind)
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
    let last = named_runners(selector_kind, selector_id, runners)
        .iter()
        .filter_map(|runner| runner.last_heartbeat_at.as_deref().and_then(parse_rfc3339))
        .max()?;
    // The scheduler's own rule for "answering": past its window, nothing in the group answers.
    (!tack_app::scheduler_wiring::runner_is_online(true, Some(last), now)).then_some(last)
}

/// A listed run's reason. A queued run waits for the computer it asked for when that has stopped
/// answering, and names when: a poll's stored reason is from before that, so it gives way. Otherwise
/// it shows what a poll stored; with nothing stored, what the full computers it may run on are busy
/// with, and failing that "no runner is online" when none is. Other
/// states show what is stored. Read at read time; never stored.
fn queued_reason(
    state: ExecutionState,
    stored: Option<String>,
    behind: Option<String>,
    selector_kind: &str,
    selector_id: &str,
    runners: &[RunnerListingRow],
    now: DateTime<Utc>,
) -> Option<String> {
    if state != ExecutionState::Queued {
        return stored;
    }
    if let Some(since) = silent_since(selector_kind, selector_id, runners, now) {
        return Some(format!(
            "The computer this run asked for has not answered since {}",
            since.to_rfc3339_opts(SecondsFormat::Secs, true)
        ));
    }
    stored.or(behind).or_else(|| {
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
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not load execution");
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
    let runners = queued_runners(&state.repo, request_state == ExecutionState::Queued).await?;
    let holdings = full_runners(
        &state.repo,
        &runners,
        request_state == ExecutionState::Queued && row.waiting_reason.is_none(),
    )
    .await?;
    let (behind, waiting_behind) = waiting_behind(
        &row.selector_kind,
        &row.selector_id,
        &runners,
        &holdings,
        now,
    );
    let request_id = row.id;
    let limit = run_limits_of(&state.repo, std::slice::from_ref(&request_id), now)
        .await
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not read the run's limit");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not read the run's limit",
                json!({}),
            )
        })?
        .remove(&request_id)
        .unwrap_or_else(|| limit_source(&Value::Null, None, None));
    let ports = state.app.as_ref().map(|app| app.ports());
    let run_facts = tack_app::runs::run_facts::run_facts(
        &state.repo,
        ports.as_ref().map(|ports| ports.artifacts.as_ref()),
        std::slice::from_ref(&request_id),
    )
    .await
    .map_err(|failure| {
        tracing::error!(error = %failure, "Could not read the run's newest attempt");
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not read the run's newest attempt",
            json!({}),
        )
    })?
    .remove(&request_id)
    .unwrap_or_default();
    Ok(Json(ExecutionDetailResponse {
        protocol_version: 1,
        never_started: run_facts.never_started,
        verifier_recommendation: run_facts.verifier_recommendation,
        request_id,
        limit,
        item_id: row.item_id,
        state: request_state,
        cancellation_requested_at: row.cancellation_requested_at,
        created_at: row.created_at,
        instructions: row.instructions,
        waiting_behind,
        waiting_reason: queued_reason(
            request_state,
            row.waiting_reason,
            behind,
            &row.selector_kind,
            &row.selector_id,
            &runners,
            now,
        ),
        waiting_since: row.waiting_since,
        agent_profile_id: row.agent_profile_id,
        requested_harness_kind: row.requested_harness_kind,
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
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not verify execution");
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
    let attempts = execution_attempts::attempts_with_outcomes(&state.repo, &request_id)
        .await
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not list attempts");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not list attempts",
                json!({}),
            )
        })?;
    let mut data = Vec::with_capacity(attempts.len());
    for AttemptWithOutcomes {
        attempt,
        pull_request,
        review,
        merge,
    } in attempts
    {
        let (enabled, reason) = match &state.app {
            Some(app) => tack_app::merge::button(&app.repo, &app.ports(), &attempt.id).await,
            None => (
                false,
                Some("Merging is not available on this board".to_owned()),
            ),
        };
        let merged = merge.and_then(|row| {
            row.merged_at.map(|merged_at| MergedBy {
                trigger: row.trigger,
                level: row.level,
                domain: row.domain,
                merged_at,
                merge_sha: row.merge_sha,
                policy_snapshot: row
                    .policy_snapshot
                    .and_then(|raw| serde_json::from_str::<Value>(&raw).ok()),
            })
        });
        let needs_human = attempt
            .terminal_reason
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .is_some_and(|reason| reason["code"] == "needs_human");
        let rerun_at_l3 = if needs_human {
            let (enabled, reason) = tack_app::autonomy::rerun_at_l3(&state.repo, &attempt.id).await;
            Some(AttemptRerun { enabled, reason })
        } else {
            None
        };
        data.push(attempt_summary(
            attempt,
            requested_model.as_deref(),
            pull_request.map(|pr| AttemptPullRequest {
                number: pr.number,
                url: pr.url,
                state: pr.state,
            }),
            review.map(|review| AttemptReview {
                verdict: review.verdict,
                note: review.note,
                reviewed_at: review.reviewed_at,
                pull_request_url: None,
            }),
            AttemptMerge {
                enabled,
                reason,
                merged,
            },
            rerun_at_l3,
        ));
    }
    Ok(Json(AttemptListResponse {
        protocol_version: 1,
        data,
    }))
}

/// One attempt's row of the operator's list: its stored columns read into their shapes, with
/// the provenance and economics derived from them.
fn attempt_summary(
    attempt: AttemptListingRow,
    requested_model: Option<&str>,
    pull_request: Option<AttemptPullRequest>,
    review: Option<AttemptReview>,
    merge: AttemptMerge,
    rerun_at_l3: Option<AttemptRerun>,
) -> AttemptSummary {
    // `derive_attempt_facts` is a convenience "service handler" —
    // takes the exact raw column shapes `AttemptListingRow` already
    // carries, so nothing here re-derives provenance/economics
    // logic. No runner infra cost-rate is stored anywhere in this
    // schema today, so `runner_rate_usd_per_hour` is always `None`
    // here — `runner_time_cost.cost_usd_estimated` stays honestly
    // `not_measured`, never a fabricated rate.
    let facts = derive_attempt_facts(
        requested_model,
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
    let never_started = tack_core::run_outcome::never_started(
        attempt.state,
        attempt
            .terminal_reason
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
            .as_ref(),
    );
    AttemptSummary {
        never_started,
        environment_names,
        sandbox,
        pull_request,
        review,
        merge,
        rerun_at_l3,
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
        &app.ports(),
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
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not verify execution");
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
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not list events");
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
    Path(request_id): Path<ExecutionRequestId>,
) -> Result<Json<CancellationRequestedResponse>, (StatusCode, Json<Value>)> {
    let outcome = cancel_run(&state.repo, state.clock.now(), &request_id)
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
        request_id: request_id.into_inner(),
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
    Path(request_id): Path<ExecutionRequestId>,
    Json(input): Json<RecoveryConfirmation>,
) -> Result<Json<RequeueResponse>, (StatusCode, Json<Value>)> {
    let actor = principal(&headers)?;
    let requeued = requeue(
        &state.repo,
        state.clock.now(),
        &request_id,
        &RecoveryKey::new(input.recovery_key.as_str()),
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
        request_id: request_id.into_inner(),
        state: execution_state::text(ExecutionState::Queued).into(),
        recovered_from: execution_state::text(ExecutionState::NeedsOperator).into(),
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
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not load item");
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
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not load project");
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
            .map_err(|failure| { tracing::error!(error = %failure, "Could not load agent profile"); 
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
    let brief = state
        .repo
        .get_item_brief(item_id)
        .await
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not load item brief");
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                StableErrorCode::InternalError,
                "Could not load item brief",
                json!({}),
            )
        })?;

    let comments = state.repo.list_comments(item_id).await.map_err(|failure| {
        tracing::error!(error = %failure, "Could not load item comments");
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not load item comments",
            json!({}),
        )
    })?;

    // Compose the instructions with the definition_of_done
    let composed = task_document(
        profile_instructions,
        &item,
        &comments,
        brief.as_ref(),
        project.definition_of_done.as_deref(),
        &kind_of(&item, &project),
    )
    .map(|task| render_instructions(&task))
    .map_err(|failure| {
        tracing::error!(error = %failure, "Could not serialize item brief");
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            StableErrorCode::InternalError,
            "Could not serialize item brief",
            json!({}),
        )
    })?;

    // The profile's limit, unless the dialog has its own: a different number is the run's, none is
    // "no limit". The sentence is still `limit_source`'s, over a snapshot of the same shape.
    let profile_tokens = profile["limits"]["budgets"]["tokens"].as_u64();
    let provisional = budgets::provisional_tokens(&state.repo, &project.id.to_string())
        .await
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not load the project's limits");
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
        .map_err(|failure| {
            tracing::error!(error = %failure, "Could not load the project's limits");
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

    // The lines are the ones the runner writes for docket's hop events.
    #[test]
    fn a_hop_is_read_from_the_line_the_runner_wrote() {
        let started =
            LatestHop::from_event("agent.hop_started", "Implementer (local/planned) started")
                .expect("started");
        assert_eq!(
            (
                started.role.as_str(),
                started.kind,
                started.verdict,
                started.cycle
            ),
            ("Implementer", HopKind::Started, None, None)
        );
        let read = |text: &str| LatestHop::from_event("agent.hop_finished", text).expect(text);
        let hop = read(
            "Reviewer (anthropic/claude-sonnet-4-6): REQUEST-CHANGES, cycle 1, 1000 tokens in, 200 out",
        );
        assert_eq!(hop.kind, HopKind::Finished);
        assert_eq!(hop.verdict.as_deref(), Some("REQUEST-CHANGES"));
        assert_eq!(hop.cycle, Some(1));
        // The first pass names no cycle; the gate and the cache figure are trailers too.
        let hop = read(
            "Lead (local/m): approved, gate tests, 900 tokens in (300 read from cache), 50 out",
        );
        assert_eq!(
            (hop.verdict.as_deref(), hop.cycle),
            (Some("approved"), None)
        );
        // A failed hop's error may hold the separator; it stays whole.
        let hop = read("Lead (local/m): failed: no tool, no answer, cycle 2");
        assert_eq!(hop.verdict.as_deref(), Some("failed: no tool, no answer"));
        assert_eq!(hop.cycle, Some(2));
        assert_eq!(
            read("Lead (no model named): done").verdict.as_deref(),
            Some("done")
        );
    }

    #[test]
    fn a_line_of_another_shape_is_not_a_hop() {
        for (kind, text) in [
            ("agent.hop_started", "Lead started"),
            ("agent.hop_started", "Lead (m) is going"),
            ("agent.hop_finished", "Lead (m) approved"),
            ("agent.hop_finished", " (m): approved"),
            ("agent.message", "Lead (m) started"),
        ] {
            assert_eq!(LatestHop::from_event(kind, text), None, "{kind}: {text}");
        }
    }

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
