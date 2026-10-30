//! The rule behind `POST /attempts/{attempt_id}/decisions`: a running attempt opens a decision
//! and waits on it, the same decision again returns the stored one, other content under the same
//! id is an idempotency conflict that overwrites nothing, and an attempt that cannot wait or a
//! lease that ended is refused.

use crate::common;
use tack_protocol::Clock;
use tack_protocol::execution::{AttemptId, DecisionOption, FencingToken, RunnerId};

use chrono::Duration;
use common::{RUNNER_ID, running_attempt, world};
use tack_app::runs::create_decision::{DecisionCreated, DecisionRequest, create_decision};
use tack_db::queries::{execution_attempts, execution_decisions, execution_requests};
use tack_protocol::execution::ExecutionState;
use tack_test_support::seed;

async fn open(
    world: &common::World,
    attempt_id: &str,
    fence: FencingToken,
    decision_id: &str,
    prompt: &str,
) -> DecisionCreated {
    let options = [DecisionOption {
        option_id: "allow".into(),
        label: "Allow".into(),
        description: None,
        risks: None,
        estimated_tokens: None,
    }];
    create_decision(
        &world.repo,
        world.clock.now(),
        DecisionRequest {
            runner_id: &RunnerId::new(RUNNER_ID),
            attempt_id: &AttemptId::new(attempt_id),
            fencing_token: fence,
            decision_id,
            kind: "tool_permission",
            prompt,
            options: &options,
            metadata: &serde_json::Map::new(),
            recommendation: None,
            expires_at: None,
        },
    )
    .await
    .expect("create")
}

#[tokio::test]
async fn a_running_attempt_opens_a_decision_and_waits_on_it() {
    let world = world().await;
    let run = running_attempt(&world, "decision-open").await;

    let DecisionCreated::Created { state, .. } =
        open(&world, &run.attempt_id, run.fence, "dec-1", "Allow A?").await
    else {
        panic!("the decision is created");
    };
    assert_eq!(state, "pending");
    assert_eq!(
        execution_attempts::attempt_state(&world.repo, &run.attempt_id)
            .await
            .unwrap(),
        Some(ExecutionState::WaitingDecision)
    );
    assert_eq!(
        execution_requests::request_state(&world.repo, &run.request_id)
            .await
            .unwrap(),
        Some(ExecutionState::WaitingDecision)
    );
}

#[tokio::test]
async fn the_same_decision_again_is_a_replay_and_other_content_conflicts_without_overwriting() {
    let world = world().await;
    let run = running_attempt(&world, "decision-reuse").await;

    let first = open(&world, &run.attempt_id, run.fence, "dec-reuse", "Allow A?").await;
    let replay = open(&world, &run.attempt_id, run.fence, "dec-reuse", "Allow A?").await;
    assert_eq!(replay, first, "an exact replay returns the stored decision");

    let conflict = open(&world, &run.attempt_id, run.fence, "dec-reuse", "Allow B?").await;
    assert_eq!(conflict, DecisionCreated::IdempotencyConflict);
    let stored =
        execution_decisions::decision_of_attempt(&world.repo, &run.attempt_id, "dec-reuse")
            .await
            .unwrap()
            .expect("the decision exists");
    assert_eq!(
        stored.prompt, "Allow A?",
        "the conflicting retry did not overwrite the original"
    );
}

#[tokio::test]
async fn a_replay_of_a_settled_decision_that_carried_a_call_is_a_replay_not_a_conflict() {
    use tack_db::repo::execution::DecisionSettlement;

    let world = world().await;
    let run = running_attempt(&world, "decision-call").await;
    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "call".into(),
        serde_json::json!("send_email to=ana@example.com"),
    );
    metadata.insert("outward".into(), serde_json::json!(true));
    let ask = |metadata: &serde_json::Map<String, serde_json::Value>| {
        let (world, run) = (&world, &run);
        let metadata = metadata.clone();
        async move {
            create_decision(
                &world.repo,
                world.clock.now(),
                DecisionRequest {
                    runner_id: &RunnerId::new(RUNNER_ID),
                    attempt_id: &AttemptId::new(&run.attempt_id),
                    fencing_token: run.fence,
                    decision_id: "dec-call",
                    kind: "approval",
                    prompt: "send: an email (cut)",
                    options: &[],
                    metadata: &metadata,
                    recommendation: None,
                    expires_at: None,
                },
            )
            .await
            .expect("create")
        }
    };
    assert!(matches!(
        ask(&metadata).await,
        DecisionCreated::Created { .. }
    ));

    world
        .repo
        .settle_execution_decision(
            &AttemptId::new(&run.attempt_id),
            "dec-call",
            world.clock.now(),
            |_| {
                (
                    DecisionSettlement::Resolve {
                        answer: r#"{"option_id":"approve_once"}"#.into(),
                        resolved_by: r#"{"kind":"operator","subject_id":"ana"}"#.into(),
                    },
                    (),
                )
            },
        )
        .await
        .unwrap();
    let stored = execution_decisions::decision_of_attempt(&world.repo, &run.attempt_id, "dec-call")
        .await
        .unwrap()
        .expect("the decision exists");
    assert!(
        !stored.metadata.contains("ana@example.com"),
        "{}",
        stored.metadata
    );

    // The runner retries its create after the answer: the same request, which the erased call
    // does not turn into a different one.
    let DecisionCreated::Created { state, .. } = ask(&metadata).await else {
        panic!("a replay of a settled decision is answered with the stored one");
    };
    assert_eq!(state, "resolved");
    // And a different call is still a different request.
    metadata.insert("role".into(), serde_json::json!("other"));
    assert_eq!(ask(&metadata).await, DecisionCreated::IdempotencyConflict);
}

#[tokio::test]
async fn a_stale_fence_or_an_ended_lease_is_stale_and_writes_nothing() {
    let world = world().await;
    let run = running_attempt(&world, "decision-stale").await;

    assert_eq!(
        open(
            &world,
            &run.attempt_id,
            FencingToken::new(run.fence.get() + 1),
            "dec-1",
            "Allow?"
        )
        .await,
        DecisionCreated::Stale
    );
    world.clock.0.advance(Duration::seconds(61));
    assert_eq!(
        open(&world, &run.attempt_id, run.fence, "dec-1", "Allow?").await,
        DecisionCreated::Stale
    );
    assert_eq!(
        execution_decisions::decision_count_of_attempt(&world.repo, &run.attempt_id)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn an_attempt_that_cannot_wait_on_a_decision_is_ineligible() {
    let world = world().await;
    let run = running_attempt(&world, "decision-ineligible").await;
    seed::execution_attempt_state(&world.repo, &run.attempt_id, ExecutionState::NeedsOperator)
        .await;

    let refused = open(&world, &run.attempt_id, run.fence, "dec-1", "Allow?").await;
    assert_eq!(
        refused,
        DecisionCreated::Ineligible {
            state: ExecutionState::NeedsOperator
        }
    );
}
