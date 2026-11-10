//! The rule behind `POST /attempts/{attempt_id}/completion`: an attempt ends once per
//! `completion_id`, a success leaves the item's status to the person, and a change the verifier
//! rejected goes back to the agent as one follow-up until a stop condition holds.

use crate::common;
use tack_protocol::Clock;
use tack_protocol::execution::{AttemptId, CompletionId, FencingToken, RunnerId};

use std::future::Future;

use common::{RUNNER_ID, Running, enqueue_with, running, running_attempt, world};
use serde_json::{Value, json};
use tack_app::runs::complete::{Completed, CompletionReport, complete};
use tack_db::queries::{execution_attempts, execution_requests};
use tack_protocol::execution::ExecutionState;
use tack_protocol::mrp::MergeReadinessPack;
use tack_test_support::completion::{actual_execution, terminal_reason_of, usage_not_measured};
use tack_test_support::seed;
use uuid::Uuid;

const REJECTING_PACK: &str =
    include_str!("../../../docs/contracts/mrp-v1/fixtures/signed-with-next-steps.json");

fn pack(edit: impl Fn(&mut Value)) -> MergeReadinessPack {
    let mut pack: Value = serde_json::from_str(REJECTING_PACK).unwrap();
    edit(&mut pack);
    serde_json::from_value(pack).unwrap()
}

async fn finish<R, Fut>(
    world: &common::World,
    run: &Running,
    completion_id: &str,
    terminal_state: ExecutionState,
    terminal_reason: Value,
    read_pack: R,
) -> Completed
where
    R: Fn(String) -> Fut,
    Fut: Future<Output = Option<MergeReadinessPack>>,
{
    complete(
        &world.repo,
        world.clock.now(),
        CompletionReport {
            runner_id: &RunnerId::new(RUNNER_ID),
            attempt_id: &AttemptId::new(&run.attempt_id),
            fencing_token: run.fence,
            completion_id: &CompletionId::new(completion_id),
            final_event_checkpoint: None,
            terminal_state,
            terminal_reason: &terminal_reason_of(terminal_reason),
            actual_execution: &actual_execution(),
            usage: &usage_not_measured(),
        },
        read_pack,
    )
    .await
    .expect("complete")
}

async fn no_pack(_reference: String) -> Option<MergeReadinessPack> {
    None
}

#[tokio::test]
async fn an_attempt_ends_once_per_completion_id_and_other_content_conflicts() {
    let world = world().await;
    let run = running_attempt(&world, "complete-once").await;

    let Completed::Committed { response } = finish(
        &world,
        &run,
        "done-1",
        ExecutionState::Failed,
        json!({"summary": "tests failed"}),
        no_pack,
    )
    .await
    else {
        panic!("the first report ends the attempt");
    };
    assert_eq!(response.terminal_state, ExecutionState::Failed);
    assert_eq!(
        execution_attempts::attempt_state(&world.repo, &run.attempt_id)
            .await
            .unwrap(),
        Some(ExecutionState::Failed)
    );

    let Completed::Replayed(replay) = finish(
        &world,
        &run,
        "done-1",
        ExecutionState::Failed,
        json!({"summary": "tests failed"}),
        no_pack,
    )
    .await
    else {
        panic!("the same report again is a replay");
    };
    assert_eq!(replay.committed_at, response.committed_at);

    let changed = finish(
        &world,
        &run,
        "done-1",
        ExecutionState::Failed,
        json!({"summary": "CHANGED"}),
        no_pack,
    )
    .await;
    assert_eq!(changed, Completed::IdempotencyConflict);
    let other_id = finish(
        &world,
        &run,
        "done-2",
        ExecutionState::Cancelled,
        json!({}),
        no_pack,
    )
    .await;
    assert_eq!(other_id, Completed::Conflict);
}

#[tokio::test]
async fn a_fence_that_is_not_current_cannot_end_the_attempt() {
    let world = world().await;
    let mut run = running_attempt(&world, "complete-stale").await;
    run.fence = FencingToken::new(run.fence.get() + 1);

    let stale = finish(
        &world,
        &run,
        "done-1",
        ExecutionState::Succeeded,
        json!({}),
        no_pack,
    )
    .await;
    assert_eq!(stale, Completed::Stale);
    assert_eq!(
        execution_attempts::attempt_state(&world.repo, &run.attempt_id)
            .await
            .unwrap(),
        Some(ExecutionState::Running)
    );
}

async fn project_of(world: &common::World) -> String {
    world
        .repo
        .get_item(Uuid::parse_str(&world.item_id).unwrap())
        .await
        .unwrap()
        .expect("item")
        .project_id
        .to_string()
}

async fn item_status(world: &common::World) -> String {
    world
        .repo
        .get_item(Uuid::parse_str(&world.item_id).unwrap())
        .await
        .unwrap()
        .expect("item")
        .status
}

#[tokio::test]
async fn a_success_leaves_the_items_status_alone() {
    for artifacts in [json!([{"kind": "patch", "size_bytes": 12}]), json!([])] {
        let world = world().await;
        let run = running_attempt(&world, "complete-status").await;
        let before = item_status(&world).await;

        let Completed::Committed { .. } = finish(
            &world,
            &run,
            "done-1",
            ExecutionState::Succeeded,
            json!({"artifacts": artifacts}),
            no_pack,
        )
        .await
        else {
            panic!("the attempt ends");
        };
        assert_eq!(item_status(&world).await, before);
    }
}

async fn follow_ups(world: &common::World) -> Vec<tack_test_support::probe::ExecutionRequestRow> {
    let mut rows =
        tack_test_support::probe::execution_requests_of_item(&world.repo, &world.item_id)
            .await
            .unwrap();
    rows.retain(|row| row.idempotency_key.starts_with("rework-"));
    rows
}

/// Completes a succeeded run that uploaded `pack` under a project whose loop cap is `cap`.
async fn succeed_with(cap: i64, pack: MergeReadinessPack) -> (common::World, Running) {
    let world = world().await;
    let project_id = project_of(&world).await;
    seed::project_verify_loop_max_attempts(&world.repo, &project_id, cap).await;
    let run = running_attempt(&world, "rework").await;
    seed::execution_pack_artifact(
        &world.repo,
        &run.attempt_id,
        "mrp-1",
        "pack-reference",
        "2026-08-06T12:00:00Z",
    )
    .await;
    finish(
        &world,
        &run,
        "done-1",
        ExecutionState::Succeeded,
        json!({}),
        |reference: String| {
            let pack = pack.clone();
            async move {
                assert_eq!(reference, "pack-reference");
                Some(pack)
            }
        },
    )
    .await;
    (world, run)
}

#[tokio::test]
async fn a_rejected_change_goes_back_to_the_agent_as_one_follow_up_with_what_to_fix() {
    let (world, run) = succeed_with(2, pack(|_| {})).await;

    let made = follow_ups(&world).await;
    assert_eq!(made.len(), 1);
    assert_eq!(
        made[0].idempotency_key,
        format!("rework-{}", run.attempt_id)
    );
    let snapshot: Value = serde_json::from_str(&made[0].request_snapshot).unwrap();
    assert_eq!(snapshot["metadata"]["rework_of"], run.request_id);
    let instructions = snapshot["resolved_agent_profile"]["instructions"]
        .as_str()
        .unwrap();
    assert!(instructions.starts_with("work safely"), "{instructions}");
    assert!(instructions.contains("What the verifier asks you to fix"));
    assert!(instructions.contains("Build failed with compilation error"));
    assert!(instructions.contains("tests/unit.rs:42"));
    assert_eq!(
        execution_requests::request_state(&world.repo, &made[0].id)
            .await
            .unwrap(),
        Some(ExecutionState::Queued)
    );
}

#[tokio::test]
async fn nothing_goes_back_when_a_stop_condition_holds() {
    let cases = [
        ("the loop cap is zero", 0, pack(|_| {})),
        (
            "the verifier only asks for a review",
            2,
            pack(|p| p["recommendation"]["decision"] = json!("review")),
        ),
        (
            "nothing is named to fix",
            2,
            pack(|p| {
                p.as_object_mut().unwrap().remove("next_steps");
                p["criteria"] = json!([]);
            }),
        ),
    ];
    for (name, cap, pack) in cases {
        let (world, _) = succeed_with(cap, pack).await;
        assert!(follow_ups(&world).await.is_empty(), "{name}");
    }
}

#[tokio::test]
async fn a_failed_attempt_is_never_sent_back() {
    let world = world().await;
    let project_id = project_of(&world).await;
    seed::project_verify_loop_max_attempts(&world.repo, &project_id, 2).await;
    let run = running_attempt(&world, "failed-run").await;
    seed::execution_pack_artifact(
        &world.repo,
        &run.attempt_id,
        "mrp-1",
        "pack-reference",
        "2026-08-06T12:00:00Z",
    )
    .await;
    finish(
        &world,
        &run,
        "done-1",
        ExecutionState::Failed,
        json!({}),
        |_reference: String| async { Some(pack(|_| {})) },
    )
    .await;
    assert!(follow_ups(&world).await.is_empty());
}

/// Ends a run of `world`'s item that the Reviewer profile made from `reviewed` (a request id), and
/// returns what the store then says the Reviewer thought of `reviewed_attempt`.
async fn reviewer_says(
    world: &common::World,
    reviewed: &Running,
    kind: &str,
    from_run: Option<&str>,
    result: &str,
    state: ExecutionState,
) -> Option<String> {
    let metadata = from_run.map_or(json!({}), |run| json!({"from_run": run}));
    let key = format!("review-{kind}-{}", Uuid::new_v4());
    let request_id = enqueue_with(world, &key, json!({"kind": kind}), metadata).await;
    let run = running(world, &key, request_id).await;
    finish(
        world,
        &run,
        "done-1",
        state,
        json!({"result": result}),
        no_pack,
    )
    .await;
    tack_test_support::probe::reviewer_decision_of_attempt(&world.repo, &reviewed.attempt_id)
        .await
        .unwrap()
}

async fn reviewed_run(world: &common::World) -> Running {
    let reviewed = running_attempt(world, "reviewed").await;
    finish(
        world,
        &reviewed,
        "reviewed-done",
        ExecutionState::Succeeded,
        json!({}),
        no_pack,
    )
    .await;
    reviewed
}

#[tokio::test]
async fn a_reviewers_verdict_is_kept_with_the_run_it_was_started_from() {
    let world = world().await;
    let reviewed = reviewed_run(&world).await;
    let said = reviewer_says(
        &world,
        &reviewed,
        "reviewer",
        Some(&reviewed.request_id),
        "One finding.\n\nVerdict: reject",
        ExecutionState::Succeeded,
    )
    .await;
    assert_eq!(said.as_deref(), Some("reject"));

    // A second Reviewer run replaces the first one's word.
    let again = reviewer_says(
        &world,
        &reviewed,
        "reviewer",
        Some(&reviewed.request_id),
        "Fixed.\n\nVerdict: merge",
        ExecutionState::Succeeded,
    )
    .await;
    assert_eq!(again.as_deref(), Some("merge"));
}

#[tokio::test]
async fn nothing_is_kept_unless_a_reviewer_succeeded_with_a_verdict_on_a_run() {
    let world = world().await;
    let reviewed = reviewed_run(&world).await;
    let from = Some(reviewed.request_id.as_str());
    let cases = [
        // Another profile that happens to end with the same line.
        (
            "implementer",
            from,
            "Verdict: merge",
            ExecutionState::Succeeded,
        ),
        // A Reviewer started from no run has nothing it reviewed.
        (
            "reviewer",
            None,
            "Verdict: merge",
            ExecutionState::Succeeded,
        ),
        (
            "reviewer",
            from,
            "Looks fine to me.",
            ExecutionState::Succeeded,
        ),
        (
            "reviewer",
            from,
            "Verdict: approve",
            ExecutionState::Succeeded,
        ),
        ("reviewer", from, "Verdict: merge", ExecutionState::Failed),
    ];
    for (kind, from_run, result, state) in cases {
        let said = reviewer_says(&world, &reviewed, kind, from_run, result, state).await;
        assert_eq!(said, None, "{kind} {from_run:?} {result:?} {state:?}");
    }
}
