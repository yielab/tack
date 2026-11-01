//! The tools that start and follow runs.
//!
//! Deliberately a *subset* of what the CLI exposes: with `fleet`'s read tools, which
//! cover discovery (an agent needs valid fleet/profile ids before it can create an
//! execution), these are the core create/list/get/cancel lifecycle. `runner
//! enroll`/`revoke`, `fleet create`, `agent-profile create`, and
//! `execution reconcile` are CLI-only, matching this server's
//! existing precedent of keeping admin-ish actions (backup/restore,
//! template/role/field management) off the agent-facing tool
//! surface (see docs/MCP.md). `reconcile` specifically is an
//! operator's explicit, audited recovery decision after reviewing an
//! ambiguous `needs_operator` state — not something an
//! agent should be able to trigger on its own say-so. `runner
//! enroll` additionally returns a one-time secret; keeping it out of
//! the MCP surface means that secret can never end up in an agent's
//! tool-call transcript.
//!
//! A run's `instructions` come back already fenced: the server fences the card's text when
//! it composes them, so `get_execution` adds no second fence.

use serde_json::{Value, json};

use super::projection::data_list;
use super::tool::{
    Kind, Spec, Tool, call, opt_object_or_empty, opt_str, require_id, require_str, require_u64,
};
use crate::client::TackClient;
use crate::execution;

pub static LIST_EXECUTIONS: Tool = Tool {
    name: "list_executions",
    spec: Spec {
        description: "List execution requests (newest first).",
        input_schema: || json!({ "type": "object", "properties": {} }),
    },
    kind: Kind::Read,
    handler: list_executions,
};

fn list_executions(client: &TackClient, _args: &Value) -> Result<Value, String> {
    let executions = call(client.get("/executions"))?;
    Ok(data_list("executions", &executions))
}

pub static GET_EXECUTION: Tool = Tool {
    name: "get_execution",
    spec: Spec {
        description: "Get one execution request's current lifecycle state. A state of \
             needs_operator or lost is an ambiguous outcome, not just another \
             in-progress value — surface it distinctly to the user rather than \
             treating it like queued/running.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": { "request_id": { "type": "string" } },
                "required": ["request_id"]
            })
        },
    },
    kind: Kind::Read,
    handler: get_execution,
};

fn get_execution(client: &TackClient, args: &Value) -> Result<Value, String> {
    let id = require_id(args, "request_id")?;
    call(client.get(&format!("/executions/{id}")))
}

pub static CANCEL_EXECUTION: Tool = Tool {
    name: "cancel_execution",
    spec: Spec {
        description: "Request cancellation of an execution. Recorded as a request only \
             — the execution is not made falsely terminal; call get_execution \
             afterward to see the actual outcome once the runner reports it.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": { "request_id": { "type": "string" } },
                "required": ["request_id"]
            })
        },
    },
    kind: Kind::Write,
    handler: cancel_execution,
};

fn cancel_execution(client: &TackClient, args: &Value) -> Result<Value, String> {
    let id = require_id(args, "request_id")?;
    call(client.post(&format!("/executions/{id}/cancel"), &json!({})))
}

pub static CREATE_EXECUTION: Tool = Tool {
    name: "create_execution",
    spec: Spec {
        description: "Starts a run that asks the person before every action. Creates \
             (or idempotently replays, if idempotency_key repeats) an \
             execution request that assigns a Tack item to a coding-harness \
             runner or fleet. At most one of runner_id/fleet_id; omitted, the \
             server picks the agent profile (the item's role, else the project's \
             default), its agent and where it runs. \
             Use list_fleets/list_agent_profiles first to \
             discover valid ids.",
        input_schema: || {
            json!({
                "type": "object",
                "properties": {
                    "item_id": { "type": "string" },
                    "runner_id": { "type": "string", "description": "Exact runner id (at most one of runner_id/fleet_id)" },
                    "fleet_id": { "type": "string", "description": "Fleet id (at most one of runner_id/fleet_id)" },
                    "agent_profile_id": { "type": "string", "description": "Omit to use the item's role profile, else the project default" },
                    "harness": { "type": "string", "description": "Requested harness kind, e.g. codex, claude-code. Omit to use the profile's agent, else the project's default agent" },
                    "timeout_seconds": { "type": "integer", "description": "Omit to use the profile's time limit" },
                    "model": { "type": "string", "description": "The model to run, passed to the agent as given. Omit to use the project's default for its default agent, else the agent's own choice" },
                    "budgets": { "type": "object", "description": "Default: {}. tokens: omit to use the profile's limit, null for no limit, or a positive number" },
                    "metadata": { "type": "object", "description": "Default: {}" },
                    "status_map_policy_id": { "type": "string", "enum": ["done_on_success", "done_on_mrp_accepted"], "description": "Moves the item to its first Done status: done_on_success when the attempt succeeds, done_on_mrp_accepted when its MRP is accepted. Omit to leave the status untouched" },
                    "idempotency_key": { "type": "string", "description": "Defaults to a fresh key each call; pass a stable value to safely retry the exact same request" }
                },
                "required": [
                    "item_id"
                ]
            })
        },
    },
    kind: Kind::Write,
    handler: create_execution,
};

fn create_execution(client: &TackClient, args: &Value) -> Result<Value, String> {
    let item_id = require_str(args, "item_id")?;
    let selector = execution::selector_from_flags(
        opt_str(args, "runner_id").as_deref(),
        opt_str(args, "fleet_id").as_deref(),
    )?;
    let agent_profile_id = opt_str(args, "agent_profile_id");
    let harness = opt_str(args, "harness");
    for key in ["environment", "repository", "agent_profile_snapshot"] {
        if args.get(key).is_some() {
            return Err(format!("unknown argument: {key}"));
        }
    }
    let timeout_seconds = match args.get("timeout_seconds") {
        None | Some(Value::Null) => None,
        Some(_) => Some(require_u64(args, "timeout_seconds")?),
    };
    let idempotency_key = opt_str(args, "idempotency_key");
    let model = opt_str(args, "model");
    let status_map_policy_id = opt_str(args, "status_map_policy_id");

    let values = execution::CreateExecutionValues {
        item_id: &item_id,
        idempotency_key: idempotency_key.as_deref(),
        agent_profile_id: agent_profile_id.as_deref(),
        requested_harness_kind: harness.as_deref(),
        requested_model: model.as_deref(),
        // Fixed: a run over MCP always asks before each action, with no network.
        permission_policy: json!({ "approvals": "ask", "network": false }),
        budgets: opt_object_or_empty(args, "budgets"),
        environment: json!({}),
        metadata: opt_object_or_empty(args, "metadata"),
        timeout_seconds,
        status_map_policy_id: status_map_policy_id.as_deref(),
    };
    let mut body = execution::create_execution_body(selector.as_ref(), values);
    body["origin"] = json!("mcp");
    call(client.post("/executions", &body))
}
