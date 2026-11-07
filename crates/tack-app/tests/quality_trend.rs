//! The daily trend pass: a cut whose success in the last seven days is clearly below the 28 days
//! before it opens one needs-you notification, once, until the cut recovers. Every count is worked
//! out by hand in the comments.

use std::sync::Arc;

use chrono::{DateTime, Duration, TimeZone, Utc};
use tack_app::notifications::Inbox;
use tack_app::quality_trend;
use tack_core::notification::{Kind, Level};
use tack_db::Repository;
use tack_protocol::Clock;
use tack_test_support::seed::statement as run;
use tack_test_support::{
    ControllableClock, FixedClock, create_test_workspace, make_item, make_project, setup_test_db,
};

struct FakeClock(ControllableClock);

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.0.now()
    }
}

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 7, 12, 0, 0).unwrap()
}

struct World {
    repo: Repository,
    item: String,
    clock: FakeClock,
    inbox: Inbox,
    next: u32,
}

impl World {
    async fn new() -> World {
        let repo = setup_test_db().await;
        let workspace = create_test_workspace(&repo).await;
        let project = make_project(&repo, workspace).await;
        let item = make_item(&repo, &project).await.id.to_string();
        run(
            &repo,
            "INSERT INTO agent_runners (id, name, credential_hash, state, labels, total_capacity, \
             available_capacity, capability_snapshot, protocol_version, created_at, updated_at) \
             VALUES ('run', 'p', 'h', 'active', '{}', 1, 1, '{}', 1, '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')",
            &[],
        )
        .await;
        let inbox = Inbox::new(repo.clone(), Arc::new(FixedClock(now())), Vec::new());
        World {
            repo,
            item,
            clock: FakeClock(ControllableClock::new(now())),
            inbox,
            next: 0,
        }
    }

    /// `ended` attempts of `profile` on `harness`, `succeeded` of them succeeded, `days` days ago.
    async fn attempts(
        &mut self,
        profile: &str,
        harness: &str,
        succeeded: u32,
        ended: u32,
        days: i64,
    ) {
        let at = (now() - Duration::days(days)).to_rfc3339();
        for n in 0..ended {
            self.next += 1;
            let id = format!("a{}", self.next);
            let state = if n < succeeded { "succeeded" } else { "failed" };
            run(
                &self.repo,
                "INSERT INTO execution_requests (id, item_id, idempotency_scope, idempotency_key, \
                 request_fingerprint, selector_kind, selector_id, agent_profile_snapshot, \
                 repository_snapshot, permission_policy, metadata, request_snapshot, \
                 requested_harness_kind, created_at, updated_at) \
                 VALUES (?1, ?2, 's', ?1, 'f', 'exact_runner', 'run', '{}', '{}', '{}', '{}', \
                 json_object('resolved_agent_profile', json_object('name', ?3)), ?4, ?5, ?5)",
                &[&id, &self.item, profile, harness, &at],
            )
            .await;
            run(
                &self.repo,
                "INSERT INTO execution_attempts (id, request_id, attempt_number, runner_id, \
                 fencing_token, state, lease_issued_at, lease_expires_at, created_at, updated_at) \
                 VALUES (?1, ?1, 1, 'run', 1, ?2, ?3, ?3, ?3, ?3)",
                &[&id, state, &at],
            )
            .await;
        }
    }

    async fn pass(&self) -> usize {
        quality_trend::tick(&self.repo, &self.clock, &self.inbox)
            .await
            .expect("tick")
    }

    async fn open(&self) -> Vec<tack_core::notification::Notification> {
        self.inbox.open_notifications(10).await.unwrap()
    }
}

/// Coder worked 63 of 70 in the 28 days before the week and 2 of 20 in it; Other, on the same
/// agent, worked every time in the week, so the agent as a whole did not fall clearly (62 of 80).
async fn coder_dropped(world: &mut World) {
    world.attempts("Coder", "claude-code", 63, 70, 15).await;
    world.attempts("Coder", "claude-code", 2, 20, 3).await;
    world.attempts("Other", "claude-code", 60, 60, 2).await;
}

#[tokio::test]
async fn a_synthetic_drop_opens_one_notification_and_the_same_drop_the_next_day_none() {
    let mut world = World::new().await;
    coder_dropped(&mut world).await;

    assert_eq!(world.pass().await, 1);
    let open = world.open().await;
    assert_eq!(open.len(), 1);
    assert_eq!(
        (open[0].kind, open[0].level),
        (Kind::QualityTrend, Level::NeedsYou)
    );
    assert_eq!(
        open[0].title,
        "Success for profile \"Coder\" fell to 10% (2 of 20) in the last 7 days, from 90% (63 of 70) in the 28 before"
    );

    // The same pass again, and the next day's with the drop still there, tell nothing more.
    assert_eq!(world.pass().await, 0);
    world.clock.0.advance(Duration::days(1));
    assert_eq!(world.pass().await, 0);
    assert_eq!(world.open().await.len(), 1);
}

#[tokio::test]
async fn trend_no_drop_opens_nothing() {
    let mut world = World::new().await;
    // 63 of 70 before, 18 of 20 in the week: about the same.
    world.attempts("Coder", "claude-code", 63, 70, 15).await;
    world.attempts("Coder", "claude-code", 18, 20, 3).await;
    // A lower week whose interval still reaches the old one, and a week too short to tell.
    world.attempts("Slow", "codex", 18, 20, 15).await;
    world.attempts("Slow", "codex", 6, 10, 3).await;
    world.attempts("New", "opencode", 9, 10, 15).await;
    world.attempts("New", "opencode", 2, 4, 3).await;

    assert_eq!(world.pass().await, 0);
    assert!(world.open().await.is_empty());
}

#[tokio::test]
async fn trend_a_recovered_cut_is_closed_and_a_later_drop_is_announced_again() {
    let mut world = World::new().await;
    coder_dropped(&mut world).await;
    assert_eq!(world.pass().await, 1);

    // Ten days on, the failures are before the week and the week brings 19 of 20: recovered.
    world.clock.0.advance(Duration::days(10));
    world.attempts("Coder", "claude-code", 19, 20, -5).await;
    assert_eq!(world.pass().await, 0);
    assert!(world.open().await.is_empty());

    // Another week on, the week fails again: announced again.
    world.clock.0.advance(Duration::days(8));
    world.attempts("Coder", "claude-code", 1, 20, -17).await;
    world.attempts("Other", "claude-code", 60, 60, -17).await;
    assert_eq!(world.pass().await, 1);
    assert_eq!(world.open().await.len(), 1);
}
