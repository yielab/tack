//! The comparison of a project's plan on the board, in a project folder and at the last revision.

use std::collections::BTreeSet;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::outline::key::key_of;
use crate::outline::{Outline, OutlineItem};

pub struct Three<'a> {
    pub base: Option<&'a Outline>,
    pub board: &'a Outline,
    pub file: &'a Outline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Prefer {
    File,
    Board,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Verb {
    Create,
    Update,
    Remove,
    Keep,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FieldChange {
    pub path: String,
    pub from: Value,
    pub to: Value,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Change {
    pub section: &'static str,
    pub key: String,
    pub name: String,
    pub verb: Verb,
    pub fields: Vec<FieldChange>,
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Conflict {
    pub section: &'static str,
    pub key: String,
    pub name: String,
    pub path: String,
    pub base: Value,
    pub board: Value,
    pub file: Value,
}

#[derive(Debug)]
pub struct Merged {
    pub outline: Outline,
    pub changes: Vec<Change>,
    pub conflicts: Vec<Conflict>,
}

/// What `merge` and `diff` say about a thing the board keeps for itself.
const THE_BOARDS: &str = "the board's";
const REMOVED_ON_THE_BOARD: &str = "removed on the board since the last revision";
const CREATED_ON_THE_BOARD: &str = "created on the board since the last revision";
const NO_REVISION: &str = "no revision yet: export once to make the folder authoritative";

/// What an item has that only the board writes; a file's value for an item that exists is kept out.
const ITEM_BOARD_KEYS: [&str; 5] = ["status", "priority", "assignee", "sprint", "due_date"];
/// A project's type and workflow are never written from a file.
const PROJECT_BOARD_KEYS: [&str; 2] = ["type", "workflow"];

/// A value that is not there, for a path one side does not have.
static NOTHING: Value = Value::Null;

/// One thing of a section, as the comparison sees it.
struct Entry {
    key: String,
    name: String,
    /// The whole thing, as its struct serializes.
    full: Value,
    /// What is compared: the whole without its key and without what the board keeps for itself.
    plan: Value,
}

type Step = (Option<Value>, Vec<Change>, Vec<Conflict>);
type Raw = (String, Value, Value, Value);

impl Entry {
    fn new(section: &str, full: Value) -> Self {
        let name = full
            .get("name")
            .or_else(|| full.get("title"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let mut plan = full.clone();
        if let Value::Object(map) = &mut plan {
            for left_out in board_keys(section).iter().chain(&["key", "ref"]) {
                map.remove(*left_out);
            }
        }
        if matches!(section, "project" | "automation") {
            let name = if name.is_empty() {
                section.to_owned()
            } else {
                name
            };
            return Self {
                key: section.to_owned(),
                name,
                full,
                plan,
            };
        }
        let key = entry_key(full.get("key").and_then(Value::as_str), &name);
        Self {
            key,
            name,
            full,
            plan,
        }
    }
}

/// The identity of a thing: the key it has, else the key of its name.
fn entry_key(key: Option<&str>, name: &str) -> String {
    key.map_or_else(|| key_of(name), str::to_owned)
}

fn board_keys(section: &str) -> &'static [&'static str] {
    match section {
        "items" => &ITEM_BOARD_KEYS,
        "project" => &PROJECT_BOARD_KEYS,
        _ => &[],
    }
}

/// The plan of a project on the board, in a file, and as the last revision had it. A field both
/// changed differently is a conflict, listed whether or not `prefer` settles it, so the plan shows
/// what was chosen; the merged outline takes `prefer`'s side, or the board's for a conflict left
/// open. Pure: removals are proposed, never carried out.
pub fn merge(three: &Three<'_>, prefer: Option<Prefer>) -> Merged {
    let (base, board, file) = (three.base, three.board, three.file);
    let (mut changes, mut conflicts) = (Vec::new(), Vec::new());
    let (c, k) = (&mut changes, &mut conflicts);
    let project = one(
        "project",
        base.map(|o| &o.project),
        &board.project,
        &file.project,
        prefer,
        c,
        k,
    );
    let types = many(
        "types",
        base.map(|o| o.types.as_slice()),
        &board.types,
        &file.types,
        prefer,
        c,
        k,
    );
    let fields = many(
        "fields",
        base.map(|o| o.fields.as_slice()),
        &board.fields,
        &file.fields,
        prefer,
        c,
        k,
    );
    let profiles = many(
        "profiles",
        base.map(|o| o.profiles.as_slice()),
        &board.profiles,
        &file.profiles,
        prefer,
        c,
        k,
    );
    let roles = many(
        "roles",
        base.map(|o| o.roles.as_slice()),
        &board.roles,
        &file.roles,
        prefer,
        c,
        k,
    );
    let sprints = many(
        "sprints",
        base.map(|o| o.sprints.as_slice()),
        &board.sprints,
        &file.sprints,
        prefer,
        c,
        k,
    );
    let automation = one(
        "automation",
        base.map(|o| &o.automation),
        &board.automation,
        &file.automation,
        prefer,
        c,
        k,
    );
    let items = many(
        "items",
        base.map(|o| o.items.as_slice()),
        &board.items,
        &file.items,
        prefer,
        c,
        k,
    );
    let outline = Outline {
        project,
        types,
        fields,
        profiles,
        roles,
        sprints,
        automation,
        teams: board.teams.clone(),
        items,
    };
    Merged {
        outline,
        changes,
        conflicts,
    }
}

/// What it takes to turn `from` into `to`: a thing only `to` has is created, one only `from` has
/// is removed, a field that differs is updated. Nothing to settle, so no conflict.
pub fn diff(from: &Outline, to: &Outline) -> Vec<Change> {
    merge(
        &Three {
            base: Some(from),
            board: from,
            file: to,
        },
        None,
    )
    .changes
}

/// A section of one thing, compared only when the file has it.
fn one<T: Clone + Serialize + DeserializeOwned>(
    section: &'static str,
    base: Option<&Option<T>>,
    board: &Option<T>,
    file: &Option<T>,
    prefer: Option<Prefer>,
    changes: &mut Vec<Change>,
    conflicts: &mut Vec<Conflict>,
) -> Option<T> {
    if file.is_none() {
        return board.clone();
    }
    let base = base.map(Option::as_slice);
    many(
        section,
        base,
        board.as_slice(),
        file.as_slice(),
        prefer,
        changes,
        conflicts,
    )
    .into_iter()
    .next()
}

/// A section: the file's things in the file's order, then the board's own by name.
fn many<T: Serialize + DeserializeOwned>(
    section: &'static str,
    base: Option<&[T]>,
    board: &[T],
    file: &[T],
    prefer: Option<Prefer>,
    changes: &mut Vec<Change>,
    conflicts: &mut Vec<Conflict>,
) -> Vec<T> {
    let entries = |list: &[T]| -> Vec<Entry> {
        list.iter()
            .filter_map(|thing| serde_json::to_value(thing).ok())
            .map(|full| Entry::new(section, full))
            .collect()
    };
    let base = base.map(entries);
    let in_base = |key: &str| {
        base.as_ref()
            .and_then(|list| list.iter().find(|e| e.key == key))
    };
    let (board, file) = (entries(board), entries(file));
    let mut merged = Vec::new();
    let mut record = |(value, mut more, mut open): Step| {
        merged.extend(value);
        changes.append(&mut more);
        conflicts.append(&mut open);
    };
    for f in &file {
        record(match board.iter().find(|b| b.key == f.key) {
            Some(b) => both(section, b, f, in_base(&f.key), prefer),
            None => file_only(section, f, in_base(&f.key), prefer),
        });
    }
    let mut board_only_things: Vec<&Entry> = board
        .iter()
        .filter(|b| !file.iter().any(|f| f.key == b.key))
        .collect();
    board_only_things.sort_by(|a, b| a.name.cmp(&b.name));
    for b in board_only_things {
        record(board_only(
            section,
            b,
            base.is_some(),
            in_base(&b.key),
            prefer,
        ));
    }
    // A merged thing is the board's or the file's, field by field, so it reads back as its struct.
    merged
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect()
}

fn change(section: &'static str, e: &Entry, verb: Verb, reason: Option<&str>) -> Change {
    Change {
        section,
        key: e.key.clone(),
        name: e.name.clone(),
        verb,
        fields: Vec::new(),
        reason: reason.map(str::to_owned),
    }
}

fn conflict(section: &'static str, e: &Entry, (path, base, board, file): Raw) -> Conflict {
    Conflict {
        section,
        key: e.key.clone(),
        name: e.name.clone(),
        path,
        base,
        board,
        file,
    }
}

/// A thing on both sides: its fields one by one, and a note for what the file says of the
/// board's own keys.
fn both(
    section: &'static str,
    b: &Entry,
    f: &Entry,
    base: Option<&Entry>,
    prefer: Option<Prefer>,
) -> Step {
    let (mut fields, mut open) = (Vec::new(), Vec::new());
    let plan = diff_values(
        base.map(|e| &e.plan),
        &b.plan,
        &f.plan,
        "",
        prefer,
        &mut fields,
        &mut open,
    );
    let mut merged = match plan {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    for left_out in board_keys(section).iter().chain(&["key", "ref"]) {
        // An item on both sides is updated in its plan only: its key goes back, the board's own
        // keys stay out, or the check would read them as the file asking to change them.
        if section == "items" && *left_out != "key" {
            continue;
        }
        if let Some(value) = b.full.get(*left_out) {
            merged.insert((*left_out).to_owned(), value.clone());
        }
    }
    let mut changes = Vec::new();
    if !fields.is_empty() {
        changes.push(Change {
            fields,
            ..change(section, b, Verb::Update, None)
        });
    }
    let kept: Vec<FieldChange> = board_keys(section)
        .iter()
        .filter_map(|name| {
            let (board, file) = (b.full.get(*name)?, f.full.get(*name)?);
            (board != file).then(|| FieldChange {
                path: (*name).to_owned(),
                from: board.clone(),
                to: file.clone(),
            })
        })
        .collect();
    if !kept.is_empty() {
        changes.push(Change {
            fields: kept,
            ..change(section, b, Verb::Keep, Some(THE_BOARDS))
        });
    }
    let open = open
        .into_iter()
        .map(|raw| conflict(section, b, raw))
        .collect();
    (Some(Value::Object(merged)), changes, open)
}

/// A thing only the file has: new, or removed on the board since the last revision.
fn file_only(
    section: &'static str,
    f: &Entry,
    base: Option<&Entry>,
    prefer: Option<Prefer>,
) -> Step {
    let create = || {
        (
            Some(f.full.clone()),
            vec![change(section, f, Verb::Create, None)],
            vec![],
        )
    };
    let kept = || {
        (
            None,
            vec![change(section, f, Verb::Keep, Some(REMOVED_ON_THE_BOARD))],
            vec![],
        )
    };
    match base {
        None => create(),
        Some(old) if old.plan == f.plan => kept(),
        Some(old) => {
            let raw = (
                String::new(),
                old.plan.clone(),
                NOTHING.clone(),
                f.plan.clone(),
            );
            let open = vec![conflict(section, f, raw)];
            let (value, notes, _) = match prefer {
                Some(Prefer::File) => create(),
                Some(Prefer::Board) => kept(),
                None => (None, vec![], vec![]),
            };
            (value, notes, open)
        }
    }
}

/// What a thing the board has goes into the merged outline as: for an item its plan and its key,
/// never the board's own keys, which the check would read as the file asking to change them.
fn as_merged(section: &str, entry: &Entry) -> Value {
    if section != "items" {
        return entry.full.clone();
    }
    let mut value = entry.plan.clone();
    if let (Value::Object(map), Some(key)) = (&mut value, entry.full.get("key")) {
        map.insert("key".to_owned(), key.clone());
    }
    value
}

/// [`as_merged`] for an item the caller puts back in a merged outline, such as one a removal
/// keeps because it has history.
pub fn without_board_keys(item: &OutlineItem) -> OutlineItem {
    let mut value = serde_json::to_value(item).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut value {
        for name in ITEM_BOARD_KEYS {
            map.remove(name);
        }
    }
    serde_json::from_value(value).unwrap_or_else(|_| item.clone())
}

/// A thing only the board has: a removal the file asks for, or something the file never saw.
fn board_only(
    section: &'static str,
    b: &Entry,
    has_base: bool,
    base: Option<&Entry>,
    prefer: Option<Prefer>,
) -> Step {
    let keep = |reason: Option<&str>| {
        let note = reason.map(|r| change(section, b, Verb::Keep, Some(r)));
        (
            Some(as_merged(section, b)),
            note.into_iter().collect(),
            vec![],
        )
    };
    let remove = || (None, vec![change(section, b, Verb::Remove, None)], vec![]);
    match base {
        _ if !has_base => keep(Some(NO_REVISION)),
        None => keep(Some(CREATED_ON_THE_BOARD)),
        Some(old) if old.plan == b.plan => remove(),
        Some(old) => {
            let raw = (
                String::new(),
                old.plan.clone(),
                b.plan.clone(),
                NOTHING.clone(),
            );
            let open = vec![conflict(section, b, raw)];
            let (value, notes, _) = match prefer {
                Some(Prefer::File) => remove(),
                Some(Prefer::Board) => keep(None),
                None => (Some(as_merged(section, b)), vec![], vec![]),
            };
            (value, notes, open)
        }
    }
}

/// The three-way rule over objects, by dotted path; arrays and scalars are leaves. Returns the
/// merged value, and records each field the file's value was taken for and each conflict.
fn diff_values(
    base: Option<&Value>,
    board: &Value,
    file: &Value,
    path: &str,
    prefer: Option<Prefer>,
    fields: &mut Vec<FieldChange>,
    conflicts: &mut Vec<Raw>,
) -> Value {
    if let (Value::Object(b), Value::Object(f)) = (board, file) {
        let names: BTreeSet<&String> = b.keys().chain(f.keys()).collect();
        let mut merged = Map::new();
        for name in names {
            let at = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}.{name}")
            };
            let side = |value: &Value| value.get(name.as_str()).unwrap_or(&NOTHING).clone();
            let value = diff_values(
                base.map(|old| old.get(name.as_str()).unwrap_or(&NOTHING)),
                &side(board),
                &side(file),
                &at,
                prefer,
                fields,
                conflicts,
            );
            if !value.is_null() {
                merged.insert(name.clone(), value);
            }
        }
        return Value::Object(merged);
    }
    let same = |a: &Value, b: &Value| as_set(path, a) == as_set(path, b);
    if same(board, file) {
        return board.clone();
    }
    let take_file = match base {
        Some(old) if same(old, board) => true,
        Some(old) if same(old, file) => false,
        _ => {
            let old = base.cloned().unwrap_or(Value::Null);
            conflicts.push((path.to_owned(), old, board.clone(), file.clone()));
            prefer == Some(Prefer::File)
        }
    };
    if !take_file {
        return board.clone();
    }
    let (from, to) = (board.clone(), file.clone());
    fields.push(FieldChange {
        path: path.to_owned(),
        from,
        to: to.clone(),
    });
    to
}

/// `depends_on` and `roles` are sets: the order they are written in is not a change.
fn as_set(path: &str, value: &Value) -> Value {
    match (path, value) {
        ("depends_on" | "roles", Value::Array(items)) => {
            let mut sorted = items.clone();
            sorted.sort_by_key(Value::to_string);
            Value::Array(sorted)
        }
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests;
