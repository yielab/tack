//! The tick-level contract oracle for docket reconciliation
//! (`docs/plans/agnostic-control-plane.md` §6): drives one full reconciler
//! tick — fetch AND persist, not `reconcile_once` in isolation — against a
//! real `wiremock` docket and an in-memory SQLite, snapshotting (A) the
//! ordered HTTP requests issued and (B) the resulting `orch_*` rows.
//! Complements `docket_wire_contract_test.rs`'s per-method oracle, which
//! can't see call COUNT or ORDER across a tick — see each scenario below
//! for the refactor it exists to catch. Regenerate: `UPDATE_GOLDEN=1 cargo
//! nextest run --workspace -E 'binary(docket_tick_contract_test)'`

#[path = "docket_tick_contract_test/support.rs"]
mod support;
use support::*;

use chrono::{DateTime, Utc};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use tack_db::Repository;
use tack_db::repo::orch::NewOrchEvent;

// ---------------------------------------------------------------------------
// Scenario 1 — cold start: no orch_trace_cursors row exists yet
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cold_start_no_cursor_row() {
    let repo = setup_repo().await;
    let workspace_id = seed_workspace(&repo).await;
    let project = seed_project(&repo, workspace_id).await;

    let server = MockServer::start().await;
    mount_cold_start_routes(&server).await;

    let plane_id = seed_plane(&repo, &server.uri()).await;
    link_project(&repo, project.id, plane_id, "demo").await;

    let requests = run_one_tick(&server, repo.clone(), 6).await;
    assert_requests_golden("cold_start", &requests);
    assert_rows_golden("cold_start", repo.pool(), plane_id).await;
}

/// Mounts health/status plus one run, one approval, one metric line and one
/// trace event. No `since` query param is expected on `/traces/demo` — that
/// is the whole point of this scenario: no `orch_trace_cursors` row exists
/// yet, so `poll_traces` must pass `since: None`.
async fn mount_cold_start_routes(server: &MockServer) {
    mount_health_status(server).await;
    mount_runs(
        server,
        "demo",
        format!(
            r#"{{"runs":[{}]}}"#,
            run_json(
                "run-cold-1",
                "cli",
                "running",
                "2026-08-01T00:00:00+00:00",
                None
            )
        ),
    )
    .await;
    mount_get(
        server,
        "/approvals",
        format!(
            r#"{{"pending":[{}]}}"#,
            approval_json(
                "apr-cold-1",
                "Confirm irreversible deploy",
                "2026-08-01T00:05:00Z"
            )
        ),
    )
    .await;
    mount_get(
        server,
        "/metrics",
        "docket_pod_cost_usd{pod=\"demo\"} 4.5\n",
    )
    .await;
    mount_traces(
        server,
        "demo",
        None,
        traces_body(
            &[trace_event_json(
                "agent:demo:dispatch",
                "2026-08-01T00:10:00Z",
                "session_start",
                serde_json::json!({"note": "cold-start"}),
            )],
            "2026-08-01T00:10:00Z:1",
        ),
    )
    .await;
}

// ---------------------------------------------------------------------------
// Scenario 2 — warm cursor: a stored cursor is resumed from
// ---------------------------------------------------------------------------

#[tokio::test]
async fn warm_cursor_resumes_from_the_stored_position() {
    const STORED_CURSOR: &str = "2026-08-01T12:00:00Z:0";

    let repo = setup_repo().await;
    let workspace_id = seed_workspace(&repo).await;
    let project = seed_project(&repo, workspace_id).await;

    let server = MockServer::start().await;
    mount_warm_cursor_routes(&server, STORED_CURSOR).await;

    let plane_id = seed_plane(&repo, &server.uri()).await;
    link_project(&repo, project.id, plane_id, "demo").await;
    repo.set_trace_cursor(plane_id, "demo", STORED_CURSOR)
        .await
        .expect("seed a warm cursor before the observed tick");

    let requests = run_one_tick(&server, repo.clone(), 6).await;
    assert_requests_golden("warm_cursor", &requests);
    assert_rows_golden("warm_cursor", repo.pool(), plane_id).await;
}

/// Mounts health/status plus one run, an empty approvals list, one metric
/// line and one trace event served only when `stored_cursor` is sent as
/// `since` — proving `poll_traces` reads and sends the stored cursor rather
/// than always polling from the beginning.
async fn mount_warm_cursor_routes(server: &MockServer, stored_cursor: &str) {
    mount_health_status(server).await;
    mount_runs(
        server,
        "demo",
        format!(
            r#"{{"runs":[{}]}}"#,
            run_json(
                "run-warm-1",
                "webhook",
                "succeeded",
                "2026-08-02T00:00:00+00:00",
                Some("2026-08-02T00:00:02+00:00")
            )
        ),
    )
    .await;
    mount_get(server, "/approvals", r#"{"pending":[]}"#).await;
    mount_get(
        server,
        "/metrics",
        "docket_pod_cost_usd{pod=\"demo\"} 9.75\n",
    )
    .await;
    mount_traces(
        server,
        "demo",
        Some(stored_cursor),
        traces_body(
            &[trace_event_json(
                "agent:demo:dispatch",
                "2026-08-01T12:05:00Z",
                "tool_call",
                serde_json::json!({"tool": "bash", "command": "cargo build"}),
            )],
            "2026-08-01T12:05:00Z:1",
        ),
    )
    .await;
}

// ---------------------------------------------------------------------------
// Scenario 3 — rewound cursor: catches a dropped retention/dedup guard
// ---------------------------------------------------------------------------

/// Pre-tick state: an event already ingested, then rolled up into
/// `orch_events_daily` and purged from the raw table by a retention sweep —
/// exactly what a real deployment looks like long after the event happened.
/// The stored cursor is then set to a REWOUND value (pointing at/before that
/// already-purged event, not to wherever a well-behaved cursor would have
/// advanced to after ingesting it), and the observed tick's mock genuinely
/// re-delivers that same event content alongside a fresh one — proving the
/// scenario's overlap is real, not just a cursor number that happens to be
/// small. See `persist_events`/`derive_event_id`'s doc comments in
/// `reconciler.rs` for why a content-derived id makes resurrection possible
/// in the first place: purge deletes the row, and re-ingesting identical
/// content later derives the identical id and would insert a fresh row
/// unless the retention-age guard at ingest time stops it.
#[tokio::test]
async fn rewound_cursor_never_resurrects_an_already_purged_event() {
    const ANCIENT_TS: &str = "2020-01-01T00:00:05Z";
    const REWOUND_CURSOR: &str = "2020-01-01T00:00:00Z:0";
    const FRESH_TS: &str = "2026-08-04T12:00:00Z";

    let repo = setup_repo().await;
    let workspace_id = seed_workspace(&repo).await;
    let project = seed_project(&repo, workspace_id).await;

    let server = MockServer::start().await;
    let plane_id = seed_plane(&repo, &server.uri()).await;
    link_project(&repo, project.id, plane_id, "demo").await;
    let ancient_payload = seed_and_purge_ancient_event(&repo, plane_id, ANCIENT_TS).await;
    repo.set_trace_cursor(plane_id, "demo", REWOUND_CURSOR)
        .await
        .expect("seed the rewound cursor");

    mount_health_status(&server).await;
    mount_runs(&server, "demo", r#"{"runs":[]}"#).await;
    mount_get(&server, "/approvals", r#"{"pending":[]}"#).await;
    mount_get(&server, "/metrics", "").await;
    mount_rewound_traces(
        &server,
        REWOUND_CURSOR,
        ANCIENT_TS,
        ancient_payload,
        FRESH_TS,
    )
    .await;

    let requests = run_one_tick(&server, repo.clone(), 6).await;
    assert_requests_golden("rewound_cursor", &requests);

    // The regression this scenario exists to catch, asserted directly and
    // not only through the golden diff: dropping persist_events's
    // retention-age guard would resurrect the purged row, and these
    // counts would read 2 instead of 1.
    assert_event_and_daily_counts(&repo, plane_id, 1).await;
    assert_rows_golden("rewound_cursor", repo.pool(), plane_id).await;
}

/// Seeds one ancient fixture event (2020-01-01), then rolls it up and
/// purges it — the pre-tick state a rewound cursor's re-delivery threatens
/// to resurrect. Returns the event's payload, reused verbatim in the mocked
/// re-delivery so the overlap is real content, not just an adjacent cursor.
async fn seed_and_purge_ancient_event(
    repo: &Repository,
    plane_id: Uuid,
    ts: &str,
) -> serde_json::Value {
    let payload = serde_json::json!({"tool": "bash", "command": "echo hi"});
    repo.upsert_orch_events(
        plane_id,
        &[NewOrchEvent {
            id: Uuid::new_v4(),
            item_id: None,
            run_id: None,
            event_type: "tool_call".into(),
            payload: payload.clone(),
            occurred_at: DateTime::parse_from_rfc3339(ts)
                .unwrap()
                .with_timezone(&Utc),
        }],
    )
    .await
    .expect("seed the ancient event");
    let purge_stats = repo
        .rollup_and_purge_orch_events(Utc::now(), 500)
        .await
        .expect("roll up and purge the ancient event before the observed tick");
    assert_eq!(
        purge_stats.rows_purged, 1,
        "setup bug: the ancient event must actually be purged before the tick runs"
    );
    payload
}

/// Mounts `/traces/demo?since=<cursor>` returning the overlapping
/// re-delivery: `ancient_payload` at `ancient_ts` (byte-identical to the
/// already-purged event) alongside a genuinely new event at `fresh_ts` in
/// the same page — proving the guard drops only the stale one.
async fn mount_rewound_traces(
    server: &MockServer,
    cursor: &str,
    ancient_ts: &str,
    ancient_payload: serde_json::Value,
    fresh_ts: &str,
) {
    let body = traces_body(
        &[
            trace_event_json(
                "agent:demo:dispatch",
                ancient_ts,
                "tool_call",
                ancient_payload,
            ),
            trace_event_json(
                "agent:demo:dispatch",
                fresh_ts,
                "tool_call",
                serde_json::json!({"tool": "bash", "command": "cargo test"}),
            ),
        ],
        &format!("{fresh_ts}:1"),
    );
    mount_traces(server, "demo", Some(cursor), body).await;
}

/// Both counters a resurrected raw row would double: the raw `orch_events`
/// count and its daily rollup aggregate for `plane_id`. Both must equal
/// `expected`.
async fn assert_event_and_daily_counts(repo: &Repository, plane_id: Uuid, expected: i64) {
    let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orch_events")
        .fetch_one(repo.pool())
        .await
        .expect("count orch_events rows");
    assert_eq!(
        event_count, expected,
        "an already-purged event must not be resurrected as a raw row"
    );
    let daily_count: i64 =
        sqlx::query_scalar("SELECT event_count FROM orch_events_daily WHERE control_plane_id = ?")
            .bind(plane_id.to_string())
            .fetch_one(repo.pool())
            .await
            .expect("read the daily rollup aggregate");
    assert_eq!(
        daily_count, expected,
        "the daily aggregate must not be double-counted by a resurrected raw row"
    );
}

// ---------------------------------------------------------------------------
// Scenario 4 — a plane with zero linked projects
// ---------------------------------------------------------------------------

#[tokio::test]
async fn zero_linked_projects_issues_no_per_project_calls() {
    let repo = setup_repo().await;

    let server = MockServer::start().await;
    mount_health_status(&server).await;
    Mock::given(method("GET"))
        .and(path("/approvals"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"pending":[]}"#))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/metrics"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("docket_pod_cost_usd{pod=\"unlinked\"} 1.0\n"),
        )
        .mount(&server)
        .await;
    // Deliberately no /runs or /traces/* mock: a plane with zero linked
    // projects must never call either — poll_runs/poll_traces both loop
    // over `projects` and issue zero calls for an empty slice. If a future
    // refactor iterates something other than "this plane's linked
    // projects", an unmounted request here surfaces as an unmatched
    // (still-recorded) request in the captured requests, and the golden
    // diff catches it immediately.

    let plane_id = seed_plane(&repo, &server.uri()).await;
    // No project is linked to this plane at all.

    let requests = run_one_tick(&server, repo.clone(), 4).await;

    assert_requests_golden("zero_projects", &requests);
    assert_rows_golden("zero_projects", repo.pool(), plane_id).await;
}

// ---------------------------------------------------------------------------
// Scenario 5 — a plane with three linked projects
// ---------------------------------------------------------------------------

/// A per-method wire test can't see this: it proves "given this input,
/// `DocketAdapter::traces` sends this request," not how many times
/// `reconcile_once` calls it. With 3 linked projects and 0 active runs (the
/// steady state), the tick must issue three `/runs?project=` calls and three
/// `/traces/{project}` calls; a refactor that iterates active runs instead
/// of linked projects would issue zero of each — caught only here. Mirrored
/// by `zero_linked_projects_issues_no_per_project_calls`, which proves the
/// loop legitimately issues nothing when there is nothing to poll, so this
/// scenario's count is a real signal and not just "zero is the only mode."
#[tokio::test]
async fn three_linked_projects_issues_three_per_project_calls_each() {
    let repo = setup_repo().await;
    let workspace_id = seed_workspace(&repo).await;

    let server = MockServer::start().await;
    mount_health_status(&server).await;
    mount_get(&server, "/approvals", r#"{"pending":[]}"#).await;
    mount_get(
        &server,
        "/metrics",
        "docket_pod_cost_usd{pod=\"fleet\"} 3.0\n",
    )
    .await;
    let plane_id = seed_plane(&repo, &server.uri()).await;

    // Alphabetical so this test's expected request order matches
    // `list_orch_links_for_plane`'s own `ORDER BY remote_project` — see that
    // function's doc comment in tack-db/src/repo/orch.rs.
    for (i, remote) in ["demo-a", "demo-b", "demo-c"].into_iter().enumerate() {
        link_and_mount_project(&repo, workspace_id, &server, plane_id, remote, i + 1).await;
    }

    let requests = run_one_tick(&server, repo.clone(), 10).await;

    // Asserted directly, not only through the golden diff — see this
    // function's doc comment for the exact regression this guards.
    assert_eq!(
        count_requests_by_path(&requests, "/runs"),
        3,
        "one /runs call per linked project"
    );
    assert_eq!(
        count_requests_by_path(&requests, "/traces/"),
        3,
        "one /traces call per linked project"
    );

    assert_requests_golden("three_projects", &requests);
    assert_rows_golden("three_projects", repo.pool(), plane_id).await;
}

/// Seeds one project, links it to `plane_id` as `remote`, and mounts its
/// empty `/runs` and `/traces/<remote>` routes (cursor `cursor-<n>`).
async fn link_and_mount_project(
    repo: &Repository,
    workspace_id: Uuid,
    server: &MockServer,
    plane_id: Uuid,
    remote: &str,
    cursor_suffix: usize,
) {
    let project = seed_project(repo, workspace_id).await;
    link_project(repo, project.id, plane_id, remote).await;
    mount_runs(server, remote, r#"{"runs":[]}"#).await;
    mount_traces(
        server,
        remote,
        None,
        traces_body(&[], &format!("cursor-{cursor_suffix}")),
    )
    .await;
}

/// Counts requests whose path is exactly `route`, or (when `route` ends in
/// `/`) starts with it — for `/runs` vs `/traces/<project>`.
fn count_requests_by_path(requests: &[Request], route: &str) -> usize {
    requests
        .iter()
        .filter(|r| {
            let path = r.url.path();
            if let Some(prefix) = route.strip_suffix('/') {
                path.starts_with(&format!("{prefix}/"))
            } else {
                path == route
            }
        })
        .count()
}
