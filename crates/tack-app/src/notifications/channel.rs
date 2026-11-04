//! How the inbox leaves the machine (ADR 0035, decision 5). A channel takes one notification and
//! either has it or says whether trying again could help; the delivery loop owns the schedule.

use async_trait::async_trait;
use tack_core::notification::Notification;

/// A channel could not take a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Undelivered {
    /// The peer was unreachable or failed (a timeout, a 5xx): another try may work.
    Retry(String),
    /// The peer refused it (a 4xx): the same request will be refused again.
    Drop(String),
}

/// A destination for notifications. Delivery is at least once: a receiver deduplicates on the
/// notification's `id`.
#[async_trait]
pub trait Channel: Send + Sync {
    /// The name the delivery rows and the settings screen know it by.
    fn name(&self) -> &'static str;

    async fn send(&self, notification: &Notification) -> Result<(), Undelivered>;
}
