//! Telling the board's listeners what a run did. The use cases of `runs/` write and return what
//! came of it; the caller that holds the outcome passes it here, and each fact is announced
//! once: a replay of a report, a question already asked or an answer already given announces
//! nothing, because it made nothing true.

use tack_core::principal::Principal;
use tack_db::{Repository, queries};
use tack_protocol::execution::{AttemptId, ExecutionState};
use uuid::Uuid;

use super::complete::{Completed, ItemMoved};
use super::create_decision::DecisionCreated;
use super::resolve_decision::ResolveOutcome;
use crate::events::{DomainEvent, Fact, RunRef};
use crate::ports::DomainEvents;

/// The project and the run an attempt belongs to, or `None` (logged) when they cannot be read.
async fn run_of(repo: &Repository, attempt_id: &AttemptId) -> Option<(Uuid, RunRef)> {
    let found = async {
        let request = queries::execution_requests::request_of_attempt(repo, attempt_id.as_str())
            .await
            .ok()?;
        let item_id = request.item_id.parse::<Uuid>().ok()?;
        let item = repo.get_item(item_id).await.ok()??;
        Some((
            item.project_id,
            RunRef {
                request_id: request.id,
                attempt_id: attempt_id.as_str().to_owned(),
                item_id,
            },
        ))
    }
    .await;
    if found.is_none() {
        tracing::warn!(%attempt_id, "events: could not tell which run the attempt belongs to");
    }
    found
}

fn item_moved(events: &dyn DomainEvents, principal: &Principal, moved: &ItemMoved) {
    events.emit(DomainEvent::item_changed(
        moved.project_id,
        principal,
        moved.item_id,
        &moved.old_status,
        &moved.new_status,
    ));
}

/// The runner reported the attempt running, and the report moved the item, or did not.
pub async fn run_started(
    repo: &Repository,
    events: &dyn DomainEvents,
    principal: &Principal,
    attempt_id: &AttemptId,
    moved: Option<&ItemMoved>,
) {
    if let Some((project_id, run)) = run_of(repo, attempt_id).await {
        events.emit(DomainEvent::new(
            project_id,
            principal,
            Fact::RunStarted { run },
        ));
    }
    if let Some(moved) = moved {
        item_moved(events, principal, moved);
    }
}

/// The attempt ended with a report that was just committed: a success finished the run, a
/// failure failed it, a cancellation is neither. The board move a success caused follows.
pub async fn run_ended(
    repo: &Repository,
    events: &dyn DomainEvents,
    principal: &Principal,
    attempt_id: &AttemptId,
    completed: &Completed,
) {
    let Completed::Committed {
        response,
        item_moved: moved,
    } = completed
    else {
        return;
    };
    if let Some((project_id, run)) = run_of(repo, attempt_id).await {
        let fact = match response.terminal_state {
            ExecutionState::Succeeded => Some(Fact::RunFinished { run }),
            ExecutionState::Failed => Some(Fact::RunFailed { run }),
            _ => None,
        };
        if let Some(fact) = fact {
            events.emit(DomainEvent::new(project_id, principal, fact));
        }
    }
    if let Some(moved) = moved {
        item_moved(events, principal, moved);
    }
}

/// A question was stored: it is opened, and the run waits on a person unless it already did.
pub async fn decision_opened(
    repo: &Repository,
    events: &dyn DomainEvents,
    principal: &Principal,
    attempt_id: &AttemptId,
    decision_id: &str,
    created: &DecisionCreated,
) {
    let DecisionCreated::Created {
        fresh: true,
        parked,
        ..
    } = created
    else {
        return;
    };
    let Some((project_id, run)) = run_of(repo, attempt_id).await else {
        return;
    };
    events.emit(DomainEvent::new(
        project_id,
        principal,
        Fact::DecisionOpened {
            run: run.clone(),
            decision_id: decision_id.to_owned(),
        },
    ));
    if *parked {
        events.emit(DomainEvent::new(
            project_id,
            principal,
            Fact::RunWaiting { run },
        ));
    }
}

/// A person's answer was stored just now.
pub async fn decision_resolved(
    repo: &Repository,
    events: &dyn DomainEvents,
    principal: &Principal,
    attempt_id: &AttemptId,
    decision_id: &str,
    outcome: &ResolveOutcome,
) {
    if !matches!(
        outcome,
        ResolveOutcome::Resolved {
            replayed: false,
            ..
        }
    ) {
        return;
    }
    if let Some((project_id, run)) = run_of(repo, attempt_id).await {
        events.emit(DomainEvent::new(
            project_id,
            principal,
            Fact::DecisionResolved {
                run,
                decision_id: decision_id.to_owned(),
            },
        ));
    }
}
