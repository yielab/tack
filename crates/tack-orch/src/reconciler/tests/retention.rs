//! The retention/rollup sweep (`spawn_retention_sweep`): the disabled
//! no-op path, a correctly-scoped enabled sweep, and a failing rollup that
//! must retry next cycle rather than stop the ticker. Self-contained: its
//! two `RetentionStore` fakes aren't shared with any other submodule.

use super::super::*;
use super::support::wait_until;
use std::sync::Mutex;

// -- Retention sweep -------------------------------------------------------

/// A [`RetentionStore`] whose two rollup methods are scripted and every
/// call recorded, for driving [`spawn_retention_sweep`] without a real
/// database.
struct FakeRetentionStore {
    events_outcome: RollupOutcome,
    metrics_outcome: RollupOutcome,
    events_calls: Mutex<Vec<DateTime<Utc>>>,
    metrics_calls: Mutex<Vec<DateTime<Utc>>>,
}

impl FakeRetentionStore {
    fn new() -> Self {
        Self {
            events_outcome: RollupOutcome::default(),
            metrics_outcome: RollupOutcome::default(),
            events_calls: Mutex::new(Vec::new()),
            metrics_calls: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl RetentionStore for FakeRetentionStore {
    async fn rollup_and_purge_events(
        &self,
        cutoff: DateTime<Utc>,
        _batch_size: i64,
    ) -> Result<RollupOutcome, OrchError> {
        self.events_calls.lock().unwrap().push(cutoff);
        Ok(self.events_outcome)
    }

    async fn rollup_and_purge_metrics(
        &self,
        cutoff: DateTime<Utc>,
        _batch_size: i64,
    ) -> Result<RollupOutcome, OrchError> {
        self.metrics_calls.lock().unwrap().push(cutoff);
        Ok(self.metrics_outcome)
    }
}

#[tokio::test]
async fn disabled_rollup_retention_sweep_never_calls_the_store() {
    let store = Arc::new(FakeRetentionStore::new());
    let handle = spawn_retention_sweep(false, store.clone(), 90, 1);
    // `handle.is_none()` already proves no task was spawned at all (the
    // `!enabled` branch returns before ever calling `tokio::spawn`) — no
    // wait afterward could surface a call from a task that doesn't exist.
    assert!(handle.is_none());
    assert!(store.events_calls.lock().unwrap().is_empty());
    assert!(store.metrics_calls.lock().unwrap().is_empty());
}

#[tokio::test]
async fn enabled_retention_sweep_calls_rollups_with_correct_cutoff() {
    let store = Arc::new(FakeRetentionStore::new());
    let retention_days = 90u32;
    let before_spawn = Utc::now();
    let handle = spawn_retention_sweep(true, store.clone(), retention_days, 1);
    assert!(handle.is_some());

    // The sweep loop runs its first rollups before ever sleeping — wait
    // for both rather than guessing how long that takes.
    wait_until(
        Duration::from_secs(5),
        "sweep never called both rollups",
        || {
            let events = store.events_calls.lock().map(|c| !c.is_empty());
            let metrics = store.metrics_calls.lock().map(|c| !c.is_empty());
            matches!((events, metrics), (Ok(true), Ok(true)))
        },
    )
    .await;
    handle.unwrap().abort();

    let events_calls = store.events_calls.lock().unwrap();
    let metrics_calls = store.metrics_calls.lock().unwrap();
    assert!(!events_calls.is_empty());
    assert!(!metrics_calls.is_empty());

    // The cutoff passed must be ~ (now - retention_days), not some other
    // arbitrary value — bounds-check against the window this test ran in.
    let expected_floor = before_spawn - chrono::Duration::days(retention_days as i64 + 1);
    let expected_ceiling = Utc::now() - chrono::Duration::days(retention_days as i64 - 1);
    for cutoff in events_calls.iter().chain(metrics_calls.iter()) {
        assert!(
            *cutoff > expected_floor && *cutoff < expected_ceiling,
            "cutoff {cutoff} not within the expected ~{retention_days}-day-ago window"
        );
    }
}

/// A [`RetentionStore`] whose `rollup_and_purge_events` always errors
/// (counting its own calls) and whose `rollup_and_purge_metrics` always
/// errors too — for proving a failed sweep tick doesn't stop the ticker.
struct AlwaysFailingRetentionStore {
    calls: Mutex<usize>,
}

#[async_trait::async_trait]
impl RetentionStore for AlwaysFailingRetentionStore {
    async fn rollup_and_purge_events(
        &self,
        _cutoff: DateTime<Utc>,
        _batch_size: i64,
    ) -> Result<RollupOutcome, OrchError> {
        *self.calls.lock().unwrap() += 1;
        Err(OrchError::Unavailable("db unreachable".into()))
    }

    async fn rollup_and_purge_metrics(
        &self,
        _cutoff: DateTime<Utc>,
        _batch_size: i64,
    ) -> Result<RollupOutcome, OrchError> {
        Err(OrchError::Unavailable("db unreachable".into()))
    }
}

#[tokio::test]
async fn a_rollup_failure_is_retried_and_does_not_stop_the_ticker() {
    let store = Arc::new(AlwaysFailingRetentionStore {
        calls: Mutex::new(0),
    });
    // The interval is a real `tokio::time::sleep` inside the sweep loop, so
    // proving a *second* tick happened needs that real second to pass; poll
    // instead of guessing how many would fit in a fixed window.
    let handle = spawn_retention_sweep(true, store.clone(), 90, 1).unwrap();
    wait_until(
        Duration::from_secs(5),
        "the ticker must keep retrying after a failed sweep, not stop",
        || store.calls.lock().map(|c| *c >= 2).unwrap_or(false),
    )
    .await;
    handle.abort();
    assert!(*store.calls.lock().unwrap() >= 2);
}
