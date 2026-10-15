//! A project outline: the small shape a person, a script or a language model
//! writes to create a project and many items in one request, and the rules
//! that turn it into rows.
//!
//! An outline item names its parent and the items it waits on by a `ref` the
//! writer picks, or by another item's exact title, so nothing in the file has
//! to know a database id. [`resolve`] checks the whole outline before anything
//! is written — every reference resolves, no parent or dependency chain loops,
//! every field passes the same validation as `POST /api/projects/{id}/items` —
//! and reports every problem at once, so a model that wrote the file can fix
//! it in one turn. Vocabulary the workflow does not know (a status, a
//! priority) is not a reason to refuse a file a spreadsheet produced: the item
//! keeps the default and the result says so in `warnings`.

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::models::{CreateItem, ItemType, Priority, ProjectType};
use crate::workflow::WorkflowConfig;

mod csv;
pub use self::csv::{CsvOutline, items_from_csv, items_to_csv};

/// The most items one outline may create. Large enough for any real plan,
/// small enough that one request cannot hold the database's write lock for
/// long.
pub const MAX_OUTLINE_ITEMS: usize = 1000;

/// What `POST /api/projects/outline` and `POST /api/projects/{id}/outline`
/// read, as JSON or YAML. Unknown keys are refused rather than dropped, so a
/// misspelt field is an error the writer sees instead of data that vanishes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Outline {
    /// The project to create. Read only when the outline creates a project.
    #[serde(default)]
    pub project: Option<OutlineProject>,
    #[serde(default)]
    pub items: Vec<OutlineItem>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct OutlineProject {
    #[serde(default)]
    pub name: Option<String>,
    /// software, web, mobile, construction, personal, homework, maintenance,
    /// legal, research, event or custom (the default).
    #[serde(default, alias = "type")]
    pub project_type: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct OutlineItem {
    /// A name other items use in `parent` and `depends_on`. Any text or
    /// number, unique within the outline.
    #[serde(
        default,
        rename = "ref",
        alias = "id",
        alias = "key",
        deserialize_with = "opt_text"
    )]
    pub reference: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    /// epic, feature, task (the default), subtask, bug, requirement, or any
    /// other word as a custom type.
    #[serde(default, rename = "type", alias = "item_type")]
    pub item_type: Option<String>,
    /// A status of the project's workflow, any case. Unknown or absent: the
    /// workflow's first status.
    #[serde(default)]
    pub status: Option<String>,
    /// critical, high, medium (the default), low or none.
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub estimate: Option<f64>,
    /// `YYYY-MM-DD` or an RFC 3339 timestamp.
    #[serde(default, alias = "due")]
    pub due_date: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// weekly, monthly or quarterly; absent: the task does not repeat.
    #[serde(default)]
    pub recurrence: Option<String>,
    /// Another item's `ref` or exact title; when adding to an existing
    /// project, also the id of an item already in it.
    #[serde(
        default,
        alias = "parent_id",
        alias = "parent_ref",
        deserialize_with = "opt_text"
    )]
    pub parent: Option<String>,
    /// Items that must finish before this one, by `ref` or exact title.
    #[serde(default, alias = "blocked_by", deserialize_with = "text_list")]
    pub depends_on: Vec<String>,
    /// Where the item came from in its file ("row 7"), for messages. Set by
    /// the CSV reader; an outline item is named by its position instead.
    #[serde(skip)]
    pub origin: Option<String>,
}

/// An outline checked against a workflow, in the order its rows can be
/// written: every item comes after the item it names as parent.
#[derive(Debug)]
pub struct ResolvedOutline {
    pub items: Vec<PlannedItem>,
    /// `(blocker, blocked)` pairs, as indexes into `items`.
    pub blocks: Vec<(usize, usize)>,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct PlannedItem {
    pub reference: Option<String>,
    pub status: String,
    pub parent: Option<PlannedParent>,
    /// Everything but `parent_id`, which only exists once the parent's row
    /// does.
    pub input: CreateItem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlannedParent {
    /// An earlier entry of [`ResolvedOutline::items`].
    Planned(usize),
    /// An item that already exists; the caller checks it belongs to the
    /// project.
    Existing(Uuid),
}

/// Every reason an outline cannot be written, in the order found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineError {
    pub problems: Vec<String>,
}

impl OutlineError {
    pub fn one(problem: impl Into<String>) -> Self {
        Self {
            problems: vec![problem.into()],
        }
    }
}

impl fmt::Display for OutlineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.problems.as_slice() {
            [only] => write!(f, "{only}"),
            all => write!(
                f,
                "{} problems in the outline: {}",
                all.len(),
                all.join("; ")
            ),
        }
    }
}

impl std::error::Error for OutlineError {}

/// Parse a project type the way a person writes it: any case, with the
/// valid values named when it is none of them.
pub fn parse_project_type(text: &str) -> Result<ProjectType, String> {
    let wanted = text.trim().to_lowercase();
    serde_json::from_value(serde_json::Value::String(wanted)).map_err(|_| {
        format!(
            "project type \"{}\" is not one of software, web, mobile, construction, personal, \
             homework, maintenance, legal, research, event, custom",
            text.trim()
        )
    })
}

/// Read an item type leniently: the built-in names in any case, anything
/// else as a custom type.
pub fn parse_item_type(text: &str) -> Option<ItemType> {
    match text.trim().to_lowercase().as_str() {
        "" => None,
        "epic" => Some(ItemType::Epic),
        "feature" => Some(ItemType::Feature),
        "task" => Some(ItemType::Task),
        "subtask" => Some(ItemType::Subtask),
        "bug" => Some(ItemType::Bug),
        "requirement" => Some(ItemType::Requirement),
        other => Some(ItemType::Custom(other.to_string())),
    }
}

fn parse_priority(text: &str) -> Option<Priority> {
    match text.trim().to_lowercase().as_str() {
        "critical" | "urgent" => Some(Priority::Critical),
        "high" => Some(Priority::High),
        "medium" | "normal" => Some(Priority::Medium),
        "low" => Some(Priority::Low),
        "none" => Some(Priority::None),
        _ => None,
    }
}

/// A date as `YYYY-MM-DD` (midnight UTC) or a full RFC 3339 timestamp.
pub fn parse_due_date(text: &str) -> Option<DateTime<Utc>> {
    let text = text.trim();
    if let Ok(at) = DateTime::parse_from_rfc3339(text) {
        return Some(at.with_timezone(&Utc));
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|d| d.and_utc())
}

enum Lookup {
    Found(usize),
    Ambiguous(usize),
    Missing,
}

struct Index<'a> {
    by_ref: HashMap<&'a str, usize>,
    by_title: HashMap<&'a str, Vec<usize>>,
}

impl Index<'_> {
    /// A `ref` wins over a title, so a writer can always disambiguate two
    /// items with the same title by giving them refs.
    fn find(&self, key: &str) -> Lookup {
        if let Some(&i) = self.by_ref.get(key) {
            return Lookup::Found(i);
        }
        match self.by_title.get(key).map(Vec::as_slice) {
            Some([i]) => Lookup::Found(*i),
            Some(many) => Lookup::Ambiguous(many.len()),
            None => Lookup::Missing,
        }
    }
}

fn trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

fn label(position: usize, item: &OutlineItem) -> String {
    let at = item
        .origin
        .clone()
        .unwrap_or_else(|| format!("item {}", position + 1));
    match item.title.trim() {
        "" => at,
        title => format!("{at} (\"{title}\")"),
    }
}

/// Check an outline against the workflow its items will live in and order it
/// for writing. Either every item can be written or nothing is returned.
pub fn resolve(
    items: &[OutlineItem],
    workflow: &WorkflowConfig,
) -> Result<ResolvedOutline, OutlineError> {
    if items.len() > MAX_OUTLINE_ITEMS {
        return Err(OutlineError::one(format!(
            "the outline has {} items; one request creates at most {MAX_OUTLINE_ITEMS}",
            items.len()
        )));
    }
    let initial_status = workflow
        .initial_status()
        .map_err(|e| OutlineError::one(e.to_string()))?;
    let labels: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(i, item)| label(i, item))
        .collect();
    let mut problems = Vec::new();
    let mut warnings = Vec::new();

    let mut index = Index {
        by_ref: HashMap::new(),
        by_title: HashMap::new(),
    };
    for (i, item) in items.iter().enumerate() {
        if let Some(reference) = trimmed(item.reference.as_deref()) {
            match index.by_ref.get(reference) {
                Some(&first) => problems.push(format!(
                    "{} reuses ref \"{reference}\" of {}",
                    labels[i], labels[first]
                )),
                None => {
                    index.by_ref.insert(reference, i);
                }
            }
        }
        let title = item.title.trim();
        if !title.is_empty() {
            index.by_title.entry(title).or_default().push(i);
        }
    }

    let mut parents: Vec<Option<PlannedParent>> = Vec::with_capacity(items.len());
    let mut blocks: Vec<(usize, usize)> = Vec::new();
    let mut inputs: Vec<Option<(String, CreateItem)>> = Vec::with_capacity(items.len());

    for (i, item) in items.iter().enumerate() {
        let at = &labels[i];
        parents.push(resolve_parent(item, i, at, &index, &mut problems));
        for key in item
            .depends_on
            .iter()
            .map(|k| k.trim())
            .filter(|k| !k.is_empty())
        {
            match index.find(key) {
                Lookup::Found(j) if j == i => problems.push(format!("{at} depends on itself")),
                Lookup::Found(j) => {
                    if !blocks.contains(&(j, i)) {
                        blocks.push((j, i));
                    }
                }
                Lookup::Ambiguous(n) => problems.push(format!(
                    "{at} depends on \"{key}\", which is the title of {n} items; give them refs"
                )),
                Lookup::Missing => problems.push(format!(
                    "{at} depends on \"{key}\", which is no ref or title in the outline"
                )),
            }
        }
        inputs.push(item_input(
            item,
            at,
            workflow,
            &initial_status,
            &mut problems,
            &mut warnings,
        ));
    }

    let parent_edges: Vec<(usize, usize)> = parents
        .iter()
        .enumerate()
        .filter_map(|(child, parent)| match parent {
            Some(PlannedParent::Planned(p)) => Some((*p, child)),
            _ => None,
        })
        .collect();
    let order = topological(items.len(), &parent_edges);
    if order.len() < items.len() {
        problems.push(format!(
            "the parents of {} form a loop",
            unplaced(&order, &labels)
        ));
    }
    let blocked_order = topological(items.len(), &blocks);
    if blocked_order.len() < items.len() {
        problems.push(format!(
            "depends_on forms a loop through {}",
            unplaced(&blocked_order, &labels)
        ));
    }
    if !problems.is_empty() {
        return Err(OutlineError { problems });
    }

    let mut position = vec![0usize; items.len()];
    for (new, &old) in order.iter().enumerate() {
        position[old] = new;
    }
    let mut planned = Vec::with_capacity(items.len());
    for &old in &order {
        let (status, input) = inputs[old].take().expect("every item resolved");
        planned.push(PlannedItem {
            reference: trimmed(items[old].reference.as_deref()).map(str::to_string),
            status,
            parent: parents[old].map(|p| match p {
                PlannedParent::Planned(j) => PlannedParent::Planned(position[j]),
                existing => existing,
            }),
            input,
        });
    }
    Ok(ResolvedOutline {
        items: planned,
        blocks: blocks
            .into_iter()
            .map(|(a, b)| (position[a], position[b]))
            .collect(),
        warnings,
    })
}

fn resolve_parent(
    item: &OutlineItem,
    i: usize,
    at: &str,
    index: &Index<'_>,
    problems: &mut Vec<String>,
) -> Option<PlannedParent> {
    let key = trimmed(item.parent.as_deref())?;
    match index.find(key) {
        Lookup::Found(j) if j == i => {
            problems.push(format!("{at} is its own parent"));
            None
        }
        Lookup::Found(j) => Some(PlannedParent::Planned(j)),
        Lookup::Ambiguous(n) => {
            problems.push(format!(
                "{at} names parent \"{key}\", which is the title of {n} items; give them refs"
            ));
            None
        }
        Lookup::Missing => match Uuid::parse_str(key) {
            Ok(id) => Some(PlannedParent::Existing(id)),
            Err(_) => {
                problems.push(format!(
                    "{at} names parent \"{key}\", which is no ref or title in the outline"
                ));
                None
            }
        },
    }
}

fn item_input(
    item: &OutlineItem,
    at: &str,
    workflow: &WorkflowConfig,
    initial_status: &str,
    problems: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Option<(String, CreateItem)> {
    let title = item.title.trim();
    if title.is_empty() {
        problems.push(format!("{at} has no title"));
        return None;
    }
    let status = match trimmed(item.status.as_deref()) {
        None => initial_status.to_string(),
        Some(wanted) => match workflow
            .statuses
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(wanted))
        {
            Some(found) => found.name.clone(),
            None => {
                warnings.push(format!(
                    "{at}: status \"{wanted}\" is not in this workflow ({}), so it starts in \
                     {initial_status}",
                    workflow.status_names().join(", ")
                ));
                initial_status.to_string()
            }
        },
    };
    let priority = trimmed(item.priority.as_deref()).and_then(|wanted| {
        let parsed = parse_priority(wanted);
        if parsed.is_none() {
            warnings.push(format!(
                "{at}: priority \"{wanted}\" is not critical, high, medium, low or none, so it \
                 is medium"
            ));
        }
        parsed
    });
    let due_date = match trimmed(item.due_date.as_deref()) {
        None => None,
        Some(text) => match parse_due_date(text) {
            Some(at) => Some(at),
            None => {
                problems.push(format!(
                    "{at}: due date \"{text}\" is not a date (write YYYY-MM-DD)"
                ));
                None
            }
        },
    };
    let recurrence = trimmed(item.recurrence.as_deref()).and_then(|wanted| {
        let parsed = wanted.parse().ok();
        if parsed.is_none() {
            warnings.push(format!(
                "{at}: recurrence \"{wanted}\" is not weekly, monthly or quarterly, so the \
                 task does not repeat"
            ));
        }
        parsed
    });
    let tags: Vec<String> = item
        .tags
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect();
    let input = CreateItem {
        title: title.to_string(),
        description: trimmed(item.description.as_deref()).map(str::to_string),
        item_type: item.item_type.as_deref().and_then(parse_item_type),
        parent_id: None,
        priority,
        estimate: item.estimate,
        estimate_unit: None,
        tags: (!tags.is_empty()).then_some(tags),
        due_date,
        sprint_id: None,
        assignee: trimmed(item.assignee.as_deref()).map(str::to_string),
        recurrence,
        recurrence_of: None,
        source_artifact_id: None,
    };
    if let Err(e) = input.validate() {
        problems.push(format!("{at}: {e}"));
        return None;
    }
    Some((status, input))
}

/// Kahn's algorithm over `edges` (`(before, after)`), always taking the
/// earliest ready position, so the written order is the file's order
/// wherever the edges allow it. Fewer nodes than `n` in the result means the
/// rest sit on a loop.
fn topological(n: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut incoming = vec![0usize; n];
    let mut after: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(a, b) in edges {
        incoming[b] += 1;
        after[a].push(b);
    }
    let mut ready: BTreeSet<usize> = (0..n).filter(|&i| incoming[i] == 0).collect();
    let mut order = Vec::with_capacity(n);
    while let Some(next) = ready.pop_first() {
        order.push(next);
        for &b in &after[next] {
            incoming[b] -= 1;
            if incoming[b] == 0 {
                ready.insert(b);
            }
        }
    }
    order
}

fn unplaced(order: &[usize], labels: &[String]) -> String {
    let mut placed = vec![false; labels.len()];
    for &i in order {
        placed[i] = true;
    }
    labels
        .iter()
        .zip(placed)
        .filter(|(_, p)| !p)
        .map(|(l, _)| l.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// A string or a number, as text: a model writing `"ref": 1` means the same
/// as `"ref": "1"`.
#[derive(Deserialize)]
#[serde(untagged)]
enum Text {
    String(String),
    Integer(i64),
    Float(f64),
}

impl From<Text> for String {
    fn from(value: Text) -> Self {
        match value {
            Text::String(s) => s,
            Text::Integer(n) => n.to_string(),
            Text::Float(n) => n.to_string(),
        }
    }
}

fn opt_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Ok(Option::<Text>::deserialize(deserializer)?.map(String::from))
}

/// One value or a list of them, each a string or a number.
fn text_list<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(Text),
        Many(Vec<Text>),
    }
    Ok(match Option::<OneOrMany>::deserialize(deserializer)? {
        None => Vec::new(),
        Some(OneOrMany::One(t)) => vec![t.into()],
        Some(OneOrMany::Many(all)) => all.into_iter().map(String::from).collect(),
    })
}

#[cfg(test)]
mod tests;
