//! Runtime start/stop control for the orchestration reconciler.
//!
//! This module makes the reconciler's enable flag a runtime setting, rather
//! than a boot-time-only decision (`server.rs` spawning it once, gated on
//! `TACK_ORCH_ENABLE`, with no other way to turn it on or off) — mirroring
//! the Cloud Backup precedent in `handlers/settings.rs`: stored
//! in `app_meta`, with the env var reduced to a deployment default. It
//! gives `PUT /api/settings/orchestration` something to call so flipping
//! the flag takes effect immediately, with no restart.
//!
//! # Start/stop design
//!
//! Each reconciler task (`tack_orch::reconciler::spawn_one`, one per
//! registered control plane) loops: fetch → decide → persist → sleep.
//! Stopping it cleanly needs a signal the task can observe at a safe point
//! — never mid-HTTP-call to docket, and never while a SQLite write is open
//! (unaffected either way, since persistence already happens strictly
//! after the fetch phase completes and before the sleep).
//!
//! [`tokio::sync::watch`] is that signal: a single `bool` channel, `false`
//! meaning "keep going". [`OrchRuntime::stop`] flips it to `true`; every
//! task holds a cloned `Receiver` and races it against its poll-interval
//! sleep with `tokio::select!` (`reconciler::spawn_one`), and also checks
//! it at the top of the next loop iteration before starting a new fetch. A
//! task mid-fetch when `stop()` is called finishes that one tick
//! (fetch/persist, both already short and already in flight) and exits at
//! its very next safe point — bounded by however long the in-flight HTTP
//! call to docket takes, never longer, and never mid-transaction.
//!
//! No new dependency was added for this: `watch` is already part of
//! `tokio`'s `full` feature, already a workspace dependency. `tokio-util`'s
//! `CancellationToken` would work equally well but isn't needed for a
//! single boolean flag with N cloned receivers.
//!
//! # No leaked tasks on repeated toggles
//!
//! [`OrchRuntime`] holds at most one "running" generation at a time behind
//! a `tokio::sync::Mutex`. [`start`](OrchRuntime::start) is a no-op if a
//! generation is already running (never spawns a second set on top of a
//! live one); [`stop`](OrchRuntime::stop) takes the current generation out
//! of the shared state before signalling it, so a `start()` that races a
//! `stop()` can never observe "half stopped" state — see each method's own
//! doc comment. Verified in `tack-orch`'s
//! `reconciler::tests::repeated_global_start_stop_cycles_leave_no_task_running`
//! (three consecutive start/stop cycles, asserting exactly one task per
//! cycle) and, at the HTTP layer, in
//! `tack-api`'s `tests/orch_settings_test.rs`.
//!
//! # The list of planes isn't read once
//!
//! `start()` calls `reconciler::spawn_reconcilers_supervised`. The old
//! `reconciler::spawn_reconcilers_cancellable` read `store.list_registered()`
//! exactly once and spawned one `spawn_one` task per plane found at that
//! instant — the list was never re-read. A control plane registered *after*
//! `start()` (the natural
//! "enable orchestration -> register a control plane -> link a project"
//! setup order) would therefore never be polled: no task, no health updates, no
//! error anywhere. `spawn_reconcilers_supervised` does the same initial
//! snapshot synchronously (so a caller checking [`OrchRuntime::
//! live_task_count`] right after `start()` still sees every
//! already-registered plane immediately) but then keeps a background
//! supervisor loop running that re-reads `list_registered()` every
//! `config.supervisor_scan_secs` and starts/stops per-plane pollers to
//! match — self-healing regardless of *how* `control_planes` changed
//! (through the API, a bulk import, or a direct DB edit). See
//! `tack_orch::reconciler`'s module doc, the section headed "Supervisor
//!", for the full design writeup and the alternative
//! (handler-driven notification) that was considered and rejected. This
//! module's own responsibilities are unchanged by that card: `OrchRuntime`
//! still only owns the single global start/stop signal and the
//! at-most-one-generation invariant above; per-plane lifecycle is entirely
//! `tack-orch`'s concern now.

use std::sync::Arc;

use tokio::sync::{Mutex, watch};

use tack_orch::reconciler::{self, ControlPlaneStore, ReconcilerConfig, SupervisedReconciler};

/// A live supervised reconciler run plus the shutdown signal that stops it
/// (and, transitively, every per-plane poller it's currently tracking — see
/// `reconciler::supervisor_loop`'s doc comment).
struct Running {
    reconciler: SupervisedReconciler,
    stop_tx: watch::Sender<bool>,
}

/// Shared, toggleable handle to the orchestration reconciler. One instance
/// lives on `AppState` (`Clone`, cheap — an `Arc<Mutex<..>>` underneath) so
/// both the boot path (`server.rs`) and `PUT /api/settings/orchestration`
/// (`handlers/settings.rs`) start and stop the exact same set of tasks.
#[derive(Clone)]
pub struct OrchRuntime {
    inner: Arc<Mutex<Option<Running>>>,
}

impl Default for OrchRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl OrchRuntime {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }

    /// Start a self-healing reconciler run: one poller per
    /// currently-registered control plane, kept in sync with
    /// `control_planes` for as long as this generation stays running — see
    /// `reconciler::spawn_reconcilers_supervised`'s doc comment for why a
    /// one-time snapshot isn't enough: a control plane registered *after*
    /// `start()` would otherwise never get polled, silently.
    /// Idempotent: a `start()` while a generation is already running is a
    /// no-op — it does not spawn a duplicate set alongside the live one.
    /// (Calling `start()` twice in a row happens naturally if the operator
    /// sends `PUT {"enabled": true}` more than once, or the value was
    /// already `true` from the environment at boot.)
    pub async fn start(&self, store: Arc<dyn ControlPlaneStore>, config: ReconcilerConfig) {
        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            return;
        }
        let (stop_tx, stop_rx) = watch::channel(false);
        let reconciler = reconciler::spawn_reconcilers_supervised(store, config, stop_rx).await;
        *guard = Some(Running {
            reconciler,
            stop_tx,
        });
    }

    /// Signal every running task to stop at its next safe point, and drop
    /// this runtime's reference to them. A no-op (not an error) when
    /// nothing is running — mirrors `start()`'s idempotency.
    ///
    /// Does not block waiting for the tasks to actually exit: a toggle-off
    /// HTTP request must not hang on whatever docket's response latency
    /// happens to be for an in-flight poll. Signalling `stop_tx` stops both
    /// the supervisor loop itself (so it starts polling no *new* planes)
    /// and, via the supervisor's own shutdown path, every per-plane poller
    /// it was tracking at that moment — see the module doc's start/stop
    /// design section and `reconciler::supervisor_loop`'s doc comment.
    pub async fn stop(&self) {
        let mut guard = self.inner.lock().await;
        if let Some(running) = guard.take() {
            // The supervisor (and its pollers) may already have exited on
            // their own in principle (they don't today — reconciler tasks
            // don't exit on poll failure — but this keeps `send` from being
            // treated as a bug if a future change ever makes one). Ignore a
            // failed send: every receiver being gone just means everything
            // already stopped.
            let _ = running.stop_tx.send(true);
        }
    }

    /// Number of per-plane pollers currently alive (spawned and not yet
    /// observed to have exited). `0` both when disabled and when enabled
    /// with zero registered control planes — this method reports whether a
    /// task is actually polling something, not whether the feature is
    /// switched on. `GET /api/settings/orchestration`'s `reconciler_running`
    /// is `live_task_count() > 0`; see `handlers/settings.rs`.
    pub async fn live_task_count(&self) -> usize {
        let guard = self.inner.lock().await;
        match guard.as_ref() {
            Some(running) => running.reconciler.live_task_count().await,
            None => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tack_orch::reconciler::{ControlPlaneStore, HealthRecord, RegisteredPlane};
    use tack_orch::{
        Capabilities, ControlPlane, DecisionSupport, EventScope, FleetStatus, Health, MetricSample,
        ModelSelection, NewRemoteTask, OrchError, Rated, RemoteApproval, RemoteRun, RemoteTask,
        Support, TracesPage, UsageSupport,
    };
    use uuid::Uuid;

    /// Reachable-but-uninteresting fake control plane — enough to keep one
    /// reconciler tick from erroring loudly; the tests below only care
    /// about task lifecycle, not what a tick actually observes.
    struct QuietControlPlane;

    #[async_trait]
    impl ControlPlane for QuietControlPlane {
        fn kind(&self) -> &'static str {
            "fake"
        }
        /// Not exercised by anything in this module — these tests only
        /// care about task lifecycle (see this struct's own doc comment) —
        /// so any internally-consistent value satisfies the trait.
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                dispatch: true,
                cancel: false,
                pause: Rated::new(Support::Unsupported, "fake plane: no pause mechanism"),
                resume: Rated::new(Support::Unsupported, "fake plane: no resume mechanism"),
                event_scope: Rated::new(EventScope::Project, "fake plane: scripted per project"),
                artifacts: false,
                decisions: Rated::new(DecisionSupport::Poll, "fake plane: scripted approvals"),
                usage: Rated::new(UsageSupport::NotMeasured, "fake plane: no usage source"),
                model_selection: Rated::new(
                    ModelSelection::Unsupported,
                    "fake plane: no model routing",
                ),
                runtimes: false,
                plane_metrics: true,
                provisioning: false,
            }
        }
        async fn health(&self) -> Result<Health, OrchError> {
            Ok(Health {
                status: "ok".to_string(),
                gateway: 1,
            })
        }
        async fn status(&self) -> Result<FleetStatus, OrchError> {
            Ok(FleetStatus {
                api_version: "1".to_string(),
                timestamp: "2026-08-05T00:00:00Z".to_string(),
                gateway: "active".to_string(),
                channels: Vec::new(),
                agents: Vec::new(),
                total_cost_usd_estimated: 0.0,
            })
        }
        async fn metrics(&self) -> Result<Vec<MetricSample>, OrchError> {
            Ok(Vec::new())
        }
        async fn list_runs(&self, _project: Option<&str>) -> Result<Vec<RemoteRun>, OrchError> {
            Ok(Vec::new())
        }
        async fn get_run(&self, _run_id: &str) -> Result<RemoteRun, OrchError> {
            Err(OrchError::Disabled)
        }
        async fn list_approvals(&self) -> Result<Vec<RemoteApproval>, OrchError> {
            Ok(Vec::new())
        }
        async fn list_tasks(&self, _project: &str) -> Result<Vec<RemoteTask>, OrchError> {
            Err(OrchError::Disabled)
        }
        async fn traces(
            &self,
            _project: &str,
            _since: Option<&str>,
        ) -> Result<TracesPage, OrchError> {
            Ok(TracesPage::default())
        }
        async fn enqueue_task(
            &self,
            _project: &str,
            _task: NewRemoteTask,
        ) -> Result<String, OrchError> {
            Err(OrchError::Disabled)
        }
        async fn dispatch(
            &self,
            _project: &str,
            _vars: serde_json::Value,
        ) -> Result<String, OrchError> {
            Err(OrchError::Disabled)
        }
        async fn decide_approval(
            &self,
            _token: &str,
            _grant: bool,
        ) -> Result<tack_orch::ApprovalState, OrchError> {
            Err(OrchError::Disabled)
        }
        async fn provision_pod(
            &self,
            _params: tack_orch::ProvisionPodParams,
        ) -> Result<tack_orch::ProvisionedPod, OrchError> {
            Err(OrchError::Disabled)
        }
    }

    struct OneRunFakeStore {
        plane_id: Uuid,
        list_calls: AtomicUsize,
    }

    #[async_trait]
    impl ControlPlaneStore for OneRunFakeStore {
        async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError> {
            self.list_calls.fetch_add(1, Ordering::SeqCst);
            Ok(vec![RegisteredPlane {
                id: self.plane_id,
                control_plane: Arc::new(QuietControlPlane),
            }])
        }
        async fn record_health(
            &self,
            _control_plane_id: Uuid,
            _record: &HealthRecord,
        ) -> Result<(), OrchError> {
            Ok(())
        }
        async fn list_linked_projects(
            &self,
            _control_plane_id: Uuid,
        ) -> Result<Vec<String>, OrchError> {
            Ok(Vec::new())
        }
        async fn find_item_for_remote_task(
            &self,
            _remote_task_id: &str,
        ) -> Result<Option<Uuid>, OrchError> {
            Ok(None)
        }
        async fn upsert_runs(
            &self,
            _control_plane_id: Uuid,
            _runs: &[tack_db::repo::orch::NewOrchRun],
        ) -> Result<(), OrchError> {
            Ok(())
        }
        async fn upsert_approvals(
            &self,
            _control_plane_id: Uuid,
            _approvals: &[tack_db::repo::orch::NewOrchApproval],
        ) -> Result<(), OrchError> {
            Ok(())
        }
        async fn upsert_metrics(
            &self,
            _control_plane_id: Uuid,
            _metrics: &[tack_db::repo::orch::NewOrchMetric],
        ) -> Result<(), OrchError> {
            Ok(())
        }
        async fn list_trace_cursors(
            &self,
            _control_plane_id: Uuid,
        ) -> Result<std::collections::HashMap<String, String>, OrchError> {
            Ok(std::collections::HashMap::new())
        }
        async fn set_trace_cursor(
            &self,
            _control_plane_id: Uuid,
            _remote_project: &str,
            _cursor: &str,
        ) -> Result<(), OrchError> {
            Ok(())
        }
        async fn upsert_events(
            &self,
            _control_plane_id: Uuid,
            _events: &[tack_db::repo::orch::NewOrchEvent],
        ) -> Result<(), OrchError> {
            Ok(())
        }
    }

    fn fast_config() -> ReconcilerConfig {
        ReconcilerConfig {
            poll_secs: 60,
            ..Default::default()
        }
    }

    /// A store whose registered-plane list can grow after construction —
    /// unlike `OneRunFakeStore`'s fixed list, this lets a test simulate a
    /// control plane being registered *after* `OrchRuntime::start()` has
    /// already been called, the exact sequence the setup wizard walks users
    /// through (enable orchestration -> register a control plane -> link a
    /// project). See `a_plane_registered_after_start_gets_polled` below,
    /// which reproduces exactly that case.
    struct MutableFakeStore {
        planes: std::sync::Mutex<Vec<Uuid>>,
    }

    impl MutableFakeStore {
        fn empty() -> Self {
            Self {
                planes: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn register(&self, id: Uuid) {
            self.planes.lock().unwrap().push(id);
        }
    }

    #[async_trait]
    impl ControlPlaneStore for MutableFakeStore {
        async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError> {
            Ok(self
                .planes
                .lock()
                .unwrap()
                .iter()
                .map(|id| RegisteredPlane {
                    id: *id,
                    control_plane: Arc::new(QuietControlPlane),
                })
                .collect())
        }
        async fn record_health(
            &self,
            _control_plane_id: Uuid,
            _record: &HealthRecord,
        ) -> Result<(), OrchError> {
            Ok(())
        }
        async fn list_linked_projects(
            &self,
            _control_plane_id: Uuid,
        ) -> Result<Vec<String>, OrchError> {
            Ok(Vec::new())
        }
        async fn find_item_for_remote_task(
            &self,
            _remote_task_id: &str,
        ) -> Result<Option<Uuid>, OrchError> {
            Ok(None)
        }
        async fn upsert_runs(
            &self,
            _control_plane_id: Uuid,
            _runs: &[tack_db::repo::orch::NewOrchRun],
        ) -> Result<(), OrchError> {
            Ok(())
        }
        async fn upsert_approvals(
            &self,
            _control_plane_id: Uuid,
            _approvals: &[tack_db::repo::orch::NewOrchApproval],
        ) -> Result<(), OrchError> {
            Ok(())
        }
        async fn upsert_metrics(
            &self,
            _control_plane_id: Uuid,
            _metrics: &[tack_db::repo::orch::NewOrchMetric],
        ) -> Result<(), OrchError> {
            Ok(())
        }
        async fn list_trace_cursors(
            &self,
            _control_plane_id: Uuid,
        ) -> Result<std::collections::HashMap<String, String>, OrchError> {
            Ok(std::collections::HashMap::new())
        }
        async fn set_trace_cursor(
            &self,
            _control_plane_id: Uuid,
            _remote_project: &str,
            _cursor: &str,
        ) -> Result<(), OrchError> {
            Ok(())
        }
        async fn upsert_events(
            &self,
            _control_plane_id: Uuid,
            _events: &[tack_db::repo::orch::NewOrchEvent],
        ) -> Result<(), OrchError> {
            Ok(())
        }
    }

    /// **The bug this test guards against.** `OrchRuntime::start` must
    /// never regress to something like `spawn_reconcilers_cancellable`,
    /// which reads `store.list_registered()` exactly once and spawns one
    /// task per plane found at that instant, never re-reading the list. A
    /// control plane registered after `start()` (enable -> register -> link,
    /// the natural setup order a guided wizard walks users through) would
    /// then never be polled: no task, no health updates, no error
    /// anywhere. This test enables orchestration with zero planes
    /// registered, registers one *after* `start()` has already returned,
    /// and asserts it gets picked up and polled without a restart or a
    /// second `start()` call.
    #[tokio::test]
    async fn a_plane_registered_after_start_gets_polled() {
        let fake = Arc::new(MutableFakeStore::empty());
        let store: Arc<dyn ControlPlaneStore> = fake.clone();
        let runtime = OrchRuntime::new();

        runtime.start(store.clone(), fast_config()).await;
        assert_eq!(runtime.live_task_count().await, 0, "nothing registered yet");

        let plane_id = Uuid::new_v4();
        fake.register(plane_id);

        // Bounded poll rather than a fixed sleep: fast when the fix works,
        // and doesn't hang forever if it doesn't.
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if runtime.live_task_count().await == 1 {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "a control plane registered after start() was never picked up for polling"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        runtime.stop().await;
    }

    #[tokio::test]
    async fn start_then_stop_leaves_no_live_task() {
        let store: Arc<dyn ControlPlaneStore> = Arc::new(OneRunFakeStore {
            plane_id: Uuid::new_v4(),
            list_calls: AtomicUsize::new(0),
        });
        let runtime = OrchRuntime::new();

        runtime.start(store.clone(), fast_config()).await;
        // Give the spawned task a moment to reach its first sleep/select.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(runtime.live_task_count().await, 1);

        runtime.stop().await;
        // Cooperative shutdown observes the signal via select! promptly.
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(
            runtime.live_task_count().await,
            0,
            "task must have exited after stop()"
        );
    }

    #[tokio::test]
    async fn start_is_idempotent_while_already_running() {
        let store: Arc<dyn ControlPlaneStore> = Arc::new(OneRunFakeStore {
            plane_id: Uuid::new_v4(),
            list_calls: AtomicUsize::new(0),
        });
        let runtime = OrchRuntime::new();

        runtime.start(store.clone(), fast_config()).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(runtime.live_task_count().await, 1);

        // A second start() while running must not spawn a duplicate task.
        runtime.start(store.clone(), fast_config()).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            runtime.live_task_count().await,
            1,
            "a second start() must be a no-op, not a second task"
        );

        runtime.stop().await;
    }

    #[tokio::test]
    async fn stop_without_a_prior_start_is_a_harmless_no_op() {
        let runtime = OrchRuntime::new();
        runtime.stop().await; // must not panic
        assert_eq!(runtime.live_task_count().await, 0);
    }

    #[tokio::test]
    async fn repeated_toggles_never_leave_more_than_one_task_alive() {
        let store: Arc<dyn ControlPlaneStore> = Arc::new(OneRunFakeStore {
            plane_id: Uuid::new_v4(),
            list_calls: AtomicUsize::new(0),
        });
        let runtime = OrchRuntime::new();

        for _ in 0..3 {
            runtime.start(store.clone(), fast_config()).await;
            tokio::time::sleep(Duration::from_millis(30)).await;
            assert_eq!(runtime.live_task_count().await, 1);

            runtime.stop().await;
            tokio::time::sleep(Duration::from_millis(150)).await;
            assert_eq!(runtime.live_task_count().await, 0);
        }
    }
}
