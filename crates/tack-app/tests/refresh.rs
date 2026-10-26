//! The rule behind `POST /refresh`: capabilities are stored for an active runner, a rotation is a
//! compare-and-set on the credential the request authenticated with, and a revoked runner is
//! refused.

mod common;

use chrono::Duration;
use common::{CREDENTIAL_HASH, RUNNER_ID, World, world};
use tack_app::runs::refresh::{Refresh, Refreshed, refresh};
use tack_protocol::Clock;

async fn refresh_with(
    world: &World,
    presented_credential_hash: &str,
    new_credential_hash: Option<&str>,
) -> Refreshed {
    refresh(
        &world.repo,
        &world.clock,
        Refresh {
            runner_id: RUNNER_ID,
            presented_credential_hash,
            runner_version: "2.0.0",
            labels_json: r#"{"os":"linux"}"#,
            capability_snapshot_json: "{}",
            new_credential_hash,
        },
    )
    .await
    .expect("refresh")
}

#[tokio::test]
async fn a_rotation_issues_a_credential_that_lasts_ninety_days() {
    let world = world().await;

    let outcome = refresh_with(&world, CREDENTIAL_HASH, Some("hash-2")).await;

    let now = world.clock.now();
    assert_eq!(
        outcome,
        Refreshed::Accepted {
            credential_expires_at: now + Duration::days(90),
            accepted_at: now,
        }
    );
}

#[tokio::test]
async fn a_refresh_without_rotation_reports_the_expiry_already_in_force() {
    let world = world().await;
    refresh_with(&world, CREDENTIAL_HASH, Some("hash-2")).await;
    let rotated_at = world.clock.now();
    world.clock.0.advance(Duration::days(10));

    let outcome = refresh_with(&world, "hash-2", None).await;

    assert_eq!(
        outcome,
        Refreshed::Accepted {
            credential_expires_at: rotated_at + Duration::days(90),
            accepted_at: world.clock.now(),
        }
    );
}

#[tokio::test]
async fn a_second_rotation_from_the_old_credential_loses() {
    let world = world().await;
    refresh_with(&world, CREDENTIAL_HASH, Some("hash-2")).await;

    let outcome = refresh_with(&world, CREDENTIAL_HASH, Some("hash-3")).await;

    assert_eq!(outcome, Refreshed::CredentialChanged);
    // The loser wrote nothing: the winner's credential still rotates.
    assert!(matches!(
        refresh_with(&world, "hash-2", Some("hash-4")).await,
        Refreshed::Accepted { .. }
    ));
}

#[tokio::test]
async fn a_revoked_runner_is_refused() {
    let world = world().await;
    world
        .repo
        .revoke_runner(RUNNER_ID, &world.clock)
        .await
        .expect("revoke");

    let outcome = refresh_with(&world, CREDENTIAL_HASH, None).await;

    assert_eq!(outcome, Refreshed::Revoked);
}
