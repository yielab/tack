//! The two fakes every other submodule of `reconciler::tests` drives
//! `reconcile_once`/`spawn_reconcilers` against: `FakeControlPlane` (a
//! scriptable `ControlPlane`) and `FakeStore` (a scriptable
//! `ControlPlaneStore`). Neither does real I/O. A fixture used by only one
//! submodule (`MutableStore`, `FailingStore`, the retention-sweep fakes)
//! stays defined in that submodule instead of here.

use super::super::*;
use crate::{
    ApprovalState, Capabilities, DecisionSupport, EventScope, ModelSelection, NewRemoteTask, Rated,
    RemoteApproval, RemoteEvent, RemoteRun, RemoteTask, RunState, Support, TaskStatus,
    UsageSupport,
};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

// -- Fake ControlPlane, for reconcile_once / spawn tests ---------------

/// `(events, next)` scripted for one `(project, since)` pair — `next` is
/// scripted explicitly (not derived) since the real cursor is opaque and
/// remote-minted.
type ScriptedTracesResponse = (Vec<RemoteEvent>, Option<String>);

/// A `ControlPlane` whose `health`/`status`/`list_runs`/`list_approvals`
/// responses are scripted; every other method (not needed by these
/// tests) returns `Disabled`. Drives `reconcile_once`/`spawn_reconcilers`
/// without a real adapter.
pub(super) struct FakeControlPlane {
    health_calls: AtomicUsize,
    healthy: bool,
    panic_on_health: bool,
    runs: Vec<RemoteRun>,
    approvals: Vec<RemoteApproval>,
    approvals_should_fail: bool,
    metrics: Vec<MetricSample>,
    metrics_should_fail: bool,
    /// Keyed by the `project`/`since` pair `traces()` was called with —
    /// lets a single test script different responses per project without
    /// needing per-project fakes. Most tests don't care about `next` and
    /// leave it `None` via [`Self::with_traces`]; [`Self::with_traces_next`]
    /// is there for the ones that do.
    traces: std::collections::HashMap<(String, Option<String>), ScriptedTracesResponse>,
    traces_should_fail: bool,
}

impl FakeControlPlane {
    pub(super) fn healthy() -> Self {
        Self {
            health_calls: AtomicUsize::new(0),
            healthy: true,
            panic_on_health: false,
            runs: Vec::new(),
            approvals: Vec::new(),
            approvals_should_fail: false,
            metrics: Vec::new(),
            metrics_should_fail: false,
            traces: std::collections::HashMap::new(),
            traces_should_fail: false,
        }
    }

    pub(super) fn panics_on_health() -> Self {
        Self {
            panic_on_health: true,
            ..Self::healthy()
        }
    }

    pub(super) fn with_runs(runs: Vec<RemoteRun>) -> Self {
        Self {
            runs,
            ..Self::healthy()
        }
    }

    pub(super) fn with_approvals(approvals: Vec<RemoteApproval>) -> Self {
        Self {
            approvals,
            ..Self::healthy()
        }
    }

    pub(super) fn healthy_with_failing_approvals() -> Self {
        Self {
            approvals_should_fail: true,
            ..Self::healthy()
        }
    }

    pub(super) fn with_metrics(metrics: Vec<MetricSample>) -> Self {
        Self {
            metrics,
            ..Self::healthy()
        }
    }

    pub(super) fn healthy_with_failing_metrics() -> Self {
        Self {
            metrics_should_fail: true,
            ..Self::healthy()
        }
    }

    /// `traces()` for `(project, since)` returns `events` with no `next`
    /// cursor (`None`) — for tests that only care about the events
    /// themselves. Call multiple times to script more than one
    /// project/cursor combination.
    pub(super) fn with_traces(
        mut self,
        project: &str,
        since: Option<&str>,
        events: Vec<RemoteEvent>,
    ) -> Self {
        self.traces.insert(
            (project.to_string(), since.map(str::to_string)),
            (events, None),
        );
        self
    }

    /// Same as [`Self::with_traces`], but also scripts the exact `next`
    /// cursor the remote "minted" for this response — for tests that
    /// assert on the persisted cursor value (the opaque cursor is
    /// remote-minted and scripted, not computed by the fake).
    pub(super) fn with_traces_next(
        mut self,
        project: &str,
        since: Option<&str>,
        events: Vec<RemoteEvent>,
        next: Option<&str>,
    ) -> Self {
        self.traces.insert(
            (project.to_string(), since.map(str::to_string)),
            (events, next.map(str::to_string)),
        );
        self
    }

    pub(super) fn healthy_with_failing_traces() -> Self {
        Self {
            traces_should_fail: true,
            ..Self::healthy()
        }
    }
}

#[async_trait::async_trait]
impl ControlPlane for FakeControlPlane {
    fn kind(&self) -> &'static str {
        "fake"
    }

    /// Not exercised by any test in this module (every test here
    /// scripts `health`/`status`/`list_runs`/`list_approvals`/etc., not
    /// capability negotiation) — a plausible, internally-consistent
    /// value so the trait is satisfied, nothing more.
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
        self.health_calls.fetch_add(1, Ordering::SeqCst);
        if self.panic_on_health {
            panic!("simulated adapter bug");
        }
        if self.healthy {
            Ok(Health {
                status: "ok".into(),
                gateway: 1,
            })
        } else {
            Err(OrchError::Unavailable("connection refused".into()))
        }
    }

    async fn status(&self) -> Result<FleetStatus, OrchError> {
        Ok(super::health::sample_status(EXPECTED_API_VERSION))
    }

    async fn metrics(&self) -> Result<Vec<crate::MetricSample>, OrchError> {
        if self.metrics_should_fail {
            Err(OrchError::Unavailable("metrics endpoint down".into()))
        } else {
            Ok(self.metrics.clone())
        }
    }

    async fn list_runs(&self, _project: Option<&str>) -> Result<Vec<RemoteRun>, OrchError> {
        Ok(self.runs.clone())
    }

    async fn get_run(&self, _run_id: &str) -> Result<RemoteRun, OrchError> {
        Err(OrchError::Disabled)
    }

    async fn list_approvals(&self) -> Result<Vec<RemoteApproval>, OrchError> {
        if self.approvals_should_fail {
            Err(OrchError::Unavailable("approvals endpoint down".into()))
        } else {
            Ok(self.approvals.clone())
        }
    }

    async fn list_tasks(&self, _project: &str) -> Result<Vec<RemoteTask>, OrchError> {
        Err(OrchError::Disabled)
    }

    async fn traces(&self, project: &str, since: Option<&str>) -> Result<TracesPage, OrchError> {
        if self.traces_should_fail {
            return Err(OrchError::Unavailable("traces endpoint down".into()));
        }
        let key = (project.to_string(), since.map(str::to_string));
        let (events, next) = self.traces.get(&key).cloned().unwrap_or_default();
        Ok(TracesPage { events, next })
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
    ) -> Result<crate::ApprovalState, OrchError> {
        Err(OrchError::Disabled)
    }

    async fn provision_pod(
        &self,
        _params: crate::ProvisionPodParams,
    ) -> Result<crate::ProvisionedPod, OrchError> {
        Err(OrchError::Disabled)
    }
}

// Silence "unused" on ApprovalState/RunState/TaskStatus imports, which
// exist only so this module compiles against the exact same import set
// future poll_* fns will need; keep them imported here as a
// living example rather than trimming and re-adding later.
#[allow(dead_code)]
fn _uses_remote_enums(_a: ApprovalState, _b: RunState, _c: TaskStatus) {}

// -- Persistence-side fake, for spawn_reconcilers tests -----------------

pub(super) struct FakeStore {
    planes: Vec<RegisteredPlane>,
    pub(super) list_called: AtomicBool,
    pub(super) health_records: Mutex<Vec<(Uuid, HealthRecord)>>,
    /// `orch_links.remote_project` stand-in — what `list_linked_projects`
    /// returns for every plane (this fake doesn't scope by
    /// `control_plane_id`; one plane per test is enough for these tests).
    linked_projects: Vec<String>,
    /// `orch_tasks.remote_task_id -> item_id` stand-in for
    /// `find_item_for_remote_task`.
    known_tasks: std::collections::HashMap<String, Uuid>,
    pub(super) upserted_runs: Mutex<Vec<(Uuid, Vec<NewOrchRun>)>>,
    pub(super) upserted_approvals: Mutex<Vec<(Uuid, Vec<NewOrchApproval>)>>,
    pub(super) upserted_metrics: Mutex<Vec<(Uuid, Vec<NewOrchMetric>)>>,
    /// `orch_trace_cursors` stand-in, seeded via [`Self::with_trace_cursor`]
    /// and mutated by `set_trace_cursor` — a `Mutex` (not the read-only
    /// `linked_projects`/`known_tasks` shape) because the reconciler
    /// itself writes to it every tick.
    pub(super) trace_cursors: Mutex<std::collections::HashMap<String, String>>,
    pub(super) upserted_events: Mutex<Vec<(Uuid, Vec<NewOrchEvent>)>>,
}

impl FakeStore {
    pub(super) fn new(planes: Vec<RegisteredPlane>) -> Self {
        Self {
            planes,
            list_called: AtomicBool::new(false),
            health_records: Mutex::new(Vec::new()),
            linked_projects: Vec::new(),
            known_tasks: std::collections::HashMap::new(),
            upserted_runs: Mutex::new(Vec::new()),
            upserted_approvals: Mutex::new(Vec::new()),
            upserted_metrics: Mutex::new(Vec::new()),
            trace_cursors: Mutex::new(std::collections::HashMap::new()),
            upserted_events: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn with_linked_projects(mut self, projects: Vec<String>) -> Self {
        self.linked_projects = projects;
        self
    }

    pub(super) fn with_known_task(mut self, remote_task_id: &str, item_id: Uuid) -> Self {
        self.known_tasks.insert(remote_task_id.to_string(), item_id);
        self
    }

    pub(super) fn with_trace_cursor(self, remote_project: &str, cursor: &str) -> Self {
        self.trace_cursors
            .lock()
            .unwrap()
            .insert(remote_project.to_string(), cursor.to_string());
        self
    }
}

#[async_trait::async_trait]
impl ControlPlaneStore for FakeStore {
    async fn list_registered(&self) -> Result<Vec<RegisteredPlane>, OrchError> {
        self.list_called.store(true, Ordering::SeqCst);
        Ok(self.planes.clone())
    }

    async fn record_health(
        &self,
        control_plane_id: Uuid,
        record: &HealthRecord,
    ) -> Result<(), OrchError> {
        self.health_records
            .lock()
            .unwrap()
            .push((control_plane_id, record.clone()));
        Ok(())
    }

    async fn list_linked_projects(
        &self,
        _control_plane_id: Uuid,
    ) -> Result<Vec<String>, OrchError> {
        Ok(self.linked_projects.clone())
    }

    async fn find_item_for_remote_task(
        &self,
        remote_task_id: &str,
    ) -> Result<Option<Uuid>, OrchError> {
        Ok(self.known_tasks.get(remote_task_id).copied())
    }

    async fn upsert_runs(
        &self,
        control_plane_id: Uuid,
        runs: &[NewOrchRun],
    ) -> Result<(), OrchError> {
        self.upserted_runs
            .lock()
            .unwrap()
            .push((control_plane_id, runs.to_vec()));
        Ok(())
    }

    async fn upsert_approvals(
        &self,
        control_plane_id: Uuid,
        approvals: &[NewOrchApproval],
    ) -> Result<(), OrchError> {
        self.upserted_approvals
            .lock()
            .unwrap()
            .push((control_plane_id, approvals.to_vec()));
        Ok(())
    }

    async fn upsert_metrics(
        &self,
        control_plane_id: Uuid,
        metrics: &[NewOrchMetric],
    ) -> Result<(), OrchError> {
        self.upserted_metrics
            .lock()
            .unwrap()
            .push((control_plane_id, metrics.to_vec()));
        Ok(())
    }

    async fn list_trace_cursors(
        &self,
        _control_plane_id: Uuid,
    ) -> Result<HashMap<String, String>, OrchError> {
        Ok(self.trace_cursors.lock().unwrap().clone())
    }

    async fn set_trace_cursor(
        &self,
        _control_plane_id: Uuid,
        remote_project: &str,
        cursor: &str,
    ) -> Result<(), OrchError> {
        self.trace_cursors
            .lock()
            .unwrap()
            .insert(remote_project.to_string(), cursor.to_string());
        Ok(())
    }

    async fn upsert_events(
        &self,
        control_plane_id: Uuid,
        events: &[NewOrchEvent],
    ) -> Result<(), OrchError> {
        self.upserted_events
            .lock()
            .unwrap()
            .push((control_plane_id, events.to_vec()));
        Ok(())
    }
}

/// Polls `f` every 20ms until it returns `true` or `deadline` passes,
/// panicking with `msg` in the latter case — a bounded wait instead of a
/// fixed sleep, shared by every submodule that spawns a real tick and
/// waits for it to reach some observable state.
pub(super) async fn wait_until(deadline: Duration, msg: &str, mut f: impl FnMut() -> bool) {
    let start = tokio::time::Instant::now();
    let mut ticker = tokio::time::interval(Duration::from_millis(20));
    loop {
        if f() {
            return;
        }
        assert!(tokio::time::Instant::now() - start < deadline, "{msg}");
        ticker.tick().await;
    }
}
