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

mod board;
mod changes;
mod fleet;
mod items;
mod projection;
mod projects;
mod runs;
mod threads;
pub mod tool;

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::client::TackClient;
#[cfg(test)]
use projection::{compact_item, due_timestamp};

/// Protocol version we advertise when the client doesn't specify one.
const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";

/// Run the stdio MCP loop until stdin closes.
///
/// `stdout` carries the protocol — nothing else may be written there.
pub fn run(client: &TackClient) -> anyhow::Result<()> {
    let client = &client.marked_mcp();
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
        "tools/list" => Some(success(id, json!({ "tools": tool::advertised() }))),
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
    match tool::find(name) {
        Some(tool) => (tool.handler)(client, args),
        None => Err(format!("unknown tool: {name}")),
    }
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

#[cfg(test)]
#[path = "mcp/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "mcp/table_tests.rs"]
mod table_tests;
