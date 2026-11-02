//! The two facts a board card needs from a run that the lightweight list used to leave out:
//! whether the newest attempt was turned away before it started, and what the verifier
//! recommended. Both are read through the real router, the pack uploaded through the runner
//! routes as a runner does.

use crate::common;
use crate::handlers::mrp::{
    READY, accept_and_start, operator_headers, request_and_claim, setup_with_bus, upload_pack_of,
};
use serde_json::{Value, json};
use tack_protocol::execution::ExecutionState;
use tack_test_support::seed;

const NOT_READY: &str = include_str!("../../../../docs/contracts/mrp-v1/fixtures/not-ready.json");

/// One run on the board: its request id, ended as `state` with `reason` as the attempt's
/// terminal reason, and `pack` uploaded as the attempt's merge-readiness pack when there is one.
async fn run(
    app: &axum::Router,
    repo: &tack_db::Repository,
    item_id: &str,
    label: &str,
    state: ExecutionState,
    reason: Value,
    pack: Option<&str>,
) -> String {
    let attempt = request_and_claim(app, item_id, label).await;
    accept_and_start(app, &attempt).await;
    if let Some(pack) = pack {
        upload_pack_of(app, &attempt, pack.as_bytes()).await;
    }
    seed::execution_attempt_ended(repo, &attempt.attempt_id, state, &reason.to_string()).await;
    seed::execution_request_state(repo, &attempt.request_id, state).await;
    attempt.request_id
}

#[tokio::test]
async fn a_run_says_whether_it_never_started_and_what_the_verifier_recommended() {
    let root = tempfile::tempdir().unwrap();
    let (app, repo, item_id, _events) = {
        let (app, repo, item, events, _bus) = setup_with_bus(root.path(), None).await;
        (app, repo, item, events)
    };
    let project = repo
        .get_item(item_id.parse().unwrap())
        .await
        .unwrap()
        .unwrap()
        .project_id;
    use ExecutionState::{Failed, Succeeded};
    let refused = run(
        &app,
        &repo,
        &item_id,
        "refused",
        Failed,
        json!({"code": "harness_rejected", "message": "the harness kind does not match"}),
        None,
    )
    .await;
    let ceiling = run(
        &app,
        &repo,
        &item_id,
        "ceiling",
        Failed,
        json!({"message": "run refused: ceiling L3 (needs approval)"}),
        None,
    )
    .await;
    let timed_out = run(
        &app,
        &repo,
        &item_id,
        "timed-out",
        Failed,
        json!({"code": "timeout"}),
        None,
    )
    .await;
    // A run that succeeded is never "never started", whatever its reason says.
    let odd = run(
        &app,
        &repo,
        &item_id,
        "odd",
        Succeeded,
        json!({"code": "harness_rejected"}),
        None,
    )
    .await;
    let rejected = run(
        &app,
        &repo,
        &item_id,
        "rejected",
        Succeeded,
        json!({"code": "completed"}),
        Some(NOT_READY),
    )
    .await;
    let ready = run(
        &app,
        &repo,
        &item_id,
        "ready",
        Succeeded,
        json!({"code": "completed"}),
        Some(READY),
    )
    .await;
    let no_pack = run(
        &app,
        &repo,
        &item_id,
        "no-pack",
        Succeeded,
        json!({"code": "completed"}),
        None,
    )
    .await;

    let (status, body) = common::send(
        &app,
        "GET",
        &format!("/api/executions?project_id={project}"),
        json!(null),
        &operator_headers(),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    let rows: std::collections::HashMap<&str, &Value> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| (row["request_id"].as_str().unwrap(), row))
        .collect();
    let facts = |id: &String| {
        (
            rows[id.as_str()]["never_started"].clone(),
            rows[id.as_str()]["verifier_recommendation"].clone(),
        )
    };
    assert_eq!(facts(&refused), (json!(true), Value::Null), "refused");
    assert_eq!(facts(&ceiling), (json!(true), Value::Null), "ceiling");
    assert_eq!(facts(&timed_out), (json!(false), Value::Null), "timed out");
    assert_eq!(facts(&odd), (json!(false), Value::Null), "succeeded");
    assert_eq!(facts(&rejected), (json!(false), json!("reject")), "reject");
    assert_eq!(facts(&ready), (json!(false), json!("merge")), "merge");
    assert_eq!(facts(&no_pack), (json!(false), Value::Null), "no pack");
}

/// The attempt list carries the same flag, from the same rule, so the drawer reads it too.
#[tokio::test]
async fn an_attempt_says_whether_it_never_started() {
    let root = tempfile::tempdir().unwrap();
    let (app, repo, item_id, _events, _bus) = setup_with_bus(root.path(), None).await;
    let refused = run(
        &app,
        &repo,
        &item_id,
        "refused",
        ExecutionState::Failed,
        json!({"code": "harness_rejected", "message": "refused"}),
        None,
    )
    .await;
    let timed_out = run(
        &app,
        &repo,
        &item_id,
        "timed-out",
        ExecutionState::Failed,
        json!({"code": "timeout"}),
        None,
    )
    .await;
    for (request, expected) in [(refused, true), (timed_out, false)] {
        let (status, body) = common::send(
            &app,
            "GET",
            &format!("/api/executions/{request}/attempts"),
            json!(null),
            &operator_headers(),
        )
        .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(body["data"][0]["never_started"], json!(expected), "{body}");
    }
}
