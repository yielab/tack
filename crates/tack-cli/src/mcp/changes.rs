//! Change sets over MCP (ADR 0034, decision 3): the tools `propose` and `get_change_set`, and
//! how an agent's writes become change sets.
//!
//! An agent credential changes the board only through a change set, so under one `create_item`,
//! `create_outline`, `update_item`, `move_item` and `add_comment` build their change here and
//! propose it instead of writing; the project's rung decides what applies at once. The
//! operator's token writes directly, as it always did. The one exception is `update_item` asked
//! for `who_does_it` or `depends_on`, which only a change set can express: it proposes under
//! either token, and the operator's applies at once.

use std::collections::HashMap;

use serde_json::{Value, json};

use super::projection::{card_fence_id, fence_card_text, fence_field};
use super::tool::{Kind, Spec, Tool, call, check_id, opt_str, require_id, require_str};
use crate::client::TackClient;

const OPERATIONS_HELP: &str = "Each operation is an object with an `op`: \
     `create` {title, description?, acceptance?, type?, role?, team?, profile_id?, parent?, depends_on?}; \
     `who_does_it` {item, role? | team? | profile_id?}; `priority` {item, priority}; \
     `move` {item, status}; `depend` {blocker, blocked}; `comment` {item, text}; \
     `due` {item, due: RFC3339 or null}; `parent` {item, parent?} puts an item under `parent`, or at \
     the top without it. An `item` is an existing item's id, or the position (from 0) of a \
     `create` earlier in the list; a `parent` must be earlier. A group never goes under work, the \
     tree has at most five levels and no loops.";

pub static PROPOSE: Tool = Tool {
    name: "propose",
    spec: Spec {
        description: "Propose changes to a project's board as one change set: an agent's only way \
             to change it. Checked whole against the workflow, the WIP limits and the dependency \
             graph: either refused with every reason, or kept. The project's rung decides which \
             operations apply at once; the rest wait for a person. The answer says `applied`, \
             `waiting for a person (change set <id>)` or both.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "reason": { "type": "string", "description": "Why, in a sentence a person reads before deciding" },
                    "operations": {
                        "type": "array",
                        "description": OPERATIONS_HELP,
                        "items": {
                            "type": "object",
                            "properties": {
                                "op": { "type": "string", "enum": ["create", "who_does_it", "priority", "move", "depend", "comment", "due", "parent"] }
                            },
                            "required": ["op"]
                        }
                    },
                    "idempotency_key": { "type": "string", "description": "The same key on a project answers the first change set again" },
                    "thread_id": { "type": "string", "description": "The conversation thread this proposal answers, when you are taking a turn of one; the reply then shows the change set" }
                },
                "required": ["project_id", "reason", "operations"]
            })
        },
    },
    kind: Kind::Write,
    handler: propose,
};

fn propose(client: &TackClient, args: &Value) -> Result<Value, String> {
    let project_id = require_id(args, "project_id")?;
    let reason = require_str(args, "reason")?;
    let operations = args
        .get("operations")
        .and_then(Value::as_array)
        .filter(|operations| !operations.is_empty())
        .ok_or("missing required argument: operations (a non-empty array)")?;
    let thread = opt_str(args, "thread_id")
        .map(|id| check_id("thread_id", id))
        .transpose()?;
    submit_from(
        client,
        &project_id,
        &reason,
        operations.clone(),
        opt_str(args, "idempotency_key"),
        thread,
    )
}

pub static GET_CHANGE_SET: Tool = Tool {
    name: "get_change_set",
    spec: Spec {
        description: "Read one change set: its reason, who proposed it, its status and each \
             operation with its state and what it did.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "Change set UUID" } },
                "required": ["id"]
            })
        },
    },
    kind: Kind::Read,
    handler: get_change_set,
};

fn get_change_set(client: &TackClient, args: &Value) -> Result<Value, String> {
    let id = require_id(args, "id")?;
    let set = call(client.get(&format!("/change-sets/{id}")))?;
    let fence = card_fence_id();
    let operations: Vec<Value> = set["ops"]
        .as_array()
        .map(|ops| {
            ops.iter()
                .enumerate()
                .map(|(index, op)| {
                    let mut row = json!({
                        "index": index,
                        "operation": fenced_operation(&op["operation"], &fence),
                        "state": op["state"],
                    });
                    if op["result"].is_object() {
                        row["result"] = op["result"].clone();
                    }
                    row
                })
                .collect()
        })
        .unwrap_or_default();
    let mut compact = json!({
        "id": set["id"],
        "project_id": set["project"],
        "principal": if set["principal_name"].is_null() { &set["principal"] } else { &set["principal_name"] },
        "origin": set["origin"],
        "status": set["status"],
        "created_at": set["created_at"],
        "decided_at": set["decided_at"],
        "decided_by": set["decided_by"],
        "reason": set["reason"],
        "rejection": set["rejection"],
        "operations": operations,
    });
    for key in ["reason", "rejection"] {
        fence_field(&mut compact, key, &fence);
    }
    Ok(compact)
}

/// An operation with the text a person or an agent wrote in it fenced.
fn fenced_operation(operation: &Value, fence: &str) -> Value {
    let mut operation = operation.clone();
    for key in ["description", "text"] {
        fence_field(&mut operation, key, fence);
    }
    if let Some(criteria) = operation["acceptance"].as_array() {
        let fenced: Vec<Value> = criteria
            .iter()
            .map(|criterion| match criterion.as_str() {
                Some(text) => json!(fence_card_text(text, fence)),
                None => criterion.clone(),
            })
            .collect();
        operation["acceptance"] = json!(fenced);
    }
    operation
}

/// Proposes `operations` on a project and answers in the words an agent acts on.
fn submit(
    client: &TackClient,
    project_id: &str,
    reason: &str,
    operations: Vec<Value>,
    key: Option<String>,
) -> Result<Value, String> {
    submit_from(client, project_id, reason, operations, key, None)
}

/// [`submit`] for a change set a thread's turn makes: its origin is the thread, so the reply
/// shows it.
fn submit_from(
    client: &TackClient,
    project_id: &str,
    reason: &str,
    operations: Vec<Value>,
    key: Option<String>,
    thread: Option<String>,
) -> Result<Value, String> {
    let mut body = json!({ "reason": reason, "operations": operations });
    if let Some(key) = key {
        body["idempotency_key"] = json!(key);
    }
    if let Some(thread) = thread {
        body["origin"] = json!("thread");
        body["origin_ref"] = json!(thread);
    }
    let proposed = call(client.post(&format!("/projects/{project_id}/change-sets"), &body))?;
    Ok(answer(&proposed))
}

/// What a proposal came to: `applied`, `waiting for a person (change set <id>)`, or part of
/// each, with every operation's state and the item it made or changed.
fn answer(proposed: &Value) -> Value {
    let set = &proposed["change_set"];
    let id = set["id"].as_str().unwrap_or_default();
    let mut outcome = match set["status"].as_str() {
        Some("applied") => "applied".to_string(),
        Some("waiting") => format!("waiting for a person (change set {id})"),
        Some("partly") => format!("partly applied; the rest waits for a person (change set {id})"),
        Some("rejected") => format!("rejected by a person (change set {id})"),
        _ => format!("change set {id}"),
    };
    if proposed["made"] == json!(false) {
        outcome = format!("already proposed with this key: {outcome}");
    }
    let operations: Vec<Value> = set["ops"]
        .as_array()
        .map(|ops| {
            ops.iter()
                .enumerate()
                .map(|(index, op)| {
                    let mut row = json!({
                        "index": index,
                        "op": op["operation"]["op"],
                        "state": op["state"],
                    });
                    for key in ["item", "title", "moved_from"] {
                        if !op["result"][key].is_null() {
                            row[key] = op["result"][key].clone();
                        }
                    }
                    row
                })
                .collect()
        })
        .unwrap_or_default();
    json!({
        "outcome": outcome,
        "change_set": id,
        "status": set["status"],
        "operations": operations,
    })
}

// ── An agent's writes, as change sets ───────────────────────────────────────

/// The way an operation names an item that is already on the board.
fn existing(text: &str, what: &str) -> Result<Value, String> {
    let groups: Vec<usize> = text.split('-').map(str::len).collect();
    if groups == [8, 4, 4, 4, 12] && text.chars().all(|c| c == '-' || c.is_ascii_hexdigit()) {
        Ok(json!(text))
    } else {
        Err(format!("{what} must be an item id (a UUID), got `{text}`"))
    }
}

/// The text of `key` when the call carries it.
fn present<'a>(args: &'a Value, key: &str) -> Option<&'a Value> {
    args.get(key).filter(|value| !value.is_null())
}

/// Refuses the fields a change set has no operation for, naming them.
fn refuse_unsupported(tool: &str, args: &Value, keys: &[&str]) -> Result<(), String> {
    let named: Vec<&str> = keys
        .iter()
        .copied()
        .filter(|key| present(args, key).is_some())
        .collect();
    if named.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{tool}: an agent changes the board through a change set, which cannot set {}; \
         leave them out",
        named.join(", ")
    ))
}

/// The project an item belongs to.
fn project_of(client: &TackClient, item: &str) -> Result<String, String> {
    let found = call(client.get(&format!("/items/{item}")))?;
    found["item"]["project_id"]
        .as_str()
        .or_else(|| found["project_id"].as_str())
        .map(str::to_owned)
        .ok_or_else(|| format!("item {item} did not say which project it is in"))
}

/// The reason the call gave, else `fallback`.
fn reason_of(args: &Value, fallback: String) -> String {
    opt_str(args, "reason").unwrap_or(fallback)
}

/// `create_item` as a change set: a `create`, then the priority and due date the item starts with.
pub fn create_item(client: &TackClient, args: &Value) -> Result<Value, String> {
    let project_id = require_id(args, "project_id")?;
    let title = require_str(args, "title")?;
    refuse_unsupported("create_item", args, &["assignee", "tags"])?;
    let mut create = json!({ "op": "create", "title": title });
    if let Some(description) = opt_str(args, "description") {
        create["description"] = json!(description);
    }
    if let Some(item_type) = opt_str(args, "item_type") {
        create["type"] = json!(item_type);
    }
    if let Some(parent) = opt_str(args, "parent_id") {
        create["parent"] = existing(&parent, "parent_id")?;
    }
    let mut operations = vec![create];
    if let Some(priority) = opt_str(args, "priority") {
        operations
            .push(json!({ "op": "priority", "item": 0, "priority": priority.to_lowercase() }));
    }
    if let Some(due) = opt_str(args, "due_date") {
        operations
            .push(json!({ "op": "due", "item": 0, "due": super::projection::due_timestamp(&due) }));
    }
    let reason = reason_of(args, format!("create_item: {title}"));
    submit(client, &project_id, &reason, operations, None)
}

/// `update_item` as a change set: one operation per field it changes.
pub fn update_item(client: &TackClient, args: &Value) -> Result<Value, String> {
    let id = require_id(args, "id")?;
    refuse_unsupported("update_item", args, &["title", "description", "assignee"])?;
    let item = existing(&id, "id")?;
    let mut operations: Vec<Value> = Vec::new();
    if let Some(priority) = present(args, "priority").and_then(Value::as_str) {
        operations
            .push(json!({ "op": "priority", "item": item, "priority": priority.to_lowercase() }));
    }
    if let Some(status) = present(args, "status").and_then(Value::as_str) {
        operations.push(json!({ "op": "move", "item": item, "status": status }));
    }
    if let Some(due) = present(args, "due_date").and_then(Value::as_str) {
        operations.push(
            json!({ "op": "due", "item": item, "due": super::projection::due_timestamp(due) }),
        );
    }
    if let Some(parent) = present(args, "parent_id") {
        let parent = parent
            .as_str()
            .ok_or("parent_id must be an item id, or an empty string for the top")?;
        let mut operation = json!({ "op": "parent", "item": item });
        if !parent.is_empty() {
            operation["parent"] = existing(parent, "parent_id")?;
        }
        operations.push(operation);
    }
    if let Some(profile) = present(args, "who_does_it") {
        let profile = profile
            .as_str()
            .filter(|profile| !profile.is_empty())
            .ok_or("who_does_it must be an agent profile id")?;
        operations.push(json!({ "op": "who_does_it", "item": item, "profile_id": profile }));
    }
    if let Some(blockers) = present(args, "depends_on") {
        let blockers = blockers
            .as_array()
            .ok_or("depends_on must be an array of item ids")?;
        for blocker in blockers {
            let blocker = blocker.as_str().unwrap_or_default();
            operations.push(json!({
                "op": "depend",
                "blocker": existing(blocker, "depends_on")?,
                "blocked": item,
            }));
        }
    }
    if operations.is_empty() {
        return Err("update_item: provide at least one field to change".into());
    }
    let reason = reason_of(args, format!("update_item {id}"));
    submit(client, &project_of(client, &id)?, &reason, operations, None)
}

/// `move_item` as a change set.
pub fn move_item(client: &TackClient, args: &Value) -> Result<Value, String> {
    let id = require_id(args, "id")?;
    let status = require_str(args, "status")?;
    let operations = vec![json!({ "op": "move", "item": existing(&id, "id")?, "status": status })];
    let reason = reason_of(args, format!("move_item {id} to {status}"));
    submit(client, &project_of(client, &id)?, &reason, operations, None)
}

/// `add_comment` as a change set. The author is the credential's, whatever `author` says.
pub fn add_comment(client: &TackClient, args: &Value) -> Result<Value, String> {
    let item_id = require_id(args, "item_id")?;
    let content = require_str(args, "content")?;
    let operations = vec![json!({
        "op": "comment",
        "item": existing(&item_id, "item_id")?,
        "text": content,
    })];
    let reason = reason_of(args, format!("add_comment to {item_id}"));
    submit(
        client,
        &project_of(client, &item_id)?,
        &reason,
        operations,
        None,
    )
}

/// What an outline item names another item by: its `ref`, else its title. A name two items
/// share names neither.
struct Names {
    refs: HashMap<String, Vec<usize>>,
    titles: HashMap<String, Vec<usize>>,
}

impl Names {
    fn of(items: &[Value]) -> Names {
        let collect = |key: &str| {
            let mut found: HashMap<String, Vec<usize>> = HashMap::new();
            for (index, item) in items.iter().enumerate() {
                if let Some(name) = opt_str(item, key) {
                    found.entry(name).or_default().push(index);
                }
            }
            found
        };
        Names {
            refs: collect("ref"),
            titles: collect("title"),
        }
    }

    /// The outline item a name picks out.
    fn find(&self, name: &str) -> Option<usize> {
        let only = |found: &HashMap<String, Vec<usize>>| match found.get(name).map(Vec::as_slice) {
            Some([index]) => Some(*index),
            _ => None,
        };
        only(&self.refs).or_else(|| only(&self.titles))
    }
}

/// A reference from an outline item to another: one of the outline's, or one already on the board.
enum Target {
    New(usize),
    Existing(Value),
}

fn target(names: &Names, what: &str, name: &str) -> Result<Target, String> {
    if let Some(index) = names.find(name) {
        return Ok(Target::New(index));
    }
    existing(name, what).map(Target::Existing).map_err(|_| {
        format!("{what} `{name}` is neither a ref or title in the outline nor an item id")
    })
}

/// The outline's items in an order where a parent comes before its children.
fn parents_first(parents: &[Option<usize>]) -> Result<Vec<usize>, String> {
    let mut order = Vec::with_capacity(parents.len());
    let mut placed = vec![false; parents.len()];
    for start in 0..parents.len() {
        let mut chain = Vec::new();
        let mut at = Some(start);
        while let Some(index) = at.filter(|&index| !placed[index]) {
            if chain.contains(&index) {
                return Err("create_outline: items are their own ancestors".into());
            }
            chain.push(index);
            at = parents[index];
        }
        for index in chain.into_iter().rev() {
            placed[index] = true;
            order.push(index);
        }
    }
    Ok(order)
}

/// `create_outline` as a change set: the items as `create`s, parents first, then the priorities,
/// due dates and statuses the outline gives them.
pub fn create_outline(client: &TackClient, args: &Value) -> Result<Value, String> {
    let items = args
        .get("items")
        .and_then(Value::as_array)
        .ok_or("missing required argument: items (an array)")?;
    if present(args, "dry_run").is_some_and(|dry| dry == &json!(true)) {
        return Err(
            "create_outline: dry_run is not available to an agent; a change set is \
                    checked whole before it is kept, and waits for a person"
                .into(),
        );
    }
    refuse_unsupported(
        "create_outline",
        args,
        &[
            "project",
            "fields",
            "profiles",
            "roles",
            "sprints",
            "automation",
            "types",
        ],
    )?;
    let project_id = opt_str(args, "project_id")
        .map(|_| require_id(args, "project_id"))
        .transpose()?
        .ok_or(
            "create_outline: pass project_id; an agent adds to a project and cannot create one",
        )?;
    for item in items {
        if opt_str(item, "title").is_none() {
            return Err("create_outline: every item needs a title".into());
        }
        refuse_unsupported("create_outline", item, &["assignee", "estimate", "tags"])?;
    }

    let names = Names::of(items);
    let mut parents: Vec<Option<usize>> = vec![None; items.len()];
    let mut parent_ids: Vec<Option<Value>> = vec![None; items.len()];
    for (index, item) in items.iter().enumerate() {
        if let Some(name) = opt_str(item, "parent") {
            match target(&names, "parent", &name)? {
                Target::New(parent) => parents[index] = Some(parent),
                Target::Existing(id) => parent_ids[index] = Some(id),
            }
        }
    }
    let order = parents_first(&parents)?;
    let mut position = vec![0usize; items.len()];
    for (at, &index) in order.iter().enumerate() {
        position[index] = at;
    }

    let mut operations: Vec<Value> = Vec::new();
    for &index in &order {
        let item = &items[index];
        let mut create = json!({ "op": "create", "title": item["title"] });
        for (from, to) in [("description", "description"), ("type", "type")] {
            if let Some(text) = opt_str(item, from) {
                create[to] = json!(text);
            }
        }
        if let Some(parent) = parents[index] {
            create["parent"] = json!(position[parent]);
        } else if let Some(id) = &parent_ids[index] {
            create["parent"] = id.clone();
        }
        let mut blockers: Vec<Value> = Vec::new();
        for name in item["depends_on"].as_array().into_iter().flatten() {
            let name = name.as_str().unwrap_or_default();
            blockers.push(match target(&names, "depends_on", name)? {
                Target::New(blocker) => json!(position[blocker]),
                Target::Existing(id) => id,
            });
        }
        create["depends_on"] = json!(blockers);
        operations.push(create);
    }
    for &index in &order {
        let item = &items[index];
        let at = position[index];
        if let Some(priority) = opt_str(item, "priority") {
            operations
                .push(json!({ "op": "priority", "item": at, "priority": priority.to_lowercase() }));
        }
        if let Some(due) = opt_str(item, "due_date") {
            operations.push(
                json!({ "op": "due", "item": at, "due": super::projection::due_timestamp(&due) }),
            );
        }
        if let Some(status) = opt_str(item, "status") {
            operations.push(json!({ "op": "move", "item": at, "status": status }));
        }
    }
    let reason = reason_of(args, format!("create_outline: {} items", items.len()));
    submit(client, &project_id, &reason, operations, None)
}
