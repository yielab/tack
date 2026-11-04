//! Feeds the inbox from the board's events and keeps it true to the store (ADR 0035, decision 3).
//! [`drain`] applies the rule of `tack_core::notification` to what the bus carried: a fact that
//! calls for a notification opens it, a fact that answers one closes it. The bus is in memory and
//! lossy, so [`reconcile`] reads what waits on a person from the store at the start, after the
//! bus said it dropped events, and after a write failed: it opens what has no notification and
//! closes what no longer waits. That is also what closes a failed run that was run again under a
//! new request, which no single fact names.
//!
//! What a team waits to have approved for its memory is no board fact: the team reports it. The
//! runner sends its report when it changes, and checks for a change every five minutes at most, so
//! [`reconcile_memory`] reads the reports the board already holds (the snapshots of the active
//! runners, the read behind the list of teams) every [`MEMORY_EVERY`] passes. A person learns of an
//! entry within the runner's check plus that pass, and the notification closes the same way.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;
use uuid::Uuid;

use tack_core::event::{DomainEvent, Fact, RunRef};
use tack_core::notification::{Key, Kind, Level, Notice, closes, memory_waiting, notice};
use tack_core::principal::Principal;
use tack_core::run_defaults::named_team;
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

/// How many passes lie between two reads of the teams' reports: thirty seconds, against a report
/// the runner refreshes every five minutes.
const MEMORY_EVERY: u32 = 15;

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

/// What the active runners report of one team: whose report it is and what it says of memory.
struct ReportedTeam {
    name: String,
    /// The agent that runs it.
    kind: String,
    runner: String,
    reported_at: chrono::DateTime<chrono::Utc>,
    memory: Option<tack_protocol::execution::TeamMemory>,
}

/// The newest report of each team by agent and name, from the capability snapshots of the active
/// runners: the read `GET /api/teams` makes.
async fn reported_teams(repo: &Repository) -> Result<Vec<ReportedTeam>, AppError> {
    let mut newest: BTreeMap<(String, String), ReportedTeam> = BTreeMap::new();
    for runner in repo.list_runners().await? {
        if runner.state != "active" || runner.revoked_at.is_some() {
            continue;
        }
        for harness in crate::autonomy::harnesses_of(&runner.capability_snapshot) {
            let kind = harness.harness_kind.as_str().to_owned();
            for team in harness.teams.into_iter().flatten() {
                let key = (kind.clone(), team.name.clone());
                if newest
                    .get(&key)
                    .is_some_and(|seen| seen.reported_at > harness.probed_at)
                {
                    continue;
                }
                newest.insert(
                    key,
                    ReportedTeam {
                        name: team.name,
                        kind: kind.clone(),
                        runner: runner.id.clone(),
                        reported_at: harness.probed_at,
                        memory: team.memory,
                    },
                );
            }
        }
    }
    Ok(newest.into_values().collect())
}

/// The projects that use each team, from the profiles their roles and their default name
/// (`run_defaults::named_team`, the join the list of teams makes). A profile that names an agent
/// uses only that agent's team.
async fn team_users(
    repo: &Repository,
    teams: &[ReportedTeam],
) -> Result<HashSet<(Uuid, String, String)>, AppError> {
    let mut users = HashSet::new();
    for (project, policy) in repo.project_profile_policies().await? {
        let policy: serde_json::Value = serde_json::from_str(&policy).unwrap_or_default();
        let Some((name, agent)) = named_team(&policy) else {
            continue;
        };
        for team in teams.iter().filter(|team| team.name == name) {
            if agent.as_ref().is_none_or(|agent| *agent == team.kind) {
                users.insert((project, team.kind.clone(), name.clone()));
            }
        }
    }
    Ok(users)
}

/// Makes the needs-you notifications for what teams wait to have approved say what the teams
/// report: for each project that uses a team, one open for each pending entry, none for an entry
/// that is no longer pending, for a team that no longer reports or for a project that no longer
/// uses the team. A team no project uses opens nothing. A team whose report carries no memory (its
/// docket does not offer it, or the read failed) says nothing, so what is open for it stays.
/// Idempotent, by the key.
pub async fn reconcile_memory(repo: &Repository, inbox: &Inbox) -> Result<Reconciled, AppError> {
    let open: Vec<_> = inbox
        .open_notifications(u32::MAX)
        .await?
        .into_iter()
        .filter(|notification| notification.kind == Kind::Memory)
        .collect();
    let teams = reported_teams(repo).await?;
    let mut done = Reconciled::default();
    let mut holding: HashSet<Key> = HashSet::new();
    let users = team_users(repo, &teams).await?;
    for team in &teams {
        for entry in team.memory.iter().flat_map(|memory| &memory.pending) {
            for (project, ..) in users
                .iter()
                .filter(|(_, kind, name)| *kind == team.kind && *name == team.name)
            {
                let notice = memory_waiting(
                    *project,
                    &team.name,
                    &entry.id,
                    &entry.subject,
                    &team.runner,
                );
                holding.insert(notice.key.clone());
                if inbox.open(&notice).await?.is_some() {
                    done.opened += 1;
                }
            }
        }
    }
    for notification in open {
        let silent = notification
            .key
            .memory_use()
            .is_some_and(|(project, name)| {
                users
                    .iter()
                    .any(|(user, _, team)| *user == project && team == name)
                    && !teams
                        .iter()
                        .any(|team| team.name == name && team.memory.is_some())
            });
        if !holding.contains(&notification.key) && !silent && inbox.close(&notification.key).await?
        {
            done.closed += 1;
        }
    }
    Ok(done)
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
            && notification.kind != Kind::Memory
            && !holding.contains(&notification.key)
            && inbox.close(&notification.key).await?
        {
            done.closed += 1;
        }
    }
    let memory = reconcile_memory(repo, inbox).await?;
    done.opened += memory.opened;
    done.closed += memory.closed;
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
    let passes = Arc::new(AtomicU32::new(0));
    loops.spawn_tick("notifications", Start::Immediately, EVERY, move || {
        let (repo, clock, inbox) = (repo.clone(), clock.clone(), inbox.clone());
        let (receiver, repair, passes) = (receiver.clone(), repair.clone(), passes.clone());
        async move {
            if let Some(receiver) = receiver
                && drain(&repo, &inbox, &mut *receiver.lock().await).await
            {
                repair.store(true, Ordering::Relaxed);
            }
            let memory_due = passes.fetch_add(1, Ordering::Relaxed) % MEMORY_EVERY == 0;
            if !repair.load(Ordering::Relaxed) && memory_due {
                match reconcile_memory(&repo, &inbox).await {
                    Ok(done) if done != Reconciled::default() => {
                        tracing::info!(
                            opened = done.opened,
                            closed = done.closed,
                            "notifications: what teams wait to learn was read"
                        );
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::warn!(%error, "notifications: the teams' memory could not be read; it will be tried again");
                    }
                }
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
