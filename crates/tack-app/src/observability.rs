//! Execution fleet health.
//!
//! One pass computes an id-free snapshot of runner/queue/lease/event counts; [`spawn`] logs
//! stuck/ambiguous alerts from it. `tack-cli` starts [`spawn`] when `TACK_EXECUTION_HEALTH_ENABLE`
//! is on.
//!
//! # Cardinality is the whole point
//!
//! [`ExecutionFleetSnapshot`] is keyed only by the two small, closed state vocabularies this
//! domain has (`agent_runners.state`: 3 values; `execution_requests.state`: the 10-value
//! `ExecutionState` vocabulary) — never by attempt/request/runner id. That is not an incidental
//! design choice: no prompt/model contents belong in metric labels, and nothing here labels by
//! attempt id, decision id, or anything else unbounded — an unbounded label set is a defect, not
//! a style preference. `tests::snapshot_label_set_is_bounded_and_id_free` proves it.

use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use chrono::Duration;
use tack_db::Repository;
pub use tack_db::repo::execution::ExecutionFleetSnapshotRow as ExecutionFleetSnapshot;
use tack_protocol::Clock;
use tracing::{debug, info, warn};

use crate::AppError;
use crate::background::{Loops, Start};

/// Trailing window used for `events_ingested_in_window` — a coarse ingestion-rate signal, not a
/// precise metric. One hour by default.
pub const DEFAULT_EVENT_WINDOW_SECS: i64 = 3600;

/// What a pass counts.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    /// Width of the trailing window `events_ingested_in_window` counts over.
    pub event_window_secs: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            event_window_secs: DEFAULT_EVENT_WINDOW_SECS,
        }
    }
}

/// What one pass saw.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Outcome {
    /// The counts.
    pub snapshot: ExecutionFleetSnapshot,
    /// What they imply about operator attention needed right now.
    pub alerts: AlertSummary,
}

/// What one snapshot implies about operator attention needed right now. Pure, no I/O — fully
/// unit-testable without a store or a clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AlertSummary {
    pub stale_lease_alert: bool,
    pub needs_operator_alert: bool,
}

impl AlertSummary {
    pub fn any(&self) -> bool {
        self.stale_lease_alert || self.needs_operator_alert
    }
}

/// A stale lease or a `needs_operator` request is *always* worth surfacing — both are, by
/// definition, states nothing else in the system resolves on its own (a stale lease has already
/// outlived the recovery service's own window without being reconciled; `needs_operator` is never
/// automatically retried). `count > 0` is deliberately the entire condition — there is no
/// "acceptable" number of ambiguous requests to stay silent about. Log-spam control is a
/// *frequency* decision made by the caller ([`spawn`]'s transition-only `warn!`), not a
/// *threshold* decision made here.
pub fn evaluate_alerts(snapshot: &ExecutionFleetSnapshot) -> AlertSummary {
    AlertSummary {
        stale_lease_alert: snapshot.stale_lease_count > 0,
        needs_operator_alert: snapshot.needs_operator_count > 0,
    }
}

/// One pass: the snapshot at the clock's now, and the alerts it implies.
pub async fn tick(
    repo: &Repository,
    clock: &dyn Clock,
    settings: &Settings,
) -> Result<Outcome, AppError> {
    let snapshot = repo
        .execution_fleet_snapshot(clock.now(), Duration::seconds(settings.event_window_secs))
        .await?;
    let alerts = evaluate_alerts(&snapshot);
    Ok(Outcome { snapshot, alerts })
}

/// The loop `tack serve` starts: a pass at once, then one every `interval`. Logs a `warn!` only
/// on the *transition into* an alert condition (and an `info!` on the transition back out) — a
/// sustained stuck condition produces one warn on onset, not one every pass, no matter how long
/// it lasts. A `debug!`-level snapshot line is emitted every pass regardless, for anyone tailing
/// logs at debug level. The caller does not start it when the health watch is off.
pub fn spawn(
    loops: &mut Loops,
    repo: Repository,
    clock: Arc<dyn Clock>,
    settings: Settings,
    interval: StdDuration,
) {
    let interval = interval.max(StdDuration::from_secs(1));
    let last_alert = Arc::new(Mutex::new(AlertSummary::default()));
    loops.spawn_tick("observability", Start::Immediately, interval, move || {
        let (repo, clock, last_alert) = (repo.clone(), clock.clone(), last_alert.clone());
        async move {
            match tick(&repo, clock.as_ref(), &settings).await {
                Ok(outcome) => {
                    let mut last_alert =
                        last_alert.lock().expect("the alert state is not poisoned");
                    log_transitions(&last_alert, &outcome);
                    *last_alert = outcome.alerts;
                }
                Err(error) => warn!(
                    %error,
                    "execution health: snapshot query failed; will retry next cycle"
                ),
            }
        }
    });
}

fn log_transitions(last_alert: &AlertSummary, outcome: &Outcome) {
    let (snapshot, alerts) = (&outcome.snapshot, &outcome.alerts);

    if alerts.stale_lease_alert && !last_alert.stale_lease_alert {
        warn!(
            stale_lease_count = snapshot.stale_lease_count,
            oldest_stale_lease_age_secs = snapshot.oldest_stale_lease_age_secs,
            "execution health: stale lease(s) detected"
        );
    } else if !alerts.stale_lease_alert && last_alert.stale_lease_alert {
        info!("execution health: stale lease alert cleared");
    }

    if alerts.needs_operator_alert && !last_alert.needs_operator_alert {
        warn!(
            needs_operator_count = snapshot.needs_operator_count,
            oldest_needs_operator_age_secs = snapshot.oldest_needs_operator_age_secs,
            "execution health: request(s) in needs_operator"
        );
    } else if !alerts.needs_operator_alert && last_alert.needs_operator_alert {
        info!("execution health: needs_operator alert cleared");
    }

    debug!(
        runner_state_counts = ?snapshot.runner_state_counts,
        request_state_counts = ?snapshot.request_state_counts,
        events_ingested_in_window = snapshot.events_ingested_in_window,
        "execution health snapshot"
    );
}

#[cfg(test)]
#[path = "observability/tests.rs"]
mod tests;
