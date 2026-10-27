//! Retention sweep for the execution domain: one pass purges the replay/idempotency
//! bookkeeping and the events of terminated attempts that are older than the retention window,
//! and expires the decisions whose time has passed.
//!
//! No roll-up table for `execution_events` yet: this purges terminal-attempt event rows
//! outright, and it is the only place that deletes from that table. `tack-cli` starts
//! [`spawn`] when `TACK_EXECUTION_RETENTION_ENABLE` is on; the artifact blobs, which need the
//! API's storage, are swept by `tack-api`'s execution runtime on the same flag.

use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::Duration;
use tack_db::Repository;
use tack_db::repo::execution::PurgeStats;
use tack_protocol::Clock;
use tracing::{debug, info, warn};

use crate::AppError;
use crate::background::{Loops, Start};
use crate::runs::expire_decisions::expire_decisions;

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
    /// Pending decisions whose `expires_at` had passed, now `expired`. They ride the retention
    /// flag rather than a looser one of their own, so expiry never runs more permissively than
    /// the deletion it is bundled with; a need to expire under other conditions is a new flag.
    pub decisions_expired: u64,
}

/// One pass: the two purges and the decision expiry all run even when one fails, so one failing
/// step never starves the others; the first error is returned and the rest are logged.
pub async fn tick(
    repo: &Repository,
    clock: &dyn Clock,
    settings: &Settings,
) -> Result<Outcome, AppError> {
    let cutoff = clock.now() - Duration::days(i64::from(settings.retention_days));

    let replays = repo
        .purge_stale_execution_replays(cutoff, settings.batch_size)
        .await
        .map_err(AppError::from);
    let events = repo
        .purge_stale_terminal_execution_events(cutoff, settings.batch_size)
        .await
        .map_err(AppError::from);
    let decisions_expired = expire_decisions(repo, clock).await;

    match (replays, events, decisions_expired) {
        (Ok(replays), Ok(events), Ok(decisions_expired)) => Ok(Outcome {
            replays,
            events,
            decisions_expired,
        }),
        (replays, events, decisions) => {
            let mut errors = [replays.err(), events.err(), decisions.err()]
                .into_iter()
                .flatten();
            let first = errors.next().expect("one step failed");
            for other in errors {
                warn!(error = %other, "execution retention: another step failed too");
            }
            Err(first)
        }
    }
}

/// The loop `tack serve` starts: a pass at once, then one every `interval`, a failed pass logged
/// and retried on the next cycle. The caller does not start it when retention is off.
pub fn spawn(
    loops: &mut Loops,
    repo: Repository,
    clock: Arc<dyn Clock>,
    settings: Settings,
    interval: StdDuration,
) {
    let interval = interval.max(StdDuration::from_secs(1));
    loops.spawn_tick("retention", Start::Immediately, interval, move || {
        let (repo, clock) = (repo.clone(), clock.clone());
        async move {
            match tick(&repo, clock.as_ref(), &settings).await {
                Ok(outcome) => log(&outcome),
                Err(error) => {
                    warn!(%error, "execution retention: purge failed; will retry next cycle");
                }
            }
        }
    });
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
    if outcome.decisions_expired > 0 {
        info!(
            decisions_expired = outcome.decisions_expired,
            "execution retention: overdue decisions expired"
        );
    } else {
        debug!("execution retention: no overdue decisions to expire");
    }
}

#[cfg(test)]
#[path = "retention/tests.rs"]
mod tests;
