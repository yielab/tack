//! The table every MCP tool is declared in, and the helpers its handlers share.
//!
//! A tool is one `Tool` value in its domain's module: its name, what `tools/list`
//! advertises, whether it reads or writes, and the function that runs it. `tools/list`
//! and `tools/call` both read this table, so a tool cannot be listed without being
//! callable, or the other way round. [`TOOLS`] fixes the order `tools/list` answers in.

use serde_json::{Value, json};

use super::{fleet, items, projects, runs};
use crate::client::TackClient;

/// Whether a tool changes anything on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Read,
    Write,
}

/// What `tools/list` advertises for one tool.
pub struct Spec {
    pub description: &'static str,
    /// Built on demand: a JSON Schema is a `Value`, which a `static` cannot hold.
    pub input_schema: fn() -> Value,
}

/// Runs a tool. Validates its arguments before any network call, so a bad request fails
/// fast and can be tested without a server.
pub type Handler = fn(&TackClient, &Value) -> Result<Value, String>;

/// One tool of the MCP server.
pub struct Tool {
    pub name: &'static str,
    pub spec: Spec,
    pub kind: Kind,
    pub handler: Handler,
}

impl Tool {
    /// The entry `tools/list` carries for this tool.
    pub fn advertised(&self) -> Value {
        json!({
            "name": self.name,
            "description": self.spec.description,
            "inputSchema": (self.spec.input_schema)(),
        })
    }
}

/// Every tool, in the order `tools/list` lists them.
pub static TOOLS: &[&Tool] = &[
    &projects::LIST_PROJECTS,
    &projects::GET_AUTONOMY,
    &items::LIST_ITEMS,
    &items::GET_ITEM,
    &items::SEARCH_ITEMS,
    &items::CREATE_ITEM,
    &projects::CREATE_PROJECT,
    &projects::CREATE_OUTLINE,
    &items::UPDATE_ITEM,
    &items::MOVE_ITEM,
    &items::ADD_COMMENT,
    &fleet::LIST_FLEETS,
    &fleet::LIST_AGENT_PROFILES,
    &runs::LIST_EXECUTIONS,
    &runs::GET_EXECUTION,
    &runs::CANCEL_EXECUTION,
    &runs::CREATE_EXECUTION,
];

/// The `tools` array `tools/list` answers with.
pub fn advertised() -> Vec<Value> {
    TOOLS.iter().map(|tool| tool.advertised()).collect()
}

/// The tool called `name`, if there is one.
pub fn find(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().copied().find(|tool| tool.name == name)
}

/// Map an HTTP-client result into a tool result, flattening the error to text.
/// Generic (not `Value`-only) so `get_with_etag`'s `(Value, Option<String>)`
/// pair goes through the same `?`-friendly path as every other client call.
pub fn call<T>(result: anyhow::Result<T>) -> Result<T, String> {
    result.map_err(|e| e.to_string())
}

// ── Argument helpers ─────────────────────────────────────────────────────────

pub fn require_str(args: &Value, key: &str) -> Result<String, String> {
    opt_str(args, key).ok_or_else(|| format!("missing required argument: {key}"))
}

pub fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

/// A required id that will be placed in a URL path. Only letters, digits,
/// `-` and `_`, 1 to 64 characters, so no `/`, `?` or `.` can change the route.
pub fn require_id(args: &Value, key: &str) -> Result<String, String> {
    let value = require_str(args, key)?;
    let valid = value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if valid {
        Ok(value)
    } else {
        Err(format!("{key} must be 1 to 64 letters, digits, '-' or '_'"))
    }
}

/// An optional JSON-object argument that defaults to `{}` when omitted
/// (`budgets`/`environment`/`metadata` on `create_execution` — all
/// genuinely untyped `Value` fields server-side).
pub fn opt_object_or_empty(args: &Value, key: &str) -> Value {
    args.get(key)
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}))
}

pub fn require_u64(args: &Value, key: &str) -> Result<u64, String> {
    args.get(key).and_then(|v| v.as_u64()).ok_or_else(|| {
        format!("missing or invalid required argument: {key} (expected a non-negative integer)")
    })
}

/// Percent-encode a query-string value (RFC 3986 unreserved set passes through).
pub fn urlencode(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => vec![c],
            ' ' => vec!['%', '2', '0'],
            _ => format!("%{:02X}", c as u32).chars().collect(),
        })
        .collect()
}
