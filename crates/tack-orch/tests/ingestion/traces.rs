//! Trace ingestion against a real `Repository`, a real `DocketAdapter`, and
//! the real reconciler loop — proving two things the reconciler's own
//! fake-store unit tests cannot: idempotent re-polling through the real
//! `orch_events` table, and that a re-ingested, already-purged event is
//! never resurrected or double-counted by a later rollup.
//!
//! `TestRepoStore` and the polling helpers live in `support.rs`, shared
//! with `runs.rs`.

use chrono::Utc;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use tack_db::Repository;
use tack_db::repo::orch::{OrchEventDailyAggregate, RollupStats};
use tack_orch::reconciler::DEFAULT_RETENTION_DAYS;

use crate::common::setup_test_db;
use crate::support::{
    fast_poll_config, last_seen_at, mount_health_and_status, orch_event_count, plane_health,
    poll_until, run_one_more_tick, seed_control_plane_and_link, seed_project_with_pending_task,
    spawn_reconciler, stop_reconciler, wait_and_stop,
};

async fn rollup_and_purge(repo: &Repository) -> RollupStats {
    repo.rollup_and_purge_orch_events(Utc::now(), 500)
        .await
        .expect("rollup and purge")
}

async fn daily_events(repo: &Repository, control_plane_id: Uuid) -> Vec<OrchEventDailyAggregate> {
    repo.list_orch_events_daily(control_plane_id)
        .await
        .expect("list daily aggregate")
}

async fn assert_daily_event_count(
    repo: &Repository,
    control_plane_id: Uuid,
    expected: i64,
    msg: &str,
) {
    assert_eq!(
        daily_events(repo, control_plane_id).await[0].event_count,
        expected,
        "{msg}"
    );
}

/// Fetches the events attributed to `item_id`, asserting there's exactly
/// one, and returns its `event_type` — every caller here goes on to check
/// that type, and this is the only test asserting correlation narrows to a
/// single event rather than mirroring every fetched event onto the item.
async fn expect_one_event_type(repo: &Repository, item_id: Uuid) -> String {
    let events = repo
        .list_orch_events_for_item(item_id, None)
        .await
        .expect("list events for item");
    assert_eq!(
        events.len(),
        1,
        "only the correlated event attributes to the item"
    );
    events[0].event_type.clone()
}

const EMPTY_RUNS_BODY: &str = r#"{"runs":[]}"#;
const EMPTY_APPROVALS_BODY: &str = r#"{"pending":[]}"#;

/// This file doesn't exercise runs/approvals, but a linked project makes
/// `poll_runs` fire too — mock both empty on top of the shared health/status
/// pair so they never error this file's health assertions or logs.
async fn mount_common(server: &MockServer) {
    mount_health_and_status(server).await;
    Mock::given(method("GET"))
        .and(path("/runs"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY_RUNS_BODY))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/approvals"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY_APPROVALS_BODY))
        .mount(server)
        .await;
}

/// Builds docket's real wire shape for `GET /traces/{project}` (see
/// `adapters/docket.rs`'s module doc): `events` is an array of raw JSON
/// **strings**, each independently encoding one event object, not an array
/// of objects. Every mock body in this file goes through this helper
/// specifically so a regression to the old (wrong) shape fails here too.
fn traces_body(events: &[serde_json::Value], next: &str) -> String {
    let encoded: Vec<String> = events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap())
        .collect();
    serde_json::json!({ "events": encoded, "next": next }).to_string()
}

fn trace_event_json(session_id: &str, ts: &str, event_type: &str) -> serde_json::Value {
    serde_json::json!({
        "ts": ts,
        "project": "demo",
        "session_id": session_id,
        "agent_role": "lead",
        "event_type": event_type,
        "payload": {"tool": "bash", "command": "cargo test -p tack-orch"},
        "cost_usd": 0.0021,
        "duration_ms": 842
    })
}

/// Mounts two events (a correlated `tool_call`, an uncorrelated
/// `session_start`) at `/traces/demo`. Ignores `since` entirely, so a
/// rewound cursor re-fetches this exact same overlapping window.
async fn mount_overlapping_trace_events(server: &MockServer) {
    let events = vec![
        trace_event_json("agent:demo:task-1", "2026-08-04T19:52:27Z", "tool_call"),
        trace_event_json(
            "agent:demo:dispatch",
            "2026-08-04T19:52:40Z",
            "session_start",
        ),
    ];
    Mock::given(method("GET"))
        .and(path("/traces/demo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(traces_body(&events, "2026-08-04T19:52:40Z:1")),
        )
        .mount(server)
        .await;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn overlapping_polls_correlate_once_and_never_duplicate() {
    let repo = setup_test_db().await;
    let fixture = seed_project_with_pending_task(&repo).await;

    let server = MockServer::start().await;
    mount_common(&server).await;
    mount_overlapping_trace_events(&server).await;

    let control_plane_id =
        seed_control_plane_and_link(&repo, fixture.project.id, &server.uri()).await;
    let config = fast_poll_config(DEFAULT_RETENTION_DAYS);
    let handles = spawn_reconciler(&repo, config).await;
    poll_until("both trace events land", || async {
        orch_event_count(&repo).await == 2
    })
    .await;
    let landed_at = last_seen_at(&repo, control_plane_id).await;
    wait_and_stop(&repo, control_plane_id, landed_at, handles).await;
    assert_eq!(
        orch_event_count(&repo).await,
        2,
        "overlapping polls duplicated rows"
    );
    assert_eq!(
        expect_one_event_type(&repo, fixture.item.id).await,
        "tool_call"
    );

    repo.set_trace_cursor(control_plane_id, "demo", "")
        .await
        .expect("rewind cursor");
    let before_repoll = last_seen_at(&repo, control_plane_id).await;
    run_one_more_tick(&repo, control_plane_id, before_repoll, config).await;
    assert_eq!(
        orch_event_count(&repo).await,
        2,
        "a rewound cursor re-ingesting an overlapping window must add zero rows"
    );
    assert_eq!(plane_health(&repo, control_plane_id).await, "healthy");
}

/// Mounts one fixture event dated `2020-01-01` — deliberately ancient,
/// standing in for what a badly-rewound cursor would re-deliver long after
/// a retention sweep already rolled it up and purged it.
async fn mount_ancient_trace_event(server: &MockServer) {
    let stale_ts = "2020-01-01T00:00:05Z";
    let events = vec![trace_event_json("agent:demo:task-1", stale_ts, "tool_call")];
    Mock::given(method("GET"))
        .and(path("/traces/demo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(traces_body(&events, &format!("{stale_ts}:1"))),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn purged_trace_events_are_never_resurrected_or_recounted() {
    let repo = setup_test_db().await;
    let fixture = seed_project_with_pending_task(&repo).await;

    let server = MockServer::start().await;
    mount_common(&server).await;
    mount_ancient_trace_event(&server).await;

    let control_plane_id =
        seed_control_plane_and_link(&repo, fixture.project.id, &server.uri()).await;

    // 1: ingest with a wide-enough retention window that 2020 isn't filtered
    // at ingest time. 2: roll up and purge it, as a retention sweep would.
    let handles = spawn_reconciler(&repo, fast_poll_config(36_500)).await;
    poll_until("the stale event lands", || async {
        orch_event_count(&repo).await == 1
    })
    .await;
    stop_reconciler(handles).await;
    assert_eq!(rollup_and_purge(&repo).await.rows_purged, 1);
    assert_eq!(orch_event_count(&repo).await, 0);
    assert_daily_event_count(&repo, control_plane_id, 1, "rolled up exactly once").await;

    // 3: re-poll with a realistic window; the mock ignores `since`, standing
    // in for a rewound/lost cursor delivering the same stale event again.
    let _ = repo.set_trace_cursor(control_plane_id, "demo", "").await;
    let since = last_seen_at(&repo, control_plane_id).await;
    run_one_more_tick(&repo, control_plane_id, since, fast_poll_config(90)).await;
    assert_eq!(orch_event_count(&repo).await, 0, "must not be resurrected");
    assert_eq!(rollup_and_purge(&repo).await.rows_purged, 0);
    assert_daily_event_count(&repo, control_plane_id, 1, "never double-counts").await;
}
