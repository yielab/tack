//! Card C1 (Wave 3, tasks 35.2/35.3/35.6, 2026-08-05): the write path that
//! makes Tack a control center rather than a dashboard. Given a Tack item
//! that has just entered (or is being manually pushed into) a
//! dispatch-eligible status, [`dispatch_item`] enqueues a governed task on
//! the project's linked control plane and records the outcome.
//!
//! # What this module does, end to end
//!
//! 1. Resolve the item's project → `orch_links` row → `status_map`
//!    (TODO.md §1.3). An unlinked project, or a `status_map` with an empty
//!    `dispatch_from`, are both valid, ordinary states — not errors — see
//!    [`DispatchOutcome::NoDispatchPolicy`].
//! 2. Refuse (without touching docket) if the item's current status isn't
//!    one of `dispatch_from` — [`DispatchOutcome::NotEligible`].
//! 3. Idempotency: if the item's most recent `orch_tasks` attempt is still
//!    active (pending/running/waiting_approval), do **not** call docket
//!    again — [`DispatchOutcome::AlreadyInFlight`]. See "Idempotency and
//!    `attempt`" below.
//!
//! **One scheduling owner (card III-G1, Wave 6).** If the item already has
//! an active runner-v1 `execution_requests` row, do **not** call docket at
//! all — `Err(ApiError::Conflict(..))`, same shape the concurrent-dispatch
//! lock already uses. Checked before any HTTP call. See
//! `tack_db::repo::orch`'s "III-G1" module section for the exact "active"
//! definition and why this is the only direction of the guard this card's
//! file ownership can add.
//! 4. Call `ControlPlane::enqueue_task` (`POST /tasks/{project}`, card V1's
//!    live-verified three-outcome contract):
//!    - **block** → [`DispatchOutcome::Blocked`], no `orch_tasks` row at
//!      all (docket never created a task).
//!    - **allow** / **require_approval** → both are `Ok(task_id)` from the
//!      adapter (see `adapters::docket`'s module doc for why the trait
//!      can't distinguish them); a follow-up `list_tasks` call recovers the
//!      real status + approval token.
//! 5. Persist `orch_tasks` (task id + attempt + trust), then apply the
//!    `status_map`-named target status (`on_waiting_approval` or
//!    `on_running`) **through the workflow engine** — never raw SQL
//!    (TODO.md §0 rule 7). A transition the engine refuses (WIP limit, an
//!    explicit-transition workflow like construction's) is recorded as a
//!    `status_map_rejected` `orch_events` row and surfaced in the response;
//!    the item is left exactly as it was.
//!
//! # Trust is not optional
//!
//! [`dispatch_item`]'s `trusted: bool` parameter has **no default** — it is
//! not `Option<bool>`, and there is no sibling function that omits it. This
//! is deliberate: `core/dispatch.py::enqueue_task`'s own `trusted: bool |
//! None` treats an omitted value as "trusted iff `source == \"operator\"\"`,
//! which — since docket's `source` is hardcoded to `"operator"` on every
//! call — silently grants operator trust (card V1 confirmed this live).
//! Card C2 (untrusted-source handling) calls this function with
//! `trusted: false` for GitHub/Linear-imported items; this module's own
//! HTTP entry point ([`handlers::orch::dispatch_item`]) defaults
//! conservatively too — see that handler's doc comment. A required
//! positional `bool` can't stop a caller from passing the wrong *value*,
//! but it makes the *unsafe omission* — the actual failure mode V1
//! documented — a compile error instead of a silent default.
//!
//! # Idempotency and `attempt`
//!
//! `orch_tasks`' PK is `(item_id, remote_task_id)` — a genuine redispatch
//! (after a previous attempt reached a terminal state) is supposed to
//! create a new row, not collide with the old one. **`attempt`** is defined
//! here as: `1 + the highest existing attempt number for this item`, and a
//! new dispatch is only attempted when no existing attempt is still
//! "active" (`pending` / `running` / `waiting_approval` — anything else,
//! including a status this version of Tack doesn't recognise, is treated as
//! terminal and redispatchable). Two protections make "double-dispatching
//! the same item creates one task, not two" hold even under concurrency:
//!
//! 1. **[`DispatchLocks`]** — a process-wide, per-`item_id` mutual-exclusion
//!    guard (a bare `HashSet<Uuid>` behind a `std::sync::Mutex`, not part of
//!    `AppState` — see its own doc comment for why). Two concurrent
//!    dispatch requests for the *same* item never both reach the "check
//!    existing tasks" step; the second is rejected immediately
//!    (`ApiError::Conflict`) rather than racing the first.
//! 2. **The `orch_tasks` read itself**, done once the lock is held, catches
//!    the sequential case (a caller retries after the first request already
//!    completed).
//!
//! Tack is a single-process, single-SQLite-writer binary (CLAUDE.md), so a
//! process-local lock is a complete solution here — it would not be if Tack
//! ever ran as multiple replicas.
//!
//! # What this module deliberately does *not* do
//!
//! - **Terminal-state (`on_succeeded`/`on_failed`/`on_cancelled`)
//!   application** is not wired here. TODO.md's task 35.6 describes the
//!   reconciler applying these once a run polled by `orch_runs` (card B1)
//!   reaches a terminal `RunState` — that requires a call site inside
//!   `tack-orch::reconciler`'s `persist_runs` (via a new
//!   `ControlPlaneStore` method, the same extension pattern B1 used for
//!   `upsert_runs`/`upsert_approvals`), and `reconciler.rs` is not a file
//!   this card owns. [`apply_mapped_status`] is written to be that call
//!   site's engine — it is generic over "which target status, which
//!   trigger name", not specific to `on_running`/`on_waiting_approval` —
//!   but nothing currently calls it for a terminal run state. See this
//!   card's TODO.md §6 handoff for the exact extension a future agent
//!   needs to make.
//! - **`ControlPlane::dispatch`** (`POST /dispatch/{project}`, pipeline
//!   `variables`) is never called. Only `enqueue_task` is used — see
//!   `adapters::docket`'s module doc and this card's handoff for why.
//! - Auto-dispatch (C2) and sprint DAG-ordered dispatch (C3) both call
//!   [`dispatch_item`] rather than duplicating any of this.

use std::collections::HashSet;
use std::sync::{Arc, LazyLock, Mutex};

use chrono::Utc;
use tracing::warn;
use uuid::Uuid;

use tack_core::models::{Item, Priority, Project};
use tack_db::repo::items::StatusUpdateOutcome;
use tack_db::repo::orch::{NewOrchEvent, NewOrchTask, OrchTask};
use tack_orch::adapters::registry::{self, RegistryError};
use tack_orch::{ControlPlane, NewRemoteTask, OrchError};

use crate::error::ApiError;
use crate::handlers::items::{maybe_sync_github, propagate_parent_completion};
use crate::handlers::orch::StatusMap;
use crate::handlers::websocket::{self, BoardEvent};
use crate::router::AppState;

/// `orch_tasks.remote_status` values that mean "docket is still working on
/// this (or waiting on a human) — do not enqueue a second one." Anything
/// else, including an unrecognised value, is treated as terminal (safe to
/// redispatch): this errs toward allowing a redispatch rather than wedging
/// an item that can never be retried because of a status string this
/// version of Tack doesn't understand.
///
/// `pub(crate)`, not private: card C3's sprint-dispatch dry-run reads this
/// directly (via [`is_active_task_status`]) to preview whether a real run
/// would report an item `already_in_flight`, without duplicating the
/// definition of "active."
pub(crate) const ACTIVE_TASK_STATUSES: &[&str] = &["pending", "running", "waiting_approval"];

/// `true` iff `remote_status` means "docket is still working on this" — see
/// [`ACTIVE_TASK_STATUSES`].
pub(crate) fn is_active_task_status(remote_status: &str) -> bool {
    ACTIVE_TASK_STATUSES.contains(&remote_status)
}

/// `true` iff `current_status` is one of `status_map.dispatch_from` — the
/// single place this check is expressed. [`dispatch_item`] and card C3's
/// sprint-dispatch preview both call this rather than each writing their own
/// membership check, so the two can never quietly disagree about what
/// "eligible" means.
pub(crate) fn is_dispatch_eligible(status_map: &StatusMap, current_status: &str) -> bool {
    status_map.dispatch_from.iter().any(|s| s == current_status)
}

// ─────────────────────────────────────────────────────────────────────────
// Per-item dispatch lock — process-wide, not part of AppState
// ─────────────────────────────────────────────────────────────────────────

/// A process-wide guard against two concurrent dispatch requests for the
/// same item racing each other. Deliberately **not** a field on
/// [`AppState`]: `AppState` is constructed via a plain struct literal in
/// dozens of pre-existing test files across this crate that this card does
/// not own, and adding a required field there would ripple into every one
/// of them. A single `static` is the narrower change and is sufficient —
/// Tack is a single-process, single-SQLite-writer binary (see this module's
/// doc comment), so there is exactly one process whose in-memory state ever
/// needs to agree.
static DISPATCH_LOCKS: LazyLock<Mutex<HashSet<Uuid>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Held for the duration of one item's dispatch attempt; removes the item
/// from the lock set on drop (including on an early `return` or a panic
/// unwind), so a held lock can never outlive the request that acquired it.
struct DispatchGuard {
    item_id: Uuid,
}

impl Drop for DispatchGuard {
    fn drop(&mut self) {
        if let Ok(mut set) = DISPATCH_LOCKS.lock() {
            set.remove(&self.item_id);
        }
    }
}

/// Try to acquire the dispatch lock for `item_id`. `None` means another
/// dispatch for the same item is already in flight in this process.
fn try_acquire(item_id: Uuid) -> Option<DispatchGuard> {
    let mut set = DISPATCH_LOCKS.lock().ok()?;
    if !set.insert(item_id) {
        return None;
    }
    Some(DispatchGuard { item_id })
}

// ─────────────────────────────────────────────────────────────────────────
// Outcome types
// ─────────────────────────────────────────────────────────────────────────

/// The result of attempting to apply a `status_map`-named target status
/// through the workflow engine (TODO.md §0 rule 7). Never an `Err` on its
/// own — a workflow-engine refusal is a normal, expected outcome
/// (`status_map_rejected`, TODO.md task 35.6's acceptance bar), not a
/// failure of the dispatch itself.
#[derive(Debug, Clone)]
pub struct StatusApplication {
    pub target_status: String,
    /// `true` iff the item's status actually changed.
    pub applied: bool,
    /// `Some(reason)` when the workflow engine refused the transition (an
    /// `InvalidTransition` or `WipLimitExceeded` `CoreError`, stringified).
    /// The item was left untouched in this case, and a `status_map_rejected`
    /// `orch_events` row was recorded.
    pub rejected_reason: Option<String>,
}

/// A successful (or approval-gated) dispatch.
#[derive(Debug, Clone)]
pub struct DispatchSuccess {
    pub task: OrchTask,
    /// `true` when docket's `pre_input` policy returned `require_approval`
    /// — the task exists on docket but is **not** running. Callers must not
    /// report this as "dispatched successfully" without qualification.
    pub waiting_approval: bool,
    pub approval_token: Option<String>,
    /// `None` when `status_map` named no target status for this trigger
    /// (`on_running` / `on_waiting_approval` absent — "do not touch the
    /// item's status", per TODO.md §1.3).
    pub status_application: Option<StatusApplication>,
}

/// Every distinct outcome [`dispatch_item`] can produce. Deliberately not
/// an `Err` variant for anything docket itself decided on purpose (blocked,
/// waiting on approval) — see the module doc.
#[derive(Debug, Clone)]
pub enum DispatchOutcome {
    /// `status_map.dispatch_from` is empty — no dispatch policy configured
    /// yet. Not an error (TODO.md's explicit non-negotiable).
    NoDispatchPolicy,
    /// The item's current status isn't in `status_map.dispatch_from`.
    NotEligible {
        current_status: String,
        dispatch_from: Vec<String>,
    },
    /// The item already has a non-terminal `orch_tasks` attempt; nothing
    /// was sent to docket.
    AlreadyInFlight {
        task: OrchTask,
    },
    /// docket's `pre_input` policy refused the request. `policy_id` is the
    /// id of the guardrail that fired (parsed by `adapters::docket` out of
    /// docket's own error text — see [`tack_orch::OrchError::PolicyBlocked`],
    /// card R1); `message` is docket's own text, verbatim, for display.
    Blocked {
        policy_id: String,
        message: String,
    },
    Success(DispatchSuccess),
}

// ─────────────────────────────────────────────────────────────────────────
// The dispatcher
// ─────────────────────────────────────────────────────────────────────────

/// Dispatch `item_id` to its project's linked control plane. See the module
/// doc for the full flow, the idempotency guarantee, and why `trusted` is a
/// required, non-optional parameter.
///
/// Errors (`Err`) are reserved for things genuinely wrong with the request
/// or the system (unknown item/project, no control-plane link, a lock
/// contention on a concurrent duplicate, a transport failure talking to the
/// control plane) — every outcome docket itself can produce on purpose
/// (block, require approval) is a variant of [`DispatchOutcome`], not an
/// `Err`.
pub async fn dispatch_item(
    state: &AppState,
    item_id: Uuid,
    trusted: bool,
) -> Result<DispatchOutcome, ApiError> {
    let item = state
        .repo
        .get_item(item_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {item_id} not found")))?;
    let project = state
        .repo
        .get_project(item.project_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Project {} not found", item.project_id)))?;

    let Some(link) = state.repo.get_orch_link(item.project_id).await? else {
        return Err(ApiError::Conflict(format!(
            "project {} is not linked to a control plane",
            item.project_id
        )));
    };

    let status_map: StatusMap = serde_json::from_value(link.status_map.clone()).unwrap_or_default();

    if status_map.dispatch_from.is_empty() {
        return Ok(DispatchOutcome::NoDispatchPolicy);
    }
    if !is_dispatch_eligible(&status_map, &item.status) {
        return Ok(DispatchOutcome::NotEligible {
            current_status: item.status.clone(),
            dispatch_from: status_map.dispatch_from.clone(),
        });
    }

    // Per-item lock — see the module doc's "Idempotency and attempt"
    // section. Acquired before any read that decides whether to call
    // docket, so two concurrent requests for the same item can never both
    // pass the "already in flight?" check below.
    let Some(_guard) = try_acquire(item_id) else {
        return Err(ApiError::Conflict(format!(
            "item {item_id} is already being dispatched by another in-flight request"
        )));
    };

    // III-G1 (Wave 6): one scheduling owner. If the item already has a live
    // runner-v1 execution request (`execution_requests`, the neutral domain — see
    // `tack_db::repo::orch`'s "III-G1" section for the exact "active" definition,
    // which mirrors `ExecutionState::is_terminal` rather than redefining it), legacy
    // Docket dispatch defers rather than racing it: nothing is sent to docket and no
    // `orch_tasks` row is written. Checked before the existing `orch_tasks`
    // idempotency read, and before any HTTP call, for the same reason the per-item
    // lock is acquired first — a caller must never observe docket being contacted for
    // an item the runner-v1 scheduler already owns. Reuses the same `ApiError::
    // Conflict` shape this function already returns for the concurrent-dispatch lock
    // case just above (not a new `DispatchOutcome` variant — see the III-G1 handoff
    // for why: every existing exhaustive match on `DispatchOutcome` lives outside
    // this card's file ownership, and `Conflict` already means exactly "another party
    // owns this item's dispatch right now").
    if state
        .repo
        .has_active_execution_request_for_item(item_id)
        .await?
    {
        return Err(ApiError::Conflict(format!(
            "item {item_id} has an active runner-v1 execution request; refusing legacy \
             Docket dispatch to preserve one scheduling owner"
        )));
    }

    let existing = state.repo.list_orch_tasks_for_item(item_id).await?; // attempt DESC
    if let Some(latest) = existing.first()
        && is_active_task_status(&latest.remote_status)
    {
        return Ok(DispatchOutcome::AlreadyInFlight {
            task: latest.clone(),
        });
    }
    let next_attempt = existing.first().map(|t| t.attempt).unwrap_or(0) + 1;

    let control_plane = build_control_plane(state, link.control_plane_id).await?;

    let new_task = NewRemoteTask {
        description: build_description(&item),
        priority: map_priority(&item.priority).map(str::to_string),
        trusted,
    };

    let task_id = match control_plane
        .enqueue_task(&link.remote_project, new_task)
        .await
    {
        Ok(id) => id,
        Err(OrchError::PolicyBlocked { policy_id, message }) => {
            return Ok(DispatchOutcome::Blocked { policy_id, message });
        }
        Err(e) => {
            return Err(ApiError::Conflict(format!(
                "failed to enqueue dispatch on control plane: {e}"
            )));
        }
    };

    // Recover the real status + approval token (see adapters::docket's
    // module doc for why enqueue_task's `Result<String, OrchError>` return
    // type can't carry them — widening it further is a separate change than
    // this dispatcher needs). A failure here is logged and treated as
    // "pending" — the task
    // genuinely was created on docket (we have its id), so this must not
    // be reported as a dispatch failure.
    let (remote_status, approval_token) = match control_plane.list_tasks(&link.remote_project).await
    {
        Ok(tasks) => tasks
            .into_iter()
            .find(|t| t.id == task_id)
            .map(|t| (t.status.as_str().to_string(), t.approval_token))
            .unwrap_or_else(|| {
                warn!(
                    item_id = %item_id,
                    task_id = %task_id,
                    "dispatched task not found in list_tasks; defaulting to pending"
                );
                ("pending".to_string(), None)
            }),
        Err(e) => {
            warn!(
                item_id = %item_id,
                task_id = %task_id,
                error = %e,
                "failed to read back task status after dispatch; defaulting to pending"
            );
            ("pending".to_string(), None)
        }
    };

    let dispatched_at = Utc::now();
    let new_orch_task = NewOrchTask {
        item_id,
        remote_task_id: task_id.clone(),
        remote_run_id: None,
        remote_status: remote_status.clone(),
        attempt: next_attempt,
        tokens_in: 0,
        tokens_out: 0,
        cost_usd_estimated: None,
        dispatched_at,
        trusted,
    };
    state
        .repo
        .upsert_orch_tasks(std::slice::from_ref(&new_orch_task))
        .await?;
    let task = state
        .repo
        .get_orch_task(item_id, &task_id)
        .await?
        .ok_or_else(|| {
            ApiError::Internal(anyhow::anyhow!(
                "orch_task {task_id} for item {item_id} vanished immediately after upsert"
            ))
        })?;

    let waiting_approval = remote_status == "waiting_approval";
    let trigger_status = if waiting_approval {
        status_map.on_waiting_approval.as_ref()
    } else {
        status_map.on_running.as_ref()
    };
    let trigger_name = if waiting_approval {
        "on_waiting_approval"
    } else {
        "on_running"
    };

    let status_application = match trigger_status {
        Some(target) => Some(
            apply_mapped_status(
                state,
                &item,
                &project,
                target,
                link.control_plane_id,
                trigger_name,
            )
            .await?,
        ),
        None => None,
    };

    Ok(DispatchOutcome::Success(DispatchSuccess {
        task,
        waiting_approval,
        approval_token,
        status_application,
    }))
}

/// Apply `target_status` to `item` **through the workflow engine** —
/// `validate_transition` + an atomic WIP-limit check-and-write, exactly the
/// same gate `handlers::items::update_item` applies to a human-driven status
/// change (TODO.md §0 rule 7). A refusal is recorded as a
/// `status_map_rejected` `orch_events` row and returned as
/// `rejected_reason`; the item is left untouched. On success, mirrors
/// `update_item`'s side effects (WebSocket broadcast, parent
/// auto-propagation, GitHub push-back) so a status_map-driven transition is
/// indistinguishable from a human dragging the card.
///
/// **The WIP-limit check and the status write happen in one SQLite
/// transaction** (`Repository::update_item_status_checked`), not as two
/// separate steps. Card R2 (2026-08-05): this used to be a plain
/// `count_items_by_status` read followed by an unguarded `update_item`
/// write, which let two concurrent dispatches into the same WIP-limited
/// column both observe "under the limit" and both commit — rare before
/// card C3's sprint dispatch made concurrent writes into one column routine
/// rather than coincidental. See that method's doc comment for the fix.
/// `validate_transition` itself stays a separate, unguarded check above —
/// it only depends on the project's static workflow config (explicit
/// transitions), not on any row count, so it isn't subject to the same
/// race.
///
/// Generic over `target_status`/`trigger` so it can serve both the
/// dispatch-time triggers this card wires up (`on_running`,
/// `on_waiting_approval`) and a future reconciler-driven call for the
/// terminal triggers (`on_succeeded`/`on_failed`/`on_cancelled`) — see the
/// module doc's "What this module deliberately does not do".
pub async fn apply_mapped_status(
    state: &AppState,
    item: &Item,
    project: &Project,
    target_status: &str,
    control_plane_id: Uuid,
    trigger: &str,
) -> Result<StatusApplication, ApiError> {
    if item.status == target_status {
        return Ok(StatusApplication {
            target_status: target_status.to_string(),
            applied: false,
            rejected_reason: None,
        });
    }

    if let Err(e) = project
        .workflow
        .validate_transition(&item.status, target_status)
    {
        record_status_map_rejected(state, item, control_plane_id, target_status, trigger, &e).await;
        return Ok(StatusApplication {
            target_status: target_status.to_string(),
            applied: false,
            rejected_reason: Some(e.to_string()),
        });
    }

    let status_category = project
        .workflow
        .statuses
        .iter()
        .find(|s| s.name == target_status)
        .map(|s| s.category.clone());

    let outcome = state
        .repo
        .update_item_status_checked(
            item.id,
            item.project_id,
            target_status,
            status_category,
            &project.workflow,
        )
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {} not found", item.id)))?;

    let updated = match outcome {
        StatusUpdateOutcome::Rejected(e) => {
            record_status_map_rejected(state, item, control_plane_id, target_status, trigger, &e)
                .await;
            return Ok(StatusApplication {
                target_status: target_status.to_string(),
                applied: false,
                rejected_reason: Some(e.to_string()),
            });
        }
        StatusUpdateOutcome::Applied(updated) => updated,
    };

    websocket::broadcast_event(
        state,
        BoardEvent::ItemUpdated {
            project_id: updated.project_id,
            item_id: updated.id,
            old_status: Some(item.status.clone()),
            new_status: updated.status.clone(),
        },
    );

    propagate_parent_completion(state, &updated, &item.status).await;
    maybe_sync_github(state, &updated, &item.status).await;

    Ok(StatusApplication {
        target_status: target_status.to_string(),
        applied: true,
        rejected_reason: None,
    })
}

/// Best-effort: record a `status_map_rejected` `orch_events` row (the
/// migration 023 doc comment names this exact event type as the intended
/// use for a locally-generated, non-docket-sourced event). Never fails the
/// caller — an audit-trail write failing must not turn a workflow-engine
/// rejection (which is already being reported to the caller) into an
/// opaque 500.
async fn record_status_map_rejected(
    state: &AppState,
    item: &Item,
    control_plane_id: Uuid,
    target_status: &str,
    trigger: &str,
    reason: &tack_core::CoreError,
) {
    let event = NewOrchEvent {
        id: Uuid::new_v4(),
        item_id: Some(item.id),
        run_id: None,
        event_type: "status_map_rejected".to_string(),
        payload: serde_json::json!({
            "trigger": trigger,
            "from_status": item.status,
            "target_status": target_status,
            "reason": reason.to_string(),
        }),
        occurred_at: Utc::now(),
    };
    if let Err(e) = state
        .repo
        .upsert_orch_events(control_plane_id, std::slice::from_ref(&event))
        .await
    {
        warn!(
            item_id = %item.id,
            error = %e,
            "failed to record status_map_rejected event"
        );
    }
}

/// Build a live [`ControlPlane`] for one control plane by id. A hard error
/// (not a best-effort skip) unlike `orch_store::RepoControlPlaneStore::
/// list_registered`'s batch/reconciler use — a user explicitly asked to
/// dispatch one item to one specific plane, so a misconfigured plane must
/// surface as a real error on this one request, not silently vanish from a
/// list.
///
/// Built on `adapters::registry::build` (card G1) — see that function's own
/// doc comment for why `config`/`secrets` are passed as placeholders here:
/// `tack_db::repo::orch::ControlPlane` (the row type `get_control_plane`
/// returns) doesn't yet surface those columns, and the one registered
/// `kind`, `"docket"`, doesn't read them anyway.
async fn build_control_plane(
    state: &AppState,
    control_plane_id: Uuid,
) -> Result<Arc<dyn ControlPlane>, ApiError> {
    let row = match state.repo.get_control_plane(control_plane_id).await {
        Ok(row) => row,
        Err(sqlx::Error::RowNotFound) => {
            return Err(ApiError::NotFound(format!(
                "control plane {control_plane_id} not found"
            )));
        }
        Err(e) => return Err(ApiError::Database(e)),
    };
    let token = state.repo.get_control_plane_token(control_plane_id).await?;

    match registry::build(
        &row.kind,
        &row.base_url,
        token,
        &serde_json::json!({}),
        None,
    ) {
        Ok(adapter) => Ok(adapter),
        Err(RegistryError::Construction(e)) => Err(ApiError::Internal(anyhow::anyhow!(
            "failed to construct control-plane adapter: {e}"
        ))),
        Err(RegistryError::UnknownKind(kind)) => Err(ApiError::Internal(anyhow::anyhow!(
            "unsupported control-plane kind {kind:?}"
        ))),
    }
}

/// `description` bound to docket's `NewRemoteTask` — title, plus the item's
/// own description when it has one.
fn build_description(item: &Item) -> String {
    match item.description.as_deref().map(str::trim) {
        Some(d) if !d.is_empty() => format!("{}\n\n{d}", item.title),
        _ => item.title.clone(),
    }
}

/// Tack's `Priority` → docket's `"high"|"normal"|"low"` (`NewRemoteTask`'s
/// doc comment). `None` lets docket apply its own default (`"normal"`)
/// rather than this crate inventing a literal for a rank docket doesn't
/// name.
fn map_priority(p: &Priority) -> Option<&'static str> {
    match p {
        Priority::Critical | Priority::High => Some("high"),
        Priority::Low => Some("low"),
        Priority::Medium | Priority::None => None,
    }
}

/// Whether `item_id` should be dispatched as `trusted: true` when a caller
/// (today, only the manual "Dispatch" HTTP handler,
/// `handlers::orch::dispatch_item`) doesn't have a stronger signal of its
/// own to pass instead.
///
/// **Card C2 (task 35.7), superseding the `github_links`-sniffing stopgap
/// this function used to be** (see git history / TODO.md's C1 handoff for
/// the old body): item provenance is now a real, sticky, creation-time
/// column (`items.source` / `tack_core::models::ItemSource`, migration
/// 029), not an inference from a side table. This function is now a thin
/// read of that column — `ItemSource::is_trusted()` is the single source of
/// truth for the trust rule itself. Unlike the old `github_links` check,
/// this correctly covers Linear-imported items too (Linear import leaves no
/// persistent correlation row of its own, which was exactly the blind spot
/// the old implementation's doc comment flagged).
///
/// The auto-dispatch hook (`handlers::items::maybe_auto_dispatch`) does not
/// call this function — it already has the freshly loaded `Item` in hand
/// and reads `.source.is_trusted()` directly, which is the same rule
/// applied one layer up rather than re-fetched here.
pub async fn resolve_default_trust(state: &AppState, item_id: Uuid) -> Result<bool, ApiError> {
    let item = state
        .repo
        .get_item(item_id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("Item {item_id} not found")))?;
    Ok(item.source.is_trusted())
}
