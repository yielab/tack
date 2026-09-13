//! One poll cycle's persistence for each of the four things a tick
//! fetches — runs, approvals, metrics, trace events — against a real
//! `FakeStore`: correlation to an `orch_tasks` row (or its absence),
//! poll-failure isolation (health still updates, nothing persists), and
//! (for traces) event-id derivation, cursor advancement and the
//! retention-age guard. Drives `FakeControlPlane`/`FakeStore` from
//! `super::support` through one manually-invoked `reconcile_once` per
//! test via the local `run_one_tick`/`run_one_tick_with_config` helpers.

use super::super::*;
use super::support::{FakeControlPlane, FakeStore, wait_until};
use crate::{ApprovalState, RemoteApproval, RemoteEvent, RemoteRun, RunSource, RunState};

// -- Runs + approvals ingestion -----------------------------------------

fn sample_run(id: &str, project: &str, task_ids: Vec<String>) -> RemoteRun {
    RemoteRun {
        id: id.to_string(),
        source: RunSource::Cli,
        project: project.to_string(),
        state: RunState::Succeeded,
        task_ids,
        error: String::new(),
        created: "2026-08-04T19:50:43.129083+00:00".to_string(),
        started_at: Some("2026-08-04T19:50:43.129674+00:00".to_string()),
        finished_at: Some("2026-08-04T19:50:43.130194+00:00".to_string()),
        pids: Vec::new(),
        variables: serde_json::json!({}),
    }
}

fn sample_approval(token: &str, context: serde_json::Value) -> RemoteApproval {
    RemoteApproval {
        token: token.to_string(),
        project: "demo".to_string(),
        role: "implementer".to_string(),
        action: "pod dispatch — task enqueue".to_string(),
        state: ApprovalState::Pending,
        created: "2026-08-04T19:50:50Z".to_string(),
        context,
    }
}

/// Runs `spawn_one`'s loop (via `spawn_reconcilers`) for one tick against
/// a `FakeStore` (whose plane list is set via [`FakeStore::new`]), then
/// aborts the task and returns the store for assertions. Every test
/// below follows this same "one tick, then inspect what got upserted"
/// shape.
async fn run_one_tick(store: Arc<FakeStore>) -> Arc<FakeStore> {
    run_one_tick_with_config(
        store,
        ReconcilerConfig {
            poll_secs: 60,
            ..Default::default()
        },
    )
    .await
}

/// Spawns `spawn_reconcilers` with `config` against `store`, waits for the
/// first tick's persist phase to finish, then aborts every handle and
/// returns the store for assertions. record_health is the first persist
/// call each tick makes (see `spawn_one`), strictly before
/// persist_runs/approvals/metrics/events — waiting for it here waits for
/// the whole tick, since none of `FakeStore`'s methods actually suspend and
/// the executor only yields this task back at a real await point (the next
/// tick's own wait).
async fn run_one_tick_with_config(
    store: Arc<FakeStore>,
    config: ReconcilerConfig,
) -> Arc<FakeStore> {
    let store_dyn: Arc<dyn ControlPlaneStore> = store.clone();
    let handles = spawn_reconcilers(true, store_dyn, config).await;
    assert_eq!(handles.len(), 1, "expected exactly one plane registered");
    wait_until(
        Duration::from_secs(5),
        "first tick never persisted a health record",
        || {
            store
                .health_records
                .lock()
                .map(|r| !r.is_empty())
                .unwrap_or(false)
        },
    )
    .await;
    for h in handles {
        h.abort();
    }
    store
}

#[tokio::test]
async fn approvals_poll_failure_leaves_plane_health_untouched() {
    let id = Uuid::new_v4();
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy_with_failing_approvals()),
    };
    let store = Arc::new(FakeStore::new(vec![plane]));
    let store = run_one_tick(store).await;

    let records = store.health_records.lock().unwrap();
    assert!(
        records
            .iter()
            .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy),
        "a /approvals failure must not degrade plane health: {records:?}"
    );
}

#[tokio::test]
async fn a_correlated_run_lands_with_the_right_item_id() {
    let id = Uuid::new_v4();
    let item_id = Uuid::new_v4();
    let run = sample_run("run-1", "demo", vec!["task-1".to_string()]);
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::with_runs(vec![run])),
    };
    let store = Arc::new(
        FakeStore::new(vec![plane])
            .with_linked_projects(vec!["demo".to_string()])
            .with_known_task("task-1", item_id),
    );
    let store = run_one_tick(store).await;

    let upserted = store.upserted_runs.lock().unwrap();
    let (_, runs) = upserted
        .iter()
        .find(|(cp_id, runs)| *cp_id == id && !runs.is_empty())
        .expect("expected at least one upserted run");
    let run = runs.iter().find(|r| r.run_id == "run-1").expect("run-1");
    assert_eq!(run.item_id, Some(item_id));
    assert_eq!(run.remote_project, "demo");
    assert_eq!(run.source, "cli");
    assert_eq!(run.state, "succeeded");
}

#[tokio::test]
async fn an_uncorrelated_run_lands_with_item_id_none() {
    let id = Uuid::new_v4();
    // Empty task_ids: the normal shape of a run dispatched from
    // docket's own CLI, not through Tack. Must not error.
    let run = sample_run("run-cli-only", "demo", vec![]);
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::with_runs(vec![run])),
    };
    let store =
        Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));
    let store = run_one_tick(store).await;

    let upserted = store.upserted_runs.lock().unwrap();
    let (_, runs) = upserted
        .iter()
        .find(|(cp_id, runs)| *cp_id == id && !runs.is_empty())
        .expect("expected the CLI-dispatched run to still be mirrored");
    let run = runs
        .iter()
        .find(|r| r.run_id == "run-cli-only")
        .expect("run-cli-only");
    assert_eq!(run.item_id, None);

    // And plane health must be entirely unaffected by this.
    let records = store.health_records.lock().unwrap();
    assert!(
        records
            .iter()
            .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy)
    );
}

#[tokio::test]
async fn a_correlated_approval_lands_with_the_right_item_id() {
    let id = Uuid::new_v4();
    let item_id = Uuid::new_v4();
    let approval = sample_approval(
        "apr-1",
        serde_json::json!({"taskId": "task-1", "pipelineIndex": 2}),
    );
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::with_approvals(vec![approval])),
    };
    let store = Arc::new(FakeStore::new(vec![plane]).with_known_task("task-1", item_id));
    let store = run_one_tick(store).await;

    let upserted = store.upserted_approvals.lock().unwrap();
    let (_, approvals) = upserted
        .iter()
        .find(|(cp_id, approvals)| *cp_id == id && !approvals.is_empty())
        .expect("expected at least one upserted approval");
    let approval = approvals
        .iter()
        .find(|a| a.token == "apr-1")
        .expect("apr-1");
    assert_eq!(approval.item_id, Some(item_id));
    assert_eq!(approval.remote_task_id.as_deref(), Some("task-1"));
    assert_eq!(approval.agent.as_deref(), Some("implementer"));
    assert_eq!(approval.state, "pending");
}

#[tokio::test]
async fn an_uncorrelated_approval_lands_with_item_id_none() {
    let id = Uuid::new_v4();
    // No "taskId" in context at all — an approval Tack cannot attribute
    // to any item. This must still persist (item_id: NULL),
    // not be dropped, since it's exactly the kind of approval most
    // likely to silently block a fleet.
    let approval = sample_approval("apr-uncorrelated", serde_json::json!({}));
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::with_approvals(vec![approval])),
    };
    let store = Arc::new(FakeStore::new(vec![plane]));
    let store = run_one_tick(store).await;

    let upserted = store.upserted_approvals.lock().unwrap();
    let (_, approvals) = upserted
        .iter()
        .find(|(cp_id, approvals)| *cp_id == id && !approvals.is_empty())
        .expect("expected the uncorrelated approval to still be mirrored");
    let approval = approvals
        .iter()
        .find(|a| a.token == "apr-uncorrelated")
        .expect("apr-uncorrelated");
    assert_eq!(approval.item_id, None);
    assert_eq!(approval.remote_task_id, None);
}

#[test]
fn extract_task_id_handles_missing_and_non_string_taskid() {
    assert_eq!(
        extract_task_id(&serde_json::json!({"taskId": "task-1", "pipelineIndex": 0})),
        Some("task-1".to_string())
    );
    assert_eq!(extract_task_id(&serde_json::json!({})), None);
    assert_eq!(
        extract_task_id(&serde_json::json!({"taskId": 42})),
        None,
        "a non-string taskId is treated as uncorrelated, not a parse error"
    );
    assert_eq!(extract_task_id(&serde_json::json!(null)), None);
}

#[test]
fn parse_optional_rfc3339_accepts_both_timestamp_conventions() {
    // core/runs.py's `+00:00` offset form.
    assert!(parse_optional_rfc3339(Some("2026-08-04T19:50:43.129083+00:00")).is_some());
    // core/approval.py's `Z` form.
    assert!(parse_optional_rfc3339(Some("2026-08-04T19:50:50Z")).is_some());
    // Malformed input degrades to None rather than panicking/erroring.
    assert_eq!(parse_optional_rfc3339(Some("not-a-timestamp")), None);
    assert_eq!(parse_optional_rfc3339(None), None);
}

// -- Metrics ingestion ----------------------------------------------------

fn sample_metric(name: &str, value: f64) -> MetricSample {
    let mut labels = std::collections::BTreeMap::new();
    labels.insert("agent".to_string(), "demo-lead".to_string());
    MetricSample {
        name: name.to_string(),
        labels,
        value,
    }
}

#[tokio::test]
async fn metrics_land_via_upsert_metrics_on_a_successful_poll() {
    let id = Uuid::new_v4();
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::with_metrics(vec![
            sample_metric("docket_agents_total", 3.0),
            sample_metric("docket_agent_cost_usd", 1.5),
        ])),
    };
    let store = Arc::new(FakeStore::new(vec![plane]));
    let store = run_one_tick(store).await;

    let upserted = store.upserted_metrics.lock().unwrap();
    let (_, metrics) = upserted
        .iter()
        .find(|(cp_id, metrics)| *cp_id == id && !metrics.is_empty())
        .expect("expected at least one upserted metric batch");
    assert_eq!(metrics.len(), 2);
    assert!(
        metrics
            .iter()
            .any(|m| m.name == "docket_agents_total" && m.value == 3.0)
    );
    assert!(
        metrics
            .iter()
            .any(|m| m.name == "docket_agent_cost_usd" && m.value == 1.5)
    );
}

#[tokio::test]
async fn metrics_poll_failure_leaves_health_untouched_no_persist() {
    let id = Uuid::new_v4();
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy_with_failing_metrics()),
    };
    let store = Arc::new(FakeStore::new(vec![plane]));
    let store = run_one_tick(store).await;

    let records = store.health_records.lock().unwrap();
    assert!(
        records
            .iter()
            .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy),
        "a /metrics failure must not degrade plane health: {records:?}"
    );

    let upserted = store.upserted_metrics.lock().unwrap();
    assert!(
        upserted.iter().all(|(_, m)| m.is_empty()),
        "a failed metrics poll must not persist anything: {upserted:?}"
    );
}

// -- Trace ingestion --------------------------------------------------------

fn sample_event(session_id: &str, ts: &str, event_type: &str) -> RemoteEvent {
    RemoteEvent {
        ts: ts.to_string(),
        project: "demo".to_string(),
        session_id: session_id.to_string(),
        agent_role: "lead".to_string(),
        event_type: event_type.to_string(),
        payload: serde_json::json!({"tool": "bash", "command": "cargo test"}),
        cost_usd_estimated: Some(0.0021),
        duration_ms: Some(842),
    }
}

#[test]
fn derive_event_id_is_deterministic_for_the_same_source_event() {
    let cp_id = Uuid::new_v4();
    let event = sample_event("agent:demo:task-1", "2026-08-04T19:52:27Z", "tool_call");
    let id1 = derive_event_id(cp_id, "demo", &event);
    let id2 = derive_event_id(cp_id, "demo", &event);
    assert_eq!(
        id1, id2,
        "the same source event must always derive the same id"
    );
}

#[test]
fn derive_event_id_differs_when_any_field_differs() {
    let cp_id = Uuid::new_v4();
    let base = sample_event("agent:demo:task-1", "2026-08-04T19:52:27Z", "tool_call");
    let base_id = derive_event_id(cp_id, "demo", &base);

    let mut different_ts = base.clone();
    different_ts.ts = "2026-08-04T19:52:28Z".to_string();
    assert_ne!(base_id, derive_event_id(cp_id, "demo", &different_ts));

    let mut different_payload = base.clone();
    different_payload.payload = serde_json::json!({"tool": "bash", "command": "cargo build"});
    assert_ne!(base_id, derive_event_id(cp_id, "demo", &different_payload));

    assert_ne!(
        base_id,
        derive_event_id(cp_id, "other-project", &base),
        "the same event on a different remote_project must derive a different id"
    );
    assert_ne!(
        base_id,
        derive_event_id(Uuid::new_v4(), "demo", &base),
        "the same event on a different control plane must derive a different id"
    );
}

#[test]
fn derive_event_id_ignores_field_boundaries_not_field_content() {
    // Naive delimiter-free concatenation would hash "a" + "bc" the same
    // as "ab" + "c" — the \u{1} separator in derive_event_id must
    // prevent that. Two events differing only in where a boundary falls
    // between session_id and agent_role must derive different ids.
    let cp_id = Uuid::new_v4();
    let mut a = sample_event("agent:demo:ab", "2026-08-04T19:52:27Z", "tool_call");
    a.agent_role = "c".to_string();
    let mut b = sample_event("agent:demo:a", "2026-08-04T19:52:27Z", "tool_call");
    b.agent_role = "bc".to_string();
    assert_ne!(
        derive_event_id(cp_id, "demo", &a),
        derive_event_id(cp_id, "demo", &b)
    );
}

/// Pins `derive_event_id`'s output to a literal UUID — the determinism
/// tests just above this one only
/// prove the function returns the same id for the same input *within
/// one build*. They would not notice a changed field separator, a
/// reordered field in the `format!` (`:1058-1066`), or a changed
/// [`ORCH_EVENT_ID_NAMESPACE`] byte constant, because both the "before"
/// and "after" id in that comparison would move together and still
/// match each other.
///
/// That distinction matters because `ORCH_EVENT_ID_NAMESPACE`'s own doc
/// comment states the real stake: this id is `orch_events.id`, and
/// `upsert_orch_events`'s `ON CONFLICT(id) DO UPDATE` is what makes
/// re-ingesting an already-seen docket trace event a no-op. Change the
/// derivation and every event a deployment already ingested gets a
/// *different* id computed for it on the next poll after the upgrade —
/// not rejected as a duplicate, but inserted again as if new. Every
/// user's event timeline doubles its history and every cost rollup
/// built from `orch_events` counts the same spend twice, silently,
/// with a fully green test suite (see
/// `docs/plans/agnostic-control-plane.md` §6's regression table, third
/// row, for the exact refactor this catches).
///
/// If this test fails, the correct response is almost always to
/// **revert whatever changed `derive_event_id`'s output**, not to
/// update the literal below to match the new value — updating the
/// literal is only correct if every already-deployed instance's
/// `orch_events` table is being intentionally, knowingly re-keyed (a
/// decision far above what a code change should make silently).
#[test]
fn derive_event_id_matches_the_pinned_literal() {
    let control_plane_id =
        Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("valid fixed uuid");
    let event = RemoteEvent {
        ts: "2026-08-04T19:52:27Z".to_string(),
        project: "proj".to_string(),
        session_id: "agent:proj:task-1".to_string(),
        agent_role: "lead".to_string(),
        event_type: "tool_call".to_string(),
        payload: serde_json::json!({"tool": "bash", "command": "cargo test"}),
        cost_usd_estimated: Some(0.0021),
        duration_ms: Some(842),
    };
    assert_eq!(
        derive_event_id(control_plane_id, "proj", &event).to_string(),
        "4808170d-9797-561e-8fbb-dd8e9b94a9fe",
        "derive_event_id's output for this exact fixed input must never move — see this test's doc comment"
    );
}

#[test]
fn session_id_task_id_parses_the_agent_suffix_convention() {
    assert_eq!(
        session_id_task_id("agent:demo:task-90e465a8"),
        Some("task-90e465a8".to_string())
    );
    // docket also mints this convention for non-task sessions
    // (core/dispatch.py's bare dispatch session, core/pod.py's project
    // key) — parsing still succeeds, correlation against orch_tasks is
    // just expected to miss, which is not this function's concern.
    assert_eq!(
        session_id_task_id("agent:demo:dispatch"),
        Some("dispatch".to_string())
    );
    assert_eq!(session_id_task_id("not-the-agent-convention"), None);
    assert_eq!(session_id_task_id(""), None);
}

#[tokio::test]
async fn a_correlated_trace_event_lands_with_the_right_item_id() {
    let id = Uuid::new_v4();
    let item_id = Uuid::new_v4();
    let event = sample_event("agent:demo:task-1", "2026-08-04T19:52:27Z", "tool_call");
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy().with_traces("demo", None, vec![event])),
    };
    let store = Arc::new(
        FakeStore::new(vec![plane])
            .with_linked_projects(vec!["demo".to_string()])
            .with_known_task("task-1", item_id),
    );
    let store = run_one_tick(store).await;

    let upserted = store.upserted_events.lock().unwrap();
    let (_, events) = upserted
        .iter()
        .find(|(cp_id, events)| *cp_id == id && !events.is_empty())
        .expect("expected at least one upserted event");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].item_id, Some(item_id));
    assert_eq!(events[0].event_type, "tool_call");
}

#[tokio::test]
async fn an_uncorrelated_trace_lands_with_item_id_none() {
    let id = Uuid::new_v4();
    // "dispatch" is docket's own non-task session suffix (see
    // session_id_task_id's doc) — never correlates to any orch_tasks
    // row, and must not be treated as an error.
    let event = sample_event(
        "agent:demo:dispatch",
        "2026-08-04T19:52:27Z",
        "session_start",
    );
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy().with_traces("demo", None, vec![event])),
    };
    let store =
        Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));
    let store = run_one_tick(store).await;

    let upserted = store.upserted_events.lock().unwrap();
    let (_, events) = upserted
        .iter()
        .find(|(cp_id, events)| *cp_id == id && !events.is_empty())
        .expect("expected the uncorrelated event to still be mirrored");
    assert_eq!(events[0].item_id, None);

    let records = store.health_records.lock().unwrap();
    assert!(
        records
            .iter()
            .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy)
    );
}

#[tokio::test]
async fn an_unrecognised_event_type_is_stored_verbatim() {
    let id = Uuid::new_v4();
    let event = sample_event(
        "agent:demo:task-1",
        "2026-08-04T19:52:40Z",
        "some_future_event_type_v3",
    );
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy().with_traces("demo", None, vec![event])),
    };
    let store =
        Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));
    let store = run_one_tick(store).await;

    let upserted = store.upserted_events.lock().unwrap();
    let (_, events) = upserted
        .iter()
        .find(|(cp_id, events)| *cp_id == id && !events.is_empty())
        .expect("expected the event to still be mirrored");
    assert_eq!(events[0].event_type, "some_future_event_type_v3");
}

#[tokio::test]
async fn traces_poll_failure_leaves_health_untouched_no_persist() {
    let id = Uuid::new_v4();
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy_with_failing_traces()),
    };
    let store =
        Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));
    let store = run_one_tick(store).await;

    let records = store.health_records.lock().unwrap();
    assert!(
        records
            .iter()
            .any(|(rid, r)| *rid == id && r.health == HealthState::Healthy),
        "a /traces failure must not degrade plane health: {records:?}"
    );

    let upserted = store.upserted_events.lock().unwrap();
    assert!(
        upserted.iter().all(|(_, e)| e.is_empty()),
        "a failed traces poll must not persist anything: {upserted:?}"
    );
}

#[tokio::test]
async fn a_successful_traces_poll_advances_the_stored_cursor() {
    // The cursor is opaque and remote-minted — this fake
    // scripts docket's "minted" next value explicitly via
    // `with_traces_next` rather than computing one, and this test just
    // proves that value is what actually gets persisted, verbatim.
    let id = Uuid::new_v4();
    let events = vec![
        sample_event("agent:demo:task-1", "2026-08-04T19:52:27Z", "tool_call"),
        sample_event("agent:demo:task-1", "2026-08-04T19:52:40Z", "session_start"),
    ];
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy().with_traces_next(
            "demo",
            None,
            events,
            Some("2026-08-04T19:52:40Z:1"),
        )),
    };
    let store =
        Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));
    let store = run_one_tick(store).await;

    let cursors = store.trace_cursors.lock().unwrap();
    assert_eq!(
        cursors.get("demo").map(String::as_str),
        Some("2026-08-04T19:52:40Z:1")
    );
}

#[tokio::test]
async fn a_stored_cursor_is_used_as_since_on_the_next_poll() {
    let id = Uuid::new_v4();
    // FakeControlPlane's traces() is keyed by the exact (project, since)
    // pair it was called with — seeding it *only* for
    // since = Some("2026-08-04T19:52:27Z:1") proves poll_traces reads
    // the stored cursor and sends it, rather than always polling with
    // since = None.
    let event = sample_event("agent:demo:task-1", "2026-08-04T19:52:40Z", "tool_result");
    let plane = RegisteredPlane {
        id,
        control_plane: Arc::new(FakeControlPlane::healthy().with_traces(
            "demo",
            Some("2026-08-04T19:52:27Z:1"),
            vec![event],
        )),
    };
    let store = Arc::new(
        FakeStore::new(vec![plane])
            .with_linked_projects(vec!["demo".to_string()])
            .with_trace_cursor("demo", "2026-08-04T19:52:27Z:1"),
    );
    let store = run_one_tick(store).await;

    let upserted = store.upserted_events.lock().unwrap();
    let (_, events) = upserted
        .iter()
        .find(|(cp_id, events)| *cp_id == id && !events.is_empty())
        .expect(
            "expected an event — only present if poll_traces actually sent \
             the stored cursor as `since`",
        );
    assert_eq!(events[0].event_type, "tool_result");
}

#[tokio::test]
async fn an_event_older_than_the_retention_cutoff_is_not_persisted() {
    // Well outside even a 1-day retention window — simulates a rewound
    // cursor re-delivering an event that was already rolled up and purged
    // by the retention sweep (see the module doc's "Trace cursor" /
    // retention-composition section).
    let stale_event = sample_event("agent:demo:task-1", "2020-01-01T00:00:00Z", "tool_call");
    let plane = RegisteredPlane {
        id: Uuid::new_v4(),
        control_plane: Arc::new(FakeControlPlane::healthy().with_traces(
            "demo",
            None,
            vec![stale_event],
        )),
    };
    let store =
        Arc::new(FakeStore::new(vec![plane]).with_linked_projects(vec!["demo".to_string()]));
    let config = ReconcilerConfig {
        poll_secs: 60,
        event_retention_days: 1,
        ..Default::default()
    };
    let store = run_one_tick_with_config(store, config).await;

    let upserted = store.upserted_events.lock().unwrap();
    assert!(
        upserted.iter().all(|(_, e)| e.is_empty()),
        "an event older than the retention cutoff must never be (re-)inserted: {upserted:?}"
    );
}
