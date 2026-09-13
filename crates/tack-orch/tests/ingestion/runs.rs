//! Runs + approvals ingestion against a real `Repository`, a real
//! `DocketAdapter`, and the real reconciler loop — proving the whole chain
//! (fetch -> correlate -> persist) composes correctly, not just that each
//! piece compiles against the others' types.
//!
//! `TestRepoStore` and the polling helpers live in `support.rs`, shared
//! with `traces.rs`.

use std::sync::atomic::{AtomicUsize, Ordering};

use tack_orch::reconciler::DEFAULT_RETENTION_DAYS;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use crate::common::setup_test_db;
use crate::support::{
    EMPTY_APPROVALS_BODY, expect_approval, expect_run, fast_poll_config, last_seen_at,
    mount_health_and_status as mount_common, orch_approval_count, orch_run_count, plane_health,
    poll_until, seed_control_plane_and_link, seed_project_with_pending_task, spawn_reconciler,
    wait_and_stop,
};

fn run_json(id: &str, task_ids: &str) -> String {
    format!(
        r#"{{"id":"{id}","source":"cli","project":"demo","state":"succeeded","taskIds":{task_ids},
        "error":"","created":"2026-08-04T19:50:43.129083+00:00",
        "startedAt":"2026-08-04T19:50:43.129674+00:00",
        "finishedAt":"2026-08-04T19:50:43.130194+00:00","pids":[],"variables":{{}}}}"#
    )
}

/// `run-1` correlates via `task-1`; `run-cli-only` and `apr-uncorrelated`
/// have no matching task and must still mirror, unattributed.
async fn mount_runs_and_approvals(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/runs"))
        .and(query_param("project", "demo"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"{{"runs":[{},{}]}}"#,
            run_json("run-1", r#"["task-1"]"#),
            run_json("run-cli-only", "[]"),
        )))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/approvals"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"pending":[
                {"token":"apr-1","project":"demo","role":"implementer","action":"gate",
                 "state":"pending","created":"2026-08-04T19:50:50Z",
                 "context":{"taskId":"task-1","pipelineIndex":0}},
                {"token":"apr-uncorrelated","project":"demo","role":"implementer","action":"gate",
                 "state":"pending","created":"2026-08-04T19:50:51Z","context":{}}
            ]}"#,
        ))
        .mount(server)
        .await;
}

/// `run-1`/`apr-1` correlate via `task-1`; `run-cli-only`/`apr-uncorrelated`
/// have no matching task and must still mirror, unattributed.
async fn assert_initial_correlation(repo: &tack_db::Repository) {
    assert_eq!(
        expect_run(repo, "run-cli-only").await.item_id,
        None,
        "an empty task_ids run must land unattributed, not be dropped or error"
    );
    assert_eq!(
        expect_approval(repo, "apr-1")
            .await
            .remote_task_id
            .as_deref(),
        Some("task-1")
    );
    assert_eq!(
        expect_approval(repo, "apr-uncorrelated").await.item_id,
        None
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn runs_and_approvals_correlate_and_repoll_idempotently() {
    let repo = setup_test_db().await;
    let fixture = seed_project_with_pending_task(&repo).await;

    let server = MockServer::start().await;
    mount_common(&server).await;
    mount_runs_and_approvals(&server).await;

    let control_plane_id =
        seed_control_plane_and_link(&repo, fixture.project.id, &server.uri()).await;
    let handles = spawn_reconciler(&repo, fast_poll_config(DEFAULT_RETENTION_DAYS)).await;
    poll_until("run-1 and apr-1 correlate", || async {
        matches!(repo.get_orch_run("run-1").await, Ok(Some(r)) if r.item_id == Some(fixture.item.id))
            && matches!(repo.get_orch_approval("apr-1").await, Ok(Some(a)) if a.item_id == Some(fixture.item.id))
    })
    .await;

    assert_initial_correlation(&repo).await;

    // At least one more tick of the exact same docket state must not
    // duplicate rows, and must leave health persistence unaffected.
    let since = last_seen_at(&repo, control_plane_id).await;
    wait_and_stop(&repo, control_plane_id, since, handles).await;
    assert_eq!(
        orch_run_count(&repo).await,
        2,
        "re-polling duplicated orch_runs"
    );
    assert_eq!(
        orch_approval_count(&repo).await,
        2,
        "re-polling duplicated orch_approvals"
    );
    assert_eq!(plane_health(&repo, control_plane_id).await, "healthy");
}

/// A `Respond` impl that returns a different body on each successive call,
/// repeating the last body forever once the list is exhausted — used to
/// simulate a run's `taskIds` becoming known and then (unrealistically, but
/// this is exactly the case the DB-layer COALESCE guards against) reverting
/// to unknown on a later poll.
struct SequentialBody {
    bodies: Vec<String>,
    calls: AtomicUsize,
}

impl Respond for SequentialBody {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let idx = self.calls.fetch_add(1, Ordering::SeqCst);
        let body = self
            .bodies
            .get(idx)
            .or_else(|| self.bodies.last())
            .cloned()
            .unwrap_or_default();
        ResponseTemplate::new(200).set_body_string(body)
    }
}

/// First poll: `task_ids` known, correlates. Every poll after: `task_ids`
/// empty again — simulating a poll that "forgot" the attribution. The
/// repo's `ON CONFLICT ... COALESCE(excluded.item_id, item_id)` must keep
/// the first poll's attribution regardless.
async fn mount_run_that_forgets_its_attribution(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/runs"))
        .and(query_param("project", "demo"))
        .respond_with(SequentialBody {
            bodies: vec![format!(
                r#"{{"runs":[{}]}}"#,
                run_json("run-1", r#"["task-1"]"#)
            )],
            calls: AtomicUsize::new(0),
        })
        .up_to_n_times(1)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/runs"))
        .and(query_param("project", "demo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!(r#"{{"runs":[{}]}}"#, run_json("run-1", "[]"))),
        )
        .mount(server)
        .await;
}

#[tokio::test]
async fn later_polls_never_erase_earlier_run_attribution() {
    let repo = setup_test_db().await;
    let fixture = seed_project_with_pending_task(&repo).await;

    let server = MockServer::start().await;
    mount_common(&server).await;
    Mock::given(method("GET"))
        .and(path("/approvals"))
        .respond_with(ResponseTemplate::new(200).set_body_string(EMPTY_APPROVALS_BODY))
        .mount(&server)
        .await;
    mount_run_that_forgets_its_attribution(&server).await;

    let control_plane_id =
        seed_control_plane_and_link(&repo, fixture.project.id, &server.uri()).await;
    let handles = spawn_reconciler(&repo, fast_poll_config(DEFAULT_RETENTION_DAYS)).await;
    poll_until("run-1 correlates on the first poll", || async {
        matches!(repo.get_orch_run("run-1").await, Ok(Some(r)) if r.item_id == Some(fixture.item.id))
    })
    .await;

    // At least one more poll, returning an empty task_ids list, must not
    // erase the attribution the first poll already learned.
    let since = last_seen_at(&repo, control_plane_id).await;
    wait_and_stop(&repo, control_plane_id, since, handles).await;

    let run_after_later_polls = expect_run(&repo, "run-1").await;
    assert_eq!(
        run_after_later_polls.item_id,
        Some(fixture.item.id),
        "a later poll that doesn't know the attribution must never erase one already learned"
    );
}
