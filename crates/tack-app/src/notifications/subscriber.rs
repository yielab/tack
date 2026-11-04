//! Feeds the inbox from the board's events and keeps it true to the store (ADR 0035, decision 3).
//! [`drain`] applies the rule of `tack_core::notification` to what the bus carried: a fact that
//! calls for a notification opens it, a fact that answers one closes it. The bus is in memory and
//! lossy, so [`reconcile`] reads what waits on a person from the store at the start, after the
//! bus said it dropped events, and after a write failed: it opens what has no notification and
//! closes what no longer waits. That is also what closes a failed run that was run again under a
//! new request, which no single fact names.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tack_core::event::{DomainEvent, Fact, RunRef};
use tack_core::notification::{Key, Level, Notice, closes, notice};
use tack_core::principal::Principal;
use tack_db::Repository;
use tack_db::repo::notifications::WaitingRun;
use tack_protocol::Clock;
use tokio::sync::{Mutex, broadcast};

use super::Inbox;
use crate::AppError;
use crate::background::{Loops, Start};
use crate::ports::DomainEvents;

/// How often the events are read.
const EVERY: Duration = Duration::from_secs(2);

/// How long a failed run stays news: an older failure is not opened by the reconcile, and one
/// that is open is closed when it ages out.
const FAILED_WITHIN: chrono::Duration = chrono::Duration::days(7);

/// What a reconcile changed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Reconciled {
    pub opened: usize,
    pub closed: usize,
}

/// Applies one event to the inbox. An event about a hidden item (a thread's or a trigger's
/// anchor) is a profile's own work, not the board's, and opens nothing.
async fn apply(repo: &Repository, inbox: &Inbox, event: &DomainEvent) -> Result<(), AppError> {
    if let Some(notice) = notice(event)
        && !is_hidden(repo, &notice).await?
    {
        inbox.open(&notice).await?;
    }
    if let Some(key) = closes(event) {
        inbox.close(&key).await?;
    }
    Ok(())
}

async fn is_hidden(repo: &Repository, notice: &Notice) -> Result<bool, AppError> {
    match notice.item_id {
        Some(item) => Ok(repo.item_is_hidden(item).await?),
        None => Ok(false),
    }
}

/// Reads what the receiver holds, without waiting, and applies each event. `true` when the inbox
/// may have missed something and should be reconciled: the receiver fell behind or an event
/// could not be written.
pub async fn drain(
    repo: &Repository,
    inbox: &Inbox,
    receiver: &mut broadcast::Receiver<DomainEvent>,
) -> bool {
    let mut repair = false;
    loop {
        match receiver.try_recv() {
            Ok(event) => {
                if let Err(error) = apply(repo, inbox, &event).await {
                    tracing::warn!(%error, "notifications: an event could not be applied");
                    repair = true;
                }
            }
            Err(broadcast::error::TryRecvError::Lagged(missed)) => {
                tracing::warn!(missed, "notifications: fell behind and lost events");
                repair = true;
            }
            Err(_) => break,
        }
    }
    repair
}

fn run_ref(run: &WaitingRun) -> RunRef {
    RunRef {
        request_id: run.request_id.clone(),
        attempt_id: run.attempt_id.clone(),
        item_id: run.item_id,
    }
}

/// The notices for everything that waits on a person now, by the same rule the events go through.
async fn waiting(repo: &Repository, clock: &dyn Clock) -> Result<Vec<Notice>, AppError> {
    let mut facts = Vec::new();
    for decision in repo.pending_decisions().await? {
        facts.push((
            decision.run.project_id,
            Fact::DecisionOpened {
                run: run_ref(&decision.run),
                decision_id: decision.decision_id,
            },
        ));
    }
    for run in repo
        .unanswered_failed_runs(clock.now() - FAILED_WITHIN)
        .await?
    {
        facts.push((run.project_id, Fact::RunFailed { run: run_ref(&run) }));
    }
    for set in repo.waiting_change_sets().await? {
        facts.push((
            set.project_id,
            Fact::ChangeSetProposed {
                change_set_id: set.id.to_string(),
                item_id: set.anchor,
            },
        ));
    }
    let tack = Principal::system();
    let mut notices = Vec::new();
    for (project, fact) in facts {
        if let Some(notice) = notice(&DomainEvent::new(project, &tack, fact))
            && !is_hidden(repo, &notice).await?
        {
            notices.push(notice);
        }
    }
    Ok(notices)
}

/// Makes the needs-you notifications say what the store says: one open for each question, failed
/// run and change set that waits on a person, none for anything else. Idempotent: the key keeps a
/// situation from being opened twice.
pub async fn reconcile(
    repo: &Repository,
    inbox: &Inbox,
    clock: &dyn Clock,
) -> Result<Reconciled, AppError> {
    // What is open is read before the store is, so a notification an event opens in between is
    // never mistaken for one that no longer holds.
    let open = inbox.open_notifications(u32::MAX).await?;
    let wanted = waiting(repo, clock).await?;
    let mut done = Reconciled::default();
    for notice in &wanted {
        if inbox.open(notice).await?.is_some() {
            done.opened += 1;
        }
    }
    let holding: HashSet<&Key> = wanted.iter().map(|notice| &notice.key).collect();
    for notification in open {
        if notification.level == Level::NeedsYou
            && !holding.contains(&notification.key)
            && inbox.close(&notification.key).await?
        {
            done.closed += 1;
        }
    }
    Ok(done)
}

/// Adds the loop to the server's loops. The first pass reconciles; after it, a pass reconciles
/// only when the events could not be trusted.
pub fn spawn(
    loops: &mut Loops,
    repo: Repository,
    clock: Arc<dyn Clock>,
    inbox: Inbox,
    events: Arc<dyn DomainEvents>,
) {
    let receiver = events
        .subscribe()
        .map(|receiver| Arc::new(Mutex::new(receiver)));
    let repair = Arc::new(AtomicBool::new(true));
    loops.spawn_tick("notifications", Start::Immediately, EVERY, move || {
        let (repo, clock, inbox) = (repo.clone(), clock.clone(), inbox.clone());
        let (receiver, repair) = (receiver.clone(), repair.clone());
        async move {
            if let Some(receiver) = receiver
                && drain(&repo, &inbox, &mut *receiver.lock().await).await
            {
                repair.store(true, Ordering::Relaxed);
            }
            if repair.swap(false, Ordering::Relaxed) {
                match reconcile(&repo, &inbox, clock.as_ref()).await {
                    Ok(done) if done != Reconciled::default() => {
                        tracing::info!(
                            opened = done.opened,
                            closed = done.closed,
                            "notifications: the inbox was reconciled with the store"
                        );
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::warn!(%error, "notifications: the reconcile failed; it will be tried again");
                        repair.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
    });
}
