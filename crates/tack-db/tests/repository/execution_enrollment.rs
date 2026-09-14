//! Enrollment tokens and credential rotation for `agent_runners`.

use crate::common::execution_fixture::{Fixture, count_where};

use chrono::Duration;
use tack_db::repo::execution::{
    CredentialRotationResult, EnrollmentToken, NewRunner, RedeemEnrollmentResult,
};

#[tokio::test]
async fn enrollment_token_is_single_use_and_revocation_fails_closed() {
    let fx = Fixture::new().await;
    sqlx::query("UPDATE agent_runners SET state = 'pending_enrollment' WHERE id = 'runner-a'")
        .execute(fx.repo.pool())
        .await
        .unwrap();
    fx.issue_token("token-1", "token-hash").await;
    let expires = fx.clock.now() + Duration::days(1);
    assert_eq!(
        fx.redeem("token-hash", "credential-hash", expires).await,
        RedeemEnrollmentResult::Redeemed("runner-a".into())
    );
    assert_eq!(
        fx.redeem("token-hash", "other", expires).await,
        RedeemEnrollmentResult::InvalidOrExpired
    );
    fx.issue_token("token-2", "revoked").await;
    assert!(
        fx.repo
            .revoke_enrollment_token("revoked", &fx.clock)
            .await
            .unwrap()
    );
    assert_eq!(
        fx.redeem("revoked", "other", expires).await,
        RedeemEnrollmentResult::InvalidOrExpired
    );
}

#[tokio::test]
async fn concurrent_enrollment_redemption_has_one_winner() {
    let fx = Fixture::new().await;
    fx.repo
        .create_pending_runner_and_issue_token(
            NewRunner {
                id: "runner-concurrent-enrollment",
                name: "Pending runner",
                credential_hash: "ignored-for-pending",
                labels: "{}",
                total_capacity: 1,
                available_capacity: 1,
                capability_snapshot: "{}",
                protocol_version: 1,
            },
            EnrollmentToken {
                id: "token-concurrent-enrollment",
                runner_id: "runner-concurrent-enrollment",
                token_hash: "hash-concurrent-enrollment",
                expires_at: fx.clock.now() + Duration::minutes(5),
            },
            &fx.clock,
        )
        .await
        .unwrap();
    let expires = fx.clock.now() + Duration::hours(1);
    let redeem = || fx.redeem("hash-concurrent-enrollment", "credential-hash", expires);
    let (left, right) = tokio::join!(redeem(), redeem());
    let results = [left, right];
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, RedeemEnrollmentResult::Redeemed(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, RedeemEnrollmentResult::InvalidOrExpired))
            .count(),
        1
    );
}

// Defect 3 regression: two concurrent or retried credential rotations from
// the same runner both authenticate against the same still-valid old hash
// (that is the whole point of a rotation race — neither has learned the
// other's new hash yet). Without a compare-and-set against the hash that was
// actually authenticated, last-writer-wins would silently discard one
// rotation's result, leaving its caller holding a credential the server no
// longer accepts and with no way to recover short of a fresh operator-issued
// enrollment token. `rotate_runner_credential` must let exactly one of two
// concurrent rotations against the same expected hash win.
#[tokio::test]
async fn concurrent_credential_rotations_have_exactly_one_winner() {
    let fx = Fixture::new().await;
    // `Fixture::new` registers "runner-a" with credential_hash "hash-only".
    let expires = fx.clock.now() + Duration::days(30);
    let (a, b) = tokio::join!(
        fx.rotate_credential("hash-only", "hash-rotated-by-left", expires),
        fx.rotate_credential("hash-only", "hash-rotated-by-right", expires),
    );
    let a = a.expect("first rotation must succeed at the sqlx level");
    let b = b.expect("second rotation must succeed at the sqlx level");

    let rotated = count_where([&a, &b], |r| {
        matches!(r, CredentialRotationResult::Rotated(_))
    });
    let mismatched = count_where([&a, &b], |r| {
        matches!(r, CredentialRotationResult::HashMismatch)
    });
    assert_eq!(
        rotated, 1,
        "exactly one concurrent rotation against the same expected hash wins: {a:?} / {b:?}"
    );
    assert_eq!(
        mismatched, 1,
        "the other concurrent rotation observes its expected hash no longer matches: {a:?} / {b:?}"
    );

    let stored_hash: String =
        sqlx::query_scalar("SELECT credential_hash FROM agent_runners WHERE id='runner-a'")
            .fetch_one(fx.repo.pool())
            .await
            .unwrap();
    // Whichever branch won, the stored hash must be exactly that branch's new
    // hash — never the loser's, and never some third, corrupted value.
    let expected = if matches!(a, CredentialRotationResult::Rotated(_)) {
        "hash-rotated-by-left"
    } else {
        "hash-rotated-by-right"
    };
    assert_eq!(
        stored_hash, expected,
        "stored hash must match whichever branch's CredentialRotationResult::Rotated fired"
    );

    // A retry against the now-stale original hash (e.g. a naive client that
    // didn't observe either response and blindly retries with what it still
    // believes is current) must not be able to rotate again.
    let stale_retry = fx
        .rotate_credential("hash-only", "hash-rotated-by-stale-retry", expires)
        .await
        .unwrap();
    assert_eq!(stale_retry, CredentialRotationResult::HashMismatch);
    let stored_hash_after_retry: String =
        sqlx::query_scalar("SELECT credential_hash FROM agent_runners WHERE id='runner-a'")
            .fetch_one(fx.repo.pool())
            .await
            .unwrap();
    assert_eq!(
        stored_hash_after_retry, expected,
        "a stale-hash retry must not overwrite the winning rotation"
    );
}
