//! The hourly pass that tells a person about items due within the next hour.
//!
//! `tick` is one pass and returns how many notifications it opened; `spawn` adds the loop
//! `tack serve` runs. An item is announced once for the day it falls due, however many passes see
//! it: the notice's key sees to that (`tack_core::notification::due_soon`).

use std::sync::Arc;
use std::time::Duration;

use tack_core::notification::due_soon;
use tack_db::Repository;
use tack_protocol::Clock;

use crate::AppError;
use crate::background::{Loops, Start};
use crate::notifications::Inbox;

/// How far ahead of now an item's due date counts as soon.
const WINDOW: chrono::Duration = chrono::Duration::hours(1);

/// How often `tack serve` runs a pass.
pub const INTERVAL: Duration = Duration::from_secs(3600);

/// One pass: every incomplete item due within the next hour is opened in `inbox`, unless it was
/// announced already. Returns how many were opened.
pub async fn tick(repo: &Repository, clock: &dyn Clock, inbox: &Inbox) -> Result<usize, AppError> {
    let now = clock.now();
    let items = repo.list_items_due_soon(now, now + WINDOW).await?;
    let mut opened = 0;
    for item in items {
        if inbox.open(&due_soon(&item, now)).await?.is_some() {
            opened += 1;
        }
    }
    Ok(opened)
}

/// The loop: a pass every `interval`, the first one interval after the start, a failed one
/// logged.
pub fn spawn(
    loops: &mut Loops,
    repo: Repository,
    clock: Arc<dyn Clock>,
    inbox: Inbox,
    interval: Duration,
) {
    loops.spawn_tick("due_soon", Start::AfterInterval, interval, move || {
        let (repo, clock, inbox) = (repo.clone(), clock.clone(), inbox.clone());
        async move {
            if let Err(error) = tick(&repo, clock.as_ref(), &inbox).await {
                tracing::warn!(%error, "Failed to announce the items due soon");
            }
        }
    });
}
