//! What happened on a board, as values (ADR 0034, decision 8): an item was created, edited,
//! moved or deleted; a run started, finished, failed or began waiting on a person; a decision
//! was opened or answered. Each carries the project it happened in and the [`Principal`] that
//! caused it, a runner and Tack itself included, so a consumer can tell a person's change from
//! its own. The place where a fact becomes true announces it once, through the
//! [`DomainEvents`](crate::ports::DomainEvents) port; nothing announces it a second time on
//! the way.

use std::sync::Mutex;

use tack_core::principal::Principal;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::ports::DomainEvents;

/// The run a fact is about: the request, the attempt that carried it and the item it works on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRef {
    pub request_id: String,
    pub attempt_id: String,
    pub item_id: Uuid,
}

/// One thing that became true.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fact {
    ItemCreated {
        item_id: Uuid,
        status: String,
    },
    /// An edit that left the status where it was.
    ItemUpdated {
        item_id: Uuid,
        status: String,
    },
    ItemMoved {
        item_id: Uuid,
        from: String,
        to: String,
    },
    ItemDeleted {
        item_id: Uuid,
    },
    /// The runner reported the attempt running.
    RunStarted {
        run: RunRef,
    },
    RunFinished {
        run: RunRef,
    },
    RunFailed {
        run: RunRef,
    },
    /// The run stopped to wait for a person's answer; not said again for a second question.
    RunWaiting {
        run: RunRef,
    },
    DecisionOpened {
        run: RunRef,
        decision_id: String,
    },
    DecisionResolved {
        run: RunRef,
        decision_id: String,
    },
}

/// A [`Fact`] with its project and the principal that caused it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainEvent {
    pub project_id: Uuid,
    pub principal: Principal,
    pub fact: Fact,
}

impl DomainEvent {
    pub fn new(project_id: Uuid, principal: &Principal, fact: Fact) -> DomainEvent {
        DomainEvent {
            project_id,
            principal: principal.clone(),
            fact,
        }
    }

    /// A saved change to an item: a move when the status changed, an update when it did not.
    pub fn item_changed(
        project_id: Uuid,
        principal: &Principal,
        item_id: Uuid,
        old_status: &str,
        new_status: &str,
    ) -> DomainEvent {
        let fact = if old_status == new_status {
            Fact::ItemUpdated {
                item_id,
                status: new_status.to_owned(),
            }
        } else {
            Fact::ItemMoved {
                item_id,
                from: old_status.to_owned(),
                to: new_status.to_owned(),
            }
        };
        DomainEvent::new(project_id, principal, fact)
    }
}

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
