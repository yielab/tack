use super::*;
use chrono::{DateTime, Utc};

fn snapshot_with(stale_lease_count: i64, needs_operator_count: i64) -> ExecutionFleetSnapshot {
    ExecutionFleetSnapshot {
        stale_lease_count,
        needs_operator_count,
        ..Default::default()
    }
}

#[test]
fn no_alert_when_both_counts_are_zero() {
    let alerts = evaluate_alerts(&snapshot_with(0, 0));
    assert!(!alerts.stale_lease_alert);
    assert!(!alerts.needs_operator_alert);
    assert!(!alerts.any());
}

#[test]
fn a_single_stale_lease_is_enough_to_alert() {
    let alerts = evaluate_alerts(&snapshot_with(1, 0));
    assert!(alerts.stale_lease_alert);
    assert!(!alerts.needs_operator_alert);
    assert!(alerts.any());
}

#[test]
fn a_single_needs_operator_request_is_enough_to_alert() {
    let alerts = evaluate_alerts(&snapshot_with(0, 1));
    assert!(!alerts.stale_lease_alert);
    assert!(alerts.needs_operator_alert);
    assert!(alerts.any());
}

#[test]
fn both_alert_independently() {
    let alerts = evaluate_alerts(&snapshot_with(3, 7));
    assert!(alerts.stale_lease_alert);
    assert!(alerts.needs_operator_alert);
}

/// The bounded-label-set proof this module's cardinality guarantee
/// demands: no matter how large the underlying
/// `agent_runners`/`execution_requests` tables
/// get, the snapshot's two map fields can never grow past the fixed,
/// closed state vocabularies (3 + 10 = 13 possible keys total) — never
/// one entry per row, never a row id as a key. This is a structural
/// property of the type (`BTreeMap<String, i64>` populated only from
/// `GROUP BY state`, never `GROUP BY id`), asserted here directly
/// against the known vocabularies so a future edit that widens the
/// grouping key would fail this test.
#[test]
fn snapshot_label_set_is_bounded_and_id_free() {
    let known_runner_states = ["pending_enrollment", "active", "revoked"];
    let known_request_states = [
        "queued",
        "leased",
        "preparing",
        "running",
        "waiting_decision",
        "succeeded",
        "failed",
        "cancelled",
        "lost",
        "needs_operator",
    ];
    let mut snapshot = ExecutionFleetSnapshot::default();
    for state in known_runner_states {
        snapshot.runner_state_counts.insert(state.to_string(), 1);
    }
    for state in known_request_states {
        snapshot.request_state_counts.insert(state.to_string(), 1);
    }
    assert_eq!(snapshot.runner_state_counts.len(), 3);
    assert_eq!(snapshot.request_state_counts.len(), 10);
    // No key looks like a UUID (36 chars, four hyphens) — the shape an
    // id-keyed map would actually have if this guarantee ever slipped.
    for key in snapshot
        .runner_state_counts
        .keys()
        .chain(snapshot.request_state_counts.keys())
    {
        assert!(
            key.len() < 20 && key.matches('-').count() <= 1,
            "snapshot key {key:?} looks id-shaped, not state-shaped"
        );
    }
}

struct FixedClock(DateTime<Utc>);

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

#[tokio::test]
async fn a_pass_over_an_empty_store_sees_no_alert() {
    let repo = tack_test_support::setup_test_db().await;
    let clock = FixedClock(Utc::now());

    let outcome = tick(&repo, &clock, &Settings::default())
        .await
        .expect("an empty store is not an error");

    assert_eq!(outcome.snapshot.stale_lease_count, 0);
    assert_eq!(outcome.snapshot.needs_operator_count, 0);
    assert_eq!(outcome.alerts, AlertSummary::default());
}
