//! The rule behind `POST /attempts/{attempt_id}/decisions/{decision_id}/viewed`: the first
//! timestamp wins, and a decision the attempt does not have is not found.

use chrono::{DateTime, Duration, TimeZone, Utc};
use tack_app::{AppError, runs::mark_decision_viewed::mark_decision_viewed};
use tack_db::Repository;
use tack_db::repo::execution::{NewAgentProfile, NewExecutionRequest, NewRunner, RequestSelection};
use tack_protocol::Clock;
use tack_test_support::{
    ControllableClock, create_test_workspace, make_item, make_project, seed, setup_test_db,
};

const RUNNER_ID: &str = "runner-viewed";
const PROFILE_ID: &str = "profile-viewed";

struct FakeClock(ControllableClock);

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.0.now()
    }
}

/// A store holding one claimed attempt, `att-viewed`, with one pending decision, `dec-1`.
async fn claimed_attempt_with_a_decision() -> (Repository, FakeClock) {
    let repo = setup_test_db().await;
    let clock = FakeClock(ControllableClock::new(
        Utc.with_ymd_and_hms(2026, 8, 12, 12, 0, 0).unwrap(),
    ));
    let workspace_id = create_test_workspace(&repo).await;
    let project = make_project(&repo, workspace_id).await;
    let item_id = make_item(&repo, &project).await.id.to_string();
    repo.register_runner(
        NewRunner {
            id: RUNNER_ID,
            name: "Viewed runner",
            credential_hash: "hash",
            labels: "{}",
            total_capacity: 1,
            available_capacity: 1,
            capability_snapshot: "{}",
            protocol_version: 1,
        },
        &clock,
    )
    .await
    .expect("runner");
    repo.create_agent_profile(
        NewAgentProfile {
            id: PROFILE_ID,
            name: "Viewed profile",
            instructions: "work",
            tool_policy: r#"{"mode":"safe"}"#,
            limits: "{}",
        },
        &clock,
    )
    .await
    .expect("profile");
    let snapshot = format!(
        r#"{{"request_id":"req-viewed","item_id":"{item_id}","idempotency_key":"k","created_by":{{"source":"test","subject_id":"viewed"}},"created_at":"2026-08-12T12:00:00Z","selector":{{"kind":"exact_runner","runner_id":"{RUNNER_ID}"}},"agent_profile_id":"{PROFILE_ID}","resolved_agent_profile":{{"name":"P","instructions":"work","tool_policy":{{"mode":"safe"}},"timeout_seconds":60,"budgets":{{}}}},"requested_harness_kind":"codex","requested_model":null,"repository":{{"kind":"git","remote":"https://example.test/v.git","base_revision":"abc123","subdirectory":null}},"permission_policy":{{"tools":[],"network":false}},"timeout_seconds":60,"budgets":{{}},"status_map_policy_id":null,"environment":{{}},"metadata":{{}}}}"#
    );
    repo.enqueue_execution(
        NewExecutionRequest {
            id: "req-viewed",
            item_id: &item_id,
            idempotency_scope: "item",
            idempotency_key: "k",
            request_fingerprint: "k",
            selector_kind: "exact_runner",
            selector_id: RUNNER_ID,
            agent_profile_id: Some(PROFILE_ID),
            agent_profile_snapshot: r#"{"name":"P","instructions":"work","tool_policy":{"mode":"safe"},"timeout_seconds":60,"budgets":{}}"#,
            requested_harness_kind: Some("codex"),
            requested_model: None,
            repository_snapshot: r#"{"kind":"git","remote":"https://example.test/v.git","base_revision":"abc123","subdirectory":null}"#,
            permission_policy: r#"{"tools":[],"network":false}"#,
            timeout_seconds: Some(60),
            budgets: "{}",
            status_map_policy_id: None,
            environment: "{}",
            metadata: "{}",
            request_snapshot: &snapshot,
        },
        clock.now(),
    )
    .await
    .expect("enqueue");
    repo.claim_execution_idempotent_with_snapshot(
        RUNNER_ID,
        "att-viewed",
        "att-viewed",
        Duration::seconds(300),
        &clock,
        RequestSelection::Naive,
    )
    .await
    .expect("claim")
    .expect("work available");
    seed::execution_decision(&repo, "att-viewed", "dec-1", "2026-08-12T12:00:00Z").await;
    (repo, clock)
}

#[tokio::test]
async fn viewing_twice_keeps_the_first_timestamp() {
    let (repo, clock) = claimed_attempt_with_a_decision().await;

    let first = mark_decision_viewed(&repo, &clock, "att-viewed", "dec-1")
        .await
        .expect("first view");
    clock.0.advance(Duration::minutes(5));
    let second = mark_decision_viewed(&repo, &clock, "att-viewed", "dec-1")
        .await
        .expect("second view");

    assert_eq!(first, second);
}

#[tokio::test]
async fn a_decision_the_attempt_does_not_have_is_not_found() {
    let (repo, clock) = claimed_attempt_with_a_decision().await;

    let missing = mark_decision_viewed(&repo, &clock, "att-viewed", "dec-other").await;
    let wrong_attempt = mark_decision_viewed(&repo, &clock, "att-other", "dec-1").await;

    assert!(matches!(missing, Err(AppError::NotFound)), "{missing:?}");
    assert!(
        matches!(wrong_attempt, Err(AppError::NotFound)),
        "{wrong_attempt:?}"
    );
}
