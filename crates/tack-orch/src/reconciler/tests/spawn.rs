//! `spawn_reconcilers`/the supervised per-plane task loop: registration,
//! global start/stop, dynamic register/delete while running, panic
//! isolation, and a store error at listing time. Drives `FakeControlPlane`
//! (from `super::support`) through `FakeStore` or, where a test needs to
//! mutate the roster after spawning, the local `MutableStore`.

use super::super::*;
use super::support::{FakeControlPlane, FakeStore, wait_until};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn panic_in_a_poll_is_isolated_by_the_task_boundary() {
    let cp: Arc<dyn ControlPlane> = Arc::new(FakeControlPlane::panics_on_health());
    // This is exactly the isolation spawn_one relies on: reconcile_once
    // runs inside its own tokio::spawn, so a panic inside it surfaces as
    // a JoinError to the caller instead of unwinding the caller's stack.
    let result = tokio::spawn(async move { reconcile_once(&cp, &[], &HashMap::new()).await }).await;
    assert!(
        result.is_err(),
        "the panic must surface as a JoinError, not propagate"
    );
}

fn healthy_plane(id: Uuid) -> RegisteredPlane {
    RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy()),
    }
}

#[tokio::test]
async fn disabled_orchestration_spawns_no_tasks_never_queries_store() {
    let store = Arc::new(FakeStore::new(vec![healthy_plane(Uuid::new_v4())]));
    let handles = spawn_reconcilers(false, store.clone(), ReconcilerConfig::default()).await;
    assert!(handles.is_empty());
    assert!(
        !store.list_called.load(Ordering::SeqCst),
        "a disabled reconciler must not even query for registered planes"
    );
}

#[tokio::test]
async fn enabled_orchestration_spawns_one_task_per_registered_plane() {
    let store = Arc::new(FakeStore::new(vec![
        healthy_plane(Uuid::new_v4()),
        healthy_plane(Uuid::new_v4()),
        healthy_plane(Uuid::new_v4()),
    ]));
    let handles = spawn_reconcilers(
        true,
        store.clone(),
        ReconcilerConfig {
            poll_secs: 1,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(handles.len(), 3);
    for h in handles {
        h.abort();
    }
}

// -- Supervised spawn: self-healing rather than a one-time snapshot —
//    see the module doc above `spawn_reconcilers_supervised`

/// A store whose registered-plane list can change after construction —
/// unlike `FakeStore`'s fixed `Vec`, this lets a test simulate a control
/// plane being registered or deleted *while the supervisor is already
/// running*.
struct MutableStore {
    planes: Mutex<Vec<RegisteredPlane>>,
}

impl MutableStore {
    fn new(planes: Vec<RegisteredPlane>) -> Self {
        Self {
            planes: Mutex::new(planes),
        }
    }

    fn register(&self, plane: RegisteredPlane) {
        self.planes.lock().unwrap().push(plane);
    }

    fn delete(&self, id: Uuid) {
        self.planes.lock().unwrap().retain(|p| p.id != id);
    }
}

#[async_trait::async_trait]
impl ControlPlaneStore for MutableStore {
    async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError> {
        Ok(self.planes.lock().unwrap().clone())
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
        _runs: &[NewOrchRun],
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn upsert_approvals(
        &self,
        _control_plane_id: Uuid,
        _approvals: &[NewOrchApproval],
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn upsert_metrics(
        &self,
        _control_plane_id: Uuid,
        _metrics: &[NewOrchMetric],
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn list_trace_cursors(
        &self,
        _control_plane_id: Uuid,
    ) -> Result<HashMap<String, String>, OrchError> {
        Ok(HashMap::new())
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
        _events: &[NewOrchEvent],
    ) -> Result<(), OrchError> {
        Ok(())
    }
}

/// A `poll_secs` long enough that, absent cancellation, a test relying
/// on it would time out rather than pass by accident, paired with a
/// `supervisor_scan_secs` fast enough to keep tests quick (the field's
/// unit is whole seconds — 1 is the floor `supervisor_loop` enforces).
fn fast_scan_config() -> ReconcilerConfig {
    ReconcilerConfig {
        poll_secs: 60,
        supervisor_scan_secs: 1,
        ..Default::default()
    }
}

#[tokio::test]
async fn supervised_spawn_starts_one_task_per_registered_plane() {
    let store = Arc::new(FakeStore::new(vec![
        healthy_plane(Uuid::new_v4()),
        healthy_plane(Uuid::new_v4()),
        healthy_plane(Uuid::new_v4()),
    ]));
    let (_stop_tx, stop_rx) = watch::channel(false);

    let reconciler = spawn_reconcilers_supervised(store.clone(), fast_scan_config(), stop_rx).await;

    // The initial reconcile_tick inside spawn_reconcilers_supervised is
    // synchronous, so all 3 are already up by the time `.await` above
    // resolves — no polling wait needed here, unlike the tests below.
    assert_eq!(reconciler.live_task_count().await, 3);
}

#[tokio::test]
async fn supervised_spawn_stops_every_task_on_global_stop() {
    let id = Uuid::new_v4();
    let store = Arc::new(FakeStore::new(vec![healthy_plane(id)]));
    let (stop_tx, stop_rx) = watch::channel(false);

    let reconciler = spawn_reconcilers_supervised(store.clone(), fast_scan_config(), stop_rx).await;
    assert_eq!(reconciler.live_task_count().await, 1);

    stop_tx.send(true).expect("receiver still alive");

    wait_until(
        Duration::from_secs(3),
        "task should stop on its own within 3s of the global stop signal",
        || {
            // Synchronous best-effort check: `live_task_count` is async,
            // so poll it via try_lock instead of blocking this closure.
            reconciler
                .tasks
                .try_lock()
                .map(|g| g.values().all(|t| t.handle.is_finished()))
                .unwrap_or(false)
        },
    )
    .await;
}

#[tokio::test]
async fn supervised_spawn_already_stopped_never_starts_a_tick() {
    let id = Uuid::new_v4();
    let store = Arc::new(FakeStore::new(vec![healthy_plane(id)]));
    // Signalled *before* spawning — spawn_reconcilers_supervised's own
    // check must skip the initial reconcile_tick entirely, and the
    // supervisor loop's top-of-loop check must never let one through
    // either.
    let (stop_tx, stop_rx) = watch::channel(true);
    drop(stop_tx);

    let reconciler = spawn_reconcilers_supervised(store.clone(), fast_scan_config(), stop_rx).await;
    assert_eq!(
        reconciler.live_task_count().await,
        0,
        "a pre-stopped supervisor must never start a poller, even for an \
         already-registered plane"
    );
}

#[tokio::test]
async fn repeated_global_start_stop_cycles_leave_no_task_running() {
    let id = Uuid::new_v4();
    let store = Arc::new(FakeStore::new(vec![healthy_plane(id)]));

    for _ in 0..3 {
        let (stop_tx, stop_rx) = watch::channel(false);
        let reconciler =
            spawn_reconcilers_supervised(store.clone(), fast_scan_config(), stop_rx).await;
        assert_eq!(
            reconciler.live_task_count().await,
            1,
            "one task per cycle, never accumulating"
        );

        stop_tx.send(true).expect("receiver still alive");
        wait_until(
            Duration::from_secs(3),
            "each cycle's task must stop before the next starts",
            || {
                reconciler
                    .tasks
                    .try_lock()
                    .map(|g| g.values().all(|t| t.handle.is_finished()))
                    .unwrap_or(false)
            },
        )
        .await;
    }
}

/// **Guards against `list_registered()` being read only once at spawn
/// time.** (`tack-api`'s `orch_runtime.rs` has the same reproduction one
/// layer up, against `OrchRuntime` itself.) A plane registered after spawn
/// must still get polled. This
/// starts the supervisor with zero planes registered, registers one
/// after it's already running, and asserts it gets picked up — the
/// "enable -> register -> link" sequence the setup wizard walks
/// users through.
#[tokio::test]
async fn a_plane_registered_after_the_supervisor_starts_gets_polled() {
    let store = Arc::new(MutableStore::new(Vec::new()));
    let (_stop_tx, stop_rx) = watch::channel(false);

    let reconciler = spawn_reconcilers_supervised(store.clone(), fast_scan_config(), stop_rx).await;
    assert_eq!(
        reconciler.live_task_count().await,
        0,
        "nothing registered yet"
    );

    let plane_id = Uuid::new_v4();
    store.register(healthy_plane(plane_id));

    wait_until(
        Duration::from_secs(5),
        "a control plane registered after the supervisor started was never picked up",
        || {
            reconciler
                .tasks
                .try_lock()
                .map(|g| g.len() == 1)
                .unwrap_or(false)
        },
    )
    .await;
}

/// The other half of the diff: a plane removed from `control_planes`
/// (deleted through the API, or vanishing by any other means — this
/// fake doesn't distinguish) must have its poller stopped on the very
/// next scan, with no global stop signal involved at all.
#[tokio::test]
async fn a_deleted_plane_stops_being_polled_with_no_global_stop() {
    let plane_id = Uuid::new_v4();
    let store = Arc::new(MutableStore::new(vec![healthy_plane(plane_id)]));
    let (_stop_tx, stop_rx) = watch::channel(false);

    let reconciler = spawn_reconcilers_supervised(store.clone(), fast_scan_config(), stop_rx).await;
    assert_eq!(reconciler.live_task_count().await, 1);

    store.delete(plane_id);

    wait_until(
        Duration::from_secs(5),
        "a plane deleted from the store kept being polled",
        || {
            reconciler
                .tasks
                .try_lock()
                .map(|g| g.is_empty())
                .unwrap_or(false)
        },
    )
    .await;
}

/// No leaked tasks across repeated register/delete churn — the map
/// converges back to empty every time, not just eventually.
#[tokio::test]
async fn repeated_register_delete_cycles_leak_no_tasks() {
    let store = Arc::new(MutableStore::new(Vec::new()));
    let (_stop_tx, stop_rx) = watch::channel(false);
    let reconciler = spawn_reconcilers_supervised(store.clone(), fast_scan_config(), stop_rx).await;

    for _ in 0..3 {
        let plane_id = Uuid::new_v4();
        store.register(healthy_plane(plane_id));
        wait_until(
            Duration::from_secs(5),
            "registered plane was never picked up during a churn cycle",
            || {
                reconciler
                    .tasks
                    .try_lock()
                    .map(|g| g.len() == 1)
                    .unwrap_or(false)
            },
        )
        .await;

        store.delete(plane_id);
        wait_until(
            Duration::from_secs(5),
            "deleted plane's poller was never stopped during a churn cycle",
            || {
                reconciler
                    .tasks
                    .try_lock()
                    .map(|g| g.is_empty())
                    .unwrap_or(false)
            },
        )
        .await;
    }
}

#[tokio::test]
async fn a_running_plane_task_persists_health_after_its_first_tick() {
    let id = Uuid::new_v4();
    let store = Arc::new(FakeStore::new(vec![healthy_plane(id)]));
    let handles = spawn_reconcilers(
        true,
        store.clone(),
        ReconcilerConfig {
            poll_secs: 1,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(handles.len(), 1);

    // The loop polls immediately on start (no artificial initial delay —
    // deliberately different from the due-soon/backup schedulers), so
    // record_health lands as soon as the executor gives the task a turn.
    wait_until(
        Duration::from_secs(5),
        "first tick never persisted a health record",
        || {
            store
                .health_records
                .lock()
                .map(|r| r.iter().any(|(rid, _)| *rid == id))
                .unwrap_or(false)
        },
    )
    .await;
    for h in handles {
        h.abort();
    }

    let records = store.health_records.lock().unwrap();
    assert!(
        records
            .iter()
            .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy)
    );
}

#[tokio::test]
async fn store_error_listing_planes_yields_no_tasks_not_a_panic() {
    let handles =
        spawn_reconcilers(true, Arc::new(FailingStore), ReconcilerConfig::default()).await;
    assert!(handles.is_empty());
}

/// A store whose `list_registered` always errors — every other method is
/// unreachable once that happens, so each just returns an empty/`Ok` value.
struct FailingStore;

#[async_trait::async_trait]
impl ControlPlaneStore for FailingStore {
    async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError> {
        Err(OrchError::Unavailable("db unreachable".into()))
    }
    async fn record_health(&self, _id: Uuid, _record: &HealthRecord) -> Result<(), OrchError> {
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
        _runs: &[NewOrchRun],
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn upsert_approvals(
        &self,
        _control_plane_id: Uuid,
        _approvals: &[NewOrchApproval],
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn upsert_metrics(
        &self,
        _control_plane_id: Uuid,
        _metrics: &[NewOrchMetric],
    ) -> Result<(), OrchError> {
        Ok(())
    }
    async fn list_trace_cursors(
        &self,
        _control_plane_id: Uuid,
    ) -> Result<HashMap<String, String>, OrchError> {
        Ok(HashMap::new())
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
        _events: &[NewOrchEvent],
    ) -> Result<(), OrchError> {
        Ok(())
    }
}
