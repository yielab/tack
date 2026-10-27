//! Live wiring between the pure `tack_orch::scheduler` decision functions and real `agent_runners`/`agent_fleet_members`/
//! `execution_requests` rows.
//!
//! [`choose_request_for_runner`] is the one entry point: the claim handler
//! calls it with a live `&Repository`, gets back the request id (if any) to
//! claim, and passes that into `claim_execution_idempotent_with_snapshot` —
//! the only thing that actually grants a fenced lease. Its queries are plain
//! `SELECT`s (`tack-db` cannot depend on `tack-orch`, so the scheduler can't
//! run inside the claim transaction itself — see `RequestSelection`'s doc in
//! `tack-db`), with one exception: a request the poll finds unclaimable gets
//! the reason written to its row, and is failed once it has waited longer
//! than [`SchedulingPolicy::unclaimable_fail_after`]. Selecting a request
//! clears the reason inside the claim transaction itself.
//!
//! Papers over two schema gaps: no `priority` column on `execution_requests`
//! yet ([`priority_from_metadata`]), and no cross-runner fleet concurrency
//! ceiling in the pure scheduler's eligibility model ([`fleet_is_saturated`]).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use tack_db::repo::execution::{FleetConcurrencySnapshot, QueuedRequestForScheduling};
use tack_db::{Repository, StoreError, queries};
use tack_orch::scheduler::select::{EXPIRED_LEASE_GRACE, SchedulingPolicy, select_runner};
use tack_orch::scheduler::types::{
    BudgetKind, IneligibleReason, Priority, RunnerCandidate, RunnerState, SchedulingRequest,
    SelectionOutcome,
};
use tack_protocol::execution::{
    Approvals, EmbeddedCapabilitySnapshot, ExecutionRequestId, HarnessKind, RunnerId,
};

/// Whether a runner is online as of `now`: the scheduler's own rule (active, and a heartbeat
/// within its window), for readers that show the answer without selecting anything.
pub fn runner_is_online(
    active: bool,
    last_heartbeat_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> bool {
    SchedulingPolicy::default().is_online(active, last_heartbeat_at, now)
}

/// Maps `agent_runners.state`'s three literal values (migration 040) to the
/// typed [`RunnerState`]. An unrecognised string (never written by this
/// codebase, but the raw `TEXT` column has no enum constraint) maps to
/// [`RunnerState::Revoked`] — unsupported is typed, unknown is explicit,
/// rather than treating unknown data as schedulable.
fn runner_state_from_str(state: &str) -> RunnerState {
    match state {
        "pending_enrollment" => RunnerState::PendingEnrollment,
        "active" => RunnerState::Active,
        _ => RunnerState::Revoked,
    }
}

/// Reads an optional `{"priority": "low"|"normal"|"high"}` convention out
/// of a request's `metadata` JSON — no `execution_requests` column carries
/// priority today. Never errors: anything else (missing key, wrong type,
/// malformed JSON, unrecognised value) yields [`Priority::Normal`].
fn priority_from_metadata(metadata_json: &str) -> Priority {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(metadata_json) else {
        return Priority::Normal;
    };
    match value.get("priority").and_then(serde_json::Value::as_str) {
        Some(raw) if raw.eq_ignore_ascii_case("low") => Priority::Low,
        Some(raw) if raw.eq_ignore_ascii_case("high") => Priority::High,
        _ => Priority::Normal,
    }
}

/// Reads `permission_policy.approvals` out of a queued row's stored JSON —
/// the scheduling-time counterpart of `priority_from_metadata` above, same
/// leniency: a missing key, malformed JSON, or unrecognised value all mean
/// `None`, which the pure scheduler already treats identically to
/// [`Approvals::Auto`], never a scheduling failure.
fn approvals_from_permission_policy(policy_json: &str) -> Option<Approvals> {
    let value: serde_json::Value = serde_json::from_str(policy_json).ok()?;
    serde_json::from_value(value.get("approvals")?.clone()).ok()
}

/// Parses `created_at` (always RFC 3339 — every write path in
/// `tack-db/src/repo/execution.rs` stores `DateTime<Utc>::to_rfc3339()`),
/// falling back to `now` only if a row is somehow malformed, so one bad row
/// degrades to "scheduled as if it just arrived" rather than panicking.
fn parse_created_at(raw: &str, now: DateTime<Utc>) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(raw)
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or(now)
}

/// Whether `fleet_id`'s configured `concurrency_limit` is already reached or
/// exceeded, pre-filtering fleet-selector requests before they reach the
/// pure scheduler (whose per-runner model has no fleet-wide ceiling). `None`
/// limit means no ceiling; no snapshot at all is treated as saturated — the
/// conservative reading for "could not prove this fleet has room."
fn fleet_is_saturated(snapshot: Option<&FleetConcurrencySnapshot>) -> bool {
    match snapshot {
        None => true,
        Some(FleetConcurrencySnapshot {
            concurrency_limit: None,
            ..
        }) => false,
        Some(FleetConcurrencySnapshot {
            concurrency_limit: Some(limit),
            in_use,
        }) => *in_use >= *limit,
    }
}

/// Builds a [`SchedulingRequest`] from a raw queued-request row, or `None` if
/// the row can't be turned into one (an absent `requested_harness_kind` —
/// never produced by validated writes, but the raw column has no `NOT
/// NULL`). That case removes this one
/// request from consideration rather than failing the whole claim; it stays
/// queued and visible via `GET /executions` until fixed or requeued.
fn build_scheduling_request(
    row: &QueuedRequestForScheduling,
    budget_reached: Option<BudgetKind>,
    now: DateTime<Utc>,
) -> Option<SchedulingRequest> {
    let harness_kind = row.requested_harness_kind.as_deref()?;
    let selector = match row.selector_kind.as_str() {
        "exact_runner" => tack_protocol::execution::RunnerSelector::ExactRunner {
            runner_id: RunnerId::new(row.selector_id.clone()),
        },
        "fleet" => tack_protocol::execution::RunnerSelector::Fleet {
            fleet_id: row.selector_id.clone(),
        },
        _ => return None,
    };
    Some(SchedulingRequest {
        request_id: ExecutionRequestId::new(row.id.clone()),
        selector,
        priority: priority_from_metadata(&row.metadata),
        requested_harness_kind: HarnessKind::new(harness_kind),
        requested_model: row.requested_model.clone(),
        approvals: approvals_from_permission_policy(&row.permission_policy),
        required_labels: BTreeMap::new(),
        budget_reached,
        created_at: parse_created_at(&row.created_at, now),
    })
}

/// The code a request held back by a project budget is recorded under. It is
/// never failed for waiting: the budget resets, or a human clears the queue.
const BUDGET_REACHED: &str = "budget_reached";

/// Which of the project's budgets, if any, forbids starting new work now.
/// A project with no `project_budgets` row has none. The token branch reads
/// `queries::budgets::day_room`: the day is closed when nothing is left of it, whether
/// finished runs used it or open ones have it reserved.
pub async fn budget_reached(
    repo: &Repository,
    project_id: &str,
    now: DateTime<Utc>,
) -> Result<Option<BudgetKind>, StoreError> {
    let Some(limits) = queries::budgets::project_budgets(repo, project_id).await? else {
        return Ok(None);
    };
    if let Some(day) = queries::budgets::day_room(repo, project_id, now).await?
        && day.room == 0
    {
        return Ok(Some(if day.counted >= day.limit {
            BudgetKind::Tokens
        } else {
            reserved(&day)
        }));
    }
    if let Some(max) = limits.queue_items_max {
        let waiting = queries::budgets::review_queue_items(repo, project_id).await?;
        if waiting >= max {
            return Ok(Some(BudgetKind::ReviewQueue {
                waiting: u32::try_from(waiting).unwrap_or(u32::MAX),
            }));
        }
    }
    Ok(None)
}

/// The day's tokens are spoken for, as [`BudgetKind`] says it.
fn reserved(day: &tack_db::repo::budgets::DayRoom) -> BudgetKind {
    BudgetKind::TokensReserved {
        open_runs: day.open_runs,
        reserved: day.reserved,
        limit: day.limit,
        room: day.room,
    }
}

/// The terminal code of a request failed for want of a runner.
const NO_RUNNER_CAN_CLAIM: &str = "no_runner_can_claim";
/// The code and sentence of a run whose lease expired and nobody renewed.
const LEASE_EXPIRED: &str = "lease_expired";
const LEASE_EXPIRED_SENTENCE: &str = "The computer that took this run stopped answering.";

/// The `(code, sentence)` an ineligible candidate is recorded under: the code
/// is the variant's snake_case name, the sentence is what an operator reads.
fn describe_ineligible(reason: &IneligibleReason) -> (&'static str, String) {
    match reason {
        IneligibleReason::NotRequestedRunner => (
            "not_requested_runner",
            "the request names a different runner".to_string(),
        ),
        IneligibleReason::NotFleetMember { fleet_id } => (
            "not_fleet_member",
            format!("the runner is not a member of fleet {fleet_id}"),
        ),
        IneligibleReason::RunnerNotActive { .. } => {
            ("runner_not_active", "the runner is not active".to_string())
        }
        IneligibleReason::HeartbeatStale { .. } => (
            "heartbeat_stale",
            "the runner has not sent a heartbeat recently".to_string(),
        ),
        IneligibleReason::NoAvailableCapacity { total } => (
            "no_available_capacity",
            format!("the runner has no free capacity (it runs {total} at a time)"),
        ),
        IneligibleReason::MissingLabel { key, expected, .. } => (
            "missing_label",
            format!("the runner lacks the label {key}={expected}"),
        ),
        IneligibleReason::HarnessNotDeclared { harness } => (
            "harness_not_declared",
            format!("the runner does not offer {}", harness.as_str()),
        ),
        IneligibleReason::HarnessProbeError { harness, error } => (
            "harness_probe_error",
            format!(
                "{} failed its check on the runner: {error}",
                harness.as_str()
            ),
        ),
        IneligibleReason::ModelNotNamed { harness } => (
            "model_not_named",
            format!("{} needs a model named", harness.as_str()),
        ),
        IneligibleReason::DecisionsNotSupported { harness } => (
            "decisions_not_supported",
            format!(
                "{} on the runner cannot stop to ask for a decision",
                harness.as_str()
            ),
        ),
        IneligibleReason::BudgetReached {
            which: BudgetKind::Tokens,
        } => (
            BUDGET_REACHED,
            "This project used its tokens for today".to_string(),
        ),
        IneligibleReason::BudgetReached {
            which:
                BudgetKind::TokensReserved {
                    open_runs,
                    reserved,
                    limit,
                    room,
                },
        } => {
            use tack_core::run_defaults::token_count;
            (
                BUDGET_REACHED,
                if *open_runs == 0 {
                    format!(
                        "This run's token limit is more than what is left of today's ({} of {})",
                        token_count(*room),
                        token_count(*limit)
                    )
                } else {
                    format!(
                        "Today's token limit is reserved by {open_runs} open run{} ({} of {})",
                        if *open_runs == 1 { "" } else { "s" },
                        token_count(*reserved),
                        token_count(*limit)
                    )
                },
            )
        }
        IneligibleReason::BudgetReached {
            which: BudgetKind::ReviewQueue { waiting },
        } => (
            BUDGET_REACHED,
            format!("This project's review queue is full ({waiting} waiting)"),
        ),
    }
}

/// What a poll that left `outcome`'s request unselected writes to its row.
/// `None` for a selection, and for a poll whose only reason is that the
/// polling runner is not the one (or not in the fleet) the request names:
/// that says nothing about whether the request can be claimed. The `bool` is
/// whether the wait could end by itself (every reason is a runner with no
/// free capacity): a runner that could take the request restarts the clock,
/// so it is never grounds for failing it.
fn waiting_note(outcome: &SelectionOutcome) -> Option<(String, String, bool)> {
    match outcome {
        SelectionOutcome::Selected(_) => None,
        SelectionOutcome::UnknownRunner { .. } => Some((
            "unknown_runner".to_string(),
            "the runner the request names is not known".to_string(),
            false,
        )),
        SelectionOutcome::NoEligibleRunner { reasons } => {
            let reasons: Vec<_> = reasons
                .iter()
                .filter(|(_, reason)| {
                    !matches!(
                        reason,
                        IneligibleReason::NotRequestedRunner
                            | IneligibleReason::NotFleetMember { .. }
                    )
                })
                .collect();
            if reasons.is_empty() {
                return None;
            }
            let described: Vec<(&str, String)> = reasons
                .iter()
                .map(|(_, reason)| describe_ineligible(reason))
                .collect();
            let transient = reasons
                .iter()
                .all(|(_, reason)| matches!(reason, IneligibleReason::NoAvailableCapacity { .. }));
            let code = described.first().map_or("no_eligible_runner", |d| d.0);
            let sentence = described
                .iter()
                .map(|d| d.1.as_str())
                .collect::<Vec<_>>()
                .join("; ");
            Some((code.to_string(), sentence, transient))
        }
    }
}

/// Writes what a poll found about every request it left unselected, and fails
/// the ones that have waited past the policy. A request no poll has reached
/// has no `waiting_since` and is never failed for that alone.
async fn record_waiting(
    repo: &Repository,
    queued: &[QueuedRequestForScheduling],
    outcomes: &[(String, SelectionOutcome)],
    now: DateTime<Utc>,
    policy: &SchedulingPolicy,
) -> Result<(), StoreError> {
    for (request_id, outcome) in outcomes {
        let Some((code, sentence, transient)) = waiting_note(outcome) else {
            continue;
        };
        let waiting_since = queued
            .iter()
            .find(|row| &row.id == request_id)
            .and_then(|row| row.waiting_since.as_deref())
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
            .map(|value| value.with_timezone(&Utc));
        let overdue = !transient
            && code != BUDGET_REACHED
            && policy.unclaimable_fail_after > chrono::Duration::zero()
            && waiting_since.is_some_and(|since| now - since > policy.unclaimable_fail_after);
        if overdue
            && repo
                .fail_unclaimable_request(request_id, NO_RUNNER_CAN_CLAIM, &sentence, now)
                .await?
        {
            continue;
        }
        repo.record_request_waiting(request_id, &code, &sentence, !transient, now)
            .await?;
    }
    Ok(())
}

/// Fetches this runner's own scheduling state plus every request it is
/// selector-eligible for, runs the pure scheduler, and returns the single
/// request id (if any) this runner should attempt to claim.
///
/// `Ok(None)` covers every "no eligible work" case (unknown runner, no
/// declared harnesses, not `active`, every candidate ineligible, nothing
/// queued) — the caller treats all of these identically to "no work."
pub async fn choose_request_for_runner(
    repo: &Repository,
    runner_id: &str,
    now: DateTime<Utc>,
    policy: &SchedulingPolicy,
) -> Result<Option<String>, StoreError> {
    // A run whose lease expired and was not renewed is nobody's: ask a person
    // about it before looking for work, since it is no longer queued.
    repo.expire_unheld_leases(
        now - EXPIRED_LEASE_GRACE,
        LEASE_EXPIRED,
        LEASE_EXPIRED_SENTENCE,
        now,
    )
    .await?;
    let Some(runner) = repo.fetch_runner_scheduling_snapshot(runner_id).await? else {
        return Ok(None);
    };
    let queued = repo.list_eligible_queued_requests(runner_id).await?;
    if queued.is_empty() {
        return Ok(None);
    }

    let parsed_capabilities =
        serde_json::from_str::<EmbeddedCapabilitySnapshot>(&runner.capability_snapshot).ok();
    // A runner that never enrolled/refreshed carries the column default
    // `'{}'`, which doesn't parse — "no declared harnesses" is the honest
    // reading, not a database error.
    let harnesses = parsed_capabilities
        .as_ref()
        .map(|snapshot| snapshot.harnesses.clone())
        .unwrap_or_default();
    // `last_heartbeat_at` is set only by `/heartbeat`, which reports active-
    // lease renewals, so a freshly enrolled runner's first claim sees `NULL`
    // here. Falls back to the capability snapshot's `reported_at` (enroll/
    // refresh already attests "alive"), rather than reading `NULL` as stale
    // and making every runner unschedulable until its first lease.
    let last_heartbeat_at = runner
        .last_heartbeat_at
        .as_deref()
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|value| value.with_timezone(&Utc))
        .or_else(|| {
            parsed_capabilities
                .as_ref()
                .map(|snapshot| snapshot.reported_at)
        });
    let labels: BTreeMap<String, String> = serde_json::from_str(&runner.labels).unwrap_or_default();
    let candidate = RunnerCandidate {
        runner_id: RunnerId::new(runner.runner_id.clone()),
        state: runner_state_from_str(&runner.state),
        fleet_memberships: runner.fleet_ids.iter().cloned().collect(),
        labels,
        total_capacity: u32::try_from(runner.total_capacity).unwrap_or(0),
        available_capacity: u32::try_from(runner.available_capacity).unwrap_or(0),
        last_heartbeat_at,
        harnesses,
    };

    // Fetch each distinct fleet's concurrency snapshot once, not once per
    // request, then drop any fleet-selector request whose fleet is already
    // saturated before it ever reaches the pure scheduler.
    let mut fleet_status: BTreeMap<String, Option<FleetConcurrencySnapshot>> = BTreeMap::new();
    for row in &queued {
        if row.selector_kind == "fleet" && !fleet_status.contains_key(&row.selector_id) {
            let snapshot = repo.fetch_fleet_concurrency(&row.selector_id).await?;
            fleet_status.insert(row.selector_id.clone(), snapshot);
        }
    }

    let mut project_budgets: BTreeMap<String, Option<BudgetKind>> = BTreeMap::new();
    // What is left of the day, for each request that has no limit of its own in a
    // project that has a daily one: it runs with that as its limit.
    let mut day_rooms: BTreeMap<String, i64> = BTreeMap::new();
    let mut requests: Vec<SchedulingRequest> = Vec::new();
    for row in queued.iter().filter(|row| {
        row.selector_kind != "fleet"
            || !fleet_is_saturated(fleet_status.get(&row.selector_id).and_then(Option::as_ref))
    }) {
        let reached = match queries::budgets::project_of_request(repo, &row.id).await? {
            Some((project_id, asking)) => {
                if !project_budgets.contains_key(&project_id) {
                    let reached = budget_reached(repo, &project_id, now).await?;
                    project_budgets.insert(project_id.clone(), reached);
                }
                let mut reached = project_budgets.get(&project_id).cloned().flatten();
                if reached.is_none()
                    && let Some(day) = queries::budgets::day_room(repo, &project_id, now).await?
                {
                    match asking {
                        Some(asking) if asking > day.room => reached = Some(reserved(&day)),
                        Some(_) => {}
                        None => {
                            day_rooms.insert(row.id.clone(), day.room);
                        }
                    }
                }
                reached
            }
            None => None,
        };
        if let Some(request) = build_scheduling_request(row, reached, now) {
            requests.push(request);
        }
    }

    let outcomes: Vec<(ExecutionRequestId, SelectionOutcome)> = if requests.len() == 1 {
        // Avoid `batch::schedule`'s capacity-ledger machinery — built for
        // many requests sharing many runners — for the overwhelmingly common
        // single-candidate poll.
        vec![(
            requests[0].request_id.clone(),
            select_runner(&requests[0], &[candidate], now, policy),
        )]
    } else {
        tack_orch::scheduler::schedule(&requests, &[candidate], now, policy)
    };

    let outcomes: Vec<(String, SelectionOutcome)> = outcomes
        .into_iter()
        .map(|(request_id, outcome)| (request_id.into_inner(), outcome))
        .collect();
    record_waiting(repo, &queued, &outcomes, now, policy).await?;
    let selected = outcomes.into_iter().find_map(|(request_id, outcome)| {
        matches!(outcome, SelectionOutcome::Selected(_)).then_some(request_id)
    });
    if let Some(request_id) = &selected
        && let Some(room) = day_rooms.get(request_id)
    {
        repo.limit_request_to_day_room(request_id, *room).await?;
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_from_metadata_reads_the_documented_convention() {
        assert_eq!(
            priority_from_metadata(r#"{"priority":"high"}"#),
            Priority::High
        );
        assert_eq!(
            priority_from_metadata(r#"{"priority":"LOW"}"#),
            Priority::Low
        );
        assert_eq!(
            priority_from_metadata(r#"{"priority":"normal"}"#),
            Priority::Normal
        );
        assert_eq!(priority_from_metadata(r#"{}"#), Priority::Normal);
        assert_eq!(
            priority_from_metadata(r#"{"priority":"urgent"}"#),
            Priority::Normal
        );
        assert_eq!(priority_from_metadata("not json"), Priority::Normal);
        assert_eq!(
            priority_from_metadata(r#"{"priority":5}"#),
            Priority::Normal
        );
    }

    #[test]
    fn fleet_saturation_reads_the_snapshot_honestly() {
        assert!(fleet_is_saturated(None), "no snapshot proves no room");
        assert!(!fleet_is_saturated(Some(&FleetConcurrencySnapshot {
            concurrency_limit: None,
            in_use: 999,
        })));
        assert!(!fleet_is_saturated(Some(&FleetConcurrencySnapshot {
            concurrency_limit: Some(5),
            in_use: 4,
        })));
        assert!(fleet_is_saturated(Some(&FleetConcurrencySnapshot {
            concurrency_limit: Some(5),
            in_use: 5,
        })));
        assert!(fleet_is_saturated(Some(&FleetConcurrencySnapshot {
            concurrency_limit: Some(5),
            in_use: 6,
        })));
    }

    #[test]
    fn unknown_runner_state_maps_to_the_conservative_revoked_state() {
        assert_eq!(runner_state_from_str("active"), RunnerState::Active);
        assert_eq!(
            runner_state_from_str("pending_enrollment"),
            RunnerState::PendingEnrollment
        );
        assert_eq!(runner_state_from_str("revoked"), RunnerState::Revoked);
        assert_eq!(runner_state_from_str("something_new"), RunnerState::Revoked);
    }

    #[test]
    fn build_scheduling_request_skips_a_row_with_no_harness_kind() {
        let row = QueuedRequestForScheduling {
            id: "req-1".into(),
            selector_kind: "exact_runner".into(),
            selector_id: "runner-1".into(),
            requested_harness_kind: None,
            requested_model: None,
            created_at: "2026-08-10T00:00:00Z".into(),
            metadata: "{}".into(),
            permission_policy: "{}".into(),
            waiting_since: None,
        };
        assert!(build_scheduling_request(&row, None, Utc::now()).is_none());
    }

    #[test]
    fn a_harness_that_needs_a_model_says_so_in_one_sentence() {
        let reason = IneligibleReason::ModelNotNamed {
            harness: HarnessKind::new("codex"),
        };
        assert_eq!(
            describe_ineligible(&reason),
            ("model_not_named", "codex needs a model named".to_string())
        );
    }

    #[test]
    fn every_ineligible_reason_reads_as_a_sentence_not_its_name() {
        use tack_protocol::execution::HarnessKind;
        let harness = || HarnessKind::new("codex");
        let reasons = vec![
            IneligibleReason::NotRequestedRunner,
            IneligibleReason::NotFleetMember {
                fleet_id: "fleet-a".into(),
            },
            IneligibleReason::RunnerNotActive {
                state: RunnerState::Revoked,
            },
            IneligibleReason::HeartbeatStale {
                last_heartbeat_at: None,
                max_age: chrono::Duration::seconds(60),
            },
            IneligibleReason::NoAvailableCapacity { total: 2 },
            IneligibleReason::MissingLabel {
                key: "gpu".into(),
                expected: "yes".into(),
                actual: None,
            },
            IneligibleReason::HarnessNotDeclared { harness: harness() },
            IneligibleReason::HarnessProbeError {
                harness: harness(),
                error: "not installed".into(),
            },
            IneligibleReason::ModelNotNamed { harness: harness() },
            IneligibleReason::DecisionsNotSupported { harness: harness() },
        ];
        for reason in reasons {
            let (code, sentence) = describe_ineligible(&reason);
            let debug = format!("{reason:?}");
            let variant = debug.split([' ', '{']).next().unwrap_or_default();
            assert!(!sentence.is_empty(), "{variant}: empty sentence");
            assert!(
                !sentence.contains(variant),
                "{variant}: the sentence repeats the enum's name: {sentence}"
            );
            assert!(!code.is_empty(), "{variant}: empty code");
        }
    }
}
