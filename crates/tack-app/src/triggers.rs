//! Triggers (ADR 0034, decision 8): what wakes a profile. [`observe`] keeps an event for every
//! trigger it belongs to; [`tick`] notices the change sets that have waited too long and starts
//! one run per trigger whose events went quiet (or whose schedule came), under the principal
//! `trigger:<id>`, with every event since its last run and the digest as the input. The rules
//! (what fires, what is an echo, the quiet period, the cap, the schedule) are
//! `tack_core::trigger`'s; this file asks the store and starts the run.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tack_core::models::Project;
use tack_core::principal::{Capability, may};
use tack_core::thread::ThreadScope;
use tack_core::trigger::{
    self, EventKind, Happened, KEEP_DAYS, Source, Trigger, burst_ready, cap_reached, fires,
    profile_labels, run_principal, schedule_due,
};
use tack_db::repo::triggers::{NewTrigger, TriggerChanges};
use tack_db::{Repository, queries};
use tack_protocol::Clock;
use tokio::sync::{Mutex, broadcast};
use uuid::Uuid;

use crate::AppError;
use crate::auto_start::{Permitted, permissions_for};
use crate::background::{Loops, Start};
use crate::events::{DomainEvent, Fact, RunRef};
use crate::ports::DomainEvents;
use crate::runs::start_run::{RunRequest, resolve_profile, start_run};

/// How far back the digest a run reads looks.
const DIGEST_WINDOW_HOURS: i64 = 24;

/// How often the triggers are looked at.
const EVERY: Duration = Duration::from_secs(5);

/// The cause of an event that time produced, not a principal.
const TIME: &str = "system:tack";

/// What makes a trigger.
pub struct OpenTrigger {
    pub scope: ThreadScope,
    pub profile: String,
    pub name: String,
    pub source: Source,
    pub quiet_seconds: u32,
    pub daily_cap: u32,
    pub enabled: bool,
}

/// What an edit changes; a field left `None` stays.
#[derive(Default)]
pub struct EditTrigger {
    pub name: Option<String>,
    pub profile: Option<String>,
    pub source: Option<Source>,
    pub quiet_seconds: Option<u32>,
    pub daily_cap: Option<u32>,
    pub enabled: Option<bool>,
}

async fn check_profile(repo: &Repository, profile: &str) -> Result<(), AppError> {
    if queries::agent_profiles::profile_exists(repo, profile).await? {
        Ok(())
    } else {
        Err(AppError::Invalid(format!(
            "the agent profile {profile} does not exist"
        )))
    }
}

/// Makes a trigger. The profile, the project and the credential must exist.
pub async fn create(
    repo: &Repository,
    clock: &dyn Clock,
    input: OpenTrigger,
) -> Result<Trigger, AppError> {
    if let Some(problem) = trigger::problem(
        &input.name,
        &input.source,
        input.quiet_seconds,
        input.daily_cap,
    ) {
        return Err(AppError::Invalid(problem));
    }
    let project = repo
        .get_project(input.scope.home())
        .await?
        .ok_or(AppError::NotFound)?;
    check_profile(repo, &input.profile).await?;
    if let Some(credential) = input.scope.credential()
        && repo.get_agent_credential(credential).await?.is_none()
    {
        return Err(AppError::Invalid(format!(
            "the credential {credential} does not exist"
        )));
    }
    let anchor_status = project.workflow.initial_status()?;
    Ok(repo
        .create_trigger(&NewTrigger {
            scope: input.scope,
            profile: &input.profile,
            name: input.name.trim(),
            source: &input.source,
            quiet_seconds: input.quiet_seconds,
            daily_cap: input.daily_cap,
            enabled: input.enabled,
            anchor_status: &anchor_status,
            now: clock.now(),
        })
        .await?)
}

/// Edits a trigger; what is not named stays.
pub async fn edit(repo: &Repository, id: Uuid, changes: EditTrigger) -> Result<Trigger, AppError> {
    let current = repo.get_trigger(id).await?.ok_or(AppError::NotFound)?;
    let name = changes.name.as_deref().unwrap_or(&current.name);
    let source = changes.source.as_ref().unwrap_or(&current.source);
    let quiet = changes.quiet_seconds.unwrap_or(current.quiet_seconds);
    let cap = changes.daily_cap.unwrap_or(current.daily_cap);
    if let Some(problem) = trigger::problem(name, source, quiet, cap) {
        return Err(AppError::Invalid(problem));
    }
    if let Some(profile) = changes.profile.as_deref() {
        check_profile(repo, profile).await?;
    }
    repo.update_trigger(
        id,
        &TriggerChanges {
            name: changes.name.as_deref().map(str::trim),
            profile: changes.profile.as_deref(),
            source: changes.source.as_ref(),
            quiet_seconds: changes.quiet_seconds,
            daily_cap: changes.daily_cap,
            enabled: changes.enabled,
        },
    )
    .await?
    .ok_or(AppError::NotFound)
}

/// Deletes a trigger with what it kept.
pub async fn delete(repo: &Repository, id: Uuid) -> Result<(), AppError> {
    if repo.delete_trigger(id).await? {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

/// What an item is called, for a line in a run's input.
async fn title_of(repo: &Repository, item: Uuid) -> String {
    match repo.get_item(item).await {
        Ok(Some(item)) => item.title,
        _ => String::new(),
    }
}

/// The event as a trigger sees it, or `None` for one no trigger can wait for: a run of a
/// thread's or a trigger's hidden item is the profile's own work, not the board's.
async fn happened_of(
    repo: &Repository,
    clock: &dyn Clock,
    event: &DomainEvent,
) -> Result<Option<Happened>, AppError> {
    let run = |run: &RunRef| Some(run.item_id);
    let (kind, status, subject, item) = match &event.fact {
        Fact::ItemCreated { item_id, status } => (
            EventKind::ItemCreated,
            Some(status.clone()),
            format!("item {item_id}"),
            Some(*item_id),
        ),
        Fact::ItemUpdated { item_id, status } => (
            EventKind::ItemUpdated,
            Some(status.clone()),
            format!("item {item_id}"),
            Some(*item_id),
        ),
        Fact::ItemMoved { item_id, to, .. } => (
            EventKind::ItemMoved,
            Some(to.clone()),
            format!("item {item_id}"),
            Some(*item_id),
        ),
        Fact::ItemDeleted { item_id } => (
            EventKind::ItemDeleted,
            None,
            format!("item {item_id}"),
            Some(*item_id),
        ),
        Fact::RunStarted { run: r } => (
            EventKind::RunStarted,
            None,
            format!("run {}", r.request_id),
            run(r),
        ),
        Fact::RunFinished { run: r } => (
            EventKind::RunFinished,
            None,
            format!("run {}", r.request_id),
            run(r),
        ),
        Fact::RunFailed { run: r } => (
            EventKind::RunFailed,
            None,
            format!("run {}", r.request_id),
            run(r),
        ),
        Fact::RunWaiting { run: r } => (
            EventKind::RunWaiting,
            None,
            format!("run {}", r.request_id),
            run(r),
        ),
        Fact::DecisionOpened {
            run: r,
            decision_id,
        } => (
            EventKind::DecisionOpened,
            None,
            format!("decision {decision_id}"),
            run(r),
        ),
        Fact::DecisionResolved {
            run: r,
            decision_id,
        } => (
            EventKind::DecisionResolved,
            None,
            format!("decision {decision_id}"),
            run(r),
        ),
        Fact::ChangeSetProposed {
            change_set_id,
            item_id,
        } => (
            EventKind::ChangeSetProposed,
            None,
            format!("change set {change_set_id}"),
            *item_id,
        ),
        Fact::ChangeSetSettled { change_set_id } => (
            EventKind::ChangeSetSettled,
            None,
            format!("change set {change_set_id}"),
            None,
        ),
        // A summary of runs the triggers have already seen one by one.
        Fact::ChildrenFinished { .. } => return Ok(None),
    };
    if let Some(item) = item
        && repo.item_is_hidden(item).await?
    {
        return Ok(None);
    }
    let title = match item {
        Some(item) => title_of(repo, item).await,
        None => String::new(),
    };
    let detail = match &event.fact {
        Fact::ItemMoved { from, to, .. } => format!("\"{title}\" moved from {from} to {to}"),
        _ if title.is_empty() => String::new(),
        _ => format!("\"{title}\""),
    };
    Ok(Some(Happened {
        kind,
        project: event.project_id,
        cause: event.principal.label(),
        at: clock.now(),
        status,
        subject,
        detail,
    }))
}

/// What a pass of one trigger needs to know about the profile and the scope.
struct Lookup<'a> {
    repo: &'a Repository,
    clock: &'a dyn Clock,
    triggers: Vec<Trigger>,
    own: HashMap<String, Vec<String>>,
}

impl<'a> Lookup<'a> {
    async fn new(repo: &'a Repository, clock: &'a dyn Clock) -> Result<Lookup<'a>, AppError> {
        Ok(Lookup {
            repo,
            clock,
            triggers: repo.list_triggers(None).await?,
            own: HashMap::new(),
        })
    }

    /// The labels the profile works under: the credentials its triggers act as, and its
    /// triggers and threads.
    async fn own(&mut self, profile: &str) -> Result<Vec<String>, AppError> {
        if let Some(known) = self.own.get(profile) {
            return Ok(known.clone());
        }
        let labels = profile_labels(
            self.repo.credentials_of_profile_triggers(profile).await?,
            self.triggers
                .iter()
                .filter(|trigger| trigger.profile == profile)
                .map(|trigger| trigger.id),
            self.repo.thread_ids_of_profile(profile).await?,
        );
        self.own.insert(profile.to_owned(), labels.clone());
        Ok(labels)
    }

    /// Whether the trigger's scope covers `project`.
    async fn reaches(&self, scope: &ThreadScope, project: Uuid) -> Result<bool, AppError> {
        let Some(credential) = scope.credential() else {
            return Ok(scope.home() == project);
        };
        let Some(credential) = self.repo.get_agent_credential(credential).await? else {
            return Ok(false);
        };
        Ok(
            credential.state(self.clock.now()) == tack_core::principal::CredentialState::Active
                && may(&credential.principal(), project, Capability::Read),
        )
    }

    /// Keeps `happened` for `trigger` when the rules say it belongs there.
    async fn deliver(
        &mut self,
        trigger: &Trigger,
        happened: &Happened,
        dedupe_key: Option<&str>,
    ) -> Result<bool, AppError> {
        let own = self.own(&trigger.profile).await?;
        let reaches = self.reaches(&trigger.scope, happened.project).await?;
        if !fires(trigger, happened, &own, reaches) {
            return Ok(false);
        }
        Ok(self
            .repo
            .record_trigger_event(trigger.id, happened, dedupe_key)
            .await?)
    }
}

/// Keeps `event` for every trigger it belongs to, and returns how many kept it.
pub async fn observe(
    repo: &Repository,
    clock: &dyn Clock,
    event: &DomainEvent,
) -> Result<usize, AppError> {
    let mut lookup = Lookup::new(repo, clock).await?;
    if lookup.triggers.iter().all(|trigger| !trigger.enabled) {
        return Ok(0);
    }
    let Some(happened) = happened_of(repo, clock, event).await? else {
        return Ok(0);
    };
    let mut kept = 0;
    for trigger in lookup.triggers.clone() {
        if lookup.deliver(&trigger, &happened, None).await? {
            kept += 1;
        }
    }
    Ok(kept)
}

/// Keeps, once each, the change sets that have waited longer than a trigger's filter says.
async fn notice_waiting_change_sets(lookup: &mut Lookup<'_>) -> Result<(), AppError> {
    let now = lookup.clock.now();
    for trigger in lookup.triggers.clone() {
        let Source::Event {
            event: EventKind::ChangeSetWaiting,
            filter,
        } = &trigger.source
        else {
            continue;
        };
        if !trigger.enabled {
            continue;
        }
        let after = i64::from(filter.after_seconds.unwrap_or(0));
        let cutoff = now - chrono::Duration::seconds(after);
        for (id, project, proposed) in lookup.repo.change_sets_waiting_since(cutoff).await? {
            let happened = Happened {
                kind: EventKind::ChangeSetWaiting,
                project,
                cause: TIME.into(),
                at: now,
                status: None,
                subject: format!("change set {id}"),
                detail: format!(
                    "proposed {} and not decided yet",
                    proposed.format("%Y-%m-%d %H:%M UTC")
                ),
            };
            lookup
                .deliver(&trigger, &happened, Some(&format!("change_set:{id}")))
                .await?;
        }
    }
    Ok(())
}

/// Starts the run of `trigger`, and returns its request id.
async fn begin_run(
    repo: &Repository,
    clock: &dyn Clock,
    trigger: &Trigger,
    events: &[Happened],
) -> Result<String, String> {
    let now = clock.now();
    let internal = |what: &'static str| {
        move |failure: &dyn std::fmt::Display| {
            tracing::error!(error = %failure, "trigger: {what}");
            format!("{what}; the board's log says why")
        }
    };
    let anchor = repo
        .trigger_anchor(trigger.id)
        .await
        .map_err(|e| internal("could not read the trigger")(&e))?
        .ok_or("the trigger has no item to run against")?;
    let item = repo
        .get_item(anchor)
        .await
        .map_err(|e| internal("could not read the trigger's item")(&e))?
        .ok_or("the trigger has no item to run against")?;
    let home: Project = repo
        .get_project(trigger.scope.home())
        .await
        .map_err(|e| internal("could not read the project")(&e))?
        .ok_or("the trigger's project is gone")?;
    let (projects, outside_board) = crate::threads::digest_projects(repo, &trigger.scope, &home)
        .await
        .map_err(|e| internal("could not tell which projects the trigger reaches")(&e))?;
    let digest = crate::digest::read(
        repo,
        clock,
        &projects,
        now - chrono::Duration::hours(DIGEST_WINDOW_HOURS),
        outside_board,
    )
    .await
    .map_err(|e| internal("could not compute the digest")(&e))?;
    let digest = serde_json::to_string(&digest).unwrap_or_else(|_| "{}".into());
    repo.set_anchor_text(anchor, &trigger::run_input(trigger, events, &digest), now)
        .await
        .map_err(|e| internal("could not write the run's input")(&e))?;

    let resolved = resolve_profile(repo, &item, &home, Some(&trigger.profile))
        .await
        .map_err(|refusal| refusal.message)?
        .ok_or("the trigger's agent profile cannot be resolved")?;
    let Permitted { permissions, .. } = permissions_for(repo, &home, &resolved).await;
    let request = RunRequest {
        item_id: anchor,
        idempotency_key: format!("trigger:{}:{}", trigger.id, now.timestamp_millis()),
        selector: None,
        agent_profile_id: Some(trigger.profile.clone()),
        requested_harness_kind: None,
        requested_model: None,
        agent_profile_snapshot: None,
        repository_snapshot: None,
        permission_policy: permissions,
        budgets: json!({}),
        environment: json!({}),
        metadata: json!({"started_by": "trigger", "trigger": trigger.id.to_string()}),
        timeout_seconds: None,
        verify: None,
        push_branch: None,
        autonomy: None,
        origin: None,
        from_run: None,
    };
    let started = start_run(repo, now, &run_principal(trigger.id), request)
        .await
        .map_err(|refusal| refusal.message)?;
    Ok(started.request_id)
}

/// Whether the trigger's last run has not ended.
async fn running(repo: &Repository, trigger: &Trigger) -> Result<bool, AppError> {
    let Some(request) = repo.last_trigger_request(trigger.id).await? else {
        return Ok(false);
    };
    Ok(queries::execution_requests::request_state(repo, &request)
        .await?
        .is_some_and(|state| !state.is_terminal()))
}

/// One pass: notices the change sets that waited too long, starts the run of every trigger
/// that is ready, and forgets old events. Returns how many runs it started.
pub async fn tick(repo: &Repository, clock: &dyn Clock) -> Result<usize, AppError> {
    let mut lookup = Lookup::new(repo, clock).await?;
    notice_waiting_change_sets(&mut lookup).await?;
    let now = clock.now();
    let mut started = 0;
    for trigger in lookup.triggers.clone() {
        if !trigger.enabled || running(repo, &trigger).await? {
            continue;
        }
        let events = repo.pending_trigger_events(trigger.id).await?;
        let ready = match &trigger.source {
            Source::Event { .. } => match (events.first(), events.last()) {
                (Some(oldest), Some(newest)) => {
                    burst_ready(oldest.at, newest.at, trigger.quiet_seconds, now)
                }
                _ => false,
            },
            Source::Schedule(schedule) => schedule_due(
                schedule,
                trigger.last_fired_at.unwrap_or(trigger.created_at),
                now,
            ),
        };
        if !ready {
            continue;
        }
        let today = repo
            .trigger_firings_since(trigger.id, now - chrono::Duration::hours(24))
            .await?;
        if cap_reached(trigger.daily_cap, today) {
            continue;
        }
        match begin_run(repo, clock, &trigger, &events).await {
            Ok(request) => {
                repo.record_trigger_firing(trigger.id, &request, now)
                    .await?;
                started += 1;
            }
            Err(why) => {
                // Counts toward the cap, so a profile that cannot start is not retried forever.
                tracing::warn!(trigger = %trigger.id, %why, "trigger: the run could not start");
                repo.record_trigger_firing(trigger.id, "", now).await?;
            }
        }
    }
    repo.prune_trigger_events(now - chrono::Duration::days(KEEP_DAYS))
        .await?;
    Ok(started)
}

/// Reads what the receiver holds, without waiting, and keeps each event for its triggers.
pub async fn drain(
    repo: &Repository,
    clock: &dyn Clock,
    receiver: &mut broadcast::Receiver<DomainEvent>,
) {
    loop {
        match receiver.try_recv() {
            Ok(event) => {
                if let Err(error) = observe(repo, clock, &event).await {
                    tracing::warn!(%error, "triggers: an event could not be kept");
                }
            }
            Err(broadcast::error::TryRecvError::Lagged(missed)) => {
                tracing::warn!(missed, "triggers: fell behind and lost events");
            }
            Err(_) => break,
        }
    }
}

/// Adds the loop to the server's loops. A channel nobody can listen to leaves only the
/// schedules and the waiting change sets to the loop.
pub fn spawn(
    loops: &mut Loops,
    repo: Repository,
    clock: Arc<dyn Clock>,
    events: Arc<dyn DomainEvents>,
) {
    let receiver = events
        .subscribe()
        .map(|receiver| Arc::new(Mutex::new(receiver)));
    loops.spawn_tick("triggers", Start::AfterInterval, EVERY, move || {
        let (repo, clock, receiver) = (repo.clone(), clock.clone(), receiver.clone());
        async move {
            if let Some(receiver) = receiver {
                drain(&repo, clock.as_ref(), &mut *receiver.lock().await).await;
            }
            if let Err(error) = tick(&repo, clock.as_ref()).await {
                tracing::warn!(%error, "triggers: a pass failed");
            }
        }
    });
}
