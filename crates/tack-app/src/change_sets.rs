//! Change sets (ADR 0034, decisions 3 and 4): propose one, let the project's rung apply what it
//! allows, and let a person apply, edit or reject the rest. Every set is checked whole before
//! anything is written and applied in one transaction, so what is refused is refused with every
//! reason and what fails halfway leaves nothing.
//!
//! The routes of the person and the tools of an agent both end here.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use tack_core::change_set::{
    BoardView, ChangeSet, ChangeSetStatus, ExistingItem, ItemRef, OpState, Operation, Origin,
    Verdict, ceiling_of, decide, describe, validate,
};
use tack_core::event::{DomainEvent, Fact};
use tack_core::item_types::Level;
use tack_core::models::{DependencyType, Project};
use tack_core::principal::Principal;
use tack_db::Repository;
use tack_db::queries::agent_profiles::profiles_by_name;
use tack_db::repo::change_sets::{ApplyError, ApplyPlan, NewChangeSet};
use tack_protocol::Clock;
use uuid::Uuid;

use crate::AppError;
use crate::ports::DomainEvents;

/// The longest reason a change set carries.
const MAX_REASON: usize = 2_000;

/// The longest idempotency key.
const MAX_KEY: usize = 200;

/// How a proposal is decided.
#[derive(Debug, Clone, Copy)]
pub enum Mode {
    /// The project's rung and the proposer's scopes decide, operation by operation.
    Rung,
    /// A person's own plan: everything applies at once, or nothing does.
    Now,
}

/// What proposes a change set.
#[derive(Debug, Clone)]
pub struct Proposal<'a> {
    pub project: Uuid,
    pub principal: &'a Principal,
    pub origin: Origin,
    /// The run or thread it came from.
    pub origin_ref: Option<String>,
    pub reason: String,
    pub idempotency_key: Option<String>,
    /// The item a plan's entries hang under (its type is the fallback type of an entry).
    pub anchor: Option<Uuid>,
    pub artifact_id: Option<String>,
    pub operations: Vec<Operation>,
    pub mode: Mode,
}

/// What a proposal came to.
#[derive(Debug, Clone)]
pub struct Proposed {
    pub change_set: ChangeSet,
    /// False when the idempotency key had been used: this is the first answer again.
    pub made: bool,
    /// The positions of the operations that applied at once.
    pub applied_now: Vec<usize>,
}

/// What a person chooses when applying.
#[derive(Debug, Clone, Default)]
pub struct Application {
    /// The positions to apply; absent means everything that waits.
    pub operations: Option<Vec<usize>>,
    /// Operations that replace the stored ones, by position.
    pub edits: Vec<(usize, Operation)>,
}

fn invalid(error: impl std::fmt::Display) -> AppError {
    AppError::Invalid(error.to_string())
}

fn apply_failure(error: ApplyError) -> AppError {
    match error {
        ApplyError::Store(error) => AppError::Store(error),
        ApplyError::Refused(rule) => AppError::Core(rule),
        ApplyError::Conflict(message) => AppError::Conflict(message),
    }
}

/// The team a profile names, from its tool policy text.
fn team_of(tool_policy: &str) -> Option<String> {
    let policy: serde_json::Value = serde_json::from_str(tool_policy).unwrap_or_default();
    tack_core::run_defaults::named_team(&policy).map(|(team, _)| team)
}

/// Every id the operations name, plus the ones already made.
fn existing_refs(operations: &[Operation], made: impl Iterator<Item = Uuid>) -> HashSet<Uuid> {
    let mut found: HashSet<Uuid> = made.collect();
    let mut note = |item: &ItemRef| {
        if let ItemRef::Existing(id) = item {
            found.insert(*id);
        }
    };
    for operation in operations {
        match operation {
            Operation::Create(create) => {
                create.parent.iter().for_each(&mut note);
                create.depends_on.iter().for_each(&mut note);
            }
            Operation::WhoDoesIt { item, .. }
            | Operation::Priority { item, .. }
            | Operation::Move { item, .. }
            | Operation::Comment { item, .. }
            | Operation::Due { item, .. } => note(item),
            Operation::Depend { blocker, blocked } => {
                note(blocker);
                note(blocked);
            }
        }
    }
    found
}

/// Reads the facts `operations` are checked against.
async fn board_view(
    repo: &Repository,
    project: &Project,
    operations: &[Operation],
    anchor: Option<Uuid>,
    made: impl Iterator<Item = Uuid>,
) -> Result<BoardView, AppError> {
    let mut items = HashMap::new();
    for id in existing_refs(operations, made) {
        if let Some(item) = repo.get_item(id).await?
            && item.project_id == project.id
        {
            items.insert(
                id,
                ExistingItem {
                    status: item.status,
                },
            );
        }
    }
    let anchor_type = match anchor {
        // Work hangs its siblings' type on them; under a group the creates take the default work type.
        Some(id) => repo.get_item(id).await?.and_then(|item| {
            project
                .item_types
                .iter()
                .find(|t| t.key == item.item_type && t.level == Level::Work)
                .map(|t| t.key.clone())
        }),
        None => None,
    };
    let edges = repo
        .list_dependencies_for_project(project.id)
        .await?
        .into_iter()
        .filter(|edge| {
            matches!(
                edge.dependency_type,
                DependencyType::Blocks | DependencyType::IsBlockedBy
            )
        })
        .map(|edge| (edge.source_item_id, edge.target_item_id))
        .collect();
    let mut profiles = HashSet::new();
    let mut profiles_by_team: HashMap<String, Vec<String>> = HashMap::new();
    for profile in profiles_by_name(repo).await? {
        if let Some(team) = team_of(&profile.tool_policy) {
            profiles_by_team
                .entry(team)
                .or_default()
                .push(profile.id.clone());
        }
        profiles.insert(profile.id);
    }
    Ok(BoardView {
        workflow: project.workflow.clone(),
        item_types: project.item_types.clone(),
        items,
        occupied: repo.item_status_counts(project.id).await?,
        edges,
        roles: repo
            .list_roles(project.id)
            .await?
            .into_iter()
            .map(|role| (role.name, role.id))
            .collect(),
        profiles,
        profiles_by_team,
        anchor_type,
    })
}

/// The start of the UTC day `now` is in, where the day's cap on creates counts from.
fn start_of_day(now: DateTime<Utc>) -> DateTime<Utc> {
    now.date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight exists")
        .and_utc()
}

/// Whether any operation of `set` still waits for a person.
fn waits(set: &ChangeSet) -> bool {
    set.ops.iter().any(|op| op.state == OpState::Waiting)
}

/// Tells the listeners that `set` waits for a person, when something in it does. A set the rung
/// let through whole never waited, so it is not announced.
fn announce_proposed(events: &dyn DomainEvents, proposer: &Principal, set: &ChangeSet) {
    if waits(set) {
        events.emit(DomainEvent::new(
            set.project,
            proposer,
            Fact::ChangeSetProposed {
                change_set_id: set.id.to_string(),
                item_id: set.anchor,
            },
        ));
    }
}

/// Tells the listeners that `set` waits for nobody any more, once a decision left nothing waiting.
fn announce_settled(events: &dyn DomainEvents, decider: &Principal, set: &ChangeSet) {
    if !waits(set) {
        events.emit(DomainEvent::new(
            set.project,
            decider,
            Fact::ChangeSetSettled {
                change_set_id: set.id.to_string(),
            },
        ));
    }
}

/// Keeps a change set and applies what the rung (or a person's own plan) lets through. A set
/// that fails any check is refused whole, with every reason, and kept nowhere. The same
/// idempotency key on a project answers the first change set again.
pub async fn propose(
    repo: &Repository,
    clock: &dyn Clock,
    events: &dyn DomainEvents,
    proposal: Proposal<'_>,
) -> Result<Proposed, AppError> {
    let project = repo
        .get_project(proposal.project)
        .await?
        .ok_or(AppError::NotFound)?;
    let reason = proposal.reason.trim();
    if reason.is_empty() || reason.chars().count() > MAX_REASON {
        return Err(AppError::Invalid(format!(
            "give a reason of 1 to {MAX_REASON} characters"
        )));
    }
    if let Some(key) = proposal.idempotency_key.as_deref() {
        if key.is_empty() || key.chars().count() > MAX_KEY {
            return Err(AppError::Invalid(format!(
                "the idempotency key is 1 to {MAX_KEY} characters"
            )));
        }
        if let Some(first) = repo.get_change_set_by_key(project.id, key).await? {
            return Ok(Proposed {
                change_set: first,
                made: false,
                applied_now: Vec::new(),
            });
        }
    }
    let now = clock.now();
    let all: Vec<usize> = (0..proposal.operations.len()).collect();
    let board = board_view(
        repo,
        &project,
        &proposal.operations,
        proposal.anchor,
        std::iter::empty(),
    )
    .await?;
    let resolutions = validate(&proposal.operations, &all, &board, &HashMap::new())
        .map_err(|problems| invalid(describe(&problems)))?;

    let verdicts = match proposal.mode {
        Mode::Now => vec![Verdict::Apply; proposal.operations.len()],
        Mode::Rung => {
            let today = repo
                .auto_creates_since(project.id, start_of_day(now))
                .await?;
            decide(
                &project.agents_on_board,
                ceiling_of(proposal.principal, project.id),
                &proposal.operations,
                today,
            )
            .map_err(AppError::Invalid)?
        }
    };
    let (kept, made) = repo
        .insert_change_set(&NewChangeSet {
            project: project.id,
            principal: &proposal.principal.label(),
            origin: proposal.origin,
            origin_ref: proposal.origin_ref.as_deref(),
            reason,
            idempotency_key: proposal.idempotency_key.as_deref(),
            anchor: proposal.anchor,
            artifact_id: proposal.artifact_id.as_deref(),
            operations: proposal.operations.clone(),
            now,
        })
        .await?;
    if !made {
        return Ok(Proposed {
            change_set: kept,
            made: false,
            applied_now: Vec::new(),
        });
    }
    let selected: Vec<usize> = verdicts
        .iter()
        .enumerate()
        .filter(|(_, verdict)| **verdict == Verdict::Apply)
        .map(|(index, _)| index)
        .collect();
    if selected.is_empty() {
        announce_proposed(events, proposal.principal, &kept);
        return Ok(Proposed {
            change_set: kept,
            made: true,
            applied_now: selected,
        });
    }
    let auto_creates = match proposal.mode {
        Mode::Now => 0,
        Mode::Rung => selected
            .iter()
            .filter(|&&index| matches!(proposal.operations[index], Operation::Create(_)))
            .count() as u32,
    };
    let applied = repo
        .apply_change_set(
            kept.id,
            &ApplyPlan {
                workflow: &project.workflow,
                selected: &selected,
                edits: &[],
                resolutions: &resolutions,
                writer: &kept.principal,
                author: kept.principal_name.as_deref(),
                decided_by: &kept.principal,
                now,
                auto_creates,
            },
        )
        .await;
    match applied {
        Ok(change_set) => {
            announce_proposed(events, proposal.principal, &change_set);
            Ok(Proposed {
                change_set,
                made: true,
                applied_now: selected,
            })
        }
        Err(error) => {
            // Refused whole: a proposal that cannot land leaves no change set behind.
            repo.discard_change_set(kept.id).await?;
            Err(apply_failure(error))
        }
    }
}

/// Applies a waiting change set, all of what waits or the operations chosen, with the edits
/// given. Checked against the board as it is now, so a set that has gone stale is refused
/// whole with every reason and stays as it was.
pub async fn apply(
    repo: &Repository,
    clock: &dyn Clock,
    events: &dyn DomainEvents,
    id: Uuid,
    actor: &Principal,
    application: Application,
) -> Result<ChangeSet, AppError> {
    let set = repo.get_change_set(id).await?.ok_or(AppError::NotFound)?;
    let project = repo
        .get_project(set.project)
        .await?
        .ok_or(AppError::NotFound)?;
    let selected = match application.operations {
        Some(chosen) => chosen,
        None => set
            .ops
            .iter()
            .enumerate()
            .filter(|(_, op)| op.state == OpState::Waiting)
            .map(|(index, _)| index)
            .collect(),
    };
    if selected.is_empty() {
        return Err(AppError::Conflict(
            "nothing waits in this change set".into(),
        ));
    }
    let mut operations: Vec<Operation> = set.ops.iter().map(|op| op.operation.clone()).collect();
    for (index, operation) in &application.edits {
        let slot = operations
            .get_mut(*index)
            .ok_or_else(|| AppError::Invalid(format!("the change set has no operation {index}")))?;
        *slot = operation.clone();
    }
    let mut seen = HashSet::new();
    for &index in &selected {
        match set.ops.get(index) {
            None => {
                return Err(AppError::Invalid(format!(
                    "the change set has no operation {index}"
                )));
            }
            Some(op) if op.state != OpState::Waiting => {
                return Err(AppError::Conflict(format!(
                    "operation {index} is not waiting"
                )));
            }
            Some(_) if !seen.insert(index) => {
                return Err(AppError::Invalid(format!(
                    "operation {index} is listed twice"
                )));
            }
            Some(_) => {}
        }
    }
    let made: HashMap<usize, Uuid> = set
        .ops
        .iter()
        .enumerate()
        .filter(|(_, op)| op.state == OpState::Applied)
        .filter_map(|(index, op)| Some((index, op.result.as_ref()?.item?)))
        .collect();
    let board = board_view(
        repo,
        &project,
        &operations,
        set.anchor,
        made.values().copied(),
    )
    .await?;
    let resolutions = validate(&operations, &selected, &board, &made)
        .map_err(|problems| invalid(describe(&problems)))?;
    let applied = repo
        .apply_change_set(
            id,
            &ApplyPlan {
                workflow: &project.workflow,
                selected: &selected,
                edits: &application.edits,
                resolutions: &resolutions,
                writer: &set.principal,
                author: set.principal_name.as_deref(),
                decided_by: &actor.label(),
                now: clock.now(),
                auto_creates: 0,
            },
        )
        .await
        .map_err(apply_failure)?;
    announce_settled(events, actor, &applied);
    Ok(applied)
}

/// Turns down what still waits in a change set, with a reason.
pub async fn reject(
    repo: &Repository,
    clock: &dyn Clock,
    events: &dyn DomainEvents,
    id: Uuid,
    actor: &Principal,
    reason: &str,
) -> Result<ChangeSet, AppError> {
    let reason = reason.trim();
    if reason.is_empty() || reason.chars().count() > MAX_REASON {
        return Err(AppError::Invalid(format!(
            "give a reason of 1 to {MAX_REASON} characters"
        )));
    }
    repo.get_change_set(id).await?.ok_or(AppError::NotFound)?;
    let rejected = repo
        .reject_change_set(id, reason, &actor.label(), clock.now())
        .await
        .map_err(apply_failure)?;
    announce_settled(events, actor, &rejected);
    Ok(rejected)
}

/// Replaces one waiting operation with another of its kind. The edit is not checked against the
/// board here: applying checks everything that is applied, with every reason, and a person is
/// often editing the very operation that no longer fits.
pub async fn edit_operation(
    repo: &Repository,
    id: Uuid,
    index: usize,
    operation: Operation,
) -> Result<ChangeSet, AppError> {
    repo.get_change_set(id).await?.ok_or(AppError::NotFound)?;
    repo.edit_change_set_operation(id, index, operation)
        .await
        .map_err(apply_failure)
}

/// The statuses a listing asks for, read from their words.
pub fn parse_statuses(text: &str) -> Result<Vec<ChangeSetStatus>, AppError> {
    text.split(',')
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .map(|word| {
            word.parse().map_err(|()| {
                AppError::Invalid(format!(
                    "\"{word}\" is not a status; use waiting, applied, rejected or partly"
                ))
            })
        })
        .collect()
}
