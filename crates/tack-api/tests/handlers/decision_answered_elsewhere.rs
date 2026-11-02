//! A question a harness asked can be answered in the harness's own tool. The runner then tells the
//! board, which closes the decision as answered elsewhere so the run can ask its next question.

use crate::common;
use crate::handlers::mrp::{
    ClaimedAttempt, accept_and_start, headers_ref, operator_headers, request_and_claim,
    setup_with_bus,
};
use axum::http::StatusCode;
use serde_json::{Value, json};
use tack_app::runs::resolve_decision::{ResolveOutcome, resolve_decision};
use tack_core::decision::DecisionResolvedBy;
use tack_db::queries::execution_decisions;
use tack_protocol::execution::AttemptId;

fn question(attempt: &ClaimedAttempt, decision_id: &str) -> Value {
    json!({
        "protocol_version": 1, "runner_id": attempt.runner_id, "attempt_id": attempt.attempt_id,
        "fencing_token": attempt.fencing_token, "decision_id": decision_id, "kind": "approval",
        "prompt": "Run the install?", "options": [{"option_id": "allow", "label": "Allow"}],
        "metadata": {},
    })
}

fn answered_elsewhere(attempt: &ClaimedAttempt, decision_id: &str) -> Value {
    json!({
        "protocol_version": 1, "runner_id": attempt.runner_id, "attempt_id": attempt.attempt_id,
        "fencing_token": attempt.fencing_token, "decision_id": decision_id, "source": "docket",
    })
}

async fn runner(
    app: &axum::Router,
    attempt: &ClaimedAttempt,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let (status, body, _) = common::send_with_raw(
        app,
        "POST",
        &format!("/api/runner/v1/attempts/{}/{path}", attempt.attempt_id),
        body,
        &headers_ref(&attempt.auth),
    )
    .await;
    (status, body)
}

#[tokio::test]
async fn a_decision_answered_in_the_harness_is_closed_and_the_next_question_can_be_asked() {
    let root = tempfile::tempdir().unwrap();
    let (app, repo, item, _ws, _bus) = setup_with_bus(root.path(), None).await;
    let attempt = request_and_claim(&app, &item, "elsewhere").await;
    accept_and_start(&app, &attempt).await;

    let (status, body) = runner(&app, &attempt, "decisions", question(&attempt, "dec-1")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["state"], "pending");

    let (status, settled) = runner(
        &app,
        &attempt,
        "decisions/answered-elsewhere",
        answered_elsewhere(&attempt, "dec-1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{settled}");
    assert_eq!(settled["state"], "answered_elsewhere");

    // The board keeps who closed it, and a poll hands it to the runner as it does any decision.
    let row = execution_decisions::decision_of_attempt(&repo, &attempt.attempt_id, "dec-1")
        .await
        .unwrap()
        .expect("the decision stays");
    assert_eq!(row.state, "answered_elsewhere");
    assert_eq!(row.answer, None);
    let by: Value = serde_json::from_str(row.resolved_by.as_deref().unwrap()).unwrap();
    assert_eq!(by, json!({"kind": "harness", "subject_id": "docket"}));

    // Telling it twice changes nothing and still answers 200.
    let (status, again) = runner(
        &app,
        &attempt,
        "decisions/answered-elsewhere",
        answered_elsewhere(&attempt, "dec-1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["state"], "answered_elsewhere");

    // The run is no longer waiting on it, so the next question is taken.
    let (status, next) = runner(&app, &attempt, "decisions", question(&attempt, "dec-2")).await;
    assert_eq!(status, StatusCode::OK, "{next}");
    assert_eq!(next["state"], "pending");

    // A person who still has it open cannot answer what was already answered.
    let late = resolve_decision(
        &repo,
        chrono::Utc::now(),
        &AttemptId::new(attempt.attempt_id.as_str()),
        "dec-1",
        &json!({"option_id": "allow"}),
        &DecisionResolvedBy::operator("operator:local"),
    )
    .await
    .unwrap();
    assert_eq!(late, ResolveOutcome::AnsweredElsewhere);
}

#[tokio::test]
async fn only_the_runner_that_holds_the_attempt_can_close_a_decision() {
    let root = tempfile::tempdir().unwrap();
    let (app, _repo, item, _ws, _bus) = setup_with_bus(root.path(), None).await;
    let attempt = request_and_claim(&app, &item, "owner").await;
    accept_and_start(&app, &attempt).await;
    let (status, body) = runner(&app, &attempt, "decisions", question(&attempt, "dec-1")).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // An operator's token is not a runner's credential.
    let (status, _, _) = common::send_with_raw(
        &app,
        "POST",
        &format!(
            "/api/runner/v1/attempts/{}/decisions/answered-elsewhere",
            attempt.attempt_id
        ),
        answered_elsewhere(&attempt, "dec-1"),
        &operator_headers(),
    )
    .await;
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
        "{status}"
    );

    // A stale fence is refused like any other attempt-scoped call.
    let mut stale = answered_elsewhere(&attempt, "dec-1");
    stale["fencing_token"] = json!(attempt.fencing_token + 1);
    let (status, body) = runner(&app, &attempt, "decisions/answered-elsewhere", stale).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    // A decision the attempt never asked is not found.
    let (status, body) = runner(
        &app,
        &attempt,
        "decisions/answered-elsewhere",
        answered_elsewhere(&attempt, "dec-unknown"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
}
