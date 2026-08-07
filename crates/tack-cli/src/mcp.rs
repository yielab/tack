//! Minimal Model Context Protocol (MCP) server over stdio.
//!
//! `tack mcp` speaks JSON-RPC 2.0 over stdin/stdout (newline-delimited, per the
//! MCP stdio transport) and proxies tool calls to a running Tack server through
//! the same blocking HTTP client every other CLI command uses. Writes therefore
//! go through the REST API — workflow validation, WIP limits, and
//! parent-auto-completion all still apply, and the live board updates over the
//! existing WebSocket.
//!
//! See `docs/MCP.md` for the transport decision and the Claude Code config.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::client::TackClient;

/// Protocol version we advertise when the client doesn't specify one.
const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";

/// Run the stdio MCP loop until stdin closes.
///
/// `stdout` carries the protocol — nothing else may be written there.
pub fn run(client: &TackClient) -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle_line(client, &line) {
            writeln!(out, "{response}")?;
            out.flush()?;
        }
    }
    Ok(())
}

/// Handle a single JSON-RPC message. Returns `Some(serialized response)`, or
/// `None` for notifications (which take no response).
fn handle_line(client: &TackClient, line: &str) -> Option<String> {
    let req: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            return Some(error_response(
                Value::Null,
                -32700,
                &format!("parse error: {e}"),
            ));
        }
    };

    let id = req.get("id").cloned();
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(Value::Null);

    match method {
        "initialize" => Some(success(id, initialize_result(&params))),
        // Notifications carry no id and get no reply.
        "notifications/initialized" | "initialized" => None,
        "ping" => Some(success(id, json!({}))),
        "tools/list" => Some(success(id, json!({ "tools": tool_specs() }))),
        "tools/call" => Some(handle_tool_call(client, id, &params)),
        other => {
            // Unknown notification → silence; unknown request → method-not-found.
            id.map(|id| error_response(id, -32601, &format!("method not found: {other}")))
        }
    }
}

fn initialize_result(params: &Value) -> Value {
    // Echo the client's requested protocol version when present for compatibility.
    let protocol = params
        .get("protocolVersion")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_PROTOCOL_VERSION);
    json!({
        "protocolVersion": protocol,
        "capabilities": { "tools": {} },
        "serverInfo": {
            "name": "tack",
            "version": env!("CARGO_PKG_VERSION"),
        },
    })
}

// ── Tool dispatch ───────────────────────────────────────────────────────────

fn handle_tool_call(client: &TackClient, id: Option<Value>, params: &Value) -> String {
    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    match dispatch_tool(client, name, &args) {
        Ok(value) => success(id, tool_text(&value, false)),
        Err(msg) => success(id, tool_text(&Value::String(msg), true)),
    }
}

/// Run a tool by name. Required-argument validation happens before any network
/// call so bad requests fail fast (and are unit-testable without a server).
fn dispatch_tool(client: &TackClient, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "list_projects" => {
            let projects = call(client.get("/projects"))?;
            Ok(project_list(&projects))
        }
        "list_items" => {
            let project_id = require_str(args, "project_id")?;
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
        "get_item" => {
            let id = require_str(args, "id")?;
            call(client.get(&format!("/items/{id}")))
        }
        "search_items" => {
            let query = require_str(args, "query")?;
            let path = match opt_str(args, "project_id") {
                Some(pid) => format!("/projects/{pid}/search?q={}", urlencode(&query)),
                None => format!("/search?q={}", urlencode(&query)),
            };
            let items = call(client.get(&path))?;
            Ok(item_list(&items))
        }
        "create_item" => {
            let project_id = require_str(args, "project_id")?;
            let title = require_str(args, "title")?;
            let mut body = json!({
                "title": title,
                "item_type": opt_str(args, "item_type").unwrap_or_else(|| "task".into()),
                "priority": opt_str(args, "priority").unwrap_or_else(|| "medium".into()),
            });
            for (arg, field) in [("parent_id", "parent_id"), ("assignee", "assignee")] {
                if let Some(v) = opt_str(args, arg) {
                    body[field] = json!(v);
                }
            }
            call(client.post(&format!("/projects/{project_id}/items"), &body))
        }
        "update_item" => {
            let id = require_str(args, "id")?;
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
        "move_item" => {
            let id = require_str(args, "id")?;
            let status = require_str(args, "status")?;
            write_item(client, &id, &json!({ "status": status }))
        }
        "add_comment" => {
            let item_id = require_str(args, "item_id")?;
            let content = require_str(args, "content")?;
            let body = json!({ "content": content, "author": opt_str(args, "author") });
            call(client.post(&format!("/items/{item_id}/comments"), &body))
        }
        other => Err(format!("unknown tool: {other}")),
    }
}

/// Map an HTTP-client result into a tool result, flattening the error to text.
/// Generic (not `Value`-only) so `get_with_etag`'s `(Value, Option<String>)`
/// pair goes through the same `?`-friendly path as every other client call.
fn call<T>(result: anyhow::Result<T>) -> Result<T, String> {
    result.map_err(|e| e.to_string())
}

/// Read-modify-write an item with an `If-Match` precondition, so the two MCP
/// tools that mutate an existing item (`update_item`, `move_item`) can never
/// clobber a change made by a human in the UI or another agent between the
/// read and the write — the exact agent-versus-human race card G3 added
/// `version`/`ETag`/`If-Match` to catch, on exactly the write path that
/// previously had no way to send the header at all. When the server sends
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
    call(client.patch_if_match(&format!("/items/{id}"), body, etag.as_deref()))
}

// ── Argument helpers ─────────────────────────────────────────────────────────

fn require_str(args: &Value, key: &str) -> Result<String, String> {
    opt_str(args, key).ok_or_else(|| format!("missing required argument: {key}"))
}

fn opt_str(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

// ── Compact projections (keep agent context small) ───────────────────────────

fn project_list(value: &Value) -> Value {
    let arr = value.as_array().cloned().unwrap_or_default();
    let projects: Vec<Value> = arr
        .iter()
        .map(|p| {
            json!({
                "id": p.get("id"),
                "name": p.get("name"),
                "project_type": p.get("project_type"),
            })
        })
        .collect();
    json!({ "projects": projects, "count": projects.len() })
}

fn item_list(value: &Value) -> Value {
    // The list endpoint returns a `{ data, total, page, per_page }` envelope;
    // fall back to a bare array for other item-producing shapes.
    let arr = value
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| value.as_array())
        .cloned()
        .unwrap_or_default();
    let items: Vec<Value> = arr.iter().map(compact_item).collect();
    json!({ "items": items, "count": items.len() })
}

fn compact_item(item: &Value) -> Value {
    json!({
        "id": item.get("id"),
        "title": item.get("title"),
        "item_type": item.get("item_type"),
        "status": item.get("status"),
        "priority": item.get("priority"),
        "assignee": item.get("assignee"),
        "project_id": item.get("project_id"),
    })
}

// ── JSON-RPC envelope helpers ────────────────────────────────────────────────

fn success(id: Option<Value>, result: Value) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id.unwrap_or(Value::Null),
        "result": result,
    })
    .to_string()
}

fn error_response(id: Value, code: i64, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
    .to_string()
}

/// Wrap a value as an MCP tool result (a single text content block).
fn tool_text(value: &Value, is_error: bool) -> Value {
    let text = match value {
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
    };
    json!({
        "content": [ { "type": "text", "text": text } ],
        "isError": is_error,
    })
}

/// Percent-encode a query-string value (RFC 3986 unreserved set passes through).
fn urlencode(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => vec![c],
            ' ' => vec!['%', '2', '0'],
            _ => format!("%{:02X}", c as u32).chars().collect(),
        })
        .collect()
}

// ── Tool schemas (advertised via tools/list) ─────────────────────────────────

fn tool_specs() -> Value {
    json!([
        tool(
            "list_projects",
            "List all projects.",
            json!({
                "type": "object", "properties": {}
            })
        ),
        tool(
            "list_items",
            "List items in a project, optionally filtered.",
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
        ),
        tool(
            "get_item",
            "Get one item with full detail, roles, and dependencies.",
            json!({
                "type": "object",
                "properties": { "id": { "type": "string", "description": "Item UUID" } },
                "required": ["id"]
            })
        ),
        tool(
            "search_items",
            "Full-text search items, globally or within a project.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "project_id": { "type": "string", "description": "Optional; omit to search all projects" }
                },
                "required": ["query"]
            })
        ),
        tool(
            "create_item",
            "Create an item in a project.",
            json!({
                "type": "object",
                "properties": {
                    "project_id": { "type": "string" },
                    "title": { "type": "string" },
                    "item_type": { "type": "string", "description": "task (default), epic, feature, bug, subtask, requirement" },
                    "priority": { "type": "string", "description": "critical, high, medium (default), low" },
                    "parent_id": { "type": "string" },
                    "assignee": { "type": "string" }
                },
                "required": ["project_id", "title"]
            })
        ),
        tool(
            "update_item",
            "Update fields on an item. Status changes are validated against the workflow.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "priority": { "type": "string" },
                    "assignee": { "type": "string" },
                    "status": { "type": "string" },
                    "due_date": { "type": "string", "description": "RFC3339 timestamp" }
                },
                "required": ["id"]
            })
        ),
        tool(
            "move_item",
            "Move an item to a new status (validated transition + WIP limits).",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "status": { "type": "string", "description": "Target status column name (case-sensitive)" }
                },
                "required": ["id", "status"]
            })
        ),
        tool(
            "add_comment",
            "Add a comment to an item.",
            json!({
                "type": "object",
                "properties": {
                    "item_id": { "type": "string" },
                    "content": { "type": "string" },
                    "author": { "type": "string" }
                },
                "required": ["item_id", "content"]
            })
        ),
    ])
}

fn tool(name: &str, description: &str, schema: Value) -> Value {
    json!({ "name": name, "description": description, "inputSchema": schema })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn test_client() -> TackClient {
        // Never connects until a request is made; safe for validation-only tests.
        TackClient::new(&Config {
            base_url: "http://127.0.0.1:0".into(),
            token: None,
        })
        .unwrap()
    }

    #[test]
    fn initialize_echoes_protocol_version() {
        let resp = handle_line(
            &test_client(),
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        )
        .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(v["result"]["serverInfo"]["name"], "tack");
        assert_eq!(v["id"], 1);
    }

    #[test]
    fn initialize_defaults_protocol_version() {
        let resp = handle_line(
            &test_client(),
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        )
        .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["result"]["protocolVersion"], DEFAULT_PROTOCOL_VERSION);
    }

    #[test]
    fn initialized_notification_has_no_response() {
        let resp = handle_line(
            &test_client(),
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        );
        assert!(resp.is_none());
    }

    #[test]
    fn tools_list_advertises_all_eight() {
        let resp = handle_line(
            &test_client(),
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        )
        .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        let tools = v["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 8);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"list_projects"));
        assert!(names.contains(&"move_item"));
        assert!(names.contains(&"add_comment"));
    }

    #[test]
    fn unknown_method_returns_method_not_found() {
        let resp = handle_line(
            &test_client(),
            r#"{"jsonrpc":"2.0","id":3,"method":"does/not/exist"}"#,
        )
        .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["error"]["code"], -32601);
    }

    #[test]
    fn parse_error_is_reported() {
        let resp = handle_line(&test_client(), "{not json").unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["error"]["code"], -32700);
    }

    #[test]
    fn tool_call_missing_required_arg_is_tool_error() {
        // Missing `project_id` must fail before any network call.
        let resp = handle_line(
            &test_client(),
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"list_items","arguments":{}}}"#,
        )
        .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["result"]["isError"], true);
        let text = v["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("project_id"), "unexpected: {text}");
    }

    #[test]
    fn tool_call_unknown_tool_is_tool_error() {
        let resp = handle_line(
            &test_client(),
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
        )
        .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["result"]["isError"], true);
        let text = v["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("unknown tool"), "unexpected: {text}");
    }

    #[test]
    fn update_item_requires_a_field() {
        let resp = handle_line(
            &test_client(),
            r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"update_item","arguments":{"id":"x"}}}"#,
        )
        .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["result"]["isError"], true);
    }

    // ── MCP write path: If-Match (card G4) ──────────────────────────────────
    //
    // `update_item`/`move_item` now read the item before writing it so they
    // can send back its `ETag` as `If-Match` — closing the race named in
    // `docs/plans/agnostic-control-plane.md` trap T2: an agent write via MCP
    // was the one path in the whole system with no way to attach a header
    // at all, so it was unconditionally last-write-wins even after G3 gave
    // every other writer a precondition to send.

    // Run a blocking closure on a thread allowed to block. `TackClient` is
    // synchronous `reqwest`; calling it directly from a `#[tokio::test]`
    // body would block that test's own runtime thread against itself,
    // since `wiremock`'s server answers requests on that same runtime.
    async fn run_blocking<F, T>(f: F) -> T
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        tokio::task::spawn_blocking(f)
            .await
            .expect("blocking task panicked")
    }

    fn mock_client(base_url: &str) -> TackClient {
        TackClient::new(&Config {
            base_url: base_url.to_string(),
            token: None,
        })
        .unwrap()
    }

    #[tokio::test]
    async fn mcp_update_item_sends_if_match() {
        let server = wiremock::MockServer::start().await;
        let item_id = "11111111-0000-0000-0000-000000000000";

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(format!("/api/items/{item_id}")))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("ETag", "\"4\"")
                    .set_body_json(json!({ "id": item_id, "title": "before" })),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("PATCH"))
            .and(wiremock::matchers::path(format!("/api/items/{item_id}")))
            .and(wiremock::matchers::header("If-Match", "\"4\""))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
                "id": item_id, "title": "after"
            })))
            .mount(&server)
            .await;

        let uri = server.uri();
        let line = format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"update_item","arguments":{{"id":"{item_id}","title":"after"}}}}}}"#
        );
        let resp = run_blocking(move || handle_line(&mock_client(&uri), &line))
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(
            v["result"]["isError"], false,
            "unexpected error: {v}\n(a wiremock 404 here means the PATCH did \
             not carry the expected If-Match header)"
        );
    }

    #[tokio::test]
    async fn mcp_move_item_sends_if_match() {
        let server = wiremock::MockServer::start().await;
        let item_id = "22222222-0000-0000-0000-000000000000";

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(format!("/api/items/{item_id}")))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("ETag", "\"9\"")
                    .set_body_json(json!({ "id": item_id, "status": "todo" })),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("PATCH"))
            .and(wiremock::matchers::path(format!("/api/items/{item_id}")))
            .and(wiremock::matchers::header("If-Match", "\"9\""))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
                "id": item_id, "status": "in_progress"
            })))
            .mount(&server)
            .await;

        let uri = server.uri();
        let line = format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"move_item","arguments":{{"id":"{item_id}","status":"in_progress"}}}}}}"#
        );
        let resp = run_blocking(move || handle_line(&mock_client(&uri), &line))
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["result"]["isError"], false, "unexpected error: {v}");
    }

    /// D4: an absent `ETag` on the read must produce an unconditional write
    /// (no `If-Match` at all), never a failure — the precondition is
    /// opt-in from the server's side, and a route/server that doesn't send
    /// an `ETag` yet must keep working exactly as it did before this card.
    #[tokio::test]
    async fn mcp_update_item_omits_if_match_when_server_sends_no_etag() {
        struct NoIfMatch;
        impl wiremock::Match for NoIfMatch {
            fn matches(&self, request: &wiremock::Request) -> bool {
                !request.headers.contains_key("if-match")
            }
        }

        let server = wiremock::MockServer::start().await;
        let item_id = "33333333-0000-0000-0000-000000000000";

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(format!("/api/items/{item_id}")))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_json(json!({ "id": item_id, "title": "before" })),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("PATCH"))
            .and(wiremock::matchers::path(format!("/api/items/{item_id}")))
            .and(NoIfMatch)
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
                "id": item_id, "title": "after"
            })))
            .mount(&server)
            .await;

        let uri = server.uri();
        let line = format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"update_item","arguments":{{"id":"{item_id}","title":"after"}}}}}}"#
        );
        let resp = run_blocking(move || handle_line(&mock_client(&uri), &line))
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["result"]["isError"], false, "unexpected error: {v}");
    }

    /// The failure mode this whole path is designed against: a 412 must
    /// read as "you raced, re-read and retry," not as an opaque failure an
    /// agent might retry blindly and clobber the change that won.
    #[tokio::test]
    async fn mcp_update_item_412_tells_the_agent_to_reread() {
        let server = wiremock::MockServer::start().await;
        let item_id = "44444444-0000-0000-0000-000000000000";

        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(format!("/api/items/{item_id}")))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("ETag", "\"1\"")
                    .set_body_json(json!({ "id": item_id, "title": "stale" })),
            )
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("PATCH"))
            .and(wiremock::matchers::path(format!("/api/items/{item_id}")))
            .respond_with(wiremock::ResponseTemplate::new(412).set_body_json(json!({
                "error": { "status": 412, "message": "version mismatch" }
            })))
            .mount(&server)
            .await;

        let uri = server.uri();
        let line = format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"update_item","arguments":{{"id":"{item_id}","title":"new"}}}}}}"#
        );
        let resp = run_blocking(move || handle_line(&mock_client(&uri), &line))
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&resp).unwrap();
        assert_eq!(v["result"]["isError"], true);
        let text = v["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("412"), "unexpected: {text}");
        assert!(
            text.to_lowercase().contains("re-read"),
            "must tell the agent to re-read, not just fail: {text}"
        );
    }

    #[test]
    fn compact_item_projects_expected_fields() {
        let full = json!({
            "id": "a", "title": "t", "item_type": "task", "status": "todo",
            "priority": "high", "assignee": "me", "project_id": "p",
            "description": "should be dropped", "tags": ["x"]
        });
        let c = compact_item(&full);
        assert_eq!(c["title"], "t");
        assert!(c.get("description").is_none());
    }
}
