//! Bounded polling for tests that must wait on a background task or process instead of
//! asserting after a fixed sleep. Both helpers share one interval so no test hand-tunes
//! its own polling cadence; `budget` is the only thing a caller picks.

use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Call `attempt` on a fixed interval, on the current tokio runtime, until it returns
/// `Some`, or give up and return `None` once `budget` has elapsed since the first call.
/// A caller that just needs to wait out a fixed amount of (possibly paused/virtual) time
/// rather than poll a condition passes an `attempt` that always returns `None`.
pub async fn poll_until<T>(
    budget: Duration,
    mut attempt: impl AsyncFnMut() -> Option<T>,
) -> Option<T> {
    let deadline = tokio::time::Instant::now() + budget;
    loop {
        if let Some(value) = attempt().await {
            return Some(value);
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Sync counterpart of [`poll_until`], for tests that block a plain OS thread instead of
/// running on a tokio runtime.
pub fn poll_until_sync<T>(budget: Duration, mut attempt: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = std::time::Instant::now() + budget;
    loop {
        if let Some(value) = attempt() {
            return Some(value);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}
