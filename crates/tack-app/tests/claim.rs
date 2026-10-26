//! The rule behind `POST /claim`: the scheduler picks, the claim leases once per
//! `claim_request_id`, and a runner with nothing eligible gets no lease.

mod common;

use chrono::Duration;
use common::{RUNNER_ID, enqueue, world};
use tack_app::runs::claim::claim;
use tack_db::queries::{agent_runners, execution_requests};
use tack_protocol::Clock;
use tack_protocol::execution::ExecutionState;
use tack_test_support::seed;

const LEASE: Duration = Duration::seconds(60);

#[tokio::test]
async fn no_queued_request_means_no_lease() {
    let world = world().await;

    let claimed = claim(&world.repo, &world.clock, RUNNER_ID, "claim-empty", LEASE)
        .await
        .expect("claim");

    assert!(claimed.is_none());
}

#[tokio::test]
async fn a_queued_request_is_leased_to_the_runner_and_uses_one_slot() {
    let world = world().await;
    let request_id = enqueue(&world, "one-key").await;

    let claimed = claim(&world.repo, &world.clock, RUNNER_ID, "claim-one", LEASE)
        .await
        .expect("claim")
        .expect("work");

    assert_eq!(claimed.lease.request_id, request_id);
    assert_eq!(claimed.lease.runner_id, RUNNER_ID);
    assert_eq!(
        execution_requests::request_state(&world.repo, &request_id)
            .await
            .unwrap(),
        Some(ExecutionState::Leased)
    );
    assert_eq!(
        agent_runners::available_capacity(&world.repo, RUNNER_ID)
            .await
            .unwrap(),
        Some(1)
    );
}

#[tokio::test]
async fn the_same_claim_request_id_returns_the_same_lease_and_takes_no_second_slot() {
    let world = world().await;
    enqueue(&world, "replay-key").await;
    enqueue(&world, "other-key").await;

    let first = claim(&world.repo, &world.clock, RUNNER_ID, "claim-replay", LEASE)
        .await
        .expect("claim")
        .expect("work");
    let replay = claim(&world.repo, &world.clock, RUNNER_ID, "claim-replay", LEASE)
        .await
        .expect("claim")
        .expect("work");

    assert_eq!(replay.lease.attempt_id, first.lease.attempt_id);
    assert_eq!(replay.lease.fencing_token, first.lease.fencing_token);
    assert_eq!(
        agent_runners::available_capacity(&world.repo, RUNNER_ID)
            .await
            .unwrap(),
        Some(1)
    );
}

#[tokio::test]
async fn a_runner_with_no_free_capacity_gets_no_lease() {
    let world = world().await;
    enqueue(&world, "a").await;
    enqueue(&world, "b").await;
    enqueue(&world, "c").await;
    for id in ["claim-a", "claim-b"] {
        claim(&world.repo, &world.clock, RUNNER_ID, id, LEASE)
            .await
            .expect("claim")
            .expect("work");
    }

    let third = claim(&world.repo, &world.clock, RUNNER_ID, "claim-c", LEASE)
        .await
        .expect("claim");

    assert!(third.is_none());
}

/// The runner reports codex, and docket with one attended team and one lights-out team.
fn docket_runner(at: chrono::DateTime<chrono::Utc>) -> String {
    let mut snapshot = common::capabilities(at, 2, 2);
    snapshot["harnesses"] = serde_json::json!([{
        "harness_kind": "codex", "installed_version": "1.0.0", "probe_error": null,
        "probed_at": "2026-08-06T11:59:55Z", "requires_model": true,
    }, {
        "harness_kind": "docket", "installed_version": "0.2.0", "probe_error": null,
        "probed_at": "2026-08-06T11:59:55Z", "autonomy_ceiling": "L3",
        "teams": [
            {"name": "reviewers", "profile": "default", "violations": []},
            {"name": "writers", "profile": "lights-out", "violations": []}],
    }]);
    snapshot.to_string()
}

/// A docket task is claimed with the project's level lowered to its team's ceiling, or to the
/// cap the request carries; the key is absent when the harness is not docket, or when the team is
/// one the computer does not report and the request set no cap.
#[tokio::test]
async fn a_docket_claim_carries_the_project_level_lowered_to_the_team_and_the_request() {
    // (harness, project level, team, request's own cap, the level sent)
    let cases = [
        ("docket", "L4", Some("reviewers"), None, Some("L3")),
        ("docket", "L4", Some("writers"), None, Some("L4")),
        ("docket", "L5", Some("writers"), None, Some("L5")),
        ("docket", "L3", Some("writers"), None, Some("L3")),
        ("docket", "L5", Some("writers"), Some("L4"), Some("L4")),
        ("docket", "L5", Some("writers"), Some("L3"), Some("L3")),
        ("docket", "L4", Some("fab-dev"), None, None),
        ("docket", "L4", Some("fab-dev"), Some("L3"), Some("L3")),
        ("codex", "L5", None, Some("L3"), None),
    ];
    for (harness, level, team, cap, expected) in cases {
        let world = world().await;
        let request_id = enqueue(&world, "level-key").await;
        let repo = &world.repo;
        let item = repo
            .get_item(world.item_id.parse().unwrap())
            .await
            .unwrap()
            .unwrap();
        seed::project_autonomy_level(repo, &item.project_id.to_string(), level).await;
        seed::agent_runner_capabilities(repo, RUNNER_ID, &docket_runner(world.clock.now())).await;
        seed::execution_request_harness(repo, &request_id, harness).await;
        if let Some(team) = team {
            seed::execution_request_team(repo, &request_id, team).await;
        }
        if let Some(cap) = cap {
            seed::execution_request_autonomy(repo, &request_id, cap).await;
        }

        let claimed = claim(repo, &world.clock, RUNNER_ID, "claim-level", LEASE)
            .await
            .expect("claim")
            .expect("work");
        let sent = claimed.request_snapshot["autonomy"].as_str();
        assert_eq!(sent, expected, "{harness} {level} {team:?} {cap:?}");
        // The stored request keeps only what was asked for; a replay says the same.
        let replay = claim(repo, &world.clock, RUNNER_ID, "claim-level", LEASE)
            .await
            .expect("claim")
            .expect("work");
        assert_eq!(replay.request_snapshot["autonomy"].as_str(), expected);
    }
}
