//! The tools that read and write items: list, read, search, create, update, move,
//! comment.

use serde_json::{Value, json};

use super::changes;
use super::projection::{
    card_fence_id, compact_item, due_timestamp, fence_card_text, fence_field, item_list, item_rows,
};
use super::tool::{Kind, Spec, Tool, call, opt_str, require_id, require_str, urlencode};
use crate::client::TackClient;

pub static LIST_ITEMS: Tool = Tool {
    name: "list_items",
    spec: Spec {
        description: "List items in a project, optionally filtered.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string", "description": "Project UUID" },
                    "status": { "type": "string", "description": "Filter by status column name" },
                    "item_type": { "type": "string", "description": "Filter by item type" },
                    "assignee": { "type": "string", "description": "Filter by assignee" }
                },
                "required": ["project_id"]
            })
        },
    },
    kind: Kind::Read,
    handler: list_items,
};

fn list_items(client: &TackClient, args: &Value) -> Result<Value, String> {
    let project_id = require_id(args, "project_id")?;
    let mut qs = Vec::new();
    for key in ["status", "item_type", "assignee"] {
        if let Some(v) = opt_str(args, key) {
            qs.push(format!("{key}={}", urlencode(&v)));
        }
    }
    let suffix = if qs.is_empty() {
        String::new()
    } else {
        format!("?{}", qs.join("&"))
    };
    let items = call(client.get(&format!("/projects/{project_id}/items{suffix}")))?;
    Ok(item_list(&items))
}

pub static GET_ITEM: Tool = Tool {
    name: "get_item",
    spec: Spec {
        description: "Get one item with full detail, roles, and dependencies.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "Item UUID" } },
                "required": ["id"]
            })
        },
    },
    kind: Kind::Read,
    handler: get_item,
};

fn get_item(client: &TackClient, args: &Value) -> Result<Value, String> {
    let id = require_id(args, "id")?;
    let mut item = call(client.get(&format!("/items/{id}")))?;
    let comments = call(client.get(&format!("/items/{id}/comments")))?;
    let fence = card_fence_id();
    fence_field(&mut item, "description", &fence);
    fence_field(&mut item["item"], "description", &fence);
    if let Some(list) = comments.as_array() {
        let fenced: Vec<Value> = list
            .iter()
            .map(|c| {
                let mut c = c.clone();
                if let Some(text) = c["content"].as_str() {
                    c["content"] = json!(fence_card_text(text, &fence));
                }
                c
            })
            .collect();
        item["comments"] = json!(fenced);
    }
    Ok(item)
}

pub static SEARCH_ITEMS: Tool = Tool {
    name: "search_items",
    spec: Spec {
        description: "Full-text search items, globally or within a project.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "project_id": { "type": "string", "description": "Optional; omit to search all projects" }
                },
                "required": ["query"]
            })
        },
    },
    kind: Kind::Read,
    handler: search_items,
};

fn search_items(client: &TackClient, args: &Value) -> Result<Value, String> {
    let query = require_str(args, "query")?;
    let path = match opt_str(args, "project_id") {
        Some(_) => format!(
            "/projects/{}/search?q={}",
            require_id(args, "project_id")?,
            urlencode(&query)
        ),
        None => format!("/search?q={}", urlencode(&query)),
    };
    let items = call(client.get(&path))?;
    let fence = card_fence_id();
    let rows: Vec<Value> = item_rows(&items)
        .iter()
        .map(|item| {
            let mut row = compact_item(item);
            if let Some(text) = item["description"].as_str().filter(|d| !d.is_empty()) {
                row["description"] = json!(fence_card_text(text, &fence));
            }
            row
        })
        .collect();
    Ok(json!({ "items": rows, "count": rows.len() }))
}

pub static CREATE_ITEM: Tool = Tool {
    name: "create_item",
    spec: Spec {
        description: "Create an item in a project. To create many items, or a project \
             with its items, use create_outline: one call instead of one per item. With an \
             agent credential this proposes a change set (no assignee or tags).",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "item_type": { "type": "string", "description": "task (default), epic, feature, bug, subtask, requirement" },
                    "priority": { "type": "string", "description": "critical, high, medium (default), low" },
                    "parent_id": { "type": "string" },
                    "assignee": { "type": "string" },
                    "due_date": { "type": "string", "description": "YYYY-MM-DD or an RFC3339 timestamp" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "reason": { "type": "string", "description": "Why, for the person who reads the change set; used when you hold an agent credential" }
                },
                "required": ["project_id", "title"]
            })
        },
    },
    kind: Kind::Write,
    handler: create_item,
};

fn create_item(client: &TackClient, args: &Value) -> Result<Value, String> {
    if client.acts_as_agent() {
        return changes::create_item(client, args);
    }
    let project_id = require_id(args, "project_id")?;
    let title = require_str(args, "title")?;
    let mut body = json!({
        "title": title,
        "item_type": opt_str(args, "item_type").unwrap_or_else(|| "task".into()),
        "priority": opt_str(args, "priority").unwrap_or_else(|| "medium".into()),
    });
    for field in ["parent_id", "assignee", "description"] {
        if let Some(v) = opt_str(args, field) {
            body[field] = json!(v);
        }
    }
    if let Some(due) = opt_str(args, "due_date") {
        body["due_date"] = json!(due_timestamp(&due));
    }
    if let Some(tags) = args.get("tags").filter(|v| v.is_array()) {
        body["tags"] = tags.clone();
    }
    let mut created = call(client.post(&format!("/projects/{project_id}/items"), &body))?;
    fence_field(&mut created, "description", &card_fence_id());
    Ok(created)
}

pub static UPDATE_ITEM: Tool = Tool {
    name: "update_item",
    spec: Spec {
        description: "Update fields on an item. Status changes are validated against the workflow. \
             who_does_it (an agent profile id) and depends_on (ids of items that must finish \
             first) propose a change set. With an agent credential every change does, and \
             title, description and assignee cannot be changed.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "priority": { "type": "string" },
                    "assignee": { "type": "string" },
                    "status": { "type": "string" },
                    "due_date": { "type": "string", "description": "RFC3339 timestamp" },
                    "who_does_it": { "type": "string", "description": "An agent profile id: the profile that does this item" },
                    "depends_on": { "type": "array", "items": { "type": "string" }, "description": "Ids of items that must finish before this one" },
                    "reason": { "type": "string", "description": "Why, for the person who reads the change set; used when you hold an agent credential" }
                },
                "required": ["id"]
            })
        },
    },
    kind: Kind::Write,
    handler: update_item,
};

fn update_item(client: &TackClient, args: &Value) -> Result<Value, String> {
    let asks_for_a_change_set = ["who_does_it", "depends_on"]
        .iter()
        .any(|key| args.get(key).is_some_and(|value| !value.is_null()));
    if client.acts_as_agent() || asks_for_a_change_set {
        return changes::update_item(client, args);
    }
    let id = require_id(args, "id")?;
    let mut body = json!({});
    let mut touched = false;
    for field in [
        "title",
        "description",
        "priority",
        "assignee",
        "status",
        "due_date",
    ] {
        if let Some(v) = args.get(field).filter(|v| !v.is_null()) {
            body[field] = v.clone();
            touched = true;
        }
    }
    if !touched {
        return Err("update_item: provide at least one field to change".into());
    }
    write_item(client, &id, &body)
}

pub static MOVE_ITEM: Tool = Tool {
    name: "move_item",
    spec: Spec {
        description: "Move an item to a new status (validated transition + WIP limits).",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "status": { "type": "string", "description": "Target status column name (case-sensitive)" },
                    "reason": { "type": "string", "description": "Why, for the person who reads the change set; used when you hold an agent credential" }
                },
                "required": ["id", "status"]
            })
        },
    },
    kind: Kind::Write,
    handler: move_item,
};

fn move_item(client: &TackClient, args: &Value) -> Result<Value, String> {
    if client.acts_as_agent() {
        return changes::move_item(client, args);
    }
    let id = require_id(args, "id")?;
    let status = require_str(args, "status")?;
    write_item(client, &id, &json!({ "status": status }))
}

pub static ADD_COMMENT: Tool = Tool {
    name: "add_comment",
    spec: Spec {
        description: "Add a comment to an item.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "item_id": { "type": "string" },
                    "content": { "type": "string" },
                    "author": { "type": "string", "description": "Ignored with an agent credential: the comment carries the credential's name" },
                    "reason": { "type": "string", "description": "Why, for the person who reads the change set; used when you hold an agent credential" }
                },
                "required": ["item_id", "content"]
            })
        },
    },
    kind: Kind::Write,
    handler: add_comment,
};

fn add_comment(client: &TackClient, args: &Value) -> Result<Value, String> {
    if client.acts_as_agent() {
        return changes::add_comment(client, args);
    }
    let item_id = require_id(args, "item_id")?;
    let content = require_str(args, "content")?;
    let body = json!({ "content": content, "author": opt_str(args, "author") });
    let mut comment = call(client.post(&format!("/items/{item_id}/comments"), &body))?;
    fence_field(&mut comment, "content", &card_fence_id());
    Ok(comment)
}

/// Read-modify-write an item with an `If-Match` precondition, so the two MCP
/// tools that mutate an existing item (`update_item`, `move_item`) can never
/// clobber a change made by a human in the UI or another agent between the
/// read and the write — the exact agent-versus-human race that
/// `version`/`ETag`/`If-Match` exists to catch, on exactly the write path
/// that would otherwise have no way to send the header at all. When the server sends
/// no `ETag` (no concurrency support on this route yet, or an older server),
/// `if_match` is `None` and `patch_if_match` sends no header — identical to
/// today's unconditional write, never a new failure mode.
///
/// A `412` surfaces through `patch_if_match`'s own error text, which already
/// names the precondition failure and tells the caller to re-read — this
/// function does not need to special-case it further; the point is only
/// that the agent sees that message instead of a generic one, and it does
/// because nothing here swallows or rewrites the error.
fn write_item(client: &TackClient, id: &str, body: &Value) -> Result<Value, String> {
    let (_, etag) = call(client.get_with_etag(&format!("/items/{id}")))?;
    let mut written = call(client.patch_if_match(&format!("/items/{id}"), body, etag.as_deref()))?;
    fence_field(&mut written, "description", &card_fence_id());
    Ok(written)
}
