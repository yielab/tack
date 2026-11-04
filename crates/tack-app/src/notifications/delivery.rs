//! The loop that hands each notification to each channel (ADR 0035, decision 5). `Inbox::open`
//! queues one delivery row per channel; a pass takes the rows that are due, tries them, and writes
//! the outcome: delivered, tried again later, or given up on with the last error kept. A pass
//! runs every few seconds and at once when the inbox opens something.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use futures::future::join_all;
use tack_db::Repository;
use tack_db::repo::notifications::DueDelivery;
use tack_protocol::Clock;
use tokio::sync::Notify;

use super::channel::{Channel, Undelivered};
use crate::AppError;
use crate::background::{Loops, Start};

/// How often a pass looks for deliveries that are due.
pub const INTERVAL: Duration = Duration::from_secs(10);

/// The most deliveries one pass takes.
const BATCH: u32 = 25;

/// A delivery that failed this many times is given up on.
pub const MAX_ATTEMPTS: u32 = 8;

const FIRST_BACKOFF_SECS: u64 = 30;
const MAX_BACKOFF_SECS: u64 = 6 * 3600;

/// How long to wait after the `attempts`-th failure (counting from zero): 30 s, doubling, up to
/// six hours.
pub fn backoff(attempts: u32) -> chrono::Duration {
    let secs = FIRST_BACKOFF_SECS
        .saturating_mul(2u64.saturating_pow(attempts))
        .min(MAX_BACKOFF_SECS);
    chrono::Duration::seconds(secs as i64)
}

/// One pass: every due delivery is tried, together. Returns how many were tried.
pub async fn tick(
    repo: &Repository,
    clock: &dyn Clock,
    channels: &[Arc<dyn Channel>],
) -> Result<usize, AppError> {
    let now = clock.now();
    let due = repo.deliveries_due(now, BATCH).await?;
    let tried = due.len();
    let outcomes = join_all(
        due.iter()
            .map(|delivery| attempt(repo, channels, delivery, now)),
    )
    .await;
    outcomes.into_iter().collect::<Result<(), _>>()?;
    Ok(tried)
}

async fn attempt(
    repo: &Repository,
    channels: &[Arc<dyn Channel>],
    delivery: &DueDelivery,
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    let id = delivery.notification.id;
    let channel = delivery.channel.as_str();
    let Some(sender) = channels.iter().find(|c| c.name() == channel) else {
        // Configured away since it was queued: sending it now, long after, would only be noise.
        repo.delivery_failed(id, channel, None, "the channel is not configured")
            .await?;
        return Ok(());
    };
    match sender.send(&delivery.notification).await {
        Ok(()) => repo.delivery_done(id, channel, now).await?,
        Err(Undelivered::Drop(error)) => {
            tracing::warn!(%id, channel, %error, "notification refused; not trying again");
            repo.delivery_failed(id, channel, None, &error).await?;
        }
        Err(Undelivered::Retry(error)) => {
            let next =
                (delivery.attempts + 1 < MAX_ATTEMPTS).then(|| now + backoff(delivery.attempts));
            tracing::warn!(%id, channel, attempts = delivery.attempts + 1, %error, "notification not delivered");
            repo.delivery_failed(id, channel, next, &error).await?;
        }
    }
    Ok(())
}

/// The loop: a pass every [`INTERVAL`] and whenever `wake` is pinged, a failed one logged.
pub fn spawn(
    loops: &mut Loops,
    repo: Repository,
    clock: Arc<dyn Clock>,
    channels: Vec<Arc<dyn Channel>>,
    wake: Arc<Notify>,
) {
    let channels: Arc<[Arc<dyn Channel>]> = channels.into();
    loops.spawn_woken(
        "notification_delivery",
        Start::Immediately,
        INTERVAL,
        wake,
        move || {
            let (repo, clock, channels) = (repo.clone(), clock.clone(), channels.clone());
            async move {
                if let Err(error) = tick(&repo, clock.as_ref(), &channels).await {
                    tracing::warn!(%error, "Failed to deliver notifications");
                }
            }
        },
    );
}
