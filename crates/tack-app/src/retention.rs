//! Retention sweep for the execution domain: one pass purges the replay/idempotency
//! bookkeeping and the events of terminated attempts that are older than the retention window.
//!
//! No roll-up table for `execution_events` yet: this purges terminal-attempt event rows
//! outright. `tack-cli` spawns [`run`] when `TACK_EXECUTION_RETENTION_ENABLE` is on.

use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::Duration;
use tack_db::repo::execution::PurgeStats;
use tack_db::{Repository, StoreError};
use tack_protocol::Clock;
use tracing::{debug, info, warn};

use crate::AppError;

/// Default retention window in days — mirrors `AppConfig::execution_retention_days`'s own
/// default (`crates/tack-api/src/config.rs`).
pub const DEFAULT_RETENTION_DAYS: u32 = 90;

/// Rows processed per purge transaction — see `Repository::purge_stale_execution_replays` for
/// why this is bounded rather than one transaction for the whole backlog.
pub const DEFAULT_BATCH_SIZE: i64 = 500;

/// What a pass purges.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    /// Rows older than this many days are stale.
    pub retention_days: u32,
    /// Rows per purge transaction.
    pub batch_size: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            retention_days: DEFAULT_RETENTION_DAYS,
            batch_size: DEFAULT_BATCH_SIZE,
        }
    }
}

/// What one pass purged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Outcome {
    /// Stale replay/idempotency bookkeeping (see `Repository::purge_stale_execution_replays` for
    /// which six tables, and why plain deletion rather than roll-up is the correct behavior).
    pub replays: PurgeStats,
    /// `execution_events` rows of terminated attempts (see
    /// `Repository::purge_stale_terminal_execution_events`).
    pub events: PurgeStats,
}

/// One pass: both purges run even when the first fails, so one failing step never starves the
/// other; the first error is returned.
pub async fn tick(
    repo: &Repository,
    clock: &dyn Clock,
    settings: &Settings,
) -> Result<Outcome, AppError> {
    let cutoff = clock.now() - Duration::days(i64::from(settings.retention_days));

    let replays = repo
        .purge_stale_execution_replays(cutoff, settings.batch_size)
        .await
        .map_err(StoreError::from);
    let events = repo
        .purge_stale_terminal_execution_events(cutoff, settings.batch_size)
        .await
        .map_err(StoreError::from);

    match (replays, events) {
        (Ok(replays), Ok(events)) => Ok(Outcome { replays, events }),
        (Err(error), Err(other)) => {
            warn!(error = %other, "execution retention: event purge failed");
            Err(error.into())
        }
        (Err(error), Ok(_)) | (Ok(_), Err(error)) => Err(error.into()),
    }
}

/// The loop `tack serve` spawns: a pass at once, then one every `interval`, a failed pass logged
/// and retried on the next cycle. The caller does not spawn it when retention is off.
pub async fn run(
    repo: Repository,
    clock: Arc<dyn Clock>,
    settings: Settings,
    interval: StdDuration,
) {
    let mut ticker = tokio::time::interval(interval.max(StdDuration::from_secs(1)));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        match tick(&repo, clock.as_ref(), &settings).await {
            Ok(outcome) => log(&outcome),
            Err(error) => warn!(%error, "execution retention: purge failed; will retry next cycle"),
        }
    }
}

fn log(outcome: &Outcome) {
    if outcome.replays.rows_purged > 0 {
        info!(
            rows_purged = outcome.replays.rows_purged,
            batches = outcome.replays.batches_run,
            "execution retention: replay bookkeeping purged"
        );
    } else {
        debug!("execution retention: no stale replay bookkeeping to purge");
    }
    if outcome.events.rows_purged > 0 {
        info!(
            rows_purged = outcome.events.rows_purged,
            batches = outcome.events.batches_run,
            "execution retention: terminal execution_events purged"
        );
    } else {
        debug!("execution retention: no stale terminal events to purge");
    }
}

#[cfg(test)]
#[path = "retention/tests.rs"]
mod tests;
