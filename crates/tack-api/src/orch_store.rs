//! Wires the orchestration reconciler (`tack-orch::reconciler`) to real
//! persistence (`tack-db::Repository`'s `repo/orch.rs`) and to a live
//! per-plane adapter, built via `tack_orch::adapters::registry::build`.
//!
//! `ControlPlaneStore` is a narrow trait rather than `tack_db::Repository`
//! directly because turning a `control_planes` row into a live
//! `Arc<dyn ControlPlane>` needs both the adapter and the repo at once, and
//! `tack-orch` has no reason to depend on `tack-db`'s concrete `Repository`
//! type. `tack-api` is the only crate that already depends on both, so the
//! wiring lives here rather than in `tack-orch` itself.
//!
//! `registry::build`'s `config`/`secrets` parameters are placeholders here
//! (`&serde_json::json!({})` and `None`): `tack_db::repo::orch::ControlPlane`
//! does not yet surface the columns migrations 032/033 added, and the only
//! registered `kind`, `"docket"`, ignores both parameters.

use std::collections::HashMap;
use std::sync::Arc;

use tracing::warn;
use uuid::Uuid;

use chrono::{DateTime, Utc};
use tokio::sync::broadcast;

use tack_core::models::Item;
use tack_db::Repository;
use tack_db::repo::orch::{NewOrchApproval, NewOrchEvent, NewOrchMetric, NewOrchRun};
use tack_orch::adapters::registry::{self, RegistryError};
use tack_orch::reconciler::{
    ControlPlaneStore, HealthRecord, RegisteredPlane, RetentionStore, RollupOutcome,
};
use tack_orch::{ControlPlane, OrchError};

use crate::config::AppConfig;
use crate::dispatcher;
use crate::handlers::orch::StatusMap;
use crate::handlers::websocket::BoardEvent;
use crate::router::AppState;
use crate::webhook::WebhookClient;

/// [`ControlPlaneStore`] backed by the real `tack-db` repository.
///
/// `list_registered` never fails the whole poll cycle over a single bad row:
/// an unknown `kind`, a token lookup that errors, or an adapter that fails
/// to construct (e.g. an unparsable `base_url`) all just skip that one plane
/// (logged at `warn`) rather than propagating an `Err` that would abort
/// `spawn_reconcilers` for every other registered plane too. Only a failure
/// to list the `control_planes` table at all — the DB being unreachable —
/// surfaces as an `Err`, matching `spawn_reconcilers`' own handling of that
/// case (log and spawn nothing, rather than panic).
#[derive(Clone)]
pub struct RepoControlPlaneStore {
    repo: Repository,
    /// The same channel `AppState` hands every WebSocket subscriber,
    /// threaded in here rather than into `tack-orch::reconciler` so that
    /// crate never grows a websocket dependency.
    broadcast_tx: broadcast::Sender<BoardEvent>,
    /// Everything else a full `AppState` carries, needed because
    /// `dispatcher::apply_mapped_status` takes `&AppState`. `None` for every
    /// store built via plain `new()`; only `server.rs`'s production wiring
    /// and this module's tests call [`with_app_context`] to opt in — with it
    /// `None`, `reconcile_terminal_status_map` below is inert.
    ///
    /// [`with_app_context`]: RepoControlPlaneStore::with_app_context
    app_context: Option<AppContext>,
}

/// The subset of `AppState` that doesn't already live on
/// [`RepoControlPlaneStore`] as `repo`/`broadcast_tx`. Kept as its own small
/// `Clone` struct (mirroring `AppState`'s own derive) rather than inlining
/// three more fields, so [`RepoControlPlaneStore::as_app_state`] reads as
/// one clear reassembly step.
#[derive(Clone)]
struct AppContext {
    config: AppConfig,
    workspace_id: Uuid,
    webhook: Option<WebhookClient>,
}

/// Build the [`ControlPlaneStore`] the reconciler needs, from an
/// already-built `AppState` — the exact wiring `server.rs`'s boot path
/// used inline before runtime enable/disable existed. Now shared between
/// that boot path and `PUT /api/settings/orchestration`
/// (`handlers/settings.rs`), which needs to start the same kind of store
/// when an operator flips the setting on without a restart.
pub fn build_control_plane_store(state: &AppState) -> Arc<dyn ControlPlaneStore> {
    Arc::new(
        RepoControlPlaneStore::new(state.repo.clone(), state.broadcast_tx.clone())
            .with_app_context(
                state.config.clone(),
                state.workspace_id,
                state.webhook.clone(),
            ),
    )
}

impl RepoControlPlaneStore {
    pub fn new(repo: Repository, broadcast_tx: broadcast::Sender<BoardEvent>) -> Self {
        Self {
            repo,
            broadcast_tx,
            app_context: None,
        }
    }

    /// Opt in to reconciler-driven `status_map` application. Call
    /// this once, right after `new()`, with the same `config`/`workspace_id`/
    /// `webhook` `server.rs` already builds its own `AppState` from — see
    /// `server.rs`'s production wiring for the intended call site.
    pub fn with_app_context(
        mut self,
        config: AppConfig,
        workspace_id: Uuid,
        webhook: Option<WebhookClient>,
    ) -> Self {
        self.app_context = Some(AppContext {
            config,
            workspace_id,
            webhook,
        });
        self
    }

    /// Reassemble a full `AppState` from this store's own fields plus the
    /// optional [`AppContext`] — `None` when [`with_app_context`] was never
    /// called.
    ///
    /// `orch_runtime` is a **fresh, inert** [`OrchRuntime`], not the live one
    /// `server.rs` shares: this reconstructed state only ever reaches
    /// `dispatcher::apply_mapped_status`, which never starts or stops the
    /// reconciler. `local_runner` is `None` for the same reason — nothing
    /// reached from here controls the embedded runner.
    fn as_app_state(&self) -> Option<AppState> {
        self.app_context.as_ref().map(|ctx| AppState {
            repo: self.repo.clone(),
            config: ctx.config.clone(),
            workspace_id: ctx.workspace_id,
            broadcast_tx: self.broadcast_tx.clone(),
            webhook: ctx.webhook.clone(),
            orch_runtime: crate::orch_runtime::OrchRuntime::new(),
            local_runner: None,
        })
    }

    /// Send a `BoardEvent` to every subscribed WebSocket client. Mirrors
    /// `handlers::websocket::broadcast_event`'s behavior exactly (including
    /// silently ignoring a send error — no subscribers is a normal, expected
    /// state, not a failure) but works from a bare sender since this store
    /// doesn't hold a full `AppState`.
    fn broadcast(&self, event: BoardEvent) {
        let _ = self.broadcast_tx.send(event);
    }

    /// Record `health = "unconfigured"` for a plane `list_registered` could
    /// not even build an adapter for (unknown `kind`, or a known `kind`'s
    /// constructor failing). Without this the plane never enters the
    /// reconciler's `healthy`/`degraded`/`unreachable` machine — that only
    /// runs against a plane whose adapter *did* construct — and would sit at
    /// the pre-poll `"unknown"` default forever with nothing but a log line.
    /// Motivating case: a restored backup nulls `secrets`
    /// (`remote_backup::scrub_snapshot_secrets`), harmless for docket (its
    /// token is optional) but fatal for a future plane that needs
    /// credentials to construct a client at all.
    ///
    /// Best-effort (a failed persist is logged, not propagated) and passes
    /// `consecutive_failures` through unchanged — `"unconfigured"` is an
    /// orthogonal "cannot even be tried" signal, not a point on the
    /// reachability failure count.
    async fn mark_unconfigured(&self, control_plane_id: Uuid, consecutive_failures: i64) {
        if let Err(e) = self
            .repo
            .update_control_plane_health(
                control_plane_id,
                "unconfigured",
                None,
                consecutive_failures,
                None,
            )
            .await
        {
            warn!(
                control_plane_id = %control_plane_id,
                error = %e,
                "failed to persist unconfigured health state"
            );
        }
    }
}

#[async_trait::async_trait]
impl ControlPlaneStore for RepoControlPlaneStore {
    async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError> {
        let rows =
            self.repo.list_control_planes().await.map_err(|e| {
                OrchError::Unavailable(format!("failed to list control planes: {e}"))
            })?;

        let mut planes = Vec::with_capacity(rows.len());
        for row in rows {
            let token = match self.repo.get_control_plane_token(row.id).await {
                Ok(token) => token,
                Err(e) => {
                    warn!(
                        control_plane_id = %row.id,
                        error = %e,
                        "failed to load control-plane token; skipping this plane this cycle"
                    );
                    continue;
                }
            };

            let control_plane: Arc<dyn ControlPlane> = match registry::build(
                &row.kind,
                &row.base_url,
                token,
                &serde_json::json!({}),
                None,
            ) {
                Ok(adapter) => adapter,
                Err(RegistryError::UnknownKind(kind)) => {
                    warn!(
                        control_plane_id = %row.id,
                        kind = %kind,
                        "unknown control-plane kind; skipping this plane"
                    );
                    self.mark_unconfigured(row.id, row.consecutive_failures)
                        .await;
                    continue;
                }
                Err(RegistryError::Construction(e)) => {
                    warn!(
                        control_plane_id = %row.id,
                        kind = %row.kind,
                        error = %e,
                        "failed to construct control-plane adapter; skipping this plane"
                    );
                    self.mark_unconfigured(row.id, row.consecutive_failures)
                        .await;
                    continue;
                }
            };

            planes.push(RegisteredPlane {
                id: row.id,
                control_plane,
            });
        }

        Ok(planes)
    }

    async fn record_health(
        &self,
        control_plane_id: Uuid,
        record: &HealthRecord,
    ) -> Result<(), OrchError> {
        self.repo
            .update_control_plane_health(
                control_plane_id,
                record.health.as_str(),
                record.last_seen_at,
                record.consecutive_failures,
                record.api_version.as_deref(),
            )
            .await
            .map_err(|e| {
                OrchError::Unavailable(format!("failed to persist control-plane health: {e}"))
            })
    }

    // Pass-throughs to `repo/orch.rs`; correlation (which item a run or
    // approval attributes to) happens in `tack-orch::reconciler`'s
    // persistence phase, not here.

    async fn list_linked_projects(&self, control_plane_id: Uuid) -> Result<Vec<String>, OrchError> {
        let links = self
            .repo
            .list_orch_links_for_plane(control_plane_id)
            .await
            .map_err(|e| OrchError::Unavailable(format!("failed to list orch links: {e}")))?;
        Ok(links.into_iter().map(|link| link.remote_project).collect())
    }

    async fn find_item_for_remote_task(
        &self,
        remote_task_id: &str,
    ) -> Result<Option<Uuid>, OrchError> {
        let task = self
            .repo
            .find_orch_task_by_remote_task_id(remote_task_id)
            .await
            .map_err(|e| OrchError::Unavailable(format!("failed to look up orch task: {e}")))?;
        Ok(task.map(|t| t.item_id))
    }

    // `upsert_runs`/`upsert_approvals` broadcast a `BoardEvent` only when the
    // write actually changed something — the reconciler polls forever, so a
    // naive "broadcast on every upsert" would resend the same event every
    // tick and eventually lag the broadcast channel's fixed capacity for a
    // slow subscriber. Both snapshot the row's state before the batch
    // upsert, run the upsert, then diff. `r.item_id.or(old_item_id)` mirrors
    // `repo/orch.rs`'s `COALESCE(excluded.item_id, orch_runs.item_id)`,
    // since a poll can carry `item_id: None` while the stored row already
    // carries a learned attribution from an earlier poll.
    //
    // `upsert_runs` also applies the terminal half of `status_map`: once a
    // run reaches `succeeded`/`failed`/`cancelled`,
    // `reconcile_terminal_status_map` below applies
    // `on_succeeded`/`on_failed`/`on_cancelled`, reusing this method's own
    // `is_new`/`state_changed`/`newly_attributed` determination so it only
    // fires on a genuine transition.

    async fn upsert_runs(
        &self,
        control_plane_id: Uuid,
        runs: &[NewOrchRun],
    ) -> Result<(), OrchError> {
        let mut previous = HashMap::with_capacity(runs.len());
        for r in runs {
            if let Ok(Some(existing)) = self.repo.get_orch_run(&r.run_id).await {
                previous.insert(r.run_id.clone(), existing);
            }
        }

        self.repo
            .upsert_orch_runs(control_plane_id, runs)
            .await
            .map_err(|e| OrchError::Unavailable(format!("failed to persist mirrored runs: {e}")))?;

        for r in runs {
            let existing = previous.get(&r.run_id);
            let old_item_id = existing.and_then(|e| e.item_id);
            let effective_item_id = r.item_id.or(old_item_id);

            let is_new = existing.is_none();
            let state_changed = existing.map(|e| e.state.as_str()) != Some(r.state.as_str());
            let newly_attributed = old_item_id.is_none() && effective_item_id.is_some();

            if !(is_new || state_changed || newly_attributed) {
                continue; // byte-identical re-poll: nothing changed, nothing to broadcast
            }

            // An uncorrelated run (e.g. dispatched from docket's own CLI, a
            // normal state) has no project to filter a `BoardEvent` into and
            // is skipped here; it is still persisted above and will
            // broadcast retroactively once a later poll learns its
            // attribution, via `newly_attributed`.
            let Some(item_id) = effective_item_id else {
                continue;
            };
            let Ok(Some(item)) = self.repo.get_item(item_id).await else {
                continue; // item deleted or lookup failed — nothing to broadcast into
            };

            self.reconcile_terminal_status_map(control_plane_id, r, &item)
                .await;

            self.broadcast(BoardEvent::AgentRunUpdated {
                project_id: item.project_id,
                item_id,
                run_id: r.run_id.clone(),
                state: r.state.clone(),
            });
        }

        Ok(())
    }

    async fn upsert_approvals(
        &self,
        control_plane_id: Uuid,
        approvals: &[NewOrchApproval],
    ) -> Result<(), OrchError> {
        let mut previous = HashMap::with_capacity(approvals.len());
        for a in approvals {
            if let Ok(Some(existing)) = self.repo.get_orch_approval(&a.token).await {
                previous.insert(a.token.clone(), existing);
            }
        }

        self.repo
            .upsert_orch_approvals(control_plane_id, approvals)
            .await
            .map_err(|e| {
                OrchError::Unavailable(format!("failed to persist mirrored approvals: {e}"))
            })?;

        for a in approvals {
            // `ApprovalPending` only fires on a transition *into* `pending`,
            // never on a grant/deny or a re-poll of one already pending —
            // guards against a future control plane or write-back path
            // turning this into a noisy "approval updated" firehose.
            if a.state != "pending" {
                continue;
            }

            let existing = previous.get(&a.token);
            let old_item_id = existing.and_then(|e| e.item_id);
            let effective_item_id = a.item_id.or(old_item_id);

            let is_new = existing.is_none();
            let became_pending = existing.map(|e| e.state.as_str()) != Some("pending");
            let newly_attributed = old_item_id.is_none() && effective_item_id.is_some();

            if !(is_new || became_pending || newly_attributed) {
                continue; // already known and still pending, no new attribution: no-op
            }

            // Same "no project, no broadcast" rule as upsert_runs above.
            let Some(item_id) = effective_item_id else {
                continue;
            };
            let Ok(Some(item)) = self.repo.get_item(item_id).await else {
                continue;
            };

            self.broadcast(BoardEvent::ApprovalPending {
                project_id: item.project_id,
                item_id,
                token: a.token.clone(),
                action: a.action.clone(),
            });
        }

        Ok(())
    }

    // Thin wrapper over repo/orch.rs; no aggregation here.
    async fn upsert_metrics(
        &self,
        control_plane_id: Uuid,
        metrics: &[NewOrchMetric],
    ) -> Result<(), OrchError> {
        self.repo
            .upsert_orch_metrics(control_plane_id, metrics)
            .await
            .map_err(|e| OrchError::Unavailable(format!("failed to persist mirrored metrics: {e}")))
    }

    // Thin pass-throughs to repo/orch.rs. No broadcast here — there is no
    // trace-event `BoardEvent` variant, and a naive "broadcast on every
    // upsert_events call" would be far noisier than upsert_runs/
    // upsert_approvals: a single poll tick can carry many trace events per
    // project, not one state transition.

    async fn list_trace_cursors(
        &self,
        control_plane_id: Uuid,
    ) -> Result<HashMap<String, String>, OrchError> {
        let cursors = self
            .repo
            .list_trace_cursors(control_plane_id)
            .await
            .map_err(|e| OrchError::Unavailable(format!("failed to list trace cursors: {e}")))?;
        Ok(cursors
            .into_iter()
            .map(|c| (c.remote_project, c.cursor))
            .collect())
    }

    async fn set_trace_cursor(
        &self,
        control_plane_id: Uuid,
        remote_project: &str,
        cursor: &str,
    ) -> Result<(), OrchError> {
        self.repo
            .set_trace_cursor(control_plane_id, remote_project, cursor)
            .await
            .map_err(|e| OrchError::Unavailable(format!("failed to persist trace cursor: {e}")))
    }

    async fn upsert_events(
        &self,
        control_plane_id: Uuid,
        events: &[NewOrchEvent],
    ) -> Result<(), OrchError> {
        self.repo
            .upsert_orch_events(control_plane_id, events)
            .await
            .map_err(|e| {
                OrchError::Unavailable(format!("failed to persist mirrored trace events: {e}"))
            })
    }
}

// ── Terminal status_map application ──
//
// The reconciler-driven counterpart to `dispatcher::apply_mapped_status`
// — a plain (non-trait) impl block since these are internal
// helpers for `upsert_runs` above, not part of `ControlPlaneStore`. Called
// only from `upsert_runs`, only on a genuine transition into a terminal
// `RunState`, and only for a correlated run.
impl RepoControlPlaneStore {
    /// If `run` just reached a terminal `RunState` (`succeeded`/`failed`/
    /// `cancelled`; `on_running`/`on_waiting_approval` apply separately, at
    /// dispatch time) and `status_map` names a target status, applies it via
    /// `dispatcher::apply_mapped_status` — unless a human has moved the card
    /// since dispatch, in which case the human wins: this records a
    /// `status_map_skipped_human_override` event and leaves the item alone.
    /// Docket's state is mirrored into `orch_runs` either way; only the
    /// board-visible status is protected.
    ///
    /// "Has a human moved it" is a value comparison against the single
    /// `status_map` key the item's latest dispatch attempt used (see
    /// [`card_has_diverged`]), not a persisted marker, so it cannot detect a
    /// human re-choosing the exact status automation already believed.
    ///
    /// [`card_has_diverged`]: RepoControlPlaneStore::card_has_diverged
    async fn reconcile_terminal_status_map(
        &self,
        control_plane_id: Uuid,
        run: &NewOrchRun,
        item: &Item,
    ) {
        let status_map: StatusMap = match self.repo.get_orch_link(item.project_id).await {
            Ok(Some(link)) => serde_json::from_value(link.status_map).unwrap_or_default(),
            _ => return, // project unlinked, deleted, or lookup failed — nothing to do
        };

        let (trigger, target) = match run.state.as_str() {
            "succeeded" => ("on_succeeded", status_map.on_succeeded.as_deref()),
            "failed" => ("on_failed", status_map.on_failed.as_deref()),
            "cancelled" => ("on_cancelled", status_map.on_cancelled.as_deref()),
            _ => return, // queued/running: no reconciler-driven trigger
        };
        let Some(target) = target else {
            return; // absent key: "do not touch the item's status"
        };
        if item.status == target {
            return; // already there — nothing to apply, nothing to protect
        }

        if self.card_has_diverged(&status_map, item).await {
            self.record_status_map_skipped(control_plane_id, item, &run.run_id, trigger, target)
                .await;
            return;
        }

        let Some(app_state) = self.as_app_state() else {
            // No production context attached — a store built via plain
            // `new()` (every pre-existing test, and any future call site
            // that never opts in). This feature is simply inert there.
            return;
        };
        let Ok(Some(project)) = self.repo.get_project(item.project_id).await else {
            return;
        };

        if let Err(e) = dispatcher::apply_mapped_status(
            &app_state,
            item,
            &project,
            target,
            control_plane_id,
            trigger,
        )
        .await
        {
            warn!(
                item_id = %item.id,
                run_id = %run.run_id,
                trigger = %trigger,
                error = %e,
                "failed to apply status_map terminal transition"
            );
        }
    }

    /// Has the item's status drifted from where our own automation last
    /// parked it? Compares against the *single* `status_map` key the item's
    /// latest `orch_tasks` attempt actually used: `on_waiting_approval` if
    /// that attempt's last known `remote_status` is `"waiting_approval"`
    /// (and the key is configured), otherwise `on_running` (if configured).
    /// With neither available — no in-flight marker configured at all —
    /// falls back to "is the item still sitting in one of `dispatch_from`",
    /// the only marker such a `status_map` claims ownership of.
    async fn card_has_diverged(&self, status_map: &StatusMap, item: &Item) -> bool {
        let latest_waiting_approval = self
            .repo
            .list_orch_tasks_for_item(item.id)
            .await
            .ok()
            .and_then(|tasks| tasks.into_iter().next())
            .is_some_and(|t| t.remote_status == "waiting_approval");

        let expected = if latest_waiting_approval && status_map.on_waiting_approval.is_some() {
            status_map.on_waiting_approval.as_deref()
        } else {
            status_map.on_running.as_deref()
        };

        match expected {
            Some(expected_status) => item.status != expected_status,
            None => !status_map.dispatch_from.iter().any(|s| s == &item.status),
        }
    }

    /// Best-effort: record a `status_map_skipped_human_override` `orch_events`
    /// row — same free-form `event_type` convention as
    /// `status_map_rejected` (migration 023's doc comment). Never fails the
    /// caller.
    async fn record_status_map_skipped(
        &self,
        control_plane_id: Uuid,
        item: &Item,
        run_id: &str,
        trigger: &str,
        target_status: &str,
    ) {
        let event = NewOrchEvent {
            id: Uuid::new_v4(),
            item_id: Some(item.id),
            run_id: Some(run_id.to_string()),
            event_type: "status_map_skipped_human_override".to_string(),
            payload: serde_json::json!({
                "trigger": trigger,
                "target_status": target_status,
                "current_status": item.status,
            }),
            occurred_at: Utc::now(),
        };
        if let Err(e) = self
            .repo
            .upsert_orch_events(control_plane_id, std::slice::from_ref(&event))
            .await
        {
            warn!(
                item_id = %item.id,
                error = %e,
                "failed to record status_map_skipped_human_override event"
            );
        }
    }
}

// A second, independent trait impl on the same struct: retention is
// fleet-wide, not per-plane, so it doesn't belong on `ControlPlaneStore`.
// Both methods are thin pass-throughs to `tack_db::Repository` — the
// atomicity/batching logic lives there, not here.
#[async_trait::async_trait]
impl RetentionStore for RepoControlPlaneStore {
    async fn rollup_and_purge_events(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<RollupOutcome, OrchError> {
        let stats = self
            .repo
            .rollup_and_purge_orch_events(cutoff, batch_size)
            .await
            .map_err(|e| {
                OrchError::Unavailable(format!("orch_events retention sweep failed: {e}"))
            })?;
        Ok(RollupOutcome {
            rows_purged: stats.rows_purged,
            batches_run: stats.batches_run,
        })
    }

    async fn rollup_and_purge_metrics(
        &self,
        cutoff: DateTime<Utc>,
        batch_size: i64,
    ) -> Result<RollupOutcome, OrchError> {
        let stats = self
            .repo
            .rollup_and_purge_orch_metrics(cutoff, batch_size)
            .await
            .map_err(|e| {
                OrchError::Unavailable(format!("orch_metrics retention sweep failed: {e}"))
            })?;
        Ok(RollupOutcome {
            rows_purged: stats.rows_purged,
            batches_run: stats.batches_run,
        })
    }
}
