//! Threads over MCP (ADR 0034, decision 7): `read_thread` and `post_message`. A message in a
//! thread starts a turn of the thread's profile, so the answer says whether one started. Every
//! message body is somebody's words, fenced as card text.

use serde_json::{Value, json};

use super::projection::{card_fence_id, fence_card_text};
use super::tool::{Kind, Spec, Tool, call, require_id, require_str};
use crate::client::TackClient;

/// The latest messages `read_thread` returns whole; older ones are counted.
const MESSAGES_SHOWN: usize = 20;

pub static READ_THREAD: Tool = Tool {
    name: "read_thread",
    spec: Spec {
        description: "Read a conversation thread: its title, status, the turns it has taken, and \
             its latest messages (20 at most; `more` counts the older ones). Needs the scope \
             `converse` on the thread's project.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": { "thread_id": { "type": "string", "description": "Thread UUID" } },
                "required": ["thread_id"]
            })
        },
    },
    kind: Kind::Read,
    handler: read_thread,
};

fn read_thread(client: &TackClient, args: &Value) -> Result<Value, String> {
    let id = require_id(args, "thread_id")?;
    let detail = call(client.get(&format!("/threads/{id}")))?;
    let fence = card_fence_id();
    let messages = detail["messages"].as_array().cloned().unwrap_or_default();
    let more = messages.len().saturating_sub(MESSAGES_SHOWN);
    let shown: Vec<Value> = messages[more..]
        .iter()
        .map(|message| {
            json!({
                "principal": message["principal"],
                "body": fence_card_text(message["body"].as_str().unwrap_or(""), &fence),
                "refs": message["refs"],
                "at": message["created_at"],
            })
        })
        .collect();
    let thread = &detail["thread"];
    Ok(json!({
        "id": thread["id"],
        "title": thread["title"],
        "status": thread["status"],
        "turns": thread["turns"],
        "messages": shown,
        "more": more,
    }))
}

pub static POST_MESSAGE: Tool = Tool {
    name: "post_message",
    spec: Spec {
        description: "Write a message in a conversation thread: a report or an alert for the \
             person. A message from an agent starts a turn of the thread's profile, and the \
             thread takes one turn at a time: it is refused while the last turn has not answered, \
             when the thread is closed, or when it reached its turn cap. Needs the scope \
             `converse` on the thread's project.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string" },
                    "body": { "type": "string", "description": "The message, at most 4000 characters" },
                    "refs": {
                        "type": "array",
                        "description": "Items, change sets and decisions the message is about",
                        "items": {
                            "type": "object",
                            "properties": {
                                "kind": { "type": "string", "enum": ["item", "change_set", "decision"] },
                                "id": { "type": "string" }
                            },
                            "required": ["kind", "id"]
                        }
                    }
                },
                "required": ["thread_id", "body"]
            })
        },
    },
    kind: Kind::Write,
    handler: post_message,
};

fn post_message(client: &TackClient, args: &Value) -> Result<Value, String> {
    let id = require_id(args, "thread_id")?;
    let body = require_str(args, "body")?;
    let refs = args.get("refs").cloned().unwrap_or_else(|| json!([]));
    let posted = call(client.post(
        &format!("/threads/{id}/messages"),
        &json!({"body": body, "refs": refs}),
    ))?;
    Ok(json!({
        "posted": true,
        "turn_started": !posted["turn"].is_null(),
        "notice": posted["notice"]["body"],
    }))
}
