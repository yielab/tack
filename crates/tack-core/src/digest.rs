//! The digest (ADR 0034, decision 6): what a reader needs to know about some projects before it
//! asks for any detail, computed from plain rows and never written by a model.
//!
//! [`digest`] is pure. It reads no store, no clock and no hash map's order: the caller hands in
//! the rows and the instant they were read at, and the same input gives the same output byte for
//! byte. It is bounded: every list shows at most [`ROWS`] rows and says how many more there are,
//! the projects and teams shown are capped, and every text is clipped, so the size of the answer
//! does not grow with the board.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::workflow::StatusCategory;
use tack_protocol::execution::{ActivityState, WaitsOn};

/// The most rows one list of the digest shows.
pub const ROWS: usize = 8;
/// The most projects the digest describes; the rest are only counted.
pub const MAX_PROJECTS: usize = 20;
/// The most teams the digest describes; the rest are only counted.
pub const MAX_TEAMS: usize = 20;
/// The most statuses one project lists with their counts.
pub const MAX_STATUSES: usize = 30;
/// How long an item in progress may go unchanged before it is stale.
pub const STALE_AFTER_DAYS: i64 = 14;
/// The longest an item's title is in the digest, in characters.
pub const TITLE_CHARS: usize = 100;
/// The longest a reason or a name is in the digest, in characters.
pub const TEXT_CHARS: usize = 160;

// ─── What goes in ───────────────────────────────────────────

/// Everything the digest is computed from.
#[derive(Debug, Clone)]
pub struct DigestInput {
    /// Runs that failed before this instant are left out.
    pub since: DateTime<Utc>,
    /// The instant the rows were read at; staleness is measured against it.
    pub now: DateTime<Utc>,
    pub projects: Vec<ProjectFacts>,
    /// The tasks teams hold that have not ended, for the projects above and, when the reader may
    /// see all of them, the ones the board did not start.
    pub team_tasks: Vec<TeamTaskFact>,
}

/// What is known of one project.
#[derive(Debug, Clone)]
pub struct ProjectFacts {
    pub id: Uuid,
    pub name: String,
    /// The workflow's statuses in the order the board shows them.
    pub statuses: Vec<StatusFact>,
    /// How many items each status holds.
    pub counts: Vec<(String, u64)>,
    /// The items not in a done status.
    pub open_items: Vec<ItemFact>,
    /// `(blocker, blocked)` for every dependency that blocks.
    pub blocks: Vec<(Uuid, Uuid)>,
    pub waiting_on_person: Vec<WaitFact>,
    pub failed_runs: Vec<FailedRunFact>,
    /// Why each task the project would start by itself still waits; `None` when the project
    /// starts none by itself.
    pub unclaimed: Option<Vec<UnclaimedFact>>,
    pub tokens: TokenFacts,
    /// The change sets with an operation still waiting for a person.
    pub change_sets: Vec<ChangeSetFact>,
}

#[derive(Debug, Clone)]
pub struct StatusFact {
    pub name: String,
    pub category: StatusCategory,
}

#[derive(Debug, Clone)]
pub struct ItemFact {
    pub id: Uuid,
    pub title: String,
    pub status: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct WaitFact {
    pub item: Uuid,
    pub title: String,
    pub kind: WaitKind,
    pub since: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct FailedRunFact {
    pub run: String,
    pub item: Uuid,
    pub title: String,
    pub failed_at: DateTime<Utc>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UnclaimedFact {
    pub item: Uuid,
    pub title: String,
    /// The code a screen switches on (`blocked`, `no_profile`, `wip`, ...).
    pub code: String,
    pub text: String,
}

/// Tokens in counts of tokens; a limit that is not set is `None`.
#[derive(Debug, Clone, Default)]
pub struct TokenFacts {
    pub used_since: u64,
    pub used_today: u64,
    /// What open runs have set aside and not yet used.
    pub reserved_today: u64,
    pub limit_per_day: Option<u64>,
    pub per_run_limit: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ChangeSetFact {
    pub id: Uuid,
    pub principal: String,
    pub reason: String,
    /// How many operations still wait.
    pub operations: u32,
    pub since: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct TeamTaskFact {
    pub harness: String,
    pub team: String,
    pub task_ref: String,
    /// The project of the item the board started the task for; `None` for work it did not start.
    pub project: Option<Uuid>,
    pub item: Option<Uuid>,
    pub title: Option<String>,
    pub state: ActivityState,
    pub waits_on: Option<WaitsOn>,
    pub step: Option<StepFact>,
    pub changed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct StepFact {
    pub name: String,
    pub role: Option<String>,
    pub since: Option<DateTime<Utc>>,
}

// ─── What comes out ─────────────────────────────────────────

/// A list cut to its first rows, with how many there were and how many are not shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Section<T> {
    pub total: u64,
    pub rows: Vec<T>,
    /// `total` less the rows shown: "and N more".
    pub more: u64,
}

impl<T> Section<T> {
    fn of(mut all: Vec<T>, shown: usize) -> Section<T> {
        let total = all.len() as u64;
        all.truncate(shown);
        let more = total - all.len() as u64;
        Section {
            total,
            rows: all,
            more,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Digest {
    pub since: DateTime<Utc>,
    /// The instant the digest was read at.
    pub as_of: DateTime<Utc>,
    /// The projects, by name.
    pub projects: Section<ProjectDigest>,
    /// The teams with work that has not ended, by name.
    pub teams: Section<TeamDigest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ProjectDigest {
    pub id: Uuid,
    pub name: String,
    pub items_total: u64,
    /// The statuses that hold items, in the board's order.
    pub items_by_status: Section<StatusCount>,
    /// What waits for a person, the longest wait first.
    pub waiting_on_person: Section<WaitingOnPerson>,
    pub blocked: Blocked,
    pub unclaimed: Unclaimed,
    /// Items in progress that nothing has changed for [`STALE_AFTER_DAYS`] days, the oldest first.
    pub stale: Section<StaleItem>,
    /// Runs that failed since the digest's `since`, the newest first.
    pub failed_runs: Section<FailedRun>,
    pub tokens: Tokens,
    /// Change sets with operations waiting for a person, the oldest first.
    pub change_sets: Section<WaitingChangeSet>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StatusCount {
    pub status: String,
    pub count: u64,
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DigestItem {
    pub id: Uuid,
    pub title: String,
}

/// What a person is waited for to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum WaitKind {
    /// A run asks for an answer or an approval.
    Decision,
    /// A run stopped and needs a person before it can go on.
    Attention,
    /// A run ended and left work nobody has reviewed.
    Review,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WaitingOnPerson {
    pub item: DigestItem,
    pub kind: WaitKind,
    pub since: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Blocked {
    /// How many open items wait for another open item.
    pub items: u64,
    /// The open items that block others and are not blocked themselves, the one holding up the
    /// most first.
    pub chains: Section<BlockedChain>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BlockedChain {
    pub blocker: DigestItem,
    pub status: String,
    /// How many open items wait for it, directly or through others.
    pub blocks: u64,
    /// How many links the longest chain below it has.
    pub depth: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Unclaimed {
    /// Whether the project starts tasks by itself; when it does not, no task is waiting for it.
    pub auto_start: bool,
    /// How many tasks wait for each reason, by code.
    pub by_reason: Vec<ReasonCount>,
    /// The tasks it would start and has not, most urgent first.
    pub tasks: Section<UnclaimedTask>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReasonCount {
    pub code: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UnclaimedTask {
    pub item: DigestItem,
    pub code: String,
    pub why: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StaleItem {
    pub item: DigestItem,
    pub status: String,
    pub last_change: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FailedRun {
    pub run: String,
    pub item: DigestItem,
    pub failed_at: DateTime<Utc>,
    pub reason: Option<String>,
}

/// Tokens counted as the project's limits count them: cache reads are not part of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Tokens {
    /// Used by the runs started since the digest's `since`.
    pub used_since: u64,
    pub used_today: u64,
    pub reserved_today: u64,
    /// The day's limit; `null` when the project sets none.
    pub limit_per_day: Option<u64>,
    /// What is left of the day's limit; `null` without one.
    pub left_today: Option<u64>,
    /// The limit of one run that sets none; `null` when off.
    pub per_run_limit: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WaitingChangeSet {
    pub id: Uuid,
    /// Who proposed it.
    pub principal: String,
    pub reason: String,
    /// How many of its operations wait.
    pub operations: u32,
    pub since: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamDigest {
    pub team: String,
    pub harness: String,
    pub running: u64,
    pub waiting: u64,
    pub blocked: u64,
    pub queued: u64,
    /// The work, what is going first and the queue after it.
    pub tasks: Section<TeamTask>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamTask {
    /// The task's name in the team's own agent.
    pub task: String,
    pub state: ActivityState,
    pub waits_on: Option<WaitsOn>,
    /// The project the board started it for; `null` for work the board did not start.
    pub project: Option<Uuid>,
    pub item: Option<DigestItem>,
    /// The step it is inside now.
    pub step: Option<TeamStep>,
    pub changed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TeamStep {
    pub name: String,
    pub role: Option<String>,
    pub since: Option<DateTime<Utc>>,
}

// ─── The computation ────────────────────────────────────────

/// `text` cut to `max` characters, with an ellipsis when something was cut.
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

fn digest_item(id: Uuid, title: &str) -> DigestItem {
    DigestItem {
        id,
        title: clip(title, TITLE_CHARS),
    }
}

/// The digest of `input`.
pub fn digest(input: &DigestInput) -> Digest {
    let mut projects: Vec<&ProjectFacts> = input.projects.iter().collect();
    projects.sort_by(|a, b| (&a.name, a.id).cmp(&(&b.name, b.id)));
    let projects = Section::of(
        projects
            .into_iter()
            .map(|facts| project_digest(facts, input))
            .collect(),
        MAX_PROJECTS,
    );
    Digest {
        since: input.since,
        as_of: input.now,
        projects,
        teams: teams(&input.team_tasks),
    }
}

fn project_digest(facts: &ProjectFacts, input: &DigestInput) -> ProjectDigest {
    ProjectDigest {
        id: facts.id,
        name: clip(&facts.name, TITLE_CHARS),
        items_total: facts.counts.iter().map(|(_, count)| count).sum(),
        items_by_status: Section::of(status_counts(facts), MAX_STATUSES),
        waiting_on_person: waiting(facts),
        blocked: blocked(facts),
        unclaimed: unclaimed(facts),
        stale: stale(facts, input.now),
        failed_runs: failed_runs(facts, input.since),
        tokens: tokens(&facts.tokens),
        change_sets: change_sets(facts),
    }
}

/// The statuses that hold items: the workflow's first, in its order, then any other by name.
fn status_counts(facts: &ProjectFacts) -> Vec<StatusCount> {
    let mut held: BTreeMap<&str, u64> = BTreeMap::new();
    for (status, count) in &facts.counts {
        *held.entry(status).or_default() += count;
    }
    let mut rows = Vec::new();
    for status in &facts.statuses {
        if let Some(count) = held.remove(status.name.as_str())
            && count > 0
        {
            rows.push(StatusCount {
                status: clip(&status.name, TEXT_CHARS),
                count,
                done: status.category == StatusCategory::Done,
            });
        }
    }
    rows.extend(
        held.into_iter()
            .filter(|(_, count)| *count > 0)
            .map(|(status, count)| StatusCount {
                status: clip(status, TEXT_CHARS),
                count,
                done: false,
            }),
    );
    rows
}

fn waiting(facts: &ProjectFacts) -> Section<WaitingOnPerson> {
    let mut rows: Vec<&WaitFact> = facts.waiting_on_person.iter().collect();
    rows.sort_by_key(|row| (row.since, row.item, row.kind as u8));
    Section::of(
        rows.into_iter()
            .map(|row| WaitingOnPerson {
                item: digest_item(row.item, &row.title),
                kind: row.kind,
                since: row.since,
            })
            .collect(),
        ROWS,
    )
}

/// The open items that wait for another open item, and the items holding the most of them up.
/// An item that is not open is finished, so it holds nothing up.
fn blocked(facts: &ProjectFacts) -> Blocked {
    let open: BTreeMap<Uuid, &ItemFact> = facts.open_items.iter().map(|i| (i.id, i)).collect();
    let mut below: BTreeMap<Uuid, BTreeSet<Uuid>> = BTreeMap::new();
    let mut waiting: BTreeSet<Uuid> = BTreeSet::new();
    for (blocker, blocked) in &facts.blocks {
        if blocker != blocked && open.contains_key(blocker) && open.contains_key(blocked) {
            below.entry(*blocker).or_default().insert(*blocked);
            waiting.insert(*blocked);
        }
    }
    let mut chains = Vec::new();
    for root in below.keys().filter(|id| !waiting.contains(id)) {
        // Level by level, so the last level reached is the longest chain.
        let mut seen: BTreeSet<Uuid> = BTreeSet::from([*root]);
        let mut level = vec![*root];
        let mut depth = 0_u32;
        loop {
            let next: BTreeSet<Uuid> = level
                .iter()
                .flat_map(|id| below.get(id).into_iter().flatten().copied())
                .filter(|id| seen.insert(*id))
                .collect();
            if next.is_empty() {
                break;
            }
            depth += 1;
            level = next.into_iter().collect();
        }
        let item = open[root];
        chains.push(BlockedChain {
            blocker: digest_item(item.id, &item.title),
            status: clip(&item.status, TEXT_CHARS),
            blocks: seen.len() as u64 - 1,
            depth,
        });
    }
    chains.sort_by_key(|chain| {
        (
            Reverse(chain.blocks),
            Reverse(chain.depth),
            chain.blocker.id,
        )
    });
    Blocked {
        items: waiting.len() as u64,
        chains: Section::of(chains, ROWS),
    }
}

fn unclaimed(facts: &ProjectFacts) -> Unclaimed {
    let Some(rows) = &facts.unclaimed else {
        return Unclaimed {
            auto_start: false,
            by_reason: Vec::new(),
            tasks: Section::of(Vec::new(), ROWS),
        };
    };
    let mut reasons: BTreeMap<&str, u64> = BTreeMap::new();
    for row in rows {
        *reasons.entry(&row.code).or_default() += 1;
    }
    Unclaimed {
        auto_start: true,
        by_reason: reasons
            .into_iter()
            .map(|(code, count)| ReasonCount {
                code: clip(code, TEXT_CHARS),
                count,
            })
            .collect(),
        // In the order the project would start them in: the most urgent first.
        tasks: Section::of(
            rows.iter()
                .map(|row| UnclaimedTask {
                    item: digest_item(row.item, &row.title),
                    code: clip(&row.code, TEXT_CHARS),
                    why: clip(&row.text, TEXT_CHARS),
                })
                .collect(),
            ROWS,
        ),
    }
}

fn stale(facts: &ProjectFacts, now: DateTime<Utc>) -> Section<StaleItem> {
    let limit = now - Duration::days(STALE_AFTER_DAYS);
    let in_progress: BTreeSet<&str> = facts
        .statuses
        .iter()
        .filter(|status| status.category == StatusCategory::InProgress)
        .map(|status| status.name.as_str())
        .collect();
    let mut rows: Vec<&ItemFact> = facts
        .open_items
        .iter()
        .filter(|item| in_progress.contains(item.status.as_str()) && item.updated_at < limit)
        .collect();
    rows.sort_by_key(|item| (item.updated_at, item.id));
    Section::of(
        rows.into_iter()
            .map(|item| StaleItem {
                item: digest_item(item.id, &item.title),
                status: clip(&item.status, TEXT_CHARS),
                last_change: item.updated_at,
            })
            .collect(),
        ROWS,
    )
}

fn failed_runs(facts: &ProjectFacts, since: DateTime<Utc>) -> Section<FailedRun> {
    let mut rows: Vec<&FailedRunFact> = facts
        .failed_runs
        .iter()
        .filter(|run| run.failed_at >= since)
        .collect();
    rows.sort_by(|a, b| (Reverse(a.failed_at), &a.run).cmp(&(Reverse(b.failed_at), &b.run)));
    Section::of(
        rows.into_iter()
            .map(|run| FailedRun {
                run: clip(&run.run, TEXT_CHARS),
                item: digest_item(run.item, &run.title),
                failed_at: run.failed_at,
                reason: run.reason.as_deref().map(|text| clip(text, TEXT_CHARS)),
            })
            .collect(),
        ROWS,
    )
}

fn tokens(facts: &TokenFacts) -> Tokens {
    Tokens {
        used_since: facts.used_since,
        used_today: facts.used_today,
        reserved_today: facts.reserved_today,
        limit_per_day: facts.limit_per_day,
        left_today: facts.limit_per_day.map(|limit| {
            limit.saturating_sub(facts.used_today.saturating_add(facts.reserved_today))
        }),
        per_run_limit: facts.per_run_limit,
    }
}

fn change_sets(facts: &ProjectFacts) -> Section<WaitingChangeSet> {
    let mut rows: Vec<&ChangeSetFact> = facts.change_sets.iter().collect();
    rows.sort_by_key(|set| (set.since, set.id));
    Section::of(
        rows.into_iter()
            .map(|set| WaitingChangeSet {
                id: set.id,
                principal: clip(&set.principal, TEXT_CHARS),
                reason: clip(&set.reason, TEXT_CHARS),
                operations: set.operations,
                since: set.since,
            })
            .collect(),
        ROWS,
    )
}

/// How far along a task is, so the ones going come before the ones queued.
fn rank(state: ActivityState) -> u8 {
    match state {
        ActivityState::Running => 0,
        ActivityState::Waiting => 1,
        ActivityState::Blocked => 2,
        ActivityState::Queued => 3,
        ActivityState::Done | ActivityState::Failed | ActivityState::Cancelled => 4,
    }
}

fn teams(tasks: &[TeamTaskFact]) -> Section<TeamDigest> {
    let mut by_team: BTreeMap<(&str, &str), Vec<&TeamTaskFact>> = BTreeMap::new();
    for task in tasks.iter().filter(|task| !task.state.over()) {
        by_team
            .entry((&task.team, &task.harness))
            .or_default()
            .push(task);
    }
    let digests = by_team
        .into_iter()
        .map(|((team, harness), mut tasks)| {
            tasks.sort_by(|a, b| {
                (rank(a.state), Reverse(a.changed_at), &a.task_ref).cmp(&(
                    rank(b.state),
                    Reverse(b.changed_at),
                    &b.task_ref,
                ))
            });
            let count = |state| tasks.iter().filter(|task| task.state == state).count() as u64;
            TeamDigest {
                team: clip(team, TEXT_CHARS),
                harness: clip(harness, TEXT_CHARS),
                running: count(ActivityState::Running),
                waiting: count(ActivityState::Waiting),
                blocked: count(ActivityState::Blocked),
                queued: count(ActivityState::Queued),
                tasks: Section::of(
                    tasks
                        .into_iter()
                        .map(|task| TeamTask {
                            task: clip(&task.task_ref, TEXT_CHARS),
                            state: task.state,
                            waits_on: task.waits_on,
                            project: task.project,
                            item: task
                                .item
                                .zip(task.title.as_deref())
                                .map(|(id, title)| digest_item(id, title)),
                            step: task.step.as_ref().map(|step| TeamStep {
                                name: clip(&step.name, TEXT_CHARS),
                                role: step.role.as_deref().map(|role| clip(role, TEXT_CHARS)),
                                since: step.since,
                            }),
                            changed_at: task.changed_at,
                        })
                        .collect(),
                    ROWS,
                ),
            }
        })
        .collect();
    Section::of(digests, MAX_TEAMS)
}

#[cfg(test)]
mod tests;
