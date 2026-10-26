//! Production-runtime proof for the retention sweep and the health watch, over a real SQLite
//! database with an injected clock: the replay rows come from the real claim path, the stale
//! lease from a real lease that ran out, and the retention loop is the real spawned task.

use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::{Duration, TimeZone, Utc};
use tack_app::{observability, retention};
use tack_db::queries;
use tack_protocol::Clock;
use tack_protocol::execution::ExecutionState;
use tack_test_support::{poll_until, seed};

use crate::common::{FakeClock, World, enqueue, running_attempt, world};

/// Runs `step` with the world's clock set to `at`, then puts the clock back.
async fn at<T>(
    world: &World,
    at: chrono::DateTime<Utc>,
    step: impl std::future::Future<Output = T>,
) -> T {
    let back = world.clock.now();
    world.clock.0.set(at);
    let out = step.await;
    world.clock.0.set(back);
    out
}

fn now_fixed() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 6, 12, 0, 0).unwrap()
}

/// The headline claim: stale raw rows purge in production runtime. The real task
/// (`retention::run`) runs over a real repository with an injected clock, so staleness is
/// deterministic: a claim remembered 100 days ago is purged by the first pass; one remembered
/// today stays.
#[tokio::test]
async fn the_spawned_retention_loop_purges_stale_rows_and_keeps_fresh_ones() {
    let world = world().await;
    let now = now_fixed();
    at(
        &world,
        now - Duration::days(100),
        running_attempt(&world, "old"),
    )
    .await;
    running_attempt(&world, "fresh").await;
    assert_eq!(
        queries::execution_claim_replays::replay_count(&world.repo)
            .await
            .unwrap(),
        2,
        "both claims are remembered before the sweep runs"
    );

    let clock: Arc<dyn Clock> = Arc::new(FakeClock(world.clock.0.clone()));
    let task = tokio::spawn(retention::run(
        world.repo.clone(),
        clock,
        retention::Settings::default(),
        StdDuration::from_secs(3600),
    ));

    let purged = poll_until(StdDuration::from_secs(3), async || {
        (queries::execution_claim_replays::replay_count(&world.repo)
            .await
            .unwrap()
            == 1)
            .then_some(())
    })
    .await;
    task.abort();
    assert!(
        purged.is_some(),
        "the real spawned task purged the stale row within 3s and kept the fresh one"
    );
}

/// A pass purges what is older than the window and reports how much; a later pass, a window
/// later, purges the rest.
#[tokio::test]
async fn a_retention_pass_purges_what_is_older_than_the_window() {
    let world = world().await;
    let now = now_fixed();
    at(
        &world,
        now - Duration::days(100),
        running_attempt(&world, "old"),
    )
    .await;
    running_attempt(&world, "fresh").await;
    let settings = retention::Settings::default();

    let first = retention::tick(&world.repo, &world.clock, &settings)
        .await
        .unwrap();
    assert_eq!(first.replays.rows_purged, 1, "only the 100-day-old claim");

    world.clock.0.advance(Duration::days(100));
    let second = retention::tick(&world.repo, &world.clock, &settings)
        .await
        .unwrap();
    assert_eq!(
        second.replays.rows_purged, 1,
        "the fresh one, a window later"
    );
}

/// Proves a stale lease and a `needs_operator` request are observable: a genuinely expired,
/// non-terminal lease and a genuinely `needs_operator` request, read through the real snapshot.
#[tokio::test]
async fn a_health_pass_reports_a_stale_lease_and_a_needs_operator_request() {
    let world = world().await;
    let now = now_fixed();
    at(
        &world,
        now - Duration::hours(1),
        running_attempt(&world, "stale"),
    )
    .await;
    let stuck = enqueue(&world, "stuck").await;
    seed::execution_request_state(&world.repo, &stuck, ExecutionState::NeedsOperator).await;

    let outcome = observability::tick(
        &world.repo,
        &world.clock,
        &observability::Settings::default(),
    )
    .await
    .unwrap();

    assert_eq!(
        outcome.snapshot.stale_lease_count, 1,
        "must observe the expired, non-terminal lease"
    );
    assert_eq!(
        outcome.snapshot.needs_operator_count, 1,
        "must observe the needs_operator request"
    );
    assert!(outcome.alerts.stale_lease_alert && outcome.alerts.needs_operator_alert);
}
