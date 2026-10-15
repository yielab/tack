//! The body of `POST /tasks/<pod>`, built from the claimed request.

use serde_json::{Map, Value, json};
use tack_orch::execution::ExecutionRequestSnapshot;

/// What docket's `gate` gets when a criterion does not say how long.
const GATE_TIMEOUT_SECONDS: u64 = 600;

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

/// The card's title, read from the fenced block `compose_instructions` puts
/// after `Title:`; the first line of the text when there is no such block.
fn objective(instructions: &str) -> String {
    let fenced = instructions
        .split_once("Title:\n")
        .and_then(|(_, rest)| rest.split_once(">>>\n"))
        .and_then(|(_, rest)| rest.split_once("\n<<<"))
        .map(|(title, _)| title.trim().to_owned())
        .filter(|title| !title.is_empty());
    fenced.unwrap_or_else(|| {
        instructions
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("the task")
            .chars()
            .take(200)
            .collect()
    })
}

/// One acceptance criterion as a sentence docket's agents read.
fn criterion(item: &Value) -> Option<String> {
    let title = text(item, "title").unwrap_or_default();
    let detail = match text(item, "kind")? {
        "command" => format!(
            "`{}` exits {}",
            text(item, "run")?,
            item.get("expect_exit").and_then(Value::as_u64).unwrap_or(0)
        ),
        "test" => format!("`{}` passes", text(item, "name")?),
        "metric" => {
            let bound = match text(item, "op") {
                Some("lte") => "at most",
                Some("eq") => "exactly",
                _ => "at least",
            };
            let threshold = item.get("threshold").and_then(Value::as_f64)?;
            format!(
                "{} {bound} {threshold}{}",
                text(item, "name")?,
                text(item, "unit").unwrap_or_default()
            )
        }
        "file" => format!("{} exists", text(item, "path")?),
        "absent" => format!("{} does not exist", text(item, "path")?),
        "manual" => text(item, "text")?.to_owned(),
        _ => return (!title.is_empty()).then(|| title.to_owned()),
    };
    Some(if title.is_empty() {
        detail
    } else {
        format!("{title}: {detail}")
    })
}

fn constraint(item: &Value) -> Option<String> {
    Some(match text(item, "kind")? {
        "forbidden_path" => format!("Do not change files matching {}", text(item, "glob")?),
        "allowed_dependency" => format!("The dependency {} is allowed", text(item, "name")?),
        "max_changed_files" => format!(
            "Change at most {} files",
            item.get("n").and_then(Value::as_u64)?
        ),
        "note" => text(item, "text")?.to_owned(),
        _ => return None,
    })
}

fn quoted(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}

/// The first criterion docket can run as an external gate: a `command` that
/// expects success, a `test`, or failing those a `file`/`absent` check run
/// with `sh -c`. `manual` stays in Tack; the definition of done is prose and
/// is never a gate.
fn gate(acceptance: &[Value]) -> Option<Value> {
    let command = acceptance
        .iter()
        .find_map(|item| match text(item, "kind")? {
            "command" if item.get("expect_exit").and_then(Value::as_u64) == Some(0) => {
                text(item, "run").map(str::to_owned)
            }
            "test" => text(item, "name").map(str::to_owned),
            _ => None,
        })
        .or_else(|| {
            acceptance
                .iter()
                .find_map(|item| match text(item, "kind")? {
                    "file" => Some(format!(
                        "sh -c {}",
                        quoted(&format!("test -e {}", quoted(text(item, "path")?)))
                    )),
                    "absent" => Some(format!(
                        "sh -c {}",
                        quoted(&format!("test ! -e {}", quoted(text(item, "path")?)))
                    )),
                    _ => None,
                })
        })?;
    Some(json!({"command": command, "timeoutS": GATE_TIMEOUT_SECONDS}))
}

/// What the server recorded about the item: `metadata.item`.
fn item(request: &ExecutionRequestSnapshot) -> Option<&Value> {
    request.metadata.get("item").filter(|item| item.is_object())
}

/// Tack's item priority in docket's three.
fn priority(request: &ExecutionRequestSnapshot) -> Option<&'static str> {
    Some(match text(item(request)?, "priority")? {
        "critical" | "high" => "high",
        "medium" => "normal",
        _ => "low",
    })
}

/// The body of `POST /tasks/<pod>` for one attempt. `base_ref` is the branch
/// or commit the task starts from.
pub(super) fn body(
    request: &ExecutionRequestSnapshot,
    attempt_id: &str,
    attempt_number: u32,
    base_ref: &str,
) -> Value {
    let instructions = &request.resolved_agent_profile.instructions;
    let docket = request.resolved_agent_profile.tool_policy.get("docket");
    let mut body = Map::new();
    body.insert("description".into(), json!(instructions));
    body.insert(
        "externalRef".into(),
        json!(format!(
            "{}/attempt-{attempt_number}",
            request.item_id.as_str()
        )),
    );
    body.insert("idempotencyKey".into(), json!(attempt_id));
    body.insert("baseRef".into(), json!(base_ref));

    if let Some(brief) = &request.brief {
        let list = |key: &str, each: fn(&Value) -> Option<String>| -> Vec<String> {
            brief
                .get(key)
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(each).collect())
                .unwrap_or_default()
        };
        let (acceptance, constraints) = (
            list("acceptance", criterion),
            list("constraints", constraint),
        );
        let mut document = Map::new();
        document.insert("objective".into(), json!(objective(instructions)));
        if !acceptance.is_empty() {
            document.insert("acceptance".into(), json!(acceptance));
        }
        if !constraints.is_empty() {
            document.insert("constraints".into(), json!(constraints));
        }
        body.insert("brief".into(), Value::Object(document));
        if let Some(gate) = brief
            .get("acceptance")
            .and_then(Value::as_array)
            .and_then(|items| gate(items))
        {
            body.insert("gate".into(), gate);
        }
    }
    if let Some(pipeline) = item(request)
        .and_then(|item| text(item, "type"))
        .and_then(|kind| docket?.get("pipeline_by_type")?.get(kind)?.as_str())
    {
        body.insert("pipeline".into(), json!(pipeline));
    }
    if let Some(tokens) = request
        .budgets
        .get("tokens")
        .and_then(Value::as_u64)
        .filter(|tokens| *tokens > 0)
    {
        body.insert("maxTokens".into(), json!(tokens));
    }
    let mut restrictions = Map::new();
    let denied: Vec<&str> = docket
        .and_then(|docket| docket.get("denied_tools")?.as_array())
        .map(|tools| tools.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if !denied.is_empty() {
        restrictions.insert("deniedTools".into(), json!(denied));
    }
    if !request.permission_policy.network {
        restrictions.insert("network".into(), json!("none"));
    }
    if !restrictions.is_empty() {
        body.insert("restrictions".into(), Value::Object(restrictions));
    }
    if let Some(priority) = priority(request) {
        body.insert("priority".into(), json!(priority));
    }
    if let Some(trusted) = item(request).and_then(|item| item.get("trusted")?.as_bool()) {
        body.insert("trusted".into(), json!(trusted));
    }
    Value::Object(body)
}
