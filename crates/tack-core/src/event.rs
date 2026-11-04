//! What happened on a board, as values (ADR 0034, decision 8): an item was created, edited,
//! moved or deleted; a run started, finished, failed or began waiting on a person; a decision
//! was opened or answered; a change set was proposed or settled. Each carries the project it
//! happened in and the [`Principal`] that caused it, a runner and Tack itself included, so a
//! consumer can tell a person's change from its own. The values live here so the pure rules
//! that read them ([`crate::notification`]) need no runtime; the bus that carries them is
//! `tack_app::events`.

use uuid::Uuid;

use crate::principal::Principal;

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
    /// The last open run among a parent item's children ended. The parent's status is the
    /// person's: nothing moves.
    ChildrenFinished {
        parent_id: Uuid,
    },
    /// An agent proposed a change set; it waits for a person.
    ChangeSetProposed {
        change_set_id: String,
        item_id: Option<Uuid>,
    },
    /// A change set was applied or rejected: it no longer waits.
    ChangeSetSettled {
        change_set_id: String,
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
