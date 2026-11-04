//! What tells a person that something needs them (ADR 0035, decision 1), as pure values and the
//! two rules that decide it: [`notice`] says which board fact is worth a notification and
//! [`closes`] says which fact ends one. A **needs-you** notification stays open until the fact
//! that answers it; an **info** one is unread until read. Nothing here touches the store, a
//! clock or HTTP; `tack_app::notifications` applies the rules and keeps the inbox.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::event::{DomainEvent, Fact};
use crate::models::Item;
use crate::principal::PrincipalKind;

/// What happened that a person is told about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Kind {
    /// An agent asked a question and the run waits for the answer.
    Decision,
    RunFailed,
    /// An agent proposed a change set.
    ChangeSet,
    RunFinished,
    /// An item is due within the hour.
    DueSoon,
    /// No child of a parent item has a run open any more.
    ChildrenFinished,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Decision => "decision",
            Kind::RunFailed => "run_failed",
            Kind::ChangeSet => "change_set",
            Kind::RunFinished => "run_finished",
            Kind::DueSoon => "due_soon",
            Kind::ChildrenFinished => "children_finished",
        }
    }
}

/// How much a notification asks of a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Level {
    /// Something waits on the person, and stays in the count until it is answered.
    NeedsYou,
    /// Something happened; it is unread until the person reads it.
    Info,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::NeedsYou => "needs_you",
            Level::Info => "info",
        }
    }
}

/// The situation a notification is about, so the same situation is never open twice and the
/// fact that ends it can name it: `decision:<id>`, `run:<request>`, `change_set:<id>`,
/// `finished:<request>`, `due:<item>:<yyyy-mm-dd>`, `children:<parent>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Key(String);

impl Key {
    pub fn decision(decision_id: &str) -> Key {
        Key(format!("decision:{decision_id}"))
    }

    /// A run that failed, until it is run again.
    pub fn run(request_id: &str) -> Key {
        Key(format!("run:{request_id}"))
    }

    pub fn change_set(change_set_id: &str) -> Key {
        Key(format!("change_set:{change_set_id}"))
    }

    pub fn finished(request_id: &str) -> Key {
        Key(format!("finished:{request_id}"))
    }

    /// One item is announced once for the day it falls due.
    pub fn due(item_id: Uuid, due: DateTime<Utc>) -> Key {
        Key(format!("due:{item_id}:{}", due.format("%Y-%m-%d")))
    }

    /// The runs of a parent's children have all ended; said again each time they do.
    pub fn children(parent_id: Uuid) -> Key {
        Key(format!("children:{parent_id}"))
    }

    /// The key as it is stored.
    pub fn from_stored(text: impl Into<String>) -> Key {
        Key(text.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What the rule decided is worth telling, before the inbox gives it an id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub kind: Kind,
    pub level: Level,
    pub key: Key,
    pub project_id: Uuid,
    pub item_id: Option<Uuid>,
    pub request_id: Option<String>,
    /// One sentence, as the rule writes it; the item it is about is `item_id`.
    pub title: String,
    /// Who or what caused the fact: a principal's label.
    pub cause: String,
}

/// A notice as the inbox keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Notification {
    pub id: Uuid,
    pub project_id: Uuid,
    pub kind: Kind,
    pub level: Level,
    pub key: Key,
    pub item_id: Option<Uuid>,
    pub request_id: Option<String>,
    pub title: String,
    pub cause: String,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
}

/// The notification a fact calls for, or `None`. A fact the operator caused never notifies the
/// operator, and an item that was created, edited, moved or deleted is not a notification: the
/// board is live already.
pub fn notice(event: &DomainEvent) -> Option<Notice> {
    if event.principal.kind == PrincipalKind::Operator {
        return None;
    }
    let cause = event.principal.label();
    let (kind, level, key, item_id, request_id, title) = match &event.fact {
        Fact::DecisionOpened { run, decision_id } => (
            Kind::Decision,
            Level::NeedsYou,
            Key::decision(decision_id),
            Some(run.item_id),
            Some(run.request_id.clone()),
            "A run is asking a question",
        ),
        Fact::RunFailed { run } => (
            Kind::RunFailed,
            Level::NeedsYou,
            Key::run(&run.request_id),
            Some(run.item_id),
            Some(run.request_id.clone()),
            "A run failed",
        ),
        Fact::ChangeSetProposed {
            change_set_id,
            item_id,
        } => (
            Kind::ChangeSet,
            Level::NeedsYou,
            Key::change_set(change_set_id),
            *item_id,
            None,
            "A change set is waiting for you",
        ),
        Fact::RunFinished { run } => (
            Kind::RunFinished,
            Level::Info,
            Key::finished(&run.request_id),
            Some(run.item_id),
            Some(run.request_id.clone()),
            "A run finished",
        ),
        Fact::ChildrenFinished { parent_id } => (
            Kind::ChildrenFinished,
            Level::Info,
            Key::children(*parent_id),
            Some(*parent_id),
            None,
            "The children of this card have finished",
        ),
        Fact::ItemCreated { .. }
        | Fact::ItemUpdated { .. }
        | Fact::ItemMoved { .. }
        | Fact::ItemDeleted { .. }
        | Fact::RunStarted { .. }
        | Fact::RunWaiting { .. }
        | Fact::DecisionResolved { .. }
        | Fact::ChangeSetSettled { .. } => return None,
    };
    Some(Notice {
        kind,
        level,
        key,
        project_id: event.project_id,
        item_id,
        request_id,
        title: title.to_owned(),
        cause,
    })
}

/// The situation a fact ends, or `None`. Who caused the fact does not matter: a question answered
/// in docket's own tool closes it as well. A run that starts closes the failure of its request;
/// a run that began as a new request leaves the old failure to the store's reconcile.
pub fn closes(event: &DomainEvent) -> Option<Key> {
    match &event.fact {
        Fact::DecisionResolved { decision_id, .. } => Some(Key::decision(decision_id)),
        Fact::RunStarted { run } => Some(Key::run(&run.request_id)),
        Fact::ChangeSetSettled { change_set_id } => Some(Key::change_set(change_set_id)),
        _ => None,
    }
}

/// The notice for an item due within the hour, from the hourly loop. A due date is announced once
/// for the day it falls on, however many passes see it.
pub fn due_soon(item: &Item, now: DateTime<Utc>) -> Notice {
    Notice {
        kind: Kind::DueSoon,
        level: Level::Info,
        key: Key::due(item.id, item.due_date.unwrap_or(now)),
        project_id: item.project_id,
        item_id: Some(item.id),
        request_id: None,
        title: format!("\"{}\" is due within the hour", item.title),
        cause: crate::principal::Principal::system().label(),
    }
}

#[cfg(test)]
#[path = "notification/tests.rs"]
mod tests;
