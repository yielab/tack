//! Change sets (migration 101): what an agent proposes to change on a board, kept with each
//! operation's state, and applied in one transaction. Nothing here decides what may be applied;
//! `tack_core::change_set` does, and the use cases in `tack-app` call it first.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::SqliteConnection;
use tack_core::CoreError;
use tack_core::change_set::{
    ChangeOp, ChangeSet, ChangeSetStatus, CreateOp, ItemRef, OpResult, OpState, Operation, Origin,
    Resolution, status_of,
};
use tack_core::models::{
    AcceptanceCriterion, CreateItem, DependencyType, ItemSource, RunSettings, UpdateItem,
    UpsertItemBrief,
};
use tack_core::workflow::WorkflowConfig;
use tracing::instrument;
use uuid::Uuid;

use super::Repository;
use super::agent_credentials::writer_name;
use super::briefs::upsert_brief_in;
use super::dependencies::insert_dependency;
use super::items::{AtomicItemUpdateOutcome, insert_item, run_settings_in, update_item_in};
use crate::{StoreError, decode};

const TABLE: &str = "change_sets";

const COLUMNS: &str = "id, project_id, principal, origin, origin_ref, reason, idempotency_key, anchor_item_id, \
     artifact_id, ops, status, created_at, decided_at, decided_by, rejection";

#[derive(sqlx::FromRow)]
struct ChangeSetRow {
    id: String,
    project_id: String,
    principal: String,
    principal_name: Option<String>,
    origin: String,
    origin_ref: Option<String>,
    reason: String,
    idempotency_key: Option<String>,
    anchor_item_id: Option<String>,
    artifact_id: Option<String>,
    ops: String,
    status: String,
    created_at: String,
    decided_at: Option<String>,
    decided_by: Option<String>,
    rejection: Option<String>,
}

impl ChangeSetRow {
    fn into_change_set(self) -> Result<ChangeSet, StoreError> {
        let agent = self.principal.starts_with("agent:");
        Ok(ChangeSet {
            id: decode::uuid(TABLE, "id", &self.id)?,
            project: decode::uuid(TABLE, "project_id", &self.project_id)?,
            principal_name: self.principal_name.filter(|_| agent),
            principal: self.principal,
            origin: decode::variant(TABLE, "origin", &self.origin)?,
            origin_ref: self.origin_ref,
            reason: self.reason,
            idempotency_key: self.idempotency_key,
            status: decode::variant(TABLE, "status", &self.status)?,
            anchor: decode::optional_uuid(TABLE, "anchor_item_id", self.anchor_item_id.as_deref())?,
            artifact_id: self.artifact_id,
            ops: decode::json(TABLE, "ops", &self.ops)?,
            created_at: decode::instant(TABLE, "created_at", &self.created_at)?,
            decided_at: decode::optional_instant(TABLE, "decided_at", self.decided_at.as_deref())?,
            decided_by: self.decided_by,
            rejection: self.rejection,
        })
    }
}

fn select(filter: &str) -> String {
    format!(
        "SELECT {COLUMNS}, {} AS principal_name FROM change_sets WHERE {filter}",
        writer_name!("principal")
    )
}

/// What a store call needs to keep a new change set.
pub struct NewChangeSet<'a> {
    pub project: Uuid,
    pub principal: &'a str,
    pub origin: Origin,
    pub origin_ref: Option<&'a str>,
    pub reason: &'a str,
    pub idempotency_key: Option<&'a str>,
    pub anchor: Option<Uuid>,
    pub artifact_id: Option<&'a str>,
    pub operations: Vec<Operation>,
    pub now: DateTime<Utc>,
}

/// What stops a change set from being applied.
#[derive(Debug, thiserror::Error)]
pub enum ApplyError {
    #[error(transparent)]
    Store(#[from] StoreError),
    /// A rule of the board, caught inside the transaction: a column filled up since the check.
    #[error(transparent)]
    Refused(#[from] CoreError),
    /// The change set is not in the state the call needs.
    #[error("{0}")]
    Conflict(String),
}

impl From<sqlx::Error> for ApplyError {
    fn from(error: sqlx::Error) -> Self {
        ApplyError::Store(error.into())
    }
}

/// Everything an application needs that the store cannot decide.
pub struct ApplyPlan<'a> {
    pub workflow: &'a WorkflowConfig,
    /// The positions of the operations to apply.
    pub selected: &'a [usize],
    /// Operations that replace the stored ones before they apply.
    pub edits: &'a [(usize, Operation)],
    /// How each selected `create` and `who_does_it` resolved.
    pub resolutions: &'a HashMap<usize, Resolution>,
    /// The principal label the writes are recorded under.
    pub writer: &'a str,
    /// What a comment is signed with.
    pub author: Option<&'a str>,
    /// The principal that decided.
    pub decided_by: &'a str,
    pub now: DateTime<Utc>,
    /// The creates the rung let through at once, for the day's count.
    pub auto_creates: u32,
}

impl Repository {
    /// Keeps a change set, every operation waiting. The same idempotency key on a project is the
    /// same change set: the first one is answered again, and the second answer says it was not
    /// made.
    #[instrument(skip(self, new), fields(project = %new.project))]
    pub async fn insert_change_set(
        &self,
        new: &NewChangeSet<'_>,
    ) -> Result<(ChangeSet, bool), StoreError> {
        let id = Uuid::new_v4();
        let ops: Vec<ChangeOp> = new
            .operations
            .iter()
            .cloned()
            .map(ChangeOp::waiting)
            .collect();
        let result = sqlx::query(
            "INSERT INTO change_sets (id, project_id, principal, origin, origin_ref, reason, idempotency_key,
                                      anchor_item_id, artifact_id, ops, status, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'waiting', ?)
             ON CONFLICT DO NOTHING",
        )
        .bind(id.to_string())
        .bind(new.project.to_string())
        .bind(new.principal)
        .bind(new.origin.as_str())
        .bind(new.origin_ref)
        .bind(new.reason)
        .bind(new.idempotency_key)
        .bind(new.anchor.map(|anchor| anchor.to_string()))
        .bind(new.artifact_id)
        .bind(decode::to_json(&ops)?)
        .bind(new.now.to_rfc3339())
        .execute(self.pool())
        .await?;
        if result.rows_affected() == 1 {
            let kept = self.get_change_set(id).await?;
            return Ok((kept.ok_or(StoreError::not_found("change set"))?, true));
        }
        let key = new
            .idempotency_key
            .ok_or_else(|| StoreError::Conflict("the change set could not be kept".to_string()))?;
        let row = sqlx::query_as::<_, ChangeSetRow>(sqlx::AssertSqlSafe(select(
            "project_id = ? AND idempotency_key = ?",
        )))
        .bind(new.project.to_string())
        .bind(key)
        .fetch_one(self.pool())
        .await?;
        Ok((row.into_change_set()?, false))
    }

    #[instrument(skip(self))]
    pub async fn get_change_set(&self, id: Uuid) -> Result<Option<ChangeSet>, StoreError> {
        let row = sqlx::query_as::<_, ChangeSetRow>(sqlx::AssertSqlSafe(select("id = ?")))
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?;
        row.map(ChangeSetRow::into_change_set).transpose()
    }

    /// The change set a project holds under an idempotency key.
    #[instrument(skip(self))]
    pub async fn get_change_set_by_key(
        &self,
        project: Uuid,
        key: &str,
    ) -> Result<Option<ChangeSet>, StoreError> {
        let row = sqlx::query_as::<_, ChangeSetRow>(sqlx::AssertSqlSafe(select(
            "project_id = ? AND idempotency_key = ?",
        )))
        .bind(project.to_string())
        .bind(key)
        .fetch_optional(self.pool())
        .await?;
        row.map(ChangeSetRow::into_change_set).transpose()
    }

    /// Forgets a change set none of whose operations was decided: a proposal that was refused
    /// after it was kept leaves no trace.
    #[instrument(skip(self))]
    pub async fn discard_change_set(&self, id: Uuid) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM change_sets WHERE id = ? AND status = 'waiting'")
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// How many items each status of a project holds.
    #[instrument(skip(self))]
    pub async fn item_status_counts(
        &self,
        project: Uuid,
    ) -> Result<HashMap<String, usize>, StoreError> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT status, COUNT(*) FROM items WHERE project_id = ? GROUP BY status",
        )
        .bind(project.to_string())
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|(status, count)| (status, usize::try_from(count).unwrap_or(0)))
            .collect())
    }

    /// A project's change sets, the newest first, of the given statuses (all when empty).
    #[instrument(skip(self))]
    pub async fn list_change_sets(
        &self,
        project: Uuid,
        statuses: &[ChangeSetStatus],
        limit: u32,
    ) -> Result<Vec<ChangeSet>, StoreError> {
        let marks = vec!["?"; statuses.len()].join(",");
        let filter = if statuses.is_empty() {
            "project_id = ? ORDER BY created_at DESC LIMIT ?".to_string()
        } else {
            format!("project_id = ? AND status IN ({marks}) ORDER BY created_at DESC LIMIT ?")
        };
        let mut query = sqlx::query_as::<_, ChangeSetRow>(sqlx::AssertSqlSafe(select(&filter)))
            .bind(project.to_string());
        for status in statuses {
            query = query.bind(status.as_str());
        }
        let rows = query.bind(i64::from(limit)).fetch_all(self.pool()).await?;
        rows.into_iter()
            .map(ChangeSetRow::into_change_set)
            .collect()
    }

    /// The creates the project's rung let through at once since `since`.
    #[instrument(skip(self))]
    pub async fn auto_creates_since(
        &self,
        project: Uuid,
        since: DateTime<Utc>,
    ) -> Result<u32, StoreError> {
        let total: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(auto_creates), 0) FROM change_sets WHERE project_id = ? AND created_at >= ?",
        )
        .bind(project.to_string())
        .bind(since.to_rfc3339())
        .fetch_one(self.pool())
        .await?;
        Ok(u32::try_from(total).unwrap_or(u32::MAX))
    }

    /// Replaces one waiting operation with another of the same kind.
    #[instrument(skip(self, operation))]
    pub async fn edit_change_set_operation(
        &self,
        id: Uuid,
        index: usize,
        operation: Operation,
    ) -> Result<ChangeSet, ApplyError> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let mut kept = load_in(&mut tx, id).await?;
        let Some(op) = kept.ops.get_mut(index) else {
            return Err(ApplyError::Conflict(format!(
                "the change set has no operation {index}"
            )));
        };
        check_editable(op, index, &operation)?;
        op.operation = operation;
        sqlx::query("UPDATE change_sets SET ops = ? WHERE id = ?")
            .bind(decode::to_json(&kept.ops)?)
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.get_change_set(id)
            .await?
            .ok_or_else(|| StoreError::not_found("change set").into())
    }

    /// Turns the waiting operations down, with a reason.
    #[instrument(skip(self))]
    pub async fn reject_change_set(
        &self,
        id: Uuid,
        reason: &str,
        decided_by: &str,
        now: DateTime<Utc>,
    ) -> Result<ChangeSet, ApplyError> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let mut kept = load_in(&mut tx, id).await?;
        if !kept.ops.iter().any(|op| op.state == OpState::Waiting) {
            return Err(ApplyError::Conflict(
                "the change set has nothing waiting".into(),
            ));
        }
        for op in kept
            .ops
            .iter_mut()
            .filter(|op| op.state == OpState::Waiting)
        {
            op.state = OpState::Rejected;
        }
        sqlx::query(
            "UPDATE change_sets SET ops = ?, status = ?, decided_at = ?, decided_by = ?, rejection = ? WHERE id = ?",
        )
        .bind(decode::to_json(&kept.ops)?)
        .bind(status_of(&kept.ops).as_str())
        .bind(now.to_rfc3339())
        .bind(decided_by)
        .bind(reason)
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.get_change_set(id)
            .await?
            .ok_or_else(|| StoreError::not_found("change set").into())
    }

    /// Applies the selected operations in one transaction: every write lands, or none does and
    /// the change set is as it was.
    #[instrument(skip(self, plan))]
    pub async fn apply_change_set(
        &self,
        id: Uuid,
        plan: &ApplyPlan<'_>,
    ) -> Result<ChangeSet, ApplyError> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let mut kept = load_in(&mut tx, id).await?;
        for (index, operation) in plan.edits {
            let op = kept.ops.get_mut(*index).ok_or_else(|| {
                ApplyError::Conflict(format!("the change set has no operation {index}"))
            })?;
            check_editable(op, *index, operation)?;
            op.operation = operation.clone();
        }
        for &index in plan.selected {
            match kept.ops.get(index) {
                Some(op) if op.state == OpState::Waiting => {}
                _ => {
                    return Err(ApplyError::Conflict(format!(
                        "operation {index} is not waiting"
                    )));
                }
            }
        }
        let initial = plan.workflow.initial_status()?;
        let source = match kept.origin {
            Origin::Run => ItemSource::Plan,
            Origin::Thread | Origin::Mcp => ItemSource::Outline,
        };
        let mut ids: HashMap<usize, Uuid> = kept
            .ops
            .iter()
            .enumerate()
            .filter_map(|(index, op)| Some((index, op.result.as_ref()?.item?)))
            .collect();
        let mut results: HashMap<usize, OpResult> = HashMap::new();

        let mut run = Run {
            tx: &mut tx,
            project: kept.project,
            writer: plan.writer,
        };
        // Creates first, in order; then the other operations; then the dependencies the
        // creates ask for, which may name a create that comes later in the list.
        for &index in plan.selected {
            if let Operation::Create(create) = &kept.ops[index].operation {
                let resolution = plan.resolutions.get(&index).cloned().unwrap_or_default();
                let result = run
                    .create(
                        create,
                        &resolution,
                        &ids,
                        &initial,
                        &source,
                        kept.artifact_id.clone(),
                        plan.workflow,
                    )
                    .await?;
                ids.insert(index, result.item.unwrap_or_default());
                results.insert(index, result);
            }
        }
        for &index in plan.selected {
            let operation = kept.ops[index].operation.clone();
            if let Some(result) = run
                .other(
                    &operation,
                    plan.resolutions.get(&index),
                    &ids,
                    plan.workflow,
                    plan.author,
                    plan.now,
                )
                .await?
            {
                results.insert(index, result);
            }
        }
        for &index in plan.selected {
            if let Operation::Create(create) = &kept.ops[index].operation {
                let blocked = ids[&index];
                for blocker in &create.depends_on {
                    let blocker = resolve(blocker, &ids)?;
                    insert_dependency(
                        &mut *run.tx,
                        blocker,
                        blocked,
                        &DependencyType::Blocks,
                        Some(plan.writer),
                    )
                    .await?;
                }
            }
        }

        for &index in plan.selected {
            kept.ops[index].state = OpState::Applied;
            kept.ops[index].result = results.remove(&index);
        }
        sqlx::query(
            "UPDATE change_sets SET ops = ?, status = ?, decided_at = ?, decided_by = ?,
                    auto_creates = auto_creates + ? WHERE id = ?",
        )
        .bind(decode::to_json(&kept.ops)?)
        .bind(status_of(&kept.ops).as_str())
        .bind(plan.now.to_rfc3339())
        .bind(plan.decided_by)
        .bind(i64::from(plan.auto_creates))
        .bind(id.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.get_change_set(id)
            .await?
            .ok_or_else(|| StoreError::not_found("change set").into())
    }
}

/// The change set as stored, read on the transaction that is about to change it.
async fn load_in(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: Uuid,
) -> Result<ChangeSet, ApplyError> {
    let row = sqlx::query_as::<_, ChangeSetRow>(sqlx::AssertSqlSafe(select("id = ?")))
        .bind(id.to_string())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::not_found("change set"))?;
    Ok(row.into_change_set()?)
}

/// An operation can be replaced while it waits, by one of its own kind (a `move` stays a
/// `move`; what the operations name by position would otherwise mean something else).
fn check_editable(op: &ChangeOp, index: usize, replacement: &Operation) -> Result<(), ApplyError> {
    if op.state != OpState::Waiting {
        return Err(ApplyError::Conflict(format!(
            "operation {index} is not waiting, so it cannot be edited"
        )));
    }
    if op.operation.kind() != replacement.kind() {
        return Err(ApplyError::Conflict(format!(
            "operation {index} is a {}; an edit keeps its kind",
            op.operation.kind()
        )));
    }
    Ok(())
}

fn resolve(item: &ItemRef, ids: &HashMap<usize, Uuid>) -> Result<Uuid, StoreError> {
    match item {
        ItemRef::Existing(id) => Ok(*id),
        ItemRef::New(index) => ids
            .get(&(*index as usize))
            .copied()
            .ok_or(StoreError::not_found("item")),
    }
}

/// The writes of one application, on its transaction.
struct Run<'a, 'c> {
    tx: &'a mut sqlx::Transaction<'c, sqlx::Sqlite>,
    project: Uuid,
    writer: &'a str,
}

impl Run<'_, '_> {
    fn conn(&mut self) -> &mut SqliteConnection {
        self.tx
    }

    async fn stamp(&mut self, item: Uuid, created: bool) -> Result<(), StoreError> {
        let sql = if created {
            "UPDATE items SET created_by = ?1, updated_by = ?1 WHERE id = ?2"
        } else {
            "UPDATE items SET updated_by = ?1 WHERE id = ?2"
        };
        let writer = self.writer;
        sqlx::query(sql)
            .bind(writer)
            .bind(item.to_string())
            .execute(self.conn())
            .await?;
        Ok(())
    }

    /// Changes the fields `input` names, the way the person's route does, and answers where the
    /// item was.
    async fn update(
        &mut self,
        item: Uuid,
        input: UpdateItem,
        workflow: &WorkflowConfig,
    ) -> Result<String, ApplyError> {
        match update_item_in(self.tx, item, input, workflow, None).await? {
            AtomicItemUpdateOutcome::Updated { old_status, .. } => {
                self.stamp(item, false).await?;
                Ok(old_status)
            }
            AtomicItemUpdateOutcome::Rejected(error) => Err(ApplyError::Refused(error)),
            AtomicItemUpdateOutcome::NotFound | AtomicItemUpdateOutcome::PreconditionFailed => {
                Err(StoreError::not_found("item").into())
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn create(
        &mut self,
        create: &CreateOp,
        resolution: &Resolution,
        ids: &HashMap<usize, Uuid>,
        initial: &str,
        source: &ItemSource,
        artifact_id: Option<String>,
        workflow: &WorkflowConfig,
    ) -> Result<OpResult, ApplyError> {
        let parent_id = create
            .parent
            .as_ref()
            .map(|parent| resolve(parent, ids))
            .transpose()?;
        let project = self.project;
        let item = insert_item(
            self.conn(),
            project,
            initial,
            CreateItem {
                title: create.title.clone(),
                description: create.description.clone(),
                item_type: resolution.item_type.clone(),
                parent_id,
                source_artifact_id: artifact_id,
                ..Default::default()
            },
            source.clone(),
        )
        .await?;
        self.stamp(item.id, true).await?;
        if !create.acceptance.is_empty() {
            let acceptance = create
                .acceptance
                .iter()
                .enumerate()
                .map(|(i, text)| AcceptanceCriterion::Manual {
                    id: format!("m{}", i + 1),
                    title: text.chars().take(60).collect(),
                    text: text.clone(),
                    checked: false,
                })
                .collect();
            upsert_brief_in(
                self.tx,
                item.id,
                UpsertItemBrief {
                    acceptance,
                    constraints: Vec::new(),
                    definition_of_done: None,
                    risk: None,
                },
            )
            .await?;
        }
        if let Some(role) = resolution.role {
            self.assign_role(item.id, role).await?;
        }
        if let Some(profile) = &resolution.profile {
            let settings = RunSettings {
                agent_profile_id: Some(profile.clone()),
                ..Default::default()
            };
            self.update(
                item.id,
                UpdateItem {
                    run_settings: Some(Some(settings)),
                    ..UpdateItem::default()
                },
                workflow,
            )
            .await?;
        }
        Ok(OpResult {
            item: Some(item.id),
            title: Some(item.title),
            unknown_role: resolution.unknown_role.clone(),
            unknown_type: resolution.unknown_type.clone(),
            unresolved_team: resolution.unresolved_team.clone(),
            unknown_profile: resolution.unknown_profile.clone(),
            ..OpResult::default()
        })
    }

    async fn assign_role(&mut self, item: Uuid, role: Uuid) -> Result<(), StoreError> {
        sqlx::query("INSERT OR IGNORE INTO item_roles (item_id, role_id) VALUES (?, ?)")
            .bind(item.to_string())
            .bind(role.to_string())
            .execute(self.conn())
            .await?;
        Ok(())
    }

    /// Every operation but a `create`.
    async fn other(
        &mut self,
        operation: &Operation,
        resolution: Option<&Resolution>,
        ids: &HashMap<usize, Uuid>,
        workflow: &WorkflowConfig,
        author: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Option<OpResult>, ApplyError> {
        let touched = |item: Uuid| OpResult {
            item: Some(item),
            ..OpResult::default()
        };
        Ok(Some(match operation {
            Operation::Create(_) => return Ok(None),
            Operation::WhoDoesIt { item, .. } => {
                let item = resolve(item, ids)?;
                let resolution = resolution.cloned().unwrap_or_default();
                if let Some(role) = resolution.role {
                    self.assign_role(item, role).await?;
                }
                if let Some(profile) = resolution.profile {
                    let mut settings = run_settings_in(self.conn(), item)
                        .await?
                        .unwrap_or_default();
                    settings.agent_profile_id = Some(profile);
                    self.update(
                        item,
                        UpdateItem {
                            run_settings: Some(Some(settings)),
                            ..UpdateItem::default()
                        },
                        workflow,
                    )
                    .await?;
                } else {
                    self.stamp(item, false).await?;
                }
                touched(item)
            }
            Operation::Priority { item, priority } => {
                let item = resolve(item, ids)?;
                let input = UpdateItem {
                    priority: Some(priority.clone()),
                    ..UpdateItem::default()
                };
                self.update(item, input, workflow).await?;
                touched(item)
            }
            Operation::Due { item, due } => {
                let item = resolve(item, ids)?;
                let input = UpdateItem {
                    due_date: Some(*due),
                    ..UpdateItem::default()
                };
                self.update(item, input, workflow).await?;
                touched(item)
            }
            Operation::Move { item, status } => {
                let item = resolve(item, ids)?;
                let input = UpdateItem {
                    status: Some(status.clone()),
                    ..UpdateItem::default()
                };
                let from = self.update(item, input, workflow).await?;
                OpResult {
                    moved_from: Some(from),
                    ..touched(item)
                }
            }
            Operation::Depend { blocker, blocked } => {
                let (blocker, blocked) = (resolve(blocker, ids)?, resolve(blocked, ids)?);
                let writer = self.writer;
                insert_dependency(
                    self.conn(),
                    blocker,
                    blocked,
                    &DependencyType::Blocks,
                    Some(writer),
                )
                .await?;
                touched(blocked)
            }
            Operation::Comment { item, text } => {
                let item = resolve(item, ids)?;
                let (comment, writer) = (Uuid::new_v4(), self.writer);
                sqlx::query(
                    "INSERT INTO comments (id, item_id, author, content, comment_type, principal, created_at, updated_at)
                     VALUES (?, ?, ?, ?, 'comment', ?, ?, ?)",
                )
                .bind(comment.to_string())
                .bind(item.to_string())
                .bind(author)
                .bind(text)
                .bind(writer)
                .bind(now.to_rfc3339())
                .bind(now.to_rfc3339())
                .execute(self.conn())
                .await?;
                touched(item)
            }
        }))
    }
}
