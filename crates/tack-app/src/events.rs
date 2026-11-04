//! The board's facts travel on one in-process bus (ADR 0034, decision 8). The values
//! ([`DomainEvent`], [`Fact`], [`RunRef`]) are `tack_core::event`; this module is the bus and a
//! recorder for tests. The place where a fact becomes true announces it once, through the
//! [`DomainEvents`](crate::ports::DomainEvents) port; nothing announces it a second time on
//! the way.

use std::sync::Mutex;

pub use tack_core::event::{DomainEvent, Fact, RunRef};
use tokio::sync::broadcast;

use crate::ports::DomainEvents;

/// The in-process [`DomainEvents`]: every event goes to each subscriber that is listening, and
/// to nobody when none is. A subscriber that falls behind loses the oldest events and is told
/// how many (`RecvError::Lagged`).
#[derive(Clone)]
pub struct EventBus(broadcast::Sender<DomainEvent>);

impl EventBus {
    /// How many events a slow subscriber may be behind before it loses the oldest.
    const CAPACITY: usize = 1024;

    pub fn subscribe(&self) -> broadcast::Receiver<DomainEvent> {
        self.0.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        EventBus(broadcast::channel(Self::CAPACITY).0)
    }
}

impl DomainEvents for EventBus {
    fn emit(&self, event: DomainEvent) {
        // No subscriber is fine.
        let _ = self.0.send(event);
    }

    fn subscribe(&self) -> Option<broadcast::Receiver<DomainEvent>> {
        Some(EventBus::subscribe(self))
    }
}

/// A [`DomainEvents`] that keeps what it was told, for a test to read.
#[derive(Default)]
pub struct RecordedEvents(Mutex<Vec<DomainEvent>>);

impl RecordedEvents {
    /// Everything emitted so far, oldest first.
    pub fn events(&self) -> Vec<DomainEvent> {
        self.0.lock().expect("recorded events").clone()
    }
}

impl DomainEvents for RecordedEvents {
    fn emit(&self, event: DomainEvent) {
        self.0.lock().expect("recorded events").push(event);
    }
}
