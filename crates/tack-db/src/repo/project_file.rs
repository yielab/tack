//! One write of a project's plan: everything a checked project file asks for, written on one
//! `BEGIN IMMEDIATE` transaction, so a failure in any section leaves the board as it was. The
//! caller (`tack-api`'s outline handler) checked the file and mapped it to a [`ProjectWrite`]; this
//! writes it, section by section, through the repositories' own connection-level functions.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};
use tack_core::CoreError;
use tack_core::hierarchy::Hierarchy;
use tack_core::item_types::ItemTypeDef;
use tack_core::models::{
    CreateCustomField, CreateProject, CreateRole, CreateSprint, DependencyType, Item, ItemSource,
    SprintStatus, UpdateCustomField, UpdateItem, UpdateProject, UpdateRole, UpdateSprint,
    UpsertItemBrief,
};
use tack_core::outline::bundle::ProfileRow;
use tack_core::outline::key::key_of;
use tack_core::outline::{PlannedParent, ResolvedOutline};
use tack_core::workflow::WorkflowConfig;
use tack_protocol::execution::ExecutionState;
use uuid::Uuid;

use super::Repository;
use super::autonomy::{Settings, put_autonomy_settings_in};
use super::briefs::upsert_brief_in;
use super::custom_fields::{
    create_field_in, delete_field_in, delete_field_value_in, set_field_value_in, update_field_in,
};
use super::dependencies::{delete_dependency_in, insert_dependency};
use super::execution::{delete_agent_profile_in, write_agent_profile_in};
use super::item_types::{CatalogError, set_item_types_in};
use super::items::{
    AtomicItemUpdateOutcome, ItemError, create_outline_items_in, delete_item_in, update_item_in,
};
use super::projects::{create_project_in, update_project_in};
use super::roles::{
    assign_role_to_item_in, create_role_in, delete_role_in, remove_role_from_item_in,
    update_role_in,
};
use super::sprints::{
    create_sprint_in, delete_sprint_in, get_sprint_in, update_sprint_in, update_sprint_status_in,
};
use crate::StoreError;
use crate::queries::item_types::in_use_in;

/// The tables that carry a key, and the column its default comes from.
const KEYED: [(&str, &str); 5] = [
    ("roles", "name"),
    ("custom_field_definitions", "name"),
    ("sprints", "name"),
    ("agent_profiles", "name"),
    ("items", "title"),
];

/// The most characters a key has; a suffix that would pass it cuts the name's part.
const MAX_KEY_CHARS: usize = 64;

/// Gives every row without a key one: `key_of` of its name or title, made unique within its
/// project (within the board, for a profile) with `-2`, `-3`, ..., in creation order. A hidden item
/// keeps none. One transaction, and a second call finds nothing to do.
pub async fn fill_missing_keys(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    for (table, column) in KEYED {
        let scope = scope_of(table);
        let visible = if table == "items" {
            "AND hidden = 0"
        } else {
            ""
        };
        let rows: Vec<(String, Option<String>, String)> =
            sqlx::query_as(sqlx::AssertSqlSafe(format!(
                "SELECT id, {scope}, {column} FROM {table} WHERE key = '' {visible} \
                 ORDER BY created_at, rowid"
            )))
            .fetch_all(&mut *tx)
            .await?;
        for (id, project, text) in rows {
            let key = key_for(&mut tx, table, project.as_deref(), None, &text).await?;
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE {table} SET key = ? WHERE id = ?"
            )))
            .bind(key)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await
}

/// The column a key is unique within: the project, except for a profile, which is the board's.
fn scope_of(table: &str) -> &'static str {
    if table == "agent_profiles" {
        "NULL"
    } else {
        "project_id"
    }
}

/// The key a new row of `table` gets: the one it was given, else `key_of(text)` made unique
/// among the keys of its project. A given key that is taken fails at the insert, which
/// [`key_conflict`] turns into a conflict.
pub(crate) async fn key_for(
    conn: &mut SqliteConnection,
    table: &'static str,
    project: Option<&str>,
    given: Option<&str>,
    text: &str,
) -> Result<String, sqlx::Error> {
    if let Some(given) = given {
        return Ok(given.to_owned());
    }
    let scope = scope_of(table);
    let taken: HashSet<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT key FROM {table} WHERE {scope} IS ? AND key <> ''"
    )))
    .bind(project)
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .collect();
    let base = key_of(text);
    let mut candidate = base.clone();
    let mut number = 2;
    while taken.contains(&candidate) {
        let suffix = format!("-{number}");
        let room = MAX_KEY_CHARS - suffix.len();
        let cut: String = base.chars().take(room).collect();
        candidate = format!("{}{suffix}", cut.trim_end_matches('-'));
        number += 1;
    }
    Ok(candidate)
}

/// A failed insert of a keyed row: a key another row holds is the store's `Conflict`.
pub(crate) fn key_conflict(error: sqlx::Error) -> StoreError {
    match &error {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            StoreError::Conflict("that key is already taken".into())
        }
        _ => StoreError::Sql(error),
    }
}

/// The project a write lands in.
pub enum Target {
    /// A project the file creates, in this workspace.
    New {
        workspace_id: Uuid,
        project: CreateProject,
    },
    Existing(Uuid),
}

/// A field the file creates, or one the board has and the file changes.
pub enum FieldWrite {
    Create(CreateCustomField),
    Update { id: Uuid, change: UpdateCustomField },
}

/// A role the file creates (`existing` is `None`) or one the board has, and the profile (an id)
/// it names.
pub struct RoleWrite {
    pub existing: Option<Uuid>,
    pub role: CreateRole,
    pub profile: Option<String>,
}

/// A sprint the file creates (`existing` is `None`) or one the board has. What the file leaves
/// out of an existing sprint keeps its stored value. `close` moves it to closed afterwards.
pub struct SprintWrite {
    pub existing: Option<Uuid>,
    pub sprint: CreateSprint,
    pub close: bool,
}

/// An item the board has and the file names by its key: what the file's plan changes in the
/// board's row, and the rest of the file's plan for it. What the file leaves as the board has it
/// is not in `change`, so an item the file does not change is not written.
pub struct ItemWrite {
    pub id: Uuid,
    /// The plan fields that differ from the board's row, but for the parent.
    pub change: UpdateItem,
    /// The parent the file names, `None` for a top-level item.
    pub parent: Option<PlannedParent>,
    /// The parent the board's row has.
    pub board_parent: Option<Uuid>,
    /// The items that must finish before this one; one the board lacks is added.
    pub blockers: Vec<PlannedParent>,
    /// Role names; a role the file adds is given, none is taken away.
    pub roles: Vec<String>,
    /// Field values by field name.
    pub fields: BTreeMap<String, serde_json::Value>,
    pub brief: Option<UpsertItemBrief>,
}

/// What a write removes, each thing by its id (a profile's is text, a type's is its key).
#[derive(Default)]
pub struct Removals {
    pub profiles: Vec<String>,
    pub items: Vec<Uuid>,
    pub roles: Vec<Uuid>,
    pub fields: Vec<Uuid>,
    pub sprints: Vec<Uuid>,
}

/// The revisions an apply of a project folder leaves, as bodies: the board as it is when the
/// write begins (kept when it differs from the last revision), then the plan as it ends, with
/// the origin (`file` or `revert`) of the second.
pub struct RevisionWrite {
    pub board: String,
    pub origin: &'static str,
    pub plan: String,
}

/// Everything one checked file writes, in the order it is written: the project, its settings,
/// the type catalog, fields, profiles, roles, sprints, automation, then the items with their
/// roles, field values and briefs, then the items the board has. The three maps hold the board's
/// own rows by name; the write adds what it creates, and the items name their sprint, roles and
/// fields through them.
pub struct ProjectWrite {
    pub target: Target,
    pub update: Option<UpdateProject>,
    pub types: Option<Vec<ItemTypeDef>>,
    pub fields: Vec<FieldWrite>,
    pub profiles: Vec<ProfileRow>,
    pub roles: Vec<RoleWrite>,
    pub sprints: Vec<SprintWrite>,
    /// The project's own automation keys.
    pub automation: Option<UpdateProject>,
    /// The autonomy settings the project ends with.
    pub autonomy: Option<Settings>,
    /// The items the file adds; its `updates` are written from `updates` below.
    pub items: ResolvedOutline,
    pub updates: Vec<ItemWrite>,
    /// The project's workflow, which an update of a plan field never asks.
    pub workflow: WorkflowConfig,
    pub source: ItemSource,
    pub field_ids: HashMap<String, Uuid>,
    pub role_ids: HashMap<String, Uuid>,
    pub sprint_ids: HashMap<String, Uuid>,
    /// What the project folder removes: profiles, then items, roles, fields and sprints.
    pub removals: Removals,
    /// Only an apply of the project folder leaves revisions.
    pub revisions: Option<RevisionWrite>,
}

/// What a write made.
pub struct Written {
    pub project_id: Uuid,
    /// The items in written order: parents before their children.
    pub items: Vec<Item>,
    pub dependencies: usize,
    /// The items of the board the write changed.
    pub updated: Vec<Uuid>,
    /// The plan's revision; none when the last revision already holds the same plan.
    pub revision: Option<RevisionRow>,
}

/// What stops a write; the board is as it was.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// The store failed in this section of the file.
    #[error("the {section} section could not be written")]
    Section {
        section: &'static str,
        #[source]
        source: StoreError,
    },
    /// A rule of the board refused what the file asked.
    #[error(transparent)]
    Refused(#[from] CoreError),
    /// The transaction could not begin or end.
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl From<sqlx::Error> for WriteError {
    fn from(error: sqlx::Error) -> Self {
        WriteError::Store(error.into())
    }
}

/// Names the section a failed store call belonged to.
fn at<E: Into<StoreError>>(section: &'static str) -> impl Fn(E) -> WriteError {
    move |source| WriteError::Section {
        section,
        source: source.into(),
    }
}

/// A revision of a project's plan: the plan as its folder carries it, as canonical JSON.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RevisionRow {
    pub number: i64,
    pub origin: String,
    pub hash: String,
    /// Left empty by [`Repository::list_revisions`].
    pub body: String,
    pub created_at: String,
}

/// The hash of a body, in hex: a revision's, and every plan's.
pub fn body_hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

const REVISION_COLUMNS: &str = "number, origin, hash, body, created_at";

async fn last_revision_in(
    conn: &mut SqliteConnection,
    project_id: Uuid,
) -> Result<Option<RevisionRow>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {REVISION_COLUMNS} FROM project_plan_revisions WHERE project_id = ? \
         ORDER BY number DESC LIMIT 1"
    )))
    .bind(project_id.to_string())
    .fetch_optional(&mut *conn)
    .await
}

/// Keeps `body` as the project's next revision, unless the last revision has the same hash.
pub(crate) async fn record_revision_in(
    conn: &mut SqliteConnection,
    project_id: Uuid,
    origin: &str,
    body: &str,
) -> Result<Option<RevisionRow>, sqlx::Error> {
    let last = last_revision_in(conn, project_id).await?;
    let hash = body_hash(body);
    if last.as_ref().is_some_and(|last| last.hash == hash) {
        return Ok(None);
    }
    let number = last.map_or(1, |last| last.number + 1);
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO project_plan_revisions (project_id, number, origin, hash, body) \
         VALUES (?, ?, ?, ?, ?) RETURNING {REVISION_COLUMNS}"
    )))
    .bind(project_id.to_string())
    .bind(number)
    .bind(origin)
    .bind(hash)
    .bind(body)
    .fetch_one(&mut *conn)
    .await
    .map(Some)
}

/// The words for a count: "1 card", "3 cards".
fn counted(n: i64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

async fn count(
    conn: &mut SqliteConnection,
    sql: &'static str,
    binds: &[&str],
) -> Result<i64, StoreError> {
    let mut query = sqlx::query_scalar(sql);
    for bind in binds {
        query = query.bind(*bind);
    }
    Ok(query.fetch_one(&mut *conn).await?)
}

/// What taking out a thing costs: `Ok` with a line for each thing that goes with it, `Err` with
/// why it stays. `id` is the row's id, and a type's key.
async fn usage_in(
    conn: &mut SqliteConnection,
    project_id: Uuid,
    section: &str,
    id: &str,
) -> Result<Result<Vec<String>, String>, StoreError> {
    let cost = |n: i64, line: String| Ok(if n > 0 { vec![line] } else { Vec::new() });
    Ok(match section {
        "roles" => {
            let n = count(
                conn,
                "SELECT COUNT(*) FROM item_roles WHERE role_id = ?",
                &[id],
            )
            .await?;
            cost(n, format!("{} lose it", counted(n, "card", "cards")))
        }
        "fields" => {
            let n = count(
                conn,
                "SELECT COUNT(*) FROM custom_field_values WHERE field_id = ?",
                &[id],
            )
            .await?;
            let words = if n == 1 { "is" } else { "are" };
            cost(
                n,
                format!("{} {words} deleted with it", counted(n, "value", "values")),
            )
        }
        "sprints" => {
            let n = count(
                conn,
                "SELECT COUNT(*) FROM items WHERE sprint_id = ?",
                &[id],
            )
            .await?;
            cost(
                n,
                format!(
                    "{} {} it",
                    counted(n, "card", "cards"),
                    if n == 1 { "leaves" } else { "leave" }
                ),
            )
        }
        "types" => {
            let used = in_use_in(conn, project_id).await?;
            let n = used
                .iter()
                .find(|(key, _)| key == id)
                .map_or(0, |(_, n)| i64::from(*n));
            if n > 0 {
                Err(format!("{} use it", counted(n, "card", "cards")))
            } else {
                Ok(Vec::new())
            }
        }
        "profiles" => profile_usage(conn, project_id, id).await?,
        "items" => item_usage(conn, id).await?,
        _ => Ok(Vec::new()),
    })
}

/// A profile stays when another project's role or default names it, or a run of it has not ended.
async fn profile_usage(
    conn: &mut SqliteConnection,
    project_id: Uuid,
    id: &str,
) -> Result<Result<Vec<String>, String>, StoreError> {
    let project = project_id.to_string();
    let mut reasons = Vec::new();
    let roles = count(
        conn,
        "SELECT COUNT(*) FROM roles WHERE agent_profile_id = ? AND project_id <> ?",
        &[id, &project],
    )
    .await?;
    if roles > 0 {
        reasons.push(format!(
            "{} of other projects use it",
            counted(roles, "role", "roles")
        ));
    }
    let defaults = count(
        conn,
        "SELECT COUNT(*) FROM projects WHERE default_profile_id = ? AND id <> ?",
        &[id, &project],
    )
    .await?;
    if defaults > 0 {
        reasons.push(format!(
            "{} use it by default",
            counted(defaults, "other project", "other projects")
        ));
    }
    let states: Vec<String> =
        sqlx::query_scalar("SELECT state FROM execution_requests WHERE agent_profile_id = ?")
            .bind(id)
            .fetch_all(&mut *conn)
            .await?;
    let running = states
        .iter()
        .filter(|state| {
            !state
                .parse::<ExecutionState>()
                .is_ok_and(ExecutionState::is_terminal)
        })
        .count();
    if running > 0 {
        reasons.push(format!(
            "{} of it has not ended",
            counted(running as i64, "run", "runs")
        ));
    }
    Ok(if reasons.is_empty() {
        Ok(Vec::new())
    } else {
        Err(reasons.join("; "))
    })
}

/// An item with history stays; one without costs its dependencies.
async fn item_usage(
    conn: &mut SqliteConnection,
    id: &str,
) -> Result<Result<Vec<String>, String>, StoreError> {
    let history: [(&'static str, &str, &str); 6] = [
        (
            "SELECT COUNT(*) FROM execution_requests WHERE item_id = ?",
            "run",
            "runs",
        ),
        (
            "SELECT COUNT(*) FROM comments WHERE item_id = ?",
            "comment",
            "comments",
        ),
        (
            "SELECT COUNT(*) FROM attachments WHERE item_id = ?",
            "attachment",
            "attachments",
        ),
        (
            "SELECT COUNT(*) FROM change_sets WHERE anchor_item_id = ?",
            "change set",
            "change sets",
        ),
        (
            "SELECT COUNT(*) FROM github_links WHERE item_id = ?",
            "GitHub link",
            "GitHub links",
        ),
        (
            "SELECT COUNT(*) FROM items WHERE parent_id = ? AND hidden = 0",
            "child",
            "children",
        ),
    ];
    let mut kept = Vec::new();
    for (sql, one, many) in history {
        let n = count(conn, sql, &[id]).await?;
        if n > 0 {
            kept.push(counted(n, one, many));
        }
    }
    if !kept.is_empty() {
        return Ok(Err(format!("it has {}", kept.join(", "))));
    }
    let n = count(
        conn,
        "SELECT COUNT(*) FROM dependencies WHERE source_item_id = ? OR target_item_id = ?",
        &[id, id],
    )
    .await?;
    Ok(Ok(if n > 0 {
        vec![format!(
            "{} go with it",
            counted(n, "dependency", "dependencies")
        )]
    } else {
        Vec::new()
    }))
}

impl Repository {
    /// The project's newest revision, with its body.
    pub async fn last_revision(&self, project_id: Uuid) -> Result<Option<RevisionRow>, StoreError> {
        let mut conn = self.pool().acquire().await?;
        Ok(last_revision_in(&mut conn, project_id).await?)
    }

    /// The project's revisions, newest first, without their bodies.
    pub async fn list_revisions(&self, project_id: Uuid) -> Result<Vec<RevisionRow>, StoreError> {
        Ok(sqlx::query_as(
            "SELECT number, origin, hash, '' AS body, created_at FROM project_plan_revisions \
             WHERE project_id = ? ORDER BY number DESC",
        )
        .bind(project_id.to_string())
        .fetch_all(self.pool())
        .await?)
    }

    /// Revision `number` of the project, with its body.
    pub async fn revision(
        &self,
        project_id: Uuid,
        number: i64,
    ) -> Result<Option<RevisionRow>, StoreError> {
        Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {REVISION_COLUMNS} FROM project_plan_revisions WHERE project_id = ? AND number = ?"
        )))
        .bind(project_id.to_string())
        .bind(number)
        .fetch_optional(self.pool())
        .await?)
    }

    /// Keeps `body` as the next revision (see [`record_revision_in`]).
    pub async fn record_revision(
        &self,
        project_id: Uuid,
        origin: &str,
        body: &str,
    ) -> Result<Option<RevisionRow>, StoreError> {
        let mut conn = self.pool().acquire().await?;
        Ok(record_revision_in(&mut conn, project_id, origin, body).await?)
    }

    /// What taking `id` out of `section` of the project costs, or why it stays (see [`usage_in`]).
    pub async fn usage_of(
        &self,
        project_id: Uuid,
        section: &str,
        id: &str,
    ) -> Result<Result<Vec<String>, String>, StoreError> {
        let mut conn = self.pool().acquire().await?;
        usage_in(&mut conn, project_id, section, id).await
    }
}

impl Repository {
    /// Writes a checked project file whole: either every section lands, or none does.
    pub async fn write_project_file(&self, write: ProjectWrite) -> Result<Written, WriteError> {
        let mut tx = self.pool().begin_with("BEGIN IMMEDIATE").await?;
        let written = write_in(&mut tx, write).await?;
        tx.commit().await?;
        Ok(written)
    }
}

async fn write_in(
    tx: &mut Transaction<'_, Sqlite>,
    write: ProjectWrite,
) -> Result<Written, WriteError> {
    let ProjectWrite {
        target,
        update,
        types,
        fields,
        profiles,
        roles,
        sprints,
        automation,
        autonomy,
        mut items,
        updates,
        workflow,
        source,
        mut field_ids,
        mut role_ids,
        mut sprint_ids,
        removals,
        revisions,
    } = write;
    let project_id = match target {
        Target::New {
            workspace_id,
            project,
        } => {
            create_project_in(tx, workspace_id, project)
                .await
                .map_err(at("project"))?
                .id
        }
        Target::Existing(id) => id,
    };
    if let Some(revisions) = &revisions {
        record_revision_in(tx, project_id, "board", &revisions.board)
            .await
            .map_err(at("revisions"))?;
    }
    remove_in(tx, removals).await?;
    if let Some(update) = update {
        update_project_in(tx, project_id, update)
            .await
            .map_err(at("project"))?;
    }

    if let Some(types) = &types {
        set_item_types_in(tx, project_id, types)
            .await
            .map_err(|failure| match failure {
                CatalogError::Store(source) => at("types")(source),
                CatalogError::Refused(rule) => WriteError::Refused(rule),
            })?;
    }

    for row in fields {
        match row {
            FieldWrite::Create(field) => {
                let name = field.name.trim().to_string();
                let created = create_field_in(tx, project_id, field)
                    .await
                    .map_err(at("fields"))?;
                field_ids.insert(name, created.id);
            }
            FieldWrite::Update { id, change } => {
                update_field_in(tx, id, change)
                    .await
                    .map_err(at("fields"))?;
            }
        }
    }

    let now = Utc::now();
    for profile in &profiles {
        write_agent_profile_in(tx, profile, now)
            .await
            .map_err(at("profiles"))?;
    }

    for row in roles {
        let name = row.role.name.trim().to_string();
        let id = match row.existing {
            Some(id) => {
                let change = UpdateRole {
                    name: Some(name.clone()),
                    color: row.role.color,
                    icon: row.role.icon,
                    agent_profile_id: row.profile.map(Some),
                };
                update_role_in(tx, id, change).await.map_err(at("roles"))?;
                id
            }
            None => {
                let created = create_role_in(
                    tx,
                    project_id,
                    CreateRole {
                        key: row.role.key,
                        name: name.clone(),
                        color: row.role.color,
                        icon: row.role.icon,
                    },
                )
                .await
                .map_err(at("roles"))?;
                if row.profile.is_some() {
                    let change = UpdateRole {
                        name: None,
                        color: None,
                        icon: None,
                        agent_profile_id: Some(row.profile),
                    };
                    update_role_in(tx, created.id, change)
                        .await
                        .map_err(at("roles"))?;
                }
                created.id
            }
        };
        role_ids.insert(name, id);
    }

    for row in sprints {
        let name = row.sprint.name.trim().to_string();
        let id = match row.existing {
            Some(id) => {
                let old = get_sprint_in(tx, id).await.map_err(at("sprints"))?;
                let change = UpdateSprint {
                    name: name.clone(),
                    goal: row
                        .sprint
                        .goal
                        .or_else(|| old.as_ref().and_then(|o| o.goal.clone())),
                    start_date: row
                        .sprint
                        .start_date
                        .or_else(|| old.as_ref().and_then(|o| o.start_date)),
                    end_date: row
                        .sprint
                        .end_date
                        .or_else(|| old.as_ref().and_then(|o| o.end_date)),
                };
                update_sprint_in(tx, id, change)
                    .await
                    .map_err(at("sprints"))?;
                id
            }
            None => {
                let sprint = CreateSprint {
                    name: name.clone(),
                    ..row.sprint
                };
                create_sprint_in(tx, project_id, sprint)
                    .await
                    .map_err(at("sprints"))?
                    .id
            }
        };
        if row.close {
            update_sprint_status_in(tx, id, SprintStatus::Closed)
                .await
                .map_err(at("sprints"))?;
        }
        sprint_ids.insert(name, id);
    }

    if let Some(automation) = automation {
        update_project_in(tx, project_id, automation)
            .await
            .map_err(at("automation"))?;
    }
    if let Some(settings) = &autonomy {
        let found = put_autonomy_settings_in(tx, &project_id.to_string(), settings)
            .await
            .map_err(at("automation"))?;
        if !found {
            return Err(at("automation")(StoreError::not_found("project")));
        }
    }

    let mut extras = Vec::with_capacity(items.items.len());
    for planned in &mut items.items {
        if let Some(sprint) = planned.sprint.as_deref() {
            planned.input.sprint_id = sprint_ids.get(sprint.trim()).copied();
        }
        extras.push((
            planned.roles.clone(),
            planned.fields.clone(),
            planned.brief.clone(),
        ));
    }
    let board_blocks = std::mem::take(&mut items.existing_blocks);
    let (created, dependencies) =
        create_outline_items_in(tx, project_id, items, source, &Hierarchy)
            .await
            .map_err(|failure| match failure {
                ItemError::Store(source) => at("items")(source),
                ItemError::Refused(rule) => WriteError::Refused(rule),
            })?;
    for &(blocker, blocked) in &board_blocks {
        insert_dependency(
            tx,
            blocker,
            created[blocked].id,
            &DependencyType::Blocks,
            None,
        )
        .await
        .map_err(at("items"))?;
    }
    for (item, (roles, fields, brief)) in created.iter().zip(extras) {
        give_roles_and_fields(tx, item.id, &roles, fields, &role_ids, &field_ids).await?;
        if let Some(brief) = brief {
            upsert_brief_in(tx, item.id, brief)
                .await
                .map_err(at("items"))?;
        }
    }

    let mut updated = Vec::new();
    for update in updates {
        let ItemWrite {
            id,
            mut change,
            parent,
            board_parent,
            blockers,
            roles,
            fields,
            brief,
        } = update;
        let named = |parent: &PlannedParent| match *parent {
            PlannedParent::Planned(i) => created[i].id,
            PlannedParent::Existing(id) => id,
        };
        let parent = parent.as_ref().map(named);
        if parent != board_parent {
            change.parent_id = Some(parent);
        }
        let mut touched = !change.is_empty();
        match update_item_in(tx, id, change, &workflow, &Hierarchy, None)
            .await
            .map_err(at("items"))?
        {
            AtomicItemUpdateOutcome::Updated { .. } => {}
            AtomicItemUpdateOutcome::Rejected(rule) => return Err(WriteError::Refused(rule)),
            AtomicItemUpdateOutcome::NotFound | AtomicItemUpdateOutcome::PreconditionFailed => {
                return Err(at("items")(StoreError::not_found("item")));
            }
        }
        let wanted: Vec<Uuid> = blockers.iter().map(named).collect();
        let held: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, source_item_id FROM dependencies WHERE target_item_id = ? AND dependency_type = ?",
        )
        .bind(id.to_string())
        .bind(DependencyType::Blocks.to_string())
        .fetch_all(&mut **tx)
        .await?;
        let mut held_sources = HashSet::new();
        for (dependency, source) in held {
            let source = parse_id("dependencies", "source_item_id", &source)?;
            if wanted.contains(&source) {
                held_sources.insert(source);
            } else {
                let dependency = parse_id("dependencies", "id", &dependency)?;
                delete_dependency_in(tx, id, dependency)
                    .await
                    .map_err(at("items"))?;
                touched = true;
            }
        }
        for blocker in wanted.into_iter().filter(|b| !held_sources.contains(b)) {
            insert_dependency(tx, blocker, id, &DependencyType::Blocks, None)
                .await
                .map_err(at("items"))?;
            touched = true;
        }
        touched |= give_roles_and_fields(tx, id, &roles, fields, &role_ids, &field_ids).await?;
        if let Some(brief) = brief {
            let stamp = "SELECT updated_at FROM item_briefs WHERE item_id = ?";
            let before: Option<String> = sqlx::query_scalar(stamp)
                .bind(id.to_string())
                .fetch_optional(&mut **tx)
                .await?;
            upsert_brief_in(tx, id, brief).await.map_err(at("items"))?;
            let after: Option<String> = sqlx::query_scalar(stamp)
                .bind(id.to_string())
                .fetch_optional(&mut **tx)
                .await?;
            touched |= before != after;
        }
        if touched {
            updated.push(id);
        }
    }
    let revision = match &revisions {
        Some(revisions) => record_revision_in(tx, project_id, revisions.origin, &revisions.plan)
            .await
            .map_err(at("revisions"))?,
        None => None,
    };
    Ok(Written {
        project_id,
        items: created,
        dependencies,
        updated,
        revision,
    })
}

/// Takes out what the project folder removes: profiles, then items, roles, fields and sprints.
async fn remove_in(tx: &mut Transaction<'_, Sqlite>, removals: Removals) -> Result<(), WriteError> {
    for id in &removals.profiles {
        delete_agent_profile_in(tx, id)
            .await
            .map_err(at("profiles"))?;
    }
    for &id in &removals.items {
        delete_item_in(tx, id).await.map_err(at("items"))?;
    }
    for &id in &removals.roles {
        delete_role_in(tx, id).await.map_err(at("roles"))?;
    }
    for &id in &removals.fields {
        delete_field_in(tx, id).await.map_err(at("fields"))?;
    }
    for &id in &removals.sprints {
        delete_sprint_in(tx, id).await.map_err(at("sprints"))?;
    }
    Ok(())
}

fn parse_id(table: &'static str, column: &'static str, text: &str) -> Result<Uuid, WriteError> {
    Uuid::parse_str(text).map_err(|error| at("items")(StoreError::corrupt(table, column, error)))
}

/// Makes the item's roles and field values the ones the file names, by name: what it names is
/// given, what the item has and the file does not is taken away. Whether anything changed.
async fn give_roles_and_fields(
    tx: &mut Transaction<'_, Sqlite>,
    item: Uuid,
    roles: &[String],
    fields: BTreeMap<String, serde_json::Value>,
    role_ids: &HashMap<String, Uuid>,
    field_ids: &HashMap<String, Uuid>,
) -> Result<bool, WriteError> {
    let mut changed = false;
    let wanted: HashSet<Uuid> = roles
        .iter()
        .map(|r| r.trim())
        .filter_map(|r| role_ids.get(r).copied())
        .collect();
    let held: Vec<String> = sqlx::query_scalar("SELECT role_id FROM item_roles WHERE item_id = ?")
        .bind(item.to_string())
        .fetch_all(&mut **tx)
        .await?;
    let mut held_ids = HashSet::new();
    for role in held {
        let role = parse_id("item_roles", "role_id", &role)?;
        held_ids.insert(role);
        if !wanted.contains(&role) {
            remove_role_from_item_in(tx, item, role)
                .await
                .map_err(at("roles"))?;
            changed = true;
        }
    }
    for role in roles.iter().map(|r| r.trim()).filter(|r| !r.is_empty()) {
        if let Some(&role_id) = role_ids.get(role) {
            assign_role_to_item_in(tx, item, role_id)
                .await
                .map_err(at("roles"))?;
            changed |= !held_ids.contains(&role_id);
        }
    }

    let held: Vec<(String, String)> =
        sqlx::query_as("SELECT field_id, value FROM custom_field_values WHERE item_id = ?")
            .bind(item.to_string())
            .fetch_all(&mut **tx)
            .await?;
    let mut held_values = HashMap::new();
    for (field, value) in held {
        held_values.insert(parse_id("custom_field_values", "field_id", &field)?, value);
    }
    let mut named = HashSet::new();
    for (name, value) in fields {
        if let Some(&field_id) = field_ids.get(name.trim()) {
            named.insert(field_id);
            let same = held_values
                .get(&field_id)
                .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
                .is_some_and(|held| held == value);
            if !same {
                set_field_value_in(tx, item, field_id, value)
                    .await
                    .map_err(at("fields"))?;
                changed = true;
            }
        }
    }
    for field_id in held_values.keys().filter(|id| !named.contains(id)) {
        delete_field_value_in(tx, item, *field_id)
            .await
            .map_err(at("fields"))?;
        changed = true;
    }
    Ok(changed)
}
