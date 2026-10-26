//! The rule behind `POST /enroll`: a token is single-use, and the runner it activates holds the
//! credential for ninety days.

use crate::common;

use chrono::Duration;
use common::{FakeClock, World, world};
use tack_app::runs::enroll::{Enrolled, Enrollment, enroll};
use tack_db::queries::agent_runners;
use tack_db::repo::execution::{EnrollmentToken, NewRunner};
use tack_protocol::Clock;

const PENDING_RUNNER_ID: &str = "runner-pending";
const TOKEN_HASH: &str = "hash-of-the-enrollment-token";

async fn world_with_a_pending_runner() -> World {
    let world = world().await;
    world
        .repo
        .create_pending_runner_and_issue_token(
            NewRunner {
                id: PENDING_RUNNER_ID,
                name: "Pending runner",
                credential_hash: "pending",
                labels: "{}",
                total_capacity: 1,
                available_capacity: 1,
                capability_snapshot: "{}",
                protocol_version: 1,
            },
            EnrollmentToken {
                id: "token-1",
                runner_id: PENDING_RUNNER_ID,
                token_hash: TOKEN_HASH,
                expires_at: world.clock.now() + Duration::minutes(5),
            },
            &world.clock,
        )
        .await
        .expect("pending runner");
    world
}

async fn enroll_with(world: &World, token_hash: &str) -> Enrolled {
    enroll_on(&world.repo, &world.clock, token_hash).await
}

async fn enroll_on(repo: &tack_db::Repository, clock: &FakeClock, token_hash: &str) -> Enrolled {
    enroll(
        repo,
        clock,
        Enrollment {
            token_hash,
            credential_hash: "hash-of-the-credential",
            runner_version: "1.0.0",
            runner_name: "self-reported",
            labels_json: "{}",
            total_capacity: 2,
            available_capacity: 2,
            capability_snapshot_json: "{}",
        },
    )
    .await
    .expect("enroll")
}

#[tokio::test]
async fn a_good_token_activates_its_runner_for_ninety_days() {
    let world = world_with_a_pending_runner().await;

    let outcome = enroll_with(&world, TOKEN_HASH).await;

    let now = world.clock.now();
    assert_eq!(
        outcome,
        Enrolled::Runner {
            runner_id: PENDING_RUNNER_ID.into(),
            credential_expires_at: now + Duration::days(90),
            server_time: now,
        }
    );
    let lifecycle = agent_runners::runner_lifecycle(&world.repo, PENDING_RUNNER_ID)
        .await
        .unwrap()
        .expect("runner exists");
    assert_eq!(lifecycle.state, "active");
}

#[tokio::test]
async fn a_token_works_once() {
    let world = world_with_a_pending_runner().await;
    enroll_with(&world, TOKEN_HASH).await;

    let again = enroll_with(&world, TOKEN_HASH).await;

    assert_eq!(again, Enrolled::InvalidToken);
}

#[tokio::test]
async fn an_unknown_or_expired_token_is_invalid() {
    let world = world_with_a_pending_runner().await;

    let unknown = enroll_with(&world, "no-such-token").await;
    world.clock.0.advance(Duration::minutes(6));
    let expired = enroll_with(&world, TOKEN_HASH).await;

    assert_eq!(unknown, Enrolled::InvalidToken);
    assert_eq!(expired, Enrolled::InvalidToken);
}
