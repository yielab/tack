//! The three values a completion carries, as real protocol values for tests that go to the
//! store directly: the smallest honest `ActualExecution`, a measured or unmeasured `Usage`,
//! and a terminal reason with a code. None of them is a placeholder the store would have to
//! tolerate: each is what a runner could send.

use serde_json::{Map, Value, json};
use tack_protocol::execution::{ActualExecution, Usage};

/// The execution a codex harness on a fixed base revision reports; it carries no extra keys.
pub fn actual_execution() -> ActualExecution {
    actual_execution_with(Map::new())
}

/// [`actual_execution`] plus the keys in `extra`, which ride beside the typed ones as a
/// harness's own report does (`result`, `git`, ...).
pub fn actual_execution_with(extra: Map<String, Value>) -> ActualExecution {
    let mut value = json!({
        "harness_kind": "codex",
        "harness_version": "1.2.3",
        "model": null,
        "model_source": "agent_default",
        "capability_snapshot": {
            "cancel": {"support": "supported", "reason": null},
            "resume": {"support": "unsupported", "reason": "no resumable session contract"},
            "decisions": {"support": "supported", "reason": null},
            "artifacts": {"support": "supported", "reason": null},
            "usage": {"support": "advisory", "reason": "usage may be absent"}
        },
        "workspace_id": "ws_01J0000000000000000000000001",
        "base_revision": "0123456789abcdef0123456789abcdef01234567",
        "started_at": "2026-08-06T12:20:05Z",
        "ended_at": "2026-08-06T12:25:00Z"
    });
    if let Value::Object(object) = &mut value {
        object.extend(extra);
    }
    serde_json::from_value(value).expect("a minimal actual_execution")
}

/// [`actual_execution`] with a `result` key holding `result`.
pub fn actual_execution_with_result(result: Value) -> ActualExecution {
    actual_execution_with(Map::from_iter([("result".to_owned(), result)]))
}

/// A measured token count and nothing else measured.
pub fn usage(tokens_in: u64, tokens_out: u64) -> Usage {
    serde_json::from_value(json!({
        "tokens_in": {"value": tokens_in, "source": "measured"},
        "tokens_out": {"value": tokens_out, "source": "measured"},
        "duration_ms": {"value": null, "source": "not_measured"},
        "cost_usd": {"value": null, "source": "not_measured"}
    }))
    .expect("a minimal usage")
}

/// Usage nothing measured, which is not zero.
pub fn usage_not_measured() -> Usage {
    serde_json::from_value(json!({
        "tokens_in": {"value": null, "source": "not_measured"},
        "tokens_out": {"value": null, "source": "not_measured"},
        "duration_ms": {"value": null, "source": "not_measured"},
        "cost_usd": {"value": null, "source": "not_measured"}
    }))
    .expect("a not-measured usage")
}

/// A terminal reason with this `code`.
pub fn terminal_reason(code: &str) -> Map<String, Value> {
    Map::from_iter([("code".to_owned(), Value::from(code))])
}

/// A terminal reason from an object literal.
pub fn terminal_reason_of(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(object) => object,
        other => panic!("a terminal reason is an object, got {other}"),
    }
}
