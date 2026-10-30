//! The body of `POST /tasks/<pod>`, built from the claimed request.

use serde_json::{Map, Value, json};
use tack_protocol::execution::{AutonomyLevel, ExecutionRequestSnapshot, TaskDocument};
use tack_protocol::prepare::PrepareSetting;

/// What docket's `gate` gets when a criterion does not say how long.
const GATE_TIMEOUT_SECONDS: u64 = 600;

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
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

/// The paths the acceptance criteria name (a `file` or `absent` path, a `command`'s `cwd`),
/// relative to the repository and without repeats. docket bounds the hop's repository map by
/// them, so one that names the whole repository, or lies outside it, is left out.
fn paths(acceptance: &[Value]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in acceptance {
        let path = match text(item, "kind") {
            Some("file" | "absent") => text(item, "path"),
            Some("command") => text(item, "cwd"),
            _ => None,
        };
        let Some(path) = path else { continue };
        let path = path.strip_prefix("./").unwrap_or(path);
        if path.is_empty()
            || path == "."
            || path.starts_with('/')
            || path.split('/').any(|part| part == "..")
        {
            continue;
        }
        if !out.iter().any(|seen| seen == path) {
            out.push(path.to_owned());
        }
    }
    out
}

/// The questions docket asked in the item's previous attempt and what a person answered, as
/// `start_run` recorded them in `metadata.previous_answers`.
fn answers(request: &ExecutionRequestSnapshot) -> Vec<Value> {
    request
        .metadata
        .get("previous_answers")
        .and_then(Value::as_array)
        .map(|pairs| {
            pairs
                .iter()
                .filter(|pair| text(pair, "question").is_some() && text(pair, "answer").is_some())
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// `task` of `POST /tasks/<pod>`: the item as the fields docket's `## Task` section renders.
/// A field with nothing to say is left out. `objective` is the title, `doneWhen` the brief's
/// definition of done else the project's, and nothing here is derived from the prose.
fn task_fields(task: &TaskDocument, request: &ExecutionRequestSnapshot) -> Value {
    let brief = task.brief.as_ref();
    let list = |key: &str, each: fn(&Value) -> Option<String>| -> Vec<String> {
        brief
            .and_then(|brief| brief.get(key))
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(each).collect())
            .unwrap_or_default()
    };
    let mut fields = Map::new();
    fields.insert("objective".into(), json!(task.title));
    let done = brief
        .and_then(|brief| text(brief, "definition_of_done"))
        .filter(|done| !done.trim().is_empty())
        .or(task.definition_of_done.as_deref());
    if let Some(done) = done {
        fields.insert("doneWhen".into(), json!(done));
    }
    for (key, values) in [
        ("acceptance", list("acceptance", criterion)),
        ("constraints", list("constraints", constraint)),
        (
            "paths",
            paths(
                brief
                    .and_then(|brief| brief.get("acceptance")?.as_array())
                    .map_or(&[][..], Vec::as_slice),
            ),
        ),
    ] {
        if !values.is_empty() {
            fields.insert(key.into(), json!(values));
        }
    }
    if let Some(risk) = brief.and_then(|brief| text(brief, "risk")) {
        fields.insert("risk".into(), json!(risk));
    }
    if !task.kind.is_empty() {
        fields.insert("kind".into(), json!(task.kind));
    }
    let answers = answers(request);
    if !answers.is_empty() {
        fields.insert("answers".into(), json!(answers));
    }
    Value::Object(fields)
}

/// The body of `POST /tasks/<pod>` for one attempt. `base_ref` is the branch
/// or commit the task starts from. `autonomy` is the level a person asked this run for, sent as
/// it is; docket decides when there is none.
///
/// The card goes once, as `task` fields, and `description` holds only the profile's instructions
/// and the card's fenced text. No `brief` is sent: docket's "Task brief" (`pod-dispatch.spec.md`)
/// folds a caller's brief into the description as a second prose copy and gives the Lead no
/// reason to skip its intake. A request without the item as data (one made before the board
/// sent it) goes as the composed instructions and no `task`.
pub(super) fn body(
    request: &ExecutionRequestSnapshot,
    attempt_id: &str,
    attempt_number: u32,
    base_ref: &str,
    autonomy: Option<AutonomyLevel>,
) -> Value {
    let docket = request.resolved_agent_profile.tool_policy.get("docket");
    let description = match &request.task {
        Some(task) => {
            let fence = uuid::Uuid::new_v4().simple().to_string();
            format!("{}{}", task.instructions, task.item_section(&fence[..8]))
        }
        None => request.resolved_agent_profile.instructions.clone(),
    };
    let mut body = Map::new();
    body.insert("description".into(), json!(description));
    if let Some(task) = &request.task {
        body.insert("task".into(), task_fields(task, request));
    }
    body.insert(
        "externalRef".into(),
        json!(format!(
            "{}/attempt-{attempt_number}",
            request.item_id.as_str()
        )),
    );
    body.insert("idempotencyKey".into(), json!(attempt_id));
    body.insert("baseRef".into(), json!(base_ref));

    if let Some(gate) = request
        .brief
        .as_ref()
        .and_then(|brief| brief.get("acceptance")?.as_array())
        .and_then(|items| gate(items))
    {
        body.insert("gate".into(), gate);
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
    // Detecting is docket's own default, so only a stated command or "off" is sent.
    match &request.repository.prepare {
        Some(PrepareSetting::Replace { command }) => {
            body.insert("prepare".into(), json!(command));
        }
        Some(PrepareSetting::Off) => {
            body.insert("prepare".into(), json!(""));
        }
        Some(PrepareSetting::Detect) | None => {}
    }
    if let Some(level) = autonomy {
        body.insert("autonomy".into(), json!(level.as_str()));
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
