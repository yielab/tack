//! Enqueueing: the request snapshot is parsed and validated, the request is stored, and the queue is read, both for listings and for what the
//! scheduler may hand out next.

use super::state::{self, Move};
use crate::StoreError;
use chrono::{DateTime, Utc};
use sqlx::{AssertSqlSafe, Row};
use tack_protocol::execution::ExecutionState;
use tracing::instrument;

use crate::Repository;
use crate::canonical_json;

#[derive(Debug, Clone)]
pub struct NewExecutionRequest<'a> {
    pub id: &'a str,
    pub item_id: &'a str,
    pub idempotency_scope: &'a str,
    pub idempotency_key: &'a str,
    /// A stable canonical representation of immutable request fields. Equal
    /// keys with different fingerprints are conflicts, never silent merges.
    pub request_fingerprint: &'a str,
    pub selector_kind: &'a str,
    pub selector_id: &'a str,
    pub agent_profile_id: Option<&'a str>,
    pub agent_profile_snapshot: &'a str,
    pub requested_harness_kind: Option<&'a str>,
    pub requested_model: Option<&'a str>,
    pub repository_snapshot: &'a str,
    pub permission_policy: &'a str,
    pub timeout_seconds: Option<i64>,
    pub budgets: &'a str,
    pub status_map_policy_id: Option<&'a str>,
    pub environment: &'a str,
    pub metadata: &'a str,
    pub request_snapshot: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnqueueResult {
    Created(String),
    Replayed(String),
    Conflict,
}

/// A read-only snapshot of one runner's scheduling-relevant state, used to
/// build a `tack_orch::scheduler::RunnerCandidate` one layer up (that type
/// cannot be constructed here — see [`RequestSelection`]'s doc comment for
/// why). Every field mirrors a live `agent_runners`/`agent_fleet_members`
/// column, not the runner's self-reported `capability_snapshot` alone,
/// because capacity/heartbeat are refreshed by every claim/heartbeat call
/// while the capability blob only changes on enroll/refresh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerSchedulingSnapshot {
    pub runner_id: String,
    /// `'pending_enrollment' | 'active' | 'revoked'` — see
    /// `crate::migrations`'s migration 040. Kept as the raw string; the
    /// caller owns mapping it to a typed enum (this crate must not depend
    /// on `tack-orch`'s `scheduler::RunnerState`).
    pub state: String,
    /// JSON object string (`agent_runners.labels`).
    pub labels: String,
    pub total_capacity: i64,
    pub available_capacity: i64,
    pub last_heartbeat_at: Option<String>,
    /// JSON string, validated at enroll/refresh time to parse as
    /// `tack_protocol::execution::EmbeddedCapabilitySnapshot` — see
    /// `crates/tack-api/src/handlers/runner_protocol.rs`'s
    /// `validate_capability_payload`. A runner that has never enrolled
    /// carries the column default `'{}'`, which does *not* parse as that
    /// type; the caller treats a parse failure as "no declared harnesses"
    /// rather than an error: unknown is explicit, not a
    /// crash.
    pub capability_snapshot: String,
    /// Every `agent_fleets.id` this runner currently belongs to, via
    /// `agent_fleet_members`.
    pub fleet_ids: Vec<String>,
}

/// A read-only view of one queued `execution_requests` row, carrying only
/// the columns scheduling needs. Exists for the same layering reason as
/// [`RunnerSchedulingSnapshot`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedRequestForScheduling {
    pub id: String,
    pub selector_kind: String,
    pub selector_id: String,
    /// Always `Some` for any request that passed `enqueue_execution`'s
    /// snapshot validation (the request snapshot's `requested_harness_kind`
    /// is a required field) — `Option` only because the raw column
    /// itself has no `NOT NULL` constraint (migration 044).
    pub requested_harness_kind: Option<String>,
    pub requested_model: Option<String>,
    pub created_at: String,
    /// JSON object string (`execution_requests.metadata`). The caller may
    /// read a `priority` key from this as a documented, best-effort
    /// convention — see `tack_app::scheduler_wiring`'s module doc for why
    /// no dedicated column exists yet.
    pub metadata: String,
    /// JSON object string (`execution_requests.permission_policy`). The
    /// caller reads an `approvals` key from this the same lenient way it
    /// reads `priority` off `metadata`.
    pub permission_policy: String,
    /// When a claim poll first found this request unclaimable; `None` until one has.
    pub waiting_since: Option<String>,
}

/// A fleet's configured concurrency ceiling alongside its current, observed
/// in-flight usage — `agent_fleets.concurrency_limit` (migration 039) has
/// never been enforced anywhere until this snapshot gave a caller something
/// to enforce it against. `in_use` is the sum of `total_capacity -
/// available_capacity` across every member runner: every unit of capacity a
/// member runner currently has reserved, regardless of which specific
/// request or selector claimed it — a fleet-wide ceiling is a statement
/// about the fleet's aggregate load, not only load arriving through the
/// `fleet` selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FleetConcurrencySnapshot {
    pub concurrency_limit: Option<i64>,
    pub in_use: i64,
}

/// One row of `GET /api/executions` (unscoped, one-item, or the
/// `item_ids` batch) or `GET /api/executions/{id}`'s detail — the scalar
/// columns this surface exposes, plus the selector a queued run waits on.
/// Deliberately thinner than `execution_requests`' full row (no agent profile,
/// requested harness/model, repository/permission/budget/timeout/metadata columns).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionListingRow {
    pub id: String,
    pub item_id: String,
    pub state: ExecutionState,
    pub cancellation_requested_at: Option<String>,
    pub created_at: String,
    pub waiting_reason: Option<String>,
    pub waiting_since: Option<String>,
    /// `exact_runner` or `fleet`, with the runner or fleet id in `selector_id`.
    pub selector_kind: String,
    pub selector_id: String,
}

/// Reads one row of the listing's `SELECT`; a state no variant names is a corrupt row.
fn listing_row(row: sqlx::sqlite::SqliteRow) -> Result<ExecutionListingRow, StoreError> {
    Ok(ExecutionListingRow {
        id: row.get("id"),
        item_id: row.get("item_id"),
        state: state::parse(&row.get::<String, _>("state"))?,
        cancellation_requested_at: row.get("cancellation_requested_at"),
        created_at: row.get("created_at"),
        waiting_reason: row.get("waiting_reason"),
        selector_kind: row.get("selector_kind"),
        selector_id: row.get("selector_id"),
        waiting_since: row.get("waiting_since"),
    })
}

/// One row of `GET /api/executions/{request_id}/attempts` — every column
/// `execution_attempts` carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptListingRow {
    pub id: String,
    pub request_id: String,
    pub attempt_number: i64,
    pub runner_id: String,
    pub fencing_token: i64,
    pub state: ExecutionState,
    pub lease_issued_at: String,
    pub lease_expires_at: String,
    pub last_heartbeat_at: Option<String>,
    pub event_checkpoint: Option<String>,
    pub completion_id: Option<String>,
    pub workspace_id: Option<String>,
    pub base_revision: Option<String>,
    pub actual_execution: Option<String>,
    pub terminal_reason: Option<String>,
    pub usage: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// JSON the runner reported when the attempt started running.
    pub start_report: Option<String>,
}

/// A request snapshot as an idempotent retry is compared: what the task is now left out. The
/// task's own metadata (its type, priority and trust, which the server writes into
/// `metadata.item`) and the instructions composed from its text (title, description, comments)
/// change when the card does, so a retry after an edit answers the request it made, not a
/// conflict. Everything else the request says still counts.
pub fn as_compared(snapshot: &serde_json::Value) -> serde_json::Value {
    let mut value = snapshot.clone();
    if let Some(metadata) = value.get_mut("metadata").and_then(|m| m.as_object_mut()) {
        metadata.remove("item");
    }
    if let Some(profile) = value
        .get_mut("resolved_agent_profile")
        .and_then(|p| p.as_object_mut())
    {
        profile.remove("instructions");
    }
    value
}

/// Whether two stored or submitted request snapshots name the same request, ignoring the task's
/// metadata. An unreadable snapshot is never the same request.
fn same_request(stored: &str, request: &str) -> bool {
    match (
        serde_json::from_str::<serde_json::Value>(stored),
        serde_json::from_str::<serde_json::Value>(request),
    ) {
        (Ok(stored), Ok(request)) => as_compared(&stored) == as_compared(&request),
        _ => false,
    }
}

/// Whether a stored snapshot's limit was written by the claim from the day's room.
fn given_the_days_room(snapshot: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(snapshot)
        .is_ok_and(|value| value["metadata"]["limit_source"] == "day")
}

fn snapshot_error(message: impl Into<String>) -> StoreError {
    StoreError::malformed(format!(
        "invalid execution request snapshot: {}",
        message.into()
    ))
}

fn snapshot_object<'a>(
    value: &'a serde_json::Value,
    field: &str,
) -> Result<&'a serde_json::Map<String, serde_json::Value>, StoreError> {
    value
        .as_object()
        .ok_or_else(|| snapshot_error(format!("{field} must be an object")))
}

fn snapshot_field<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<&'a serde_json::Value, StoreError> {
    object
        .get(field)
        .ok_or_else(|| snapshot_error(format!("missing {field}")))
}

fn snapshot_string(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<String, StoreError> {
    snapshot_field(object, field)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| snapshot_error(format!("{field} must be a string")))
}

fn snapshot_nullable_string(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<Option<String>, StoreError> {
    match snapshot_field(object, field)? {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(value) => Ok(Some(value.clone())),
        _ => Err(snapshot_error(format!("{field} must be string or null"))),
    }
}

pub(super) fn parse_execution_request_snapshot(
    serialized: &str,
) -> Result<serde_json::Value, StoreError> {
    let value: serde_json::Value =
        serde_json::from_str(serialized).map_err(|error| snapshot_error(error.to_string()))?;
    let root = snapshot_object(&value, "snapshot")?;
    for field in [
        "request_id",
        "item_id",
        "idempotency_key",
        "agent_profile_id",
        "requested_harness_kind",
    ] {
        snapshot_string(root, field)?;
    }
    let created_by = snapshot_object(snapshot_field(root, "created_by")?, "created_by")?;
    snapshot_string(created_by, "source")?;
    snapshot_string(created_by, "subject_id")?;
    DateTime::parse_from_rfc3339(&snapshot_string(root, "created_at")?)
        .map_err(|error| snapshot_error(format!("created_at must be RFC3339: {error}")))?;
    let selector = snapshot_object(snapshot_field(root, "selector")?, "selector")?;
    match snapshot_string(selector, "kind")?.as_str() {
        "exact_runner" => {
            snapshot_string(selector, "runner_id")?;
        }
        "fleet" => {
            snapshot_string(selector, "fleet_id")?;
        }
        "any" => {}
        _ => return Err(snapshot_error("selector.kind is unsupported")),
    }
    let profile = snapshot_object(
        snapshot_field(root, "resolved_agent_profile")?,
        "resolved_agent_profile",
    )?;
    snapshot_string(profile, "name")?;
    snapshot_string(profile, "instructions")?;
    snapshot_object(
        snapshot_field(profile, "tool_policy")?,
        "resolved_agent_profile.tool_policy",
    )?;
    snapshot_object(
        snapshot_field(profile, "budgets")?,
        "resolved_agent_profile.budgets",
    )?;
    snapshot_field(profile, "timeout_seconds")?
        .as_u64()
        .ok_or_else(|| snapshot_error("resolved_agent_profile.timeout_seconds must be u64"))?;
    snapshot_nullable_string(root, "requested_model")?;
    let repository = snapshot_object(snapshot_field(root, "repository")?, "repository")?;
    for field in ["kind", "remote", "base_revision"] {
        snapshot_string(repository, field)?;
    }
    snapshot_nullable_string(repository, "subdirectory")?;
    let policy = snapshot_object(
        snapshot_field(root, "permission_policy")?,
        "permission_policy",
    )?;
    let tools = snapshot_field(policy, "tools")?
        .as_array()
        .ok_or_else(|| snapshot_error("permission_policy.tools must be an array"))?;
    if tools.iter().any(|tool| tool.as_str().is_none()) {
        return Err(snapshot_error(
            "permission_policy.tools entries must be strings",
        ));
    }
    snapshot_field(policy, "network")?
        .as_bool()
        .ok_or_else(|| snapshot_error("permission_policy.network must be bool"))?;
    snapshot_field(root, "timeout_seconds")?
        .as_u64()
        .ok_or_else(|| snapshot_error("timeout_seconds must be u64"))?;
    snapshot_object(snapshot_field(root, "budgets")?, "budgets")?;
    snapshot_nullable_string(root, "status_map_policy_id")?;
    let environment = snapshot_object(snapshot_field(root, "environment")?, "environment")?;
    for value in environment.values() {
        let environment_value = snapshot_object(value, "environment value")?;
        let value = snapshot_nullable_string(environment_value, "value")?;
        let secret_reference = snapshot_nullable_string(environment_value, "secret_reference")?;
        if value.is_some() == secret_reference.is_some() {
            return Err(snapshot_error(
                "environment entry must contain exactly one value or secret_reference",
            ));
        }
    }
    snapshot_object(snapshot_field(root, "metadata")?, "metadata")?;
    Ok(value)
}

pub(super) fn validate_execution_request_snapshot(
    input: &NewExecutionRequest<'_>,
) -> Result<String, StoreError> {
    let snapshot = parse_execution_request_snapshot(input.request_snapshot)?;
    let root = snapshot_object(&snapshot, "snapshot")?;
    let matches = |field: &str, expected: &str| -> Result<(), StoreError> {
        if snapshot_string(root, field)? == expected {
            Ok(())
        } else {
            Err(snapshot_error(format!(
                "{field} contradicts normalized request"
            )))
        }
    };
    matches("request_id", input.id)?;
    matches("item_id", input.item_id)?;
    matches("idempotency_key", input.idempotency_key)?;
    let selector = snapshot_object(snapshot_field(root, "selector")?, "selector")?;
    if snapshot_string(selector, "kind")? != input.selector_kind {
        return Err(snapshot_error("selector contradicts normalized request"));
    }
    let selector_matches = match input.selector_kind {
        "exact_runner" => snapshot_string(selector, "runner_id")? == input.selector_id,
        "fleet" => snapshot_string(selector, "fleet_id")? == input.selector_id,
        "any" => input.selector_id.is_empty(),
        _ => false,
    };
    if !selector_matches {
        return Err(snapshot_error("selector contradicts normalized request"));
    }
    if input.agent_profile_id != Some(snapshot_string(root, "agent_profile_id")?.as_str())
        || input.requested_harness_kind
            != Some(snapshot_string(root, "requested_harness_kind")?.as_str())
        || input.requested_model.map(str::to_owned)
            != snapshot_nullable_string(root, "requested_model")?
        || input.status_map_policy_id.map(str::to_owned)
            != snapshot_nullable_string(root, "status_map_policy_id")?
    {
        return Err(snapshot_error(
            "requested fields contradict normalized request",
        ));
    }
    let timeout = input
        .timeout_seconds
        .filter(|value| *value >= 0)
        .map(|value| value as u64);
    if timeout != snapshot_field(root, "timeout_seconds")?.as_u64() {
        return Err(snapshot_error(
            "timeout_seconds contradicts normalized request",
        ));
    }
    for (snapshot_field_name, normalized) in [
        ("resolved_agent_profile", input.agent_profile_snapshot),
        ("repository", input.repository_snapshot),
        ("permission_policy", input.permission_policy),
        ("budgets", input.budgets),
        ("environment", input.environment),
        ("metadata", input.metadata),
    ] {
        let normalized: serde_json::Value =
            serde_json::from_str(normalized).map_err(|error| snapshot_error(error.to_string()))?;
        if snapshot_field(root, snapshot_field_name)? != &normalized {
            return Err(snapshot_error(format!(
                "{snapshot_field_name} contradicts normalized request"
            )));
        }
    }
    serde_json::to_string(&canonical_json(snapshot))
        .map_err(|error| snapshot_error(error.to_string()))
}

fn snapshot_created_at_matches_now(snapshot: &str, now: &str) -> Result<(), StoreError> {
    let snapshot = parse_execution_request_snapshot(snapshot)?;
    let root = snapshot_object(&snapshot, "snapshot")?;
    let snapshot_created_at =
        DateTime::parse_from_rfc3339(&snapshot_string(root, "created_at")?)
            .map_err(|error| snapshot_error(format!("created_at must be RFC3339: {error}")))?;
    let normalized_created_at = DateTime::parse_from_rfc3339(now).map_err(|error| {
        snapshot_error(format!("normalized created_at must be RFC3339: {error}"))
    })?;
    if snapshot_created_at != normalized_created_at {
        return Err(snapshot_error("created_at contradicts normalized request"));
    }
    Ok(())
}

impl Repository {
    /// Read-only scheduling input: this runner's own current state plus its
    /// fleet memberships. `None` if `runner_id` does not exist. See
    /// [`RequestSelection`]'s doc comment for why this is a plain read
    /// rather than something `tack-orch` fetches itself.
    pub async fn fetch_runner_scheduling_snapshot(
        &self,
        runner_id: &str,
    ) -> Result<Option<RunnerSchedulingSnapshot>, StoreError> {
        let row = sqlx::query(
            "SELECT state, labels, total_capacity, available_capacity, last_heartbeat_at, \
             capability_snapshot FROM agent_runners WHERE id = ?",
        )
        .bind(runner_id)
        .fetch_optional(self.pool())
        .await?;
        let Some(row) = row else { return Ok(None) };
        let fleet_ids: Vec<String> =
            sqlx::query_scalar("SELECT fleet_id FROM agent_fleet_members WHERE runner_id = ?")
                .bind(runner_id)
                .fetch_all(self.pool())
                .await?;
        Ok(Some(RunnerSchedulingSnapshot {
            runner_id: runner_id.to_string(),
            state: row.get("state"),
            labels: row.get("labels"),
            total_capacity: row.get("total_capacity"),
            available_capacity: row.get("available_capacity"),
            last_heartbeat_at: row.get("last_heartbeat_at"),
            capability_snapshot: row.get("capability_snapshot"),
            fleet_ids,
        }))
    }

    /// Read-only scheduling input: every `queued` request this runner is
    /// selector-eligible for (exact match, or fleet membership) — the exact
    /// same `WHERE` clause `claim_execution_idempotent_with_snapshot`'s
    /// naive path uses, so the scheduler considers precisely the same
    /// candidate pool the pre-Wave-4 query did, only with real eligibility
    /// filtering applied on top instead of a bare `ORDER BY created_at`.
    pub async fn list_eligible_queued_requests(
        &self,
        runner_id: &str,
    ) -> Result<Vec<QueuedRequestForScheduling>, StoreError> {
        let rows = sqlx::query(state::sql(
            "SELECT id, selector_kind, selector_id, requested_harness_kind, \
             requested_model, created_at, metadata, \
             permission_policy, waiting_since \
             FROM execution_requests WHERE state={queued} AND \
             ((selector_kind='exact_runner' AND selector_id=?) OR \
             (selector_kind='fleet' AND EXISTS(SELECT 1 FROM agent_fleet_members m \
             WHERE m.fleet_id=selector_id AND m.runner_id=?))) ORDER BY created_at",
        ))
        .bind(runner_id)
        .bind(runner_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| QueuedRequestForScheduling {
                id: row.get("id"),
                selector_kind: row.get("selector_kind"),
                selector_id: row.get("selector_id"),
                requested_harness_kind: row.get("requested_harness_kind"),
                requested_model: row.get("requested_model"),
                created_at: row.get("created_at"),
                metadata: row.get("metadata"),
                permission_policy: row.get("permission_policy"),
                waiting_since: row.get("waiting_since"),
            })
            .collect())
    }

    /// Records why a poll left this queued request unselected: the code and
    /// the plain sentence, the latest poll's word replacing the last one's.
    /// While `blocking`, `waiting_since` is set the first time and kept after
    /// that; a reason that is not blocking (a runner that could take it once
    /// it has room) clears it, so the clock only runs while no runner can.
    pub async fn record_request_waiting(
        &self,
        request_id: &str,
        code: &str,
        reason: &str,
        blocking: bool,
        now: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        sqlx::query(state::sql("UPDATE execution_requests SET waiting_code=?,waiting_reason=?,waiting_since=CASE WHEN ? THEN COALESCE(waiting_since,?) ELSE NULL END WHERE id=? AND state={queued}"))
            .bind(code).bind(reason).bind(blocking).bind(now.to_rfc3339()).bind(request_id)
            .execute(self.pool()).await?;
        Ok(())
    }

    /// Fails a queued request no runner could claim: `state` becomes `failed`,
    /// `waiting_code` holds the terminal code and `waiting_reason` the message.
    /// `false` when the request was claimed or cancelled first.
    pub async fn fail_unclaimable_request(
        &self,
        request_id: &str,
        code: &str,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<bool, StoreError> {
        let step = Move::FailUnclaimable;
        let updated = sqlx::query(state::sql(&format!("UPDATE execution_requests SET {},waiting_code=?,waiting_reason=?,updated_at=? WHERE id=? AND {}", step.set(), step.guard())))
            .bind(code).bind(reason).bind(now.to_rfc3339()).bind(request_id)
            .execute(self.pool()).await?;
        Ok(updated.rows_affected() == 1)
    }

    /// Read-only scheduling input: `fleet_id`'s configured
    /// `concurrency_limit` alongside its current aggregate in-use capacity.
    /// `None` if `fleet_id` does not exist.
    pub async fn fetch_fleet_concurrency(
        &self,
        fleet_id: &str,
    ) -> Result<Option<FleetConcurrencySnapshot>, StoreError> {
        let row = sqlx::query("SELECT concurrency_limit FROM agent_fleets WHERE id = ?")
            .bind(fleet_id)
            .fetch_optional(self.pool())
            .await?;
        let Some(row) = row else { return Ok(None) };
        let concurrency_limit: Option<i64> = row.get("concurrency_limit");
        let in_use: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(r.total_capacity - r.available_capacity),0) FROM agent_runners r \
             JOIN agent_fleet_members m ON m.runner_id = r.id WHERE m.fleet_id = ?",
        )
        .bind(fleet_id)
        .fetch_one(self.pool())
        .await?;
        Ok(Some(FleetConcurrencySnapshot {
            concurrency_limit,
            in_use,
        }))
    }

    /// Backs the unscoped and one-item shapes of `GET /api/executions` —
    /// newest first, bounded by `limit` either way. `item_id`, when given,
    /// scopes the result to that one item's requests instead of every
    /// request the install has ever recorded.
    #[instrument(skip(self))]
    pub async fn list_executions(
        &self,
        item_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<ExecutionListingRow>, StoreError> {
        let rows = match item_id {
            Some(item_id) => {
                sqlx::query("SELECT id, item_id, state, cancellation_requested_at, created_at, waiting_reason, waiting_since, selector_kind, selector_id FROM execution_requests WHERE item_id = ? ORDER BY created_at DESC LIMIT ?")
                    .bind(item_id)
                    .bind(limit)
                    .fetch_all(self.pool())
                    .await?
            }
            None => {
                sqlx::query("SELECT id, item_id, state, cancellation_requested_at, created_at, waiting_reason, waiting_since, selector_kind, selector_id FROM execution_requests ORDER BY created_at DESC LIMIT ?")
                    .bind(limit)
                    .fetch_all(self.pool())
                    .await?
            }
        };
        rows.into_iter().map(listing_row).collect()
    }

    /// Backs the `item_ids` batch shape of `GET /api/executions` — exactly
    /// one row per id in `item_ids` that has at least one execution
    /// request (an id with none is simply absent, never padded with a
    /// null row), carrying only its most recent request by `created_at`.
    /// A window function, not `GROUP BY item_id HAVING MAX(created_at)`:
    /// two requests for the same item can share a `created_at` value, and
    /// only `id DESC` as a tiebreak keeps the choice deterministic across
    /// repeated calls. `Ok(vec![])` for an empty `item_ids` rather than
    /// issuing a query with an empty `IN ()`, which is invalid SQL —
    /// matching [`Repository::list_dependencies_for_items`]'s guard.
    #[instrument(skip(self))]
    pub async fn list_latest_executions_for_items(
        &self,
        item_ids: &[String],
    ) -> Result<Vec<ExecutionListingRow>, StoreError> {
        if item_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = item_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT id, item_id, state, cancellation_requested_at, created_at, waiting_reason, waiting_since, selector_kind, selector_id FROM ( \
                 SELECT id, item_id, state, cancellation_requested_at, created_at, waiting_reason, waiting_since, selector_kind, selector_id, \
                        ROW_NUMBER() OVER (PARTITION BY item_id ORDER BY created_at DESC, id DESC) AS rn \
                 FROM execution_requests WHERE item_id IN ({placeholders}) \
             ) WHERE rn = 1"
        );
        let mut q = sqlx::query(AssertSqlSafe(query));
        for id in item_ids {
            q = q.bind(id);
        }
        let rows = q.fetch_all(self.pool()).await?;
        rows.into_iter().map(listing_row).collect()
    }

    /// Every attempt ever made against `request_id`, oldest first. Backs
    /// `GET /api/executions/{request_id}/attempts` — `execution_attempts`
    /// (migration 045) has been written by the runner-v1 protocol with no
    /// operator read path until now.
    pub async fn list_attempts_for_request(
        &self,
        request_id: &str,
    ) -> Result<Vec<AttemptListingRow>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, request_id, attempt_number, runner_id, fencing_token, state, \
             lease_issued_at, lease_expires_at, last_heartbeat_at, event_checkpoint, \
             completion_id, workspace_id, base_revision, actual_execution, terminal_reason, \
             usage, started_at, ended_at, created_at, updated_at, start_report FROM execution_attempts \
             WHERE request_id = ? ORDER BY attempt_number",
        )
        .bind(request_id)
        .fetch_all(self.pool())
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(AttemptListingRow {
                    id: row.get("id"),
                    request_id: row.get("request_id"),
                    attempt_number: row.get("attempt_number"),
                    runner_id: row.get("runner_id"),
                    fencing_token: row.get("fencing_token"),
                    state: state::parse(&row.get::<String, _>("state"))?,
                    lease_issued_at: row.get("lease_issued_at"),
                    lease_expires_at: row.get("lease_expires_at"),
                    last_heartbeat_at: row.get("last_heartbeat_at"),
                    event_checkpoint: row.get("event_checkpoint"),
                    completion_id: row.get("completion_id"),
                    workspace_id: row.get("workspace_id"),
                    base_revision: row.get("base_revision"),
                    actual_execution: row.get("actual_execution"),
                    terminal_reason: row.get("terminal_reason"),
                    usage: row.get("usage"),
                    started_at: row.get("started_at"),
                    ended_at: row.get("ended_at"),
                    created_at: row.get("created_at"),
                    updated_at: row.get("updated_at"),
                    start_report: row.get("start_report"),
                })
            })
            .collect()
    }

    #[instrument(skip(self, input))]
    pub async fn enqueue_execution(
        &self,
        input: NewExecutionRequest<'_>,
        now: DateTime<Utc>,
    ) -> Result<EnqueueResult, StoreError> {
        let now = now.to_rfc3339();
        let request_snapshot = validate_execution_request_snapshot(&input)?;
        // Serialize duplicate enqueue requests for the same idempotency key
        // (e.g. a client retrying an unacknowledged enqueue POST) before
        // either transaction reads the idempotency-scope row. Two deferred
        // readers can otherwise deadlock while both try to upgrade to
        // writers. Mirrors the redeem_enrollment_token and
        // claim_execution_idempotent_with_snapshot fixes above.
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let result = enqueue_in(&mut tx, &input, &request_snapshot, &now).await?;
        tx.commit().await?;
        Ok(result)
    }
}

/// The body of [`Repository::enqueue_execution`] over a transaction the caller opened with
/// `BEGIN IMMEDIATE` and ends, so a completion can enqueue its follow-up in its own transaction.
/// `request_snapshot` is what [`validate_execution_request_snapshot`] returned for `input`.
pub(super) async fn enqueue_in(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    input: &NewExecutionRequest<'_>,
    request_snapshot: &str,
    now: &str,
) -> Result<EnqueueResult, StoreError> {
    let existing: Option<(String, String, String)> = sqlx::query_as(
        "SELECT id, request_fingerprint, request_snapshot FROM execution_requests \
         WHERE idempotency_scope = ? AND idempotency_key = ?",
    )
    .bind(input.idempotency_scope)
    .bind(input.idempotency_key)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((id, fingerprint, stored_snapshot)) = existing {
        return Ok(
            // The claim may since have given a limitless run what was left of the day
            // (`Repository::limit_request_to_day_room`); the fingerprint is still the one it was sent with.
            if fingerprint == input.request_fingerprint
                && (same_request(&stored_snapshot, request_snapshot)
                    || given_the_days_room(&stored_snapshot))
            {
                EnqueueResult::Replayed(id)
            } else {
                EnqueueResult::Conflict
            },
        );
    }
    snapshot_created_at_matches_now(request_snapshot, now)?;
    sqlx::query(
        "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key, \
         request_fingerprint, selector_kind, selector_id, agent_profile_id, agent_profile_snapshot, \
         requested_harness_kind, requested_model, repository_snapshot, \
         permission_policy, timeout_seconds, budgets, status_map_policy_id, environment, metadata, request_snapshot, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(input.id).bind(input.item_id).bind(input.idempotency_scope).bind(input.idempotency_key)
    .bind(input.request_fingerprint).bind(input.selector_kind).bind(input.selector_id)
    .bind(input.agent_profile_id).bind(input.agent_profile_snapshot).bind(input.requested_harness_kind)
    .bind(input.requested_model).bind(input.repository_snapshot)
    .bind(input.permission_policy).bind(input.timeout_seconds).bind(input.budgets)
    .bind(input.status_map_policy_id).bind(input.environment).bind(input.metadata).bind(request_snapshot).bind(now).bind(now)
    .execute(&mut **tx).await?;
    Ok(EnqueueResult::Created(input.id.to_string()))
}
