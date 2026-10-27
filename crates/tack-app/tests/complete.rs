//! The rule behind `POST /attempts/{attempt_id}/completion`: an attempt ends once per
//! `completion_id`, a success moves the item per the project's finish status, and a change the
//! verifier rejected goes back to the agent as one follow-up until a stop condition holds.

use crate::common;

use std::future::Future;

use common::{RUNNER_ID, Running, running_attempt, world};
use serde_json::{Value, json};
use tack_app::runs::complete::{Completed, CompletionReport, complete};
use tack_db::queries::{execution_attempts, execution_request_rows, execution_requests};
use tack_protocol::execution::ExecutionState;
use tack_protocol::mrp::MergeReadinessPack;
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
        &world.clock,
        CompletionReport {
            runner_id: RUNNER_ID,
            attempt_id: &run.attempt_id,
            fencing_token: run.fence,
            completion_id,
            final_event_checkpoint: None,
            terminal_state,
            terminal_reason_json: &terminal_reason.to_string(),
            actual_execution_json: "{}",
            usage_json: "{}",
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

    let Completed::Committed {
        response,
        item_moved,
    } = finish(
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
    assert_eq!(item_moved, None);
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
    run.fence += 1;

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
async fn a_success_moves_the_item_to_the_finish_status_only_when_the_attempt_left_a_patch() {
    for (artifacts, moved) in [
        (json!([{"kind": "patch", "size_bytes": 12}]), true),
        (json!([]), false),
    ] {
        let world = world().await;
        let project_id = project_of(&world).await;
        seed::project_on_finish_status(&world.repo, &project_id, "In Progress").await;
        let run = running_attempt(&world, "complete-finish").await;
        let before = item_status(&world).await;

        let Completed::Committed { item_moved, .. } = finish(
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
        let status = item_status(&world).await;
        if moved {
            let moved = item_moved.expect("the item moved");
            assert_eq!(moved.old_status, before);
            assert_eq!(moved.new_status, "In Progress");
            assert_eq!(status, "In Progress");
        } else {
            assert_eq!(item_moved, None);
            assert_eq!(status, before);
        }
    }
}

#[tokio::test]
async fn the_requests_status_map_policy_moves_the_item_to_the_first_done_status() {
    for (policy, expected_move) in [(Some("done_on_success"), true), (None, false)] {
        let world = world().await;
        let run = running_attempt(&world, "complete-policy").await;
        let before = item_status(&world).await;
        let mut snapshot: Value = serde_json::from_str(
            &execution_requests::request_snapshot(&world.repo, &run.request_id)
                .await
                .unwrap()
                .expect("snapshot"),
        )
        .unwrap();
        snapshot["status_map_policy_id"] = json!(policy);
        seed::execution_request_snapshot(&world.repo, &run.request_id, &snapshot.to_string()).await;

        let Completed::Committed { item_moved, .. } = finish(
            &world,
            &run,
            "done-1",
            ExecutionState::Succeeded,
            json!({}),
            no_pack,
        )
        .await
        else {
            panic!("the attempt ends");
        };
        if expected_move {
            let moved = item_moved.expect("the item moved");
            assert_eq!(moved.old_status, before);
            assert_eq!(moved.new_status, "Done");
            assert_eq!(item_status(&world).await, "Done");
        } else {
            assert_eq!(item_moved, None);
            assert_eq!(item_status(&world).await, before);
        }
    }
}

async fn follow_ups(world: &common::World) -> Vec<execution_request_rows::ExecutionRequestRow> {
    let mut rows = execution_request_rows::execution_requests_of_item(&world.repo, &world.item_id)
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
