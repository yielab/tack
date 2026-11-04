//! The inbox (ADR 0035): the one place that writes it. A [`Notice`] the rule in
//! `tack_core::notification` decided on is opened here, the fact that answers it closes it, and
//! every change is told to whoever listens (the live socket) so a screen need not poll. Nothing
//! else writes the `notifications` tables.

use std::sync::Arc;

use serde::Serialize;
use tack_core::notification::{Key, Notice, Notification};
use tack_db::Repository;
use tack_db::repo::notifications::Listing;
use tack_protocol::Clock;
use tokio::sync::{Notify, broadcast};
use uuid::Uuid;

use crate::AppError;

pub mod channel;
pub mod delivery;
pub mod subscriber;
pub mod webhook;

/// What happened to the inbox, as the live socket tells it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Change {
    Opened(Notification),
    Closed { id: Uuid, key: Key },
    Read { id: Uuid },
}

/// How many changes a slow listener may be behind before it loses the oldest.
const CAPACITY: usize = 256;

/// The inbox: a repository, a clock, the names of the channels a new notification is queued for,
/// and the stream of what changes.
#[derive(Clone)]
pub struct Inbox {
    repo: Repository,
    clock: Arc<dyn Clock>,
    channels: Arc<Vec<String>>,
    live: broadcast::Sender<Change>,
    /// Pinged when something was queued for a channel, so delivery need not wait for its tick.
    queued: Arc<Notify>,
}

impl Inbox {
    pub fn new(repo: Repository, clock: Arc<dyn Clock>, channels: Vec<String>) -> Inbox {
        Inbox {
            repo,
            clock,
            channels: Arc::new(channels),
            live: broadcast::channel(CAPACITY).0,
            queued: Arc::new(Notify::new()),
        }
    }

    /// Pinged each time a notification opens with a channel to deliver it through.
    pub fn queued(&self) -> Arc<Notify> {
        self.queued.clone()
    }

    /// Opens `notice` and queues it for each channel. `None` when its situation was open already:
    /// nothing was written and nothing is told.
    pub async fn open(&self, notice: &Notice) -> Result<Option<Notification>, AppError> {
        let opened = self
            .repo
            .insert_open_notification(notice, &self.channels, self.clock.now())
            .await?;
        if let Some(notification) = &opened {
            self.tell(Change::Opened(notification.clone()));
            if !self.channels.is_empty() {
                self.queued.notify_one();
            }
        }
        Ok(opened)
    }

    /// Closes the situation `key` names. `false` when none was open.
    pub async fn close(&self, key: &Key) -> Result<bool, AppError> {
        let closed = self
            .repo
            .close_notification_by_key(key, self.clock.now())
            .await?;
        if let Some(id) = closed {
            self.tell(Change::Closed {
                id,
                key: key.clone(),
            });
        }
        Ok(closed.is_some())
    }

    /// Marks one notification read. `false` when it is not there or was read already.
    pub async fn read(&self, id: Uuid) -> Result<bool, AppError> {
        let read = self
            .repo
            .mark_notification_read(id, self.clock.now())
            .await?;
        if read {
            self.tell(Change::Read { id });
        }
        Ok(read)
    }

    /// Marks every unread notification read; how many there were.
    pub async fn read_all(&self) -> Result<usize, AppError> {
        let ids = self
            .repo
            .mark_all_notifications_read(self.clock.now())
            .await?;
        for id in &ids {
            self.tell(Change::Read { id: *id });
        }
        Ok(ids.len())
    }

    /// What still asks something of the person, newest first.
    pub async fn open_notifications(&self, limit: u32) -> Result<Vec<Notification>, AppError> {
        Ok(self.repo.list_notifications(Listing::Open, limit).await?)
    }

    /// Everything kept, newest first.
    pub async fn recent(&self, limit: u32) -> Result<Vec<Notification>, AppError> {
        Ok(self.repo.list_notifications(Listing::All, limit).await?)
    }

    /// How many notifications are open.
    pub async fn open_count(&self) -> Result<u64, AppError> {
        Ok(self.repo.count_open_notifications().await?)
    }

    /// Everything that changes from now on.
    pub fn subscribe(&self) -> broadcast::Receiver<Change> {
        self.live.subscribe()
    }

    fn tell(&self, change: Change) {
        // No listener is fine.
        let _ = self.live.send(change);
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
