//! Change sets (ADR 0034, decisions 3 and 4): the one way an agent changes the board, as a list of
//! operations checked together, applied together, and decided one by one by the project's rung.
//!
//! Nothing here touches the store, a clock or HTTP. The caller reads the facts the operations
//! name into a [`BoardView`]; [`validate`] answers every reason a set cannot be applied, or how
//! each operation resolves; [`decide`] answers which operations the rung lets through at once.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::iter;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::dependency::DependencyGraph;
use crate::hierarchy::{Entry, Forest, admit_parent};
use crate::item_types::{ItemTypeDef, Level, default_work_key};
use crate::models::{DependencyType, Priority};
use crate::principal::{Capability, Principal, may};
use crate::workflow::{StatusGate, WorkflowConfig};

/// The most operations one change set holds.
pub const MAX_OPERATIONS: usize = 100;

/// Where a change set came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Origin {
    /// A run's proposal: its plan file, its records, or a file a person uploaded.
    Run,
    /// A turn of a thread.
    Thread,
    /// A tool call over Tack's MCP server.
    Mcp,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Run => "run",
            Origin::Thread => "thread",
            Origin::Mcp => "mcp",
        }
    }
}

impl FromStr for Origin {
    type Err = ();
    fn from_str(text: &str) -> Result<Self, ()> {
        match text {
            "run" => Ok(Origin::Run),
            "thread" => Ok(Origin::Thread),
            "mcp" => Ok(Origin::Mcp),
            _ => Err(()),
        }
    }
}

/// Where a change set stands. It is derived from its operations, never set on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ChangeSetStatus {
    /// Nothing has been decided yet.
    Waiting,
    /// Every operation was applied.
    Applied,
    /// Every operation was turned down.
    Rejected,
    /// Some operations were applied and the rest are waiting or were turned down.
    Partly,
}

impl ChangeSetStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ChangeSetStatus::Waiting => "waiting",
            ChangeSetStatus::Applied => "applied",
            ChangeSetStatus::Rejected => "rejected",
            ChangeSetStatus::Partly => "partly",
        }
    }
}

impl FromStr for ChangeSetStatus {
    type Err = ();
    fn from_str(text: &str) -> Result<Self, ()> {
        match text {
            "waiting" => Ok(ChangeSetStatus::Waiting),
            "applied" => Ok(ChangeSetStatus::Applied),
            "rejected" => Ok(ChangeSetStatus::Rejected),
            "partly" => Ok(ChangeSetStatus::Partly),
            _ => Err(()),
        }
    }
}

/// An item an operation names: one the change set creates, or one the board already has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ItemRef {
    /// The position, in the change set's operations, of a `create` that makes the item.
    New(u32),
    Existing(Uuid),
}

/// One `create`: an entry of a plan (`docs/contracts/plan-v1`), with refs where the plan has
/// indexes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateOp {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Acceptance criteria the item starts with.
    #[serde(default)]
    pub acceptance: Vec<String>,
    /// The items that block this one.
    #[serde(default)]
    pub depends_on: Vec<ItemRef>,
    /// The item type, when the project has it; else the type of the set's anchor when that is work, else the default work type.
    #[serde(default, rename = "type")]
    pub item_type: Option<String>,
    /// A project role, by name.
    #[serde(default)]
    pub role: Option<String>,
    /// A team, by name; the one profile that names it does the item.
    #[serde(default)]
    pub team: Option<String>,
    /// A profile, by id; it wins over `team`.
    #[serde(default)]
    pub profile_id: Option<String>,
    /// The item this one is a sub-item of; absent means a top-level item. A `New` parent is an
    /// earlier operation.
    #[serde(default)]
    pub parent: Option<ItemRef>,
}

/// One change to the board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Operation {
    Create(CreateOp),
    /// Gives an item a role, a team's profile, or a profile: at least one.
    WhoDoesIt {
        item: ItemRef,
        #[serde(default)]
        role: Option<String>,
        #[serde(default)]
        team: Option<String>,
        #[serde(default)]
        profile_id: Option<String>,
    },
    Priority {
        item: ItemRef,
        priority: Priority,
    },
    Move {
        item: ItemRef,
        status: String,
    },
    /// `blocker` must finish before `blocked`.
    Depend {
        blocker: ItemRef,
        blocked: ItemRef,
    },
    Comment {
        item: ItemRef,
        text: String,
    },
    /// A date, or `null` to clear it.
    Due {
        item: ItemRef,
        #[serde(default)]
        due: Option<DateTime<Utc>>,
    },
    /// Puts an item under `parent`, or at the top when there is none.
    Parent {
        item: ItemRef,
        #[serde(default)]
        parent: Option<ItemRef>,
    },
}

impl Operation {
    /// The word an operation goes by in a reason and on a screen.
    pub fn kind(&self) -> &'static str {
        match self {
            Operation::Create(_) => "create",
            Operation::WhoDoesIt { .. } => "who_does_it",
            Operation::Priority { .. } => "priority",
            Operation::Move { .. } => "move",
            Operation::Depend { .. } => "depend",
            Operation::Comment { .. } => "comment",
            Operation::Due { .. } => "due",
            Operation::Parent { .. } => "parent",
        }
    }

    /// The positions of the creates this operation needs to exist first.
    pub fn needs(&self) -> Vec<u32> {
        let refs: Vec<&ItemRef> = match self {
            Operation::Create(create) => create
                .parent
                .iter()
                .chain(create.depends_on.iter())
                .collect(),
            Operation::WhoDoesIt { item, .. }
            | Operation::Priority { item, .. }
            | Operation::Move { item, .. }
            | Operation::Comment { item, .. }
            | Operation::Due { item, .. } => vec![item],
            Operation::Parent { item, parent } => iter::once(item).chain(parent.iter()).collect(),
            Operation::Depend { blocker, blocked } => vec![blocker, blocked],
        };
        refs.into_iter()
            .filter_map(|item| match item {
                ItemRef::New(index) => Some(*index),
                ItemRef::Existing(_) => None,
            })
            .collect()
    }

    fn is_create(&self) -> bool {
        matches!(self, Operation::Create(_))
    }
}

/// Whether an operation was decided.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum OpState {
    #[default]
    Waiting,
    Applied,
    Rejected,
}

/// What an applied operation did, in the words the plan's answer has always used.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct OpResult {
    /// The item a `create` made, or the one the operation changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// For a `move`, where the item was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moved_from: Option<String>,
    /// The role name the project does not have; the item has no role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unknown_role: Option<String>,
    /// The type the project does not have; the item has the fallback type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unknown_type: Option<String>,
    /// The team that no profile, or more than one, names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unresolved_team: Option<String>,
    /// The profile id that no profile has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unknown_profile: Option<String>,
}

/// An operation with its state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ChangeOp {
    pub operation: Operation,
    #[serde(default)]
    pub state: OpState,
    #[serde(default)]
    pub result: Option<OpResult>,
}

impl ChangeOp {
    pub fn waiting(operation: Operation) -> ChangeOp {
        ChangeOp {
            operation,
            state: OpState::Waiting,
            result: None,
        }
    }
}

/// A list of operations on one project's board, from one principal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ChangeSet {
    pub id: Uuid,
    pub project: Uuid,
    /// Who proposed it: a principal label (`agent:<credential id>`, `operator:local`).
    pub principal: String,
    /// The proposer's name when it is an agent credential.
    #[serde(default)]
    pub principal_name: Option<String>,
    pub origin: Origin,
    /// The run or thread it came from, when it came from one.
    #[serde(default)]
    pub origin_ref: Option<String>,
    /// Why the proposer wants it.
    pub reason: String,
    /// The same key twice is the same change set.
    #[serde(default)]
    pub idempotency_key: Option<String>,
    pub status: ChangeSetStatus,
    /// The item a plan's entries hang under; its type is the type of an entry that names none
    /// the project has.
    #[serde(default)]
    pub anchor: Option<Uuid>,
    /// The plan artifact the creates came from.
    #[serde(default)]
    pub artifact_id: Option<String>,
    pub ops: Vec<ChangeOp>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub decided_at: Option<DateTime<Utc>>,
    /// Who applied or turned it down last: a principal label.
    #[serde(default)]
    pub decided_by: Option<String>,
    /// Why a person turned the waiting operations down.
    #[serde(default)]
    pub rejection: Option<String>,
}

/// What a change set is in now, from its operations.
pub fn status_of(ops: &[ChangeOp]) -> ChangeSetStatus {
    let count = |state| ops.iter().filter(|op| op.state == state).count();
    let (waiting, applied, rejected) = (
        count(OpState::Waiting),
        count(OpState::Applied),
        count(OpState::Rejected),
    );
    if applied == 0 && rejected == 0 {
        ChangeSetStatus::Waiting
    } else if waiting == 0 && rejected == 0 {
        ChangeSetStatus::Applied
    } else if waiting == 0 && applied == 0 {
        ChangeSetStatus::Rejected
    } else {
        ChangeSetStatus::Partly
    }
}

// ─── The rung ───────────────────────────────────────────────

/// How far an agent may go on a project (ADR 0034, decision 4), in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum RungLevel {
    /// Reads and talks; changes nothing, proposes nothing.
    Read,
    /// Every change set waits for a person.
    Propose,
    /// Everything but creates applies at once.
    Act,
    /// Creates apply too, inside the two caps.
    Create,
}

impl RungLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            RungLevel::Read => "read",
            RungLevel::Propose => "propose",
            RungLevel::Act => "act",
            RungLevel::Create => "create",
        }
    }
}

impl FromStr for RungLevel {
    type Err = ();
    fn from_str(text: &str) -> Result<Self, ()> {
        match text {
            "read" => Ok(RungLevel::Read),
            "propose" => Ok(RungLevel::Propose),
            "act" => Ok(RungLevel::Act),
            "create" => Ok(RungLevel::Create),
            _ => Err(()),
        }
    }
}

/// A project's `agents_on_board` setting: the level, and the two caps on creates that apply at
/// once. A cap of 0 lets none through, so Create with no caps behaves as Act.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AgentsOnBoard {
    pub level: RungLevel,
    /// Creates that apply at once, per change set, at Create.
    pub creates_per_change_set: u32,
    /// Creates that apply at once, per day (UTC), at Create.
    pub creates_per_day: u32,
}

impl Default for AgentsOnBoard {
    fn default() -> Self {
        AgentsOnBoard {
            level: RungLevel::Propose,
            creates_per_change_set: 0,
            creates_per_day: 0,
        }
    }
}

/// The most creates a day's cap may let through.
pub const MAX_CREATES_PER_DAY: u32 = 10_000;

impl AgentsOnBoard {
    /// Why this setting cannot be saved, or `None` when it can.
    pub fn problem(&self) -> Option<String> {
        if self.creates_per_change_set as usize > MAX_OPERATIONS {
            return Some(format!(
                "creates per change set is at most {MAX_OPERATIONS}, the size of a change set"
            ));
        }
        if self.creates_per_day > MAX_CREATES_PER_DAY {
            return Some(format!("creates per day is at most {MAX_CREATES_PER_DAY}"));
        }
        None
    }
}

/// The most a principal's scopes let it do on `project`: Act reaches as far as the rung does,
/// Propose only to proposing, anything less not even that.
pub fn ceiling_of(principal: &Principal, project: Uuid) -> RungLevel {
    if may(principal, project, Capability::Act) {
        RungLevel::Create
    } else if may(principal, project, Capability::Propose) {
        RungLevel::Propose
    } else {
        RungLevel::Read
    }
}

/// What the rung lets through for one operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Apply,
    Wait,
}

/// Which operations apply now and which wait, for a change set an agent proposes: the stricter
/// of the project's rung and the credential's `ceiling` wins. `creates_today` is how many
/// creates the rung already let through today. An error says why nothing is accepted at all
/// (the rung is Read).
///
/// An operation that needs a create that waits, waits with it.
pub fn decide(
    rung: &AgentsOnBoard,
    ceiling: RungLevel,
    operations: &[Operation],
    creates_today: u32,
) -> Result<Vec<Verdict>, String> {
    let level = rung.level.min(ceiling);
    if level == RungLevel::Read {
        return Err(if rung.level == RungLevel::Read {
            "this project lets agents read and talk only; change nothing here, or raise the rung in Settings, Automation"
                .to_string()
        } else {
            "this credential may read only; it needs the scope `propose` to propose changes"
                .to_string()
        });
    }
    let mut room_in_set = rung.creates_per_change_set;
    let mut room_today = rung.creates_per_day.saturating_sub(creates_today);
    let mut verdicts: Vec<Verdict> = operations
        .iter()
        .map(|operation| match (level, operation) {
            (RungLevel::Propose, _) => Verdict::Wait,
            (RungLevel::Act, operation) if operation.is_create() => Verdict::Wait,
            (RungLevel::Create, Operation::Create(_)) => {
                if room_in_set > 0 && room_today > 0 {
                    room_in_set -= 1;
                    room_today -= 1;
                    Verdict::Apply
                } else {
                    Verdict::Wait
                }
            }
            _ => Verdict::Apply,
        })
        .collect();
    loop {
        let mut changed = false;
        for (index, operation) in operations.iter().enumerate() {
            if verdicts[index] == Verdict::Apply
                && operation.needs().iter().any(|&need| {
                    verdicts
                        .get(need as usize)
                        .is_none_or(|verdict| *verdict == Verdict::Wait)
                })
            {
                verdicts[index] = Verdict::Wait;
                changed = true;
            }
        }
        if !changed {
            return Ok(verdicts);
        }
    }
}

// ─── Validation ─────────────────────────────────────────────

/// An existing item an operation names.
#[derive(Debug, Clone)]
pub struct ExistingItem {
    pub status: String,
}

/// The facts the operations of one change set are checked against, read by the caller from the
/// store: the project's workflow and vocabulary, the items the operations name, how many items
/// each column holds, the dependencies, and the roles and profiles.
#[derive(Debug, Clone)]
pub struct BoardView {
    pub workflow: WorkflowConfig,
    /// The project's type catalog.
    pub item_types: Vec<ItemTypeDef>,
    /// Only the items of this project that the operations name; any other id is not found.
    pub items: HashMap<Uuid, ExistingItem>,
    /// Items per status, over the whole project.
    pub occupied: HashMap<String, usize>,
    /// Every dependency of the project as (blocker, blocked).
    pub edges: Vec<(Uuid, Uuid)>,
    /// The project's roles by name.
    pub roles: Vec<(String, Uuid)>,
    /// Every profile id.
    pub profiles: HashSet<String>,
    /// Team name to the profiles that name it.
    pub profiles_by_team: HashMap<String, Vec<String>>,
    /// The parents of every item of the project, to check where the operations put items.
    pub forest: Forest,
    /// The type key of the item the creates hang under, when it is work; a group anchor gives
    /// `None`, and the creates take the default work type.
    pub anchor_type: Option<String>,
}

/// One reason a change set cannot be applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Problem {
    /// The operation it is about; absent when it is about the change set.
    pub operation: Option<usize>,
    pub message: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.operation {
            Some(index) => write!(f, "operation {index}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

/// Every reason on one line each, for an error a person reads.
pub fn describe(problems: &[Problem]) -> String {
    let lines: Vec<String> = problems.iter().map(Problem::to_string).collect();
    if let [only] = lines.as_slice() {
        return format!("The change set cannot be applied: {only}");
    }
    format!(
        "The change set cannot be applied, {} reasons:\n{}",
        lines.len(),
        lines.join("\n")
    )
}

/// How a `create` or `who_does_it` resolved against the project's roles, types and profiles.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Resolution {
    pub item_type: Option<String>,
    pub role: Option<Uuid>,
    pub profile: Option<String>,
    pub unknown_role: Option<String>,
    pub unknown_type: Option<String>,
    pub unresolved_team: Option<String>,
    pub unknown_profile: Option<String>,
}

/// A name from a plan, or `None` when it is absent or blank.
fn named(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

/// What a `create`'s item type, role and doer resolve to. A name the project lacks is reported
/// in the resolution and the item is made without it: a plan has always been read leniently.
fn resolve_create(create: &CreateOp, board: &BoardView) -> Resolution {
    let fallback = board
        .anchor_type
        .as_deref()
        .or_else(|| default_work_key(&board.item_types))
        .unwrap_or_default()
        .to_owned();
    let mut unknown_type = None;
    let item_type = match named(&create.item_type) {
        None => fallback,
        Some(wanted) => match board
            .item_types
            .iter()
            .find(|t| t.key == wanted.to_lowercase() || t.name.eq_ignore_ascii_case(wanted))
        {
            Some(found) => found.key.clone(),
            None => {
                unknown_type = Some(wanted.to_owned());
                fallback
            }
        },
    };
    let mut resolution = Resolution {
        item_type: Some(item_type),
        unknown_type,
        ..Resolution::default()
    };
    if let Some(wanted) = named(&create.role) {
        match board
            .roles
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
        {
            Some((_, id)) => resolution.role = Some(*id),
            None => resolution.unknown_role = Some(wanted.to_owned()),
        }
    }
    // A profile named by id wins over the team; an id no profile has is reported, and the team,
    // if any, is still tried.
    if let Some(wanted) = named(&create.profile_id) {
        if board.profiles.contains(wanted) {
            resolution.profile = Some(wanted.to_owned());
        } else {
            resolution.unknown_profile = Some(wanted.to_owned());
        }
    }
    if resolution.profile.is_none()
        && let Some(wanted) = named(&create.team)
    {
        match board.profiles_by_team.get(wanted).map(Vec::as_slice) {
            Some([profile]) => resolution.profile = Some(profile.clone()),
            _ => resolution.unresolved_team = Some(wanted.to_owned()),
        }
    }
    resolution
}

/// What a `who_does_it` resolves to; a name the project lacks is a reason, there being nothing
/// else for the operation to do.
fn resolve_doer(
    role: &Option<String>,
    team: &Option<String>,
    profile_id: &Option<String>,
    board: &BoardView,
    problems: &mut Vec<String>,
) -> Resolution {
    let mut resolution = Resolution::default();
    if named(role).is_none() && named(team).is_none() && named(profile_id).is_none() {
        problems.push("name a role, a team or a profile".into());
    }
    if let Some(wanted) = named(role) {
        match board
            .roles
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
        {
            Some((_, id)) => resolution.role = Some(*id),
            None => problems.push(format!("the project has no role \"{wanted}\"")),
        }
    }
    if let Some(wanted) = named(profile_id) {
        if board.profiles.contains(wanted) {
            resolution.profile = Some(wanted.to_owned());
        } else {
            problems.push(format!("no profile has the id \"{wanted}\""));
        }
    } else if let Some(wanted) = named(team) {
        match board.profiles_by_team.get(wanted).map(Vec::as_slice) {
            Some([profile]) => resolution.profile = Some(profile.clone()),
            Some(_) => problems.push(format!(
                "more than one profile names the team \"{wanted}\"; name a profile instead"
            )),
            None => problems.push(format!("no profile names the team \"{wanted}\"")),
        }
    }
    resolution
}

/// The facts a validation walks forward as it simulates the operations in order.
struct Simulation<'a> {
    board: &'a BoardView,
    /// Item to its status now.
    status: HashMap<Uuid, String>,
    occupied: HashMap<String, usize>,
    graph: DependencyGraph,
    /// The ids the creates of this set stand for: real for the ones already applied.
    ids: HashMap<usize, Uuid>,
    /// The tree as the operations leave it so far.
    forest: Forest,
}

impl Simulation<'_> {
    /// The item a ref names, or why there is none.
    fn item(&self, item: &ItemRef, operations: &[Operation]) -> Result<Uuid, String> {
        match item {
            ItemRef::Existing(id) if self.board.items.contains_key(id) => Ok(*id),
            ItemRef::Existing(id) => Err(format!("item {id} is not in this project")),
            ItemRef::New(index) => {
                let index = *index as usize;
                match operations.get(index) {
                    Some(operation) if operation.is_create() => {
                        self.ids.get(&index).copied().ok_or_else(|| {
                            format!("operation {index} makes this item and is not applied with it")
                        })
                    }
                    _ => Err(format!("operation {index} is not a create")),
                }
            }
        }
    }
}

impl Simulation<'_> {
    /// Puts `item` under `parent` (or at the top) when the tree rule admits it, as the store will
    /// when it applies the operation; otherwise the sentence that says why not.
    fn put_under(&mut self, item: Uuid, parent: Option<Uuid>) -> Result<(), String> {
        let placement = self
            .forest
            .placement(item)
            .ok_or_else(|| format!("item {item} is not in this project"))?;
        let target = match parent {
            Some(parent) => Some(
                self.forest
                    .target(parent)
                    .ok_or_else(|| format!("the parent: item {parent} is not in this project"))?,
            ),
            None => None,
        };
        admit_parent(&placement, target.as_ref()).map_err(|error| error.to_string())?;
        self.forest.set_parent(item, parent);
        Ok(())
    }
}

/// Checks the `selected` operations (positions in `operations`) against `board`, the way the
/// person's own routes check them, one after the other as they would be applied: creates first
/// in order, then the rest in order, then the dependencies the creates ask for. Answers how
/// each resolves, or every reason at once. `created` holds the items that operations applied
/// earlier made; a selected operation may name those.
pub fn validate(
    operations: &[Operation],
    selected: &[usize],
    board: &BoardView,
    created: &HashMap<usize, Uuid>,
) -> Result<HashMap<usize, Resolution>, Vec<Problem>> {
    let mut problems: Vec<Problem> = Vec::new();
    let mut resolutions: HashMap<usize, Resolution> = HashMap::new();
    if operations.is_empty() {
        return Err(vec![Problem {
            operation: None,
            message: "a change set needs at least one operation".into(),
        }]);
    }
    if operations.len() > MAX_OPERATIONS {
        return Err(vec![Problem {
            operation: None,
            message: format!("a change set holds at most {MAX_OPERATIONS} operations"),
        }]);
    }
    let mut graph = DependencyGraph::new();
    for &(blocker, blocked) in &board.edges {
        graph.add_edge(blocker, blocked, DependencyType::Blocks);
    }
    let mut sim = Simulation {
        board,
        status: board
            .items
            .iter()
            .map(|(id, item)| (*id, item.status.clone()))
            .collect(),
        occupied: board.occupied.clone(),
        graph,
        ids: created.clone().into_iter().collect(),
        forest: board.forest.clone(),
    };
    let initial = board.workflow.initial_status().ok();
    let mut report = |operation: usize, message: String| {
        problems.push(Problem {
            operation: Some(operation),
            message,
        })
    };

    // Creates, in order.
    for &index in selected {
        let Operation::Create(create) = &operations[index] else {
            continue;
        };
        let title = create.title.trim();
        if title.is_empty() || title.chars().count() > 500 {
            report(index, "the title must be 1 to 500 characters".into());
        }
        if create
            .description
            .as_ref()
            .is_some_and(|text| text.chars().count() > 50_000)
        {
            report(index, "the description is too long (at most 50 000)".into());
        }
        if create.acceptance.len() > 50 {
            report(index, "at most 50 acceptance criteria".into());
        }
        let mut parent_id = None;
        if let Some(parent) = &create.parent {
            match parent {
                ItemRef::New(earlier) if (*earlier as usize) >= index => report(
                    index,
                    "the parent must be an earlier operation of the change set".into(),
                ),
                other => match sim.item(other, operations) {
                    Ok(id) => parent_id = Some(id),
                    Err(message) => report(index, format!("the parent: {message}")),
                },
            }
        }
        let id = Uuid::new_v4();
        sim.ids.insert(index, id);
        match &initial {
            Some(initial) => {
                sim.status.insert(id, initial.clone());
                *sim.occupied.entry(initial.clone()).or_default() += 1;
            }
            None => report(index, "the project's workflow has no status".into()),
        }
        let resolution = resolve_create(create, board);
        let made = resolution
            .item_type
            .as_deref()
            .and_then(|key| board.item_types.iter().find(|t| t.key == key));
        sim.forest.insert(
            id,
            Entry {
                parent: None,
                title: create.title.trim().to_owned(),
                type_name: made.map(|t| t.name.clone()).unwrap_or_default(),
                level: made.map_or(Level::Work, |t| t.level),
            },
        );
        if let Some(parent) = parent_id
            && let Err(reason) = sim.put_under(id, Some(parent))
        {
            report(index, reason);
        }
        resolutions.insert(index, resolution);
    }

    // The other operations, in order.
    for &index in selected {
        let operation = &operations[index];
        let mut reasons: Vec<String> = Vec::new();
        match operation {
            Operation::Create(_) => {}
            Operation::WhoDoesIt {
                item,
                role,
                team,
                profile_id,
            } => {
                if let Err(message) = sim.item(item, operations) {
                    reasons.push(message);
                }
                let resolution = resolve_doer(role, team, profile_id, board, &mut reasons);
                resolutions.insert(index, resolution);
            }
            Operation::Priority { item, .. } => {
                if let Err(message) = sim.item(item, operations) {
                    reasons.push(message);
                }
            }
            Operation::Due { item, .. } => {
                if let Err(message) = sim.item(item, operations) {
                    reasons.push(message);
                }
            }
            Operation::Parent { item, parent } => {
                let item = sim.item(item, operations);
                let parent = parent.as_ref().map(|parent| sim.item(parent, operations));
                match (item, parent) {
                    (Ok(item), None) => {
                        let _ = sim.put_under(item, None);
                    }
                    (Ok(item), Some(Ok(parent))) => {
                        if let Err(reason) = sim.put_under(item, Some(parent)) {
                            reasons.push(reason);
                        }
                    }
                    (item, parent) => {
                        reasons.extend(item.err());
                        reasons.extend(parent.and_then(Result::err));
                    }
                }
            }
            Operation::Comment { item, text } => {
                if let Err(message) = sim.item(item, operations) {
                    reasons.push(message);
                }
                if text.trim().is_empty() || text.chars().count() > 10_000 {
                    reasons.push("a comment is 1 to 10 000 characters".into());
                }
            }
            Operation::Move { item, status } => match sim.item(item, operations) {
                Err(message) => reasons.push(message),
                Ok(id) => {
                    let from = sim.status.get(&id).cloned().unwrap_or_default();
                    let occupied = sim.occupied.get(status).copied().unwrap_or(0);
                    match board.workflow.admit_move(&from, status, occupied) {
                        Ok(_) => {
                            if &from != status {
                                if let Some(count) = sim.occupied.get_mut(&from) {
                                    *count = count.saturating_sub(1);
                                }
                                *sim.occupied.entry(status.clone()).or_default() += 1;
                                sim.status.insert(id, status.clone());
                            }
                        }
                        Err(error) => reasons.push(format!("moving {id}: {error}")),
                    }
                }
            },
            Operation::Depend { blocker, blocked } => {
                match (sim.item(blocker, operations), sim.item(blocked, operations)) {
                    (Ok(blocker), Ok(blocked)) => {
                        if board.edges.contains(&(blocker, blocked)) {
                            reasons.push(format!("{blocker} already blocks {blocked}"));
                        } else if sim.graph.validate_new_edge(blocker, blocked).is_err() {
                            reasons.push(format!(
                                "{blocker} blocking {blocked} would make a loop of dependencies"
                            ));
                        } else {
                            sim.graph.add_edge(blocker, blocked, DependencyType::Blocks);
                        }
                    }
                    (first, second) => {
                        reasons.extend(first.err());
                        reasons.extend(second.err());
                    }
                }
            }
        }
        for message in reasons {
            report(index, message);
        }
    }

    // The dependencies the creates ask for, in order.
    for &index in selected {
        let Operation::Create(create) = &operations[index] else {
            continue;
        };
        let Some(&blocked) = sim.ids.get(&index) else {
            continue;
        };
        for blocker in &create.depends_on {
            if *blocker == ItemRef::New(index as u32) {
                report(index, "an entry cannot depend on itself".into());
                continue;
            }
            match sim.item(blocker, operations) {
                Err(message) => report(index, format!("depends_on: {message}")),
                Ok(blocker) => {
                    if sim.graph.validate_new_edge(blocker, blocked).is_err() {
                        report(
                            index,
                            format!(
                                "{blocker} blocking {blocked} would make a loop of dependencies"
                            ),
                        );
                    } else {
                        sim.graph.add_edge(blocker, blocked, DependencyType::Blocks);
                    }
                }
            }
        }
    }

    if problems.is_empty() {
        Ok(resolutions)
    } else {
        Err(problems)
    }
}

#[cfg(test)]
#[path = "change_set/tests.rs"]
mod tests;
