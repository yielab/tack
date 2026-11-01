//! The rows the digest is computed from, read for the projects a caller may see. What the digest
//! says of them is `tack_core::digest`'s; this file asks the store, turns the rows into the plain
//! facts that function takes, and hands them over.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tack_core::change_set::{ChangeSetStatus, OpState};
use tack_core::digest::{
    ChangeSetFact, Digest, DigestInput, FailedRunFact, ItemFact, ProjectFacts, StatusFact,
    StepFact, TeamTaskFact, TokenFacts, UnclaimedFact, WaitFact, WaitKind, digest,
};
use tack_core::models::{DependencyType, Project};
use tack_core::workflow::StatusCategory;
use tack_db::repo::metrics;
use tack_db::{Repository, queries};
use tack_protocol::Clock;
use tack_protocol::execution::CurrentStep;
use uuid::Uuid;

use crate::AppError;

/// The most change sets read per project; the digest shows far fewer and counts the rest.
const CHANGE_SETS_READ: u32 = 1_000;

/// A stored instant: RFC 3339, or the `YYYY-MM-DD HH:MM:SS` a column default writes.
fn instant(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .map(|at| at.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|at| at.and_utc())
        })
}

fn word<T: DeserializeOwned>(text: &str) -> Option<T> {
    serde_json::from_value(Value::String(text.to_owned())).ok()
}

fn tokens(count: i64) -> u64 {
    u64::try_from(count).unwrap_or(0)
}

/// The digest of `projects` since `since`. `outside_board` adds the tasks teams hold that the
/// board did not start; they belong to no project, so the caller says whether it may see them.
pub async fn read(
    repo: &Repository,
    clock: &dyn Clock,
    projects: &[Project],
    since: DateTime<Utc>,
    outside_board: bool,
) -> Result<Digest, AppError> {
    let now = clock.now();
    let mut facts = Vec::with_capacity(projects.len());
    for project in projects {
        facts.push(project_facts(repo, clock, project, since).await?);
    }
    let shown: BTreeSet<Uuid> = projects.iter().map(|project| project.id).collect();
    let team_tasks = team_tasks(repo, &shown, outside_board).await?;
    Ok(digest(&DigestInput {
        since,
        now,
        projects: facts,
        team_tasks,
    }))
}

async fn project_facts(
    repo: &Repository,
    clock: &dyn Clock,
    project: &Project,
    since: DateTime<Utc>,
) -> Result<ProjectFacts, AppError> {
    let id = project.id.to_string();
    let done: Vec<String> = project
        .workflow
        .statuses
        .iter()
        .filter(|status| status.category == StatusCategory::Done)
        .map(|status| status.name.clone())
        .collect();
    let mut statuses: Vec<_> = project.workflow.statuses.iter().collect();
    statuses.sort_by(|a, b| (a.order, &a.name).cmp(&(b.order, &b.name)));

    let counts = repo.item_status_counts(project.id).await?;
    let open_items: Vec<ItemFact> = queries::digest::open_items(repo, &id, &done)
        .await?
        .into_iter()
        .filter_map(|row| {
            Some(ItemFact {
                id: row.id.parse().ok()?,
                title: row.title,
                status: row.status,
                updated_at: instant(&row.updated_at)?,
            })
        })
        .collect();
    let blocks = repo
        .list_dependencies_for_project(project.id)
        .await?
        .into_iter()
        .filter(|dependency| {
            matches!(
                dependency.dependency_type,
                DependencyType::Blocks | DependencyType::IsBlockedBy
            )
        })
        .map(|dependency| (dependency.source_item_id, dependency.target_item_id))
        .collect();

    let mut waiting_on_person = Vec::new();
    let runs = queries::digest::runs_waiting_on_a_person(repo, &id).await?;
    let reviews = queries::digest::items_awaiting_review(repo, &id).await?;
    for (row, run) in runs
        .into_iter()
        .map(|row| (row, true))
        .chain(reviews.into_iter().map(|row| (row, false)))
    {
        let kind = match (run, row.kind.as_str()) {
            (false, _) => WaitKind::Review,
            (true, "waiting_decision") => WaitKind::Decision,
            (true, _) => WaitKind::Attention,
        };
        if let (Ok(item), Some(since)) = (row.item_id.parse(), instant(&row.since)) {
            waiting_on_person.push(WaitFact {
                item,
                title: row.title,
                kind,
                since,
            });
        }
    }

    let failed_runs = queries::digest::failed_runs_since(repo, &id, &since.to_rfc3339())
        .await?
        .into_iter()
        .filter_map(|row| {
            Some(FailedRunFact {
                run: row.id,
                item: row.item_id.parse().ok()?,
                title: row.title,
                failed_at: instant(&row.failed_at)?,
                reason: row.reason.filter(|reason| !reason.is_empty()),
            })
        })
        .collect();

    let unclaimed = if project.auto_start_enabled && project.auto_start_status.is_some() {
        let waiting = crate::auto_start::explain(repo, clock, project).await?;
        let known: HashMap<Uuid, &str> = open_items
            .iter()
            .map(|item| (item.id, item.title.as_str()))
            .collect();
        let missing: Vec<String> = waiting
            .iter()
            .filter(|(item, _)| !known.contains_key(item))
            .map(|(item, _)| item.to_string())
            .collect();
        let found: HashMap<String, String> = queries::digest::item_titles(repo, &missing)
            .await?
            .into_iter()
            .map(|row| (row.id, row.title))
            .collect();
        Some(
            waiting
                .into_iter()
                .map(|(item, why)| UnclaimedFact {
                    item,
                    title: known
                        .get(&item)
                        .map(|title| (*title).to_owned())
                        .or_else(|| found.get(&item.to_string()).cloned())
                        .unwrap_or_default(),
                    code: why.code.to_owned(),
                    text: why.text,
                })
                .collect(),
        )
    } else {
        None
    };

    let now = clock.now();
    let limits = queries::budgets::project_budgets(repo, &id).await?;
    let day = queries::budgets::day_room(repo, &id, now).await?;
    let used = metrics::attempt_totals(repo, &id, Some(&since.to_rfc3339())).await?;
    let tokens = TokenFacts {
        used_since: tokens(used.implementation.tokens + used.rework.tokens),
        used_today: tokens(queries::budgets::spent_today(repo, &id, now).await?),
        reserved_today: day.map_or(0, |day| tokens(day.reserved)),
        limit_per_day: limits
            .as_ref()
            .and_then(|limits| limits.tokens_per_day)
            .map(tokens),
        per_run_limit: queries::budgets::provisional_tokens(repo, &id)
            .await?
            .map(tokens),
    };

    let change_sets = repo
        .list_change_sets(
            project.id,
            &[ChangeSetStatus::Waiting, ChangeSetStatus::Partly],
            CHANGE_SETS_READ,
        )
        .await?
        .into_iter()
        .filter_map(|set| {
            let waiting = set
                .ops
                .iter()
                .filter(|op| op.state == OpState::Waiting)
                .count();
            (waiting > 0).then(|| ChangeSetFact {
                id: set.id,
                principal: set.principal_name.unwrap_or(set.principal),
                reason: set.reason,
                operations: u32::try_from(waiting).unwrap_or(u32::MAX),
                since: set.created_at,
            })
        })
        .collect();

    Ok(ProjectFacts {
        id: project.id,
        name: project.name.clone(),
        statuses: statuses
            .into_iter()
            .map(|status| StatusFact {
                name: status.name.clone(),
                category: status.category.clone(),
            })
            .collect(),
        counts: counts
            .into_iter()
            .map(|(status, count)| (status, count as u64))
            .collect::<BTreeMap<_, _>>()
            .into_iter()
            .collect(),
        open_items,
        blocks,
        waiting_on_person,
        failed_runs,
        unclaimed,
        tokens,
        change_sets,
    })
}

/// The tasks teams hold that have not ended: the ones the board started for an item of `shown`
/// and, when `outside_board`, the ones it did not start.
async fn team_tasks(
    repo: &Repository,
    shown: &BTreeSet<Uuid>,
    outside_board: bool,
) -> Result<Vec<TeamTaskFact>, AppError> {
    Ok(queries::digest::open_team_tasks(repo)
        .await?
        .into_iter()
        .filter_map(|row| {
            let project = row.project_id.as_deref().and_then(|id| id.parse().ok());
            match project {
                Some(project) if !shown.contains(&project) => return None,
                None if !outside_board => return None,
                _ => {}
            }
            Some(TeamTaskFact {
                harness: row.harness,
                team: row.team,
                task_ref: row.task_ref,
                project,
                item: row.item_id.as_deref().and_then(|id| id.parse().ok()),
                title: row.title,
                state: word(&row.state)?,
                waits_on: row.waits_on.as_deref().and_then(word),
                step: row
                    .current_step
                    .as_deref()
                    .and_then(|text| serde_json::from_str::<CurrentStep>(text).ok())
                    .map(|step| StepFact {
                        name: step.name,
                        role: step.role,
                        since: step.since,
                    }),
                changed_at: row.changed_at.as_deref().and_then(instant),
            })
        })
        .collect())
}
