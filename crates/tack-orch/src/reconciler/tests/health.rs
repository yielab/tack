//! `HealthTracker`'s pure state machine (failure/recovery thresholds,
//! backoff, jitter, warn-log suppression) and `evaluate`'s apiVersion
//! mismatch policy — neither needs a `ControlPlane` or store fake.

use super::super::*;

// -- Health state machine (pure; no ControlPlane needed) --------------

#[test]
fn health_state_transitions_at_3_and_10_failures() {
    let mut t = HealthTracker::new();
    let now = Utc::now();

    // 1 and 2 failures: still healthy.
    assert_eq!(t.observe(false, false, now).state, HealthState::Healthy);
    assert_eq!(t.observe(false, false, now).state, HealthState::Healthy);
    // 3rd failure: degraded.
    let tr = t.observe(false, false, now);
    assert_eq!(tr.state, HealthState::Degraded);
    assert_eq!(tr.consecutive_failures, 3);

    // 4..=9 stay degraded.
    for _ in 4..=9 {
        assert_eq!(t.observe(false, false, now).state, HealthState::Degraded);
    }
    // 10th failure: unreachable.
    let tr = t.observe(false, false, now);
    assert_eq!(tr.state, HealthState::Unreachable);
    assert_eq!(tr.consecutive_failures, 10);
}

#[test]
fn health_recovers_immediately_on_a_single_success() {
    let mut t = HealthTracker::new();
    let now = Utc::now();
    for _ in 0..15 {
        t.observe(false, false, now);
    }
    assert_eq!(t.state, HealthState::Unreachable);

    let tr = t.observe(true, false, now);
    assert_eq!(tr.state, HealthState::Healthy);
    assert_eq!(tr.consecutive_failures, 0);
    assert_eq!(tr.last_seen_at, Some(now));
}

#[test]
fn last_seen_at_is_none_on_a_failed_poll_untouched() {
    let mut t = HealthTracker::new();
    let tr = t.observe(false, false, Utc::now());
    assert_eq!(tr.last_seen_at, None);
}

#[test]
fn warn_logging_is_suppressed_during_a_sustained_outage() {
    // This is the exact suppression logic spawn_one's loop uses to
    // decide whether to `tracing::warn!` — a real sustained outage
    // (docket down for an hour, say) must not spam the log at every
    // tick. Across a long failure streak, `warn` should only fire on
    // the two severity transitions (entering degraded, entering
    // unreachable), never on every one of the (here) 30 failed polls.
    let mut t = HealthTracker::new();
    let now = Utc::now();
    let mut warns = 0;
    for _ in 0..30 {
        if t.observe(false, false, now).log == Some(LogSeverity::Warn) {
            warns += 1;
        }
    }
    assert_eq!(
        warns, 2,
        "expected exactly 2 warns (healthy->degraded, degraded->unreachable) across a 30-failure streak"
    );

    // Recovery logs once at `info`, not `warn`.
    let tr = t.observe(true, false, now);
    assert_eq!(tr.log, Some(LogSeverity::Info));
}

#[test]
fn backoff_is_capped_at_five_minutes() {
    assert_eq!(backoff_secs(100, 10), MAX_BACKOFF_SECS);
    assert_eq!(backoff_secs(1_000_000, 10), MAX_BACKOFF_SECS);
}

#[test]
fn backoff_grows_with_failures_and_resets_when_healthy() {
    let a = backoff_secs(1, 10);
    let b = backoff_secs(2, 10);
    let c = backoff_secs(3, 10);
    assert!(
        a < b && b < c,
        "backoff should strictly increase: {a} < {b} < {c}"
    );
    assert_eq!(backoff_secs(0, 10), 10, "no backoff while healthy");
}

#[test]
fn jitter_stays_within_20_percent_of_base() {
    let id = Uuid::new_v4();
    for tick in 0..200 {
        let j = jittered_secs(&id, tick, 100);
        assert!(
            (80..=120).contains(&j),
            "tick {tick} produced {j}, expected within [80, 120] for base 100"
        );
    }
}

#[test]
fn jitter_varies_so_planes_do_not_stampede_in_lockstep() {
    // Different plane ids polled on the same tick should not all land
    // on the exact same jittered interval.
    let base = 100;
    let values: std::collections::HashSet<u64> = (0..20)
        .map(|_| jittered_secs(&Uuid::new_v4(), 1, base))
        .collect();
    assert!(
        values.len() > 1,
        "expected jitter to differ across plane ids on the same tick, got a single value {values:?}"
    );
}

#[test]
fn jitter_never_produces_a_zero_or_negative_sleep() {
    let id = Uuid::new_v4();
    for tick in 0..50 {
        assert!(jittered_secs(&id, tick, 1) >= 1);
    }
}

// -- apiVersion policy --------------------------------------------------

pub(super) fn sample_status(api_version: &str) -> FleetStatus {
    FleetStatus {
        api_version: api_version.to_string(),
        timestamp: "2026-08-04T00:00:00Z".to_string(),
        gateway: "active".to_string(),
        channels: vec![],
        agents: vec![],
        total_cost_usd_estimated: 0.0,
    }
}

#[test]
fn evaluate_is_reachable_and_matched_on_success() {
    let outcome = FetchOutcome {
        health: Ok(Health {
            status: "ok".into(),
            gateway: 1,
        }),
        status: Ok(sample_status(EXPECTED_API_VERSION)),
        runs: Vec::new(),
        approvals: Ok(Vec::new()),
        metrics: Ok(Vec::new()),
        traces: Vec::new(),
    };
    let eval = evaluate(&outcome);
    assert!(eval.reachable);
    assert!(!eval.version_mismatch);
    assert_eq!(
        eval.observed_api_version.as_deref(),
        Some(EXPECTED_API_VERSION)
    );
}

#[test]
fn evaluate_is_unreachable_when_health_call_fails() {
    let outcome = FetchOutcome {
        health: Err(OrchError::Unavailable("connection refused".into())),
        status: Ok(sample_status(EXPECTED_API_VERSION)),
        runs: Vec::new(),
        approvals: Ok(Vec::new()),
        metrics: Ok(Vec::new()),
        traces: Vec::new(),
    };
    assert!(!evaluate(&outcome).reachable);
}

#[test]
fn evaluate_is_unreachable_when_status_call_fails() {
    let outcome = FetchOutcome {
        health: Ok(Health {
            status: "ok".into(),
            gateway: 1,
        }),
        status: Err(OrchError::Decode("malformed json".into())),
        runs: Vec::new(),
        approvals: Ok(Vec::new()),
        metrics: Ok(Vec::new()),
        traces: Vec::new(),
    };
    let eval = evaluate(&outcome);
    assert!(!eval.reachable);
    assert!(
        !eval.version_mismatch,
        "an unparseable status has no apiVersion to compare"
    );
}

#[test]
fn evaluate_flags_major_version_mismatch_but_stays_reachable() {
    let outcome = FetchOutcome {
        health: Ok(Health {
            status: "ok".into(),
            gateway: 1,
        }),
        status: Ok(sample_status("3")),
        runs: Vec::new(),
        approvals: Ok(Vec::new()),
        metrics: Ok(Vec::new()),
        traces: Vec::new(),
    };
    let eval = evaluate(&outcome);
    assert!(eval.reachable, "the HTTP calls themselves succeeded");
    assert!(eval.version_mismatch);
    assert_eq!(eval.observed_api_version.as_deref(), Some("3"));
    assert!(eval.detail.contains("apiVersion mismatch"));
}

#[test]
fn evaluate_ignores_a_minor_version_difference() {
    // "2.1" vs expected "2" (or a future "2.0"): same major, not a
    // mismatch — see major_version's doc comment.
    let outcome = FetchOutcome {
        health: Ok(Health {
            status: "ok".into(),
            gateway: 1,
        }),
        status: Ok(sample_status("2.1")),
        runs: Vec::new(),
        approvals: Ok(Vec::new()),
        metrics: Ok(Vec::new()),
        traces: Vec::new(),
    };
    assert!(!evaluate(&outcome).version_mismatch);
}

#[test]
fn version_mismatch_forces_at_least_degraded_while_reachable() {
    let mut t = HealthTracker::new();
    let tr = t.observe(true, true, Utc::now());
    assert_eq!(tr.state, HealthState::Degraded);
    assert_eq!(tr.log, Some(LogSeverity::Warn));
}

#[test]
fn version_mismatch_does_not_downgrade_an_unreachable_plane() {
    let mut t = HealthTracker::new();
    let now = Utc::now();
    for _ in 0..10 {
        t.observe(false, false, now);
    }
    assert_eq!(t.state, HealthState::Unreachable);
    // Reachable again but with a version mismatch on this same tick:
    // reachability resets failures to 0 (healthy floor), but the
    // mismatch keeps it at degraded, not unreachable and not healthy.
    let tr = t.observe(true, true, now);
    assert_eq!(tr.state, HealthState::Degraded);
}
