//! The hourly pass that tells the outside world about items due within the next hour.
//!
//! `tick` is one pass and returns how many items it announced; `run` is the loop `tack serve`
//! spawns. What an announcement does (a webhook POST) is the caller's: it hands in a
//! [`DueSoonSink`].

use std::sync::Arc;
use std::time::Duration;

use tack_db::Repository;
use tack_protocol::Clock;

use crate::AppError;

/// The event name an announcement carries.
pub const EVENT: &str = "item.due_soon";

/// How far ahead of now an item's due date counts as soon.
const WINDOW: chrono::Duration = chrono::Duration::hours(1);

/// How often `tack serve` runs a pass.
pub const INTERVAL: Duration = Duration::from_secs(3600);

/// Where announcements go. Delivery is fire-and-forget: a failure is the sink's to log.
pub trait DueSoonSink: Send + Sync {
    fn fire(&self, event: &str, payload: serde_json::Value);
}

/// One pass: every incomplete item due within the next hour is announced to `sink`.
pub async fn tick(
    repo: &Repository,
    clock: &dyn Clock,
    sink: &dyn DueSoonSink,
) -> Result<usize, AppError> {
    let now = clock.now();
    let items = repo.list_items_due_soon(now, now + WINDOW).await?;
    let announced = items.len();
    for item in items {
        let payload = serde_json::json!({
            "event": EVENT,
            "timestamp": now.to_rfc3339(),
            "project_id": item.project_id,
            "item": item,
        });
        sink.fire(EVENT, payload);
    }
    Ok(announced)
}

/// The loop: wait `interval`, run a pass, log a failed one, repeat. The first, immediate
/// tick of the timer is skipped.
pub async fn run(
    repo: Repository,
    clock: Arc<dyn Clock>,
    sink: Arc<dyn DueSoonSink>,
    interval: Duration,
) {
    let mut timer = tokio::time::interval(interval);
    timer.tick().await;
    loop {
        timer.tick().await;
        if let Err(error) = tick(&repo, clock.as_ref(), sink.as_ref()).await {
            tracing::warn!(%error, "Failed to query due-soon items for webhook");
        }
    }
}
