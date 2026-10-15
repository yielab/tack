//! An in-process docket, implemented key by key from its published specs:
//! `specs/api/operator-api.spec.md` (version 5.0.1) and
//! `specs/data/serve-read-api.spec.md` (`/status.json`, `/traces`, `/inbox`,
//! `/approvals`, `/tasks/<id>/answer`). Which spec version it mirrors, and
//! every key it leaves out, is in
//! `src/harness/fixtures/docket/v5/README.md`.
//!
//! It serves on a loopback port the OS picked, so nothing is ever spawned and
//! no real model is ever called. A test scripts a task before the adapter posts
//! it; the fake records every request for assertion.

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, Method, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

pub const TOKEN: &str = "fake-docket-token";

/// The tool names `restrictions.deniedTools` accepts (`pod set deniedTools`).
const TOOLS: [&str; 9] = [
    "read", "write", "edit", "glob", "grep", "bash", "fetch", "skill", "consult",
];

/// What a task waits on before it can finish.
#[derive(Clone, Debug)]
pub enum Gate {
    None,
    /// A `pre_tool_call` policy parked the task on an approval.
    Approval {
        token: String,
        tool: String,
        action: String,
    },
    /// A step parked the task on a question (`waiting_input`).
    Consult {
        question: String,
        property: String,
    },
    /// Never finishes by itself; only a cancel ends it.
    Hold,
}

/// What a task does once it is posted: the test fills this in first.
#[derive(Clone, Debug)]
pub struct Script {
    /// Trace events written when the task is posted: `(event_type, payload)`.
    pub before: Vec<(String, Value)>,
    pub gate: Gate,
    /// Trace events written when the gate resolves (at once without one).
    pub after: Vec<(String, Value)>,
    pub status: String,
    pub failure_kind: Option<String>,
    pub error: String,
    /// A file docket commits on the task's branch: `(path, content)`.
    pub commit: Option<(String, String)>,
    pub untracked: Vec<String>,
    pub usage: Value,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            before: Vec::new(),
            gate: Gate::None,
            after: Vec::new(),
            status: "done".to_owned(),
            failure_kind: None,
            error: String::new(),
            commit: None,
            untracked: Vec::new(),
            usage: json!({"input": 0, "output": 0, "byModel": {}}),
        }
    }
}

/// One request the fake saw.
#[derive(Clone, Debug)]
pub struct Recorded {
    pub method: String,
    pub path: String,
    pub query: String,
    pub body: Value,
}

struct Task {
    pod: String,
    id: String,
    body: Value,
    status: String,
    script: Script,
    branch: String,
    base_commit: String,
    hops: Vec<Value>,
    gate_open: bool,
}

struct Pod {
    codebase: String,
    profile: String,
    violations: Vec<(String, String)>,
}

struct World {
    api_version: String,
    pods: BTreeMap<String, Pod>,
    tasks: Vec<Task>,
    idempotency: HashMap<(String, String), String>,
    script: Script,
    trace: Vec<(String, Value)>,
    requests: Vec<Recorded>,
    next: u32,
    repository: Option<PathBuf>,
    models: Vec<String>,
}

pub struct FakeDocket {
    pub base_url: String,
    shared: Arc<Mutex<World>>,
    token_file: tempfile::TempDir,
}

impl FakeDocket {
    pub async fn start() -> Self {
        let shared = Arc::new(Mutex::new(World {
            api_version: "5".to_owned(),
            pods: BTreeMap::new(),
            tasks: Vec::new(),
            idempotency: HashMap::new(),
            script: Script::default(),
            trace: Vec::new(),
            requests: Vec::new(),
            next: 0,
            repository: None,
            models: vec!["local/local-model".to_owned()],
        }));
        let app = Router::new()
            .fallback(handle)
            .with_state(Arc::clone(&shared));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port");
        let base_url = format!("http://{}", listener.local_addr().expect("address"));
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        let token_file = tempfile::tempdir().expect("token directory");
        std::fs::write(token_file.path().join("token"), format!("{TOKEN}\n")).expect("token");
        Self {
            base_url,
            shared,
            token_file,
        }
    }

    pub fn token_file(&self) -> PathBuf {
        self.token_file.path().join("token")
    }

    pub fn set_api_version(&self, version: &str) {
        self.shared.lock().unwrap().api_version = version.to_owned();
    }

    /// The pod `GET /pods` reports, with the violations its profile carries.
    pub fn add_pod(&self, name: &str, profile: &str, violations: &[(&str, &str)]) {
        self.shared.lock().unwrap().pods.insert(
            name.to_owned(),
            Pod {
                codebase: String::new(),
                profile: profile.to_owned(),
                violations: violations
                    .iter()
                    .map(|(requirement, fix)| ((*requirement).to_owned(), (*fix).to_owned()))
                    .collect(),
            },
        );
    }

    /// The repository docket commits a task's branch in.
    pub fn set_repository(&self, path: &Path) {
        self.shared.lock().unwrap().repository = Some(path.to_path_buf());
    }

    pub fn script(&self, script: Script) {
        self.shared.lock().unwrap().script = script;
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.shared.lock().unwrap().requests.clone()
    }

    /// The requests with this method whose path starts with `prefix`.
    pub fn requests_to(&self, method: &str, prefix: &str) -> Vec<Recorded> {
        self.requests()
            .into_iter()
            .filter(|request| request.method == method && request.path.starts_with(prefix))
            .collect()
    }

    pub fn task_ids(&self) -> Vec<String> {
        let state = self.shared.lock().unwrap();
        state.tasks.iter().map(|task| task.id.clone()).collect()
    }
}

fn reply(status: u16, body: Value) -> Response {
    (
        StatusCode::from_u16(status).expect("status"),
        [
            ("content-type", "application/json"),
            ("cache-control", "no-store"),
        ],
        body.to_string(),
    )
        .into_response()
}

fn error(status: u16, sentence: &str) -> Response {
    reply(status, json!({"error": sentence}))
}

fn git(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .env("GIT_AUTHOR_NAME", "docket")
        .env("GIT_AUTHOR_EMAIL", "docket@localhost")
        .env("GIT_COMMITTER_NAME", "docket")
        .env("GIT_COMMITTER_EMAIL", "docket@localhost")
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// `2026-10-14T10:00:SSZ`, two events to a second so the compound cursor's
/// `:n` suffix is exercised.
fn timestamp(index: usize) -> String {
    let seconds = index / 2;
    format!("2026-10-14T10:{:02}:{:02}Z", seconds / 60, seconds % 60)
}

fn trace_event(state: &mut World, pod: &str, task: &str, event_type: &str, payload: Value) {
    let mut payload = payload;
    payload["task"] = json!(task);
    let line = json!({
        "ts": timestamp(state.trace.len()),
        "project": pod,
        "session_id": format!("agent:{pod}:{task}"),
        "agent_role": payload["role"].as_str().unwrap_or("implementer"),
        "event_type": event_type,
        "payload": payload,
    });
    state.trace.push((pod.to_owned(), line));
}

async fn handle(
    State(shared): State<Arc<Mutex<World>>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let path = uri.path().to_owned();
    let query = uri.query().unwrap_or_default().to_owned();
    let parsed: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let mut state = shared.lock().unwrap();
    state.requests.push(Recorded {
        method: method.to_string(),
        path: path.clone(),
        query: query.clone(),
        body: parsed.clone(),
    });
    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    // `/status.json` and `/health` need no token; everything else answers
    // `401` before any other check.
    let open = matches!(segments.as_slice(), ["status.json"] | ["health"]);
    if !open {
        let sent = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok());
        if sent != Some(&format!("Bearer {TOKEN}")) {
            return error(401, "Unauthorized");
        }
    }
    let raw_body = !body.is_empty() && parsed.is_null();
    match (method.as_str(), segments.as_slice()) {
        ("GET", ["status.json"]) => status(&state),
        ("GET", ["health"]) => reply(
            200,
            json!({"ok": true, "apiVersion": state.api_version,
                   "dispatch": {"on": true, "inflight": [], "stopping": false, "workers": 4}}),
        ),
        ("GET", ["pods"]) => pods(&state),
        ("GET", ["pods", pod]) => pod_report(&state, pod),
        ("POST", ["pods"]) if raw_body => error(400, "Invalid JSON body"),
        ("POST", ["pods"]) => post_pod(&mut state, &parsed),
        ("POST", ["tasks", pod]) if raw_body => {
            let _ = pod;
            error(400, "Invalid JSON body")
        }
        ("POST", ["tasks", pod]) => post_task(&mut state, pod, &parsed),
        ("GET", ["tasks", pod, id]) => get_task(&state, pod, id),
        ("POST", ["tasks", pod, id, "cancel"]) => cancel_task(&mut state, pod, id, &parsed),
        ("GET", ["tasks", pod, id, "diff"]) => diff(&state, pod, id),
        ("POST", ["tasks", id, "answer"]) => answer(&mut state, id, &parsed),
        ("GET", ["traces", pod]) => traces(&state, pod, &query),
        ("GET", ["inbox"]) => inbox(&state),
        ("POST", ["approvals", token]) => approve(&mut state, token, &parsed),
        _ => error(404, "not found"),
    }
}

fn status(state: &World) -> Response {
    let agents: Vec<Value> = state
        .models
        .iter()
        .enumerate()
        .map(|(index, model)| {
            json!({"id": format!("member-{index}"), "name": "member", "kind": "project",
                   "scope": "project", "model": model, "registered": true,
                   "bindings": [], "lastActivity": "never", "costUsd": 0.0, "budgetUsd": null})
        })
        .collect();
    reply(
        200,
        json!({"apiVersion": state.api_version, "timestamp": "2026-10-14T10:00:00Z",
               "channels": [], "agents": agents, "totalCostUsd": 0.0}),
    )
}

fn pods(state: &World) -> Response {
    let pods: Vec<Value> = state
        .pods
        .iter()
        .map(|(name, pod)| {
            let pending = state
                .tasks
                .iter()
                .filter(|task| &task.pod == name && task.status == "pending")
                .count();
            let running = state
                .tasks
                .iter()
                .filter(|task| &task.pod == name && task.status == "running")
                .count();
            json!({"name": name, "codebase": pod.codebase, "members": 2,
                   "profile": pod.profile, "queue": {"pending": pending, "running": running}})
        })
        .collect();
    reply(200, json!({"pods": pods}))
}

fn pod_report(state: &World, name: &str) -> Response {
    let Some(pod) = state.pods.get(name) else {
        return error(404, "no such pod");
    };
    let violations: Vec<Value> = pod
        .violations
        .iter()
        .map(|(requirement, fix)| json!({"requirement": requirement, "fix": fix}))
        .collect();
    reply(
        200,
        json!({"members": [{"role": "implementer", "model": "local/local-model", "modelSource": "pod"}],
               "settings": {}, "pipeline": {}, "configOfRecord": "pod", "mcpServers": [],
               "skills": [], "exporters": [],
               "profile": {"name": pod.profile, "violations": violations},
               "pipelineSource": "pod", "repoConfigDir": ""}),
    )
}

fn post_pod(state: &mut World, body: &Value) -> Response {
    let Some(name) = body["name"].as_str().filter(|name| !name.is_empty()) else {
        return error(400, "name is required");
    };
    let ensure = body["ensure"] == json!(true);
    if state.pods.contains_key(name) {
        return if ensure {
            reply(200, json!({"ok": true, "created": false, "project": name}))
        } else {
            error(409, "pod already exists")
        };
    }
    state.pods.insert(
        name.to_owned(),
        Pod {
            codebase: body["location"].as_str().unwrap_or_default().to_owned(),
            profile: "lights-out".to_owned(),
            violations: Vec::new(),
        },
    );
    reply(201, json!({"ok": true, "created": true, "project": name}))
}

/// The body rules of `POST /tasks/<pod>`: `400` for a wrong shape, `422` for
/// a restriction that would widen.
fn check_task_body(body: &Value) -> Option<Response> {
    let Some(object) = body.as_object() else {
        return Some(error(400, "Invalid JSON body"));
    };
    if object
        .get("description")
        .and_then(Value::as_str)
        .is_none_or(|text| text.trim().is_empty())
    {
        return Some(error(400, "description is required"));
    }
    for key in ["externalRef", "idempotencyKey"] {
        if let Some(value) = object.get(key)
            && !value
                .as_str()
                .is_some_and(|text| (1..=200).contains(&text.chars().count()))
        {
            return Some(error(400, &format!("{key} must be 1-200 characters")));
        }
    }
    for key in ["baseRef", "pipeline"] {
        if object.get(key).is_some_and(|value| !value.is_string()) {
            return Some(error(400, &format!("{key} must be a string")));
        }
    }
    if let Some(gate) = object.get("gate")
        && !gate["command"].as_str().is_some_and(|c| !c.is_empty())
    {
        return Some(error(400, "gate.command must be a string"));
    }
    if let Some(max) = object.get("maxTokens")
        && max.as_u64().is_none_or(|n| n == 0)
    {
        return Some(error(400, "maxTokens must be an integer above 0"));
    }
    if let Some(priority) = object.get("priority")
        && !matches!(priority.as_str(), Some("high" | "normal" | "low"))
    {
        return Some(error(400, "priority must be high, normal or low"));
    }
    if let Some(brief) = object.get("brief")
        && !brief["objective"]
            .as_str()
            .is_some_and(|text| !text.trim().is_empty())
    {
        return Some(error(422, "brief: objective must not be empty"));
    }
    if let Some(restrictions) = object.get("restrictions") {
        if restrictions["network"]
            .as_str()
            .is_some_and(|n| n != "none")
        {
            return Some(error(422, "restrictions.network may only be none"));
        }
        for tool in restrictions["deniedTools"].as_array().into_iter().flatten() {
            if !tool.as_str().is_some_and(|name| TOOLS.contains(&name)) {
                return Some(error(
                    422,
                    &format!("restrictions.deniedTools names an unknown tool: {tool}"),
                ));
            }
        }
    }
    None
}

fn post_task(state: &mut World, pod: &str, body: &Value) -> Response {
    if !state.pods.contains_key(pod) {
        return error(404, "no such pod");
    }
    if let Some(refused) = check_task_body(body) {
        return refused;
    }
    if let Some(key) = body["idempotencyKey"].as_str()
        && let Some(id) = state.idempotency.get(&(pod.to_owned(), key.to_owned()))
    {
        let task = state
            .tasks
            .iter()
            .find(|task| &task.id == id)
            .expect("task");
        return reply(
            200,
            json!({"ok": true, "created": false, "task": task.id, "project": pod,
                   "status": task.status}),
        );
    }
    state.next += 1;
    let id = format!("task-{:04x}", state.next);
    let script = state.script.clone();
    let branch = format!("docket/{pod}/{id}");
    let mut base_commit = String::new();
    if let Some(repository) = state.repository.clone() {
        let base_ref = body["baseRef"].as_str().unwrap_or("HEAD");
        base_commit = git(
            &repository,
            &["rev-parse", &format!("{base_ref}^{{commit}}")],
        );
        git(&repository, &["branch", &branch, &base_commit]);
    }
    if let Some(key) = body["idempotencyKey"].as_str() {
        state
            .idempotency
            .insert((pod.to_owned(), key.to_owned()), id.clone());
    }
    for (event_type, payload) in script.before.clone() {
        trace_event(state, pod, &id, &event_type, payload);
    }
    state.tasks.push(Task {
        pod: pod.to_owned(),
        id: id.clone(),
        body: body.clone(),
        status: "running".to_owned(),
        script,
        branch,
        base_commit,
        hops: Vec::new(),
        gate_open: true,
    });
    let index = state.tasks.len() - 1;
    if matches!(state.tasks[index].script.gate, Gate::None) {
        finish(state, index);
    } else if matches!(state.tasks[index].script.gate, Gate::Consult { .. }) {
        state.tasks[index].status = "waiting_input".to_owned();
    } else if matches!(state.tasks[index].script.gate, Gate::Approval { .. }) {
        state.tasks[index].status = "waiting_approval".to_owned();
    }
    reply(
        201,
        json!({"ok": true, "created": true, "task": id, "project": pod, "status": "pending"}),
    )
}

/// Writes the `after` events, commits the file on the task's branch and sets
/// the scripted terminal status.
fn finish(state: &mut World, index: usize) {
    let (pod, id, script) = {
        let task = &state.tasks[index];
        (task.pod.clone(), task.id.clone(), task.script.clone())
    };
    for (event_type, payload) in script.after.clone() {
        trace_event(state, &pod, &id, &event_type, payload);
    }
    if script.status == "done"
        && let (Some((path, content)), Some(repository)) =
            (script.commit.clone(), state.repository.clone())
    {
        let branch = state.tasks[index].branch.clone();
        let scratch = tempfile::tempdir().expect("scratch");
        let directory = scratch.path().join("wt");
        git(
            &repository,
            &["worktree", "add", directory.to_str().unwrap(), &branch],
        );
        std::fs::create_dir_all(directory.join(&path).parent().unwrap()).expect("directory");
        std::fs::write(directory.join(&path), content).expect("file");
        git(&directory, &["add", "-A"]);
        git(&directory, &["commit", "-m", "docket: the task's change"]);
        git(
            &repository,
            &["worktree", "remove", "--force", directory.to_str().unwrap()],
        );
    }
    state.tasks[index].status = script.status.clone();
    state.tasks[index].gate_open = false;
}

fn task_record(task: &Task) -> Value {
    let body = &task.body;
    let mut record = json!({
        "id": task.id, "status": task.status,
        "priority": body["priority"].as_str().unwrap_or("normal"),
        "externalRef": body["externalRef"], "idempotencyKey": body["idempotencyKey"],
        "baseRef": body["baseRef"], "pipeline": body["pipeline"], "gate": body["gate"],
        "maxTokens": body["maxTokens"], "restrictions": body["restrictions"],
        "worktree": {"dir": "", "branch": task.branch, "baseCommit": task.base_commit},
        "usage": task.script.usage, "hops": task.hops,
        "error": task.script.error,
    });
    if let Some(kind) = &task.script.failure_kind
        && matches!(task.status.as_str(), "failed" | "blocked")
    {
        record["failureKind"] = json!(kind);
    }
    record
}

fn get_task(state: &World, pod: &str, id: &str) -> Response {
    match state.tasks.iter().find(|t| t.pod == pod && t.id == id) {
        Some(task) => reply(200, task_record(task)),
        None => error(404, "no such task"),
    }
}

fn cancel_task(state: &mut World, pod: &str, id: &str, body: &Value) -> Response {
    let _ = body;
    let Some(index) = state.tasks.iter().position(|t| t.pod == pod && t.id == id) else {
        return error(404, "no such task");
    };
    let status = state.tasks[index].status.clone();
    if !matches!(
        status.as_str(),
        "running" | "waiting_input" | "waiting_approval" | "pending"
    ) {
        return error(409, &format!("task is {status}, not running or parked"));
    }
    state.tasks[index].status = "cancelled".to_owned();
    state.tasks[index].gate_open = false;
    reply(200, json!({"ok": true, "task": id, "status": "cancelled"}))
}

fn diff(state: &World, pod: &str, id: &str) -> Response {
    let Some(task) = state.tasks.iter().find(|t| t.pod == pod && t.id == id) else {
        return error(404, "no such task");
    };
    let (head, patch) = match &state.repository {
        Some(repository) => (
            git(repository, &["rev-parse", &task.branch]),
            git(
                repository,
                &["diff", &format!("{}..{}", task.base_commit, task.branch)],
            ),
        ),
        None => (String::new(), String::new()),
    };
    reply(
        200,
        json!({"task": id, "branch": task.branch, "baseCommit": task.base_commit,
               "head": head, "patch": patch, "untracked": task.script.untracked,
               "worktreePresent": true}),
    )
}

/// `GET /traces/<pod>?since=`: verbatim JSONL lines and the compound
/// `<ts>:<n>` cursor. The one-second hold-back is not implemented: the fake's
/// timestamps are all in the past.
fn traces(state: &World, pod: &str, query: &str) -> Response {
    let since = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("since="))
        .unwrap_or_default()
        .replace("%3A", ":");
    // `<ts>:<n>`, or a bare timestamp (n = 0).
    let (since_ts, skip) = match since.rsplit_once(':') {
        Some((ts, n)) if ts.contains(':') && n.chars().all(|c| c.is_ascii_digit()) => {
            (ts.to_owned(), n.parse::<usize>().unwrap_or(0))
        }
        _ => (since.clone(), 0),
    };
    let lines: Vec<(&str, String)> = state
        .trace
        .iter()
        .filter(|(event_pod, _)| event_pod == pod)
        .map(|(_, line)| (line["ts"].as_str().unwrap_or_default(), line.to_string()))
        .collect();
    let mut events = Vec::new();
    let mut next = since.clone();
    for (index, (ts, line)) in lines.iter().enumerate() {
        let at_ts = lines[..=index]
            .iter()
            .filter(|(other, _)| other == ts)
            .count();
        let delivered = since_ts.is_empty()
            || *ts > since_ts.as_str()
            || (*ts == since_ts.as_str() && at_ts > skip);
        if delivered {
            events.push(Value::String(line.clone()));
            next = format!("{ts}:{at_ts}");
        }
    }
    reply(200, json!({"events": events, "next": next}))
}

fn inbox(state: &World) -> Response {
    let mut needs_you = Vec::new();
    let mut running = Vec::new();
    for task in &state.tasks {
        let view = |status: &str| {
            json!({"id": task.id, "pod": task.pod, "status": status,
                   "a2aState": "WORKING", "description": task.body["description"],
                   "priority": "normal", "createdAt": "2026-10-14T10:00:00Z",
                   "updatedAt": "2026-10-14T10:00:00Z"})
        };
        match (&task.script.gate, task.gate_open, task.status.as_str()) {
            (
                Gate::Approval {
                    token,
                    tool,
                    action,
                },
                true,
                "waiting_approval",
            ) => {
                needs_you.push(json!({
                    "token": token, "pod": task.pod, "taskId": task.id, "role": "implementer",
                    "tool": tool, "action": action, "policy": "ask-bash", "state": "pending",
                    "createdAt": "2026-10-14T10:00:00Z", "expiresAt": null,
                    "a2aState": "INPUT_REQUIRED"
                }));
            }
            (Gate::Consult { question, property }, true, "waiting_input") => {
                let mut item = view("waiting_input");
                item["a2aState"] = json!("INPUT_REQUIRED");
                item["question"] = json!({
                    "id": format!("q-{}", task.id), "taskId": task.id, "pod": task.pod,
                    "step": "implementer", "message": question,
                    "requestedSchema": {"type": "object", "properties": {property: {"type": "string"}},
                                        "required": [property]},
                    "createdAt": "2026-10-14T10:00:00Z", "expiresAt": null
                });
                needs_you.push(item);
            }
            (_, _, "running") => running.push(view("running")),
            _ => {}
        }
    }
    reply(
        200,
        json!({"needsYou": needs_you, "failed": [], "doneSince": [], "running": running,
               "next": "2026-10-14T10:00:00+00:00"}),
    )
}

fn approve(state: &mut World, token: &str, body: &Value) -> Response {
    let Some(object) = body.as_object() else {
        return error(400, "Invalid JSON body");
    };
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(action, "grant" | "deny") {
        return error(400, "action must be \"grant\" or \"deny\"");
    }
    let channel = object.get("channel").cloned().unwrap_or(json!("http"));
    if !matches!(channel.as_str(), Some("http" | "tack")) {
        return error(400, &format!("Unrecognised channel: {channel}"));
    }
    let option = object
        .get("option")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if (action != "grant" && !option.is_empty())
        || !matches!(option, "" | "approve_once" | "approve_task")
    {
        return error(
            400,
            "option must be \"approve_once\" or \"approve_task\" (grant only)",
        );
    }
    let Some(index) = state.tasks.iter().position(
        |task| matches!(&task.script.gate, Gate::Approval { token: t, .. } if t == token),
    ) else {
        return error(404, "no such approval");
    };
    if !state.tasks[index].gate_open {
        return error(409, "approval already resolved");
    }
    if action == "grant" {
        finish(state, index);
    } else {
        state.tasks[index].gate_open = false;
        state.tasks[index].status = "blocked".to_owned();
        state.tasks[index].script.error = "the approval was denied".to_owned();
    }
    let outcome = if action == "grant" {
        "granted"
    } else {
        "denied"
    };
    reply(200, json!({"ok": true, "state": outcome}))
}

fn answer(state: &mut World, id: &str, body: &Value) -> Response {
    let Some(object) = body.as_object() else {
        return error(400, "Invalid JSON body");
    };
    let pod = object
        .get("pod")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if pod.is_empty() || action.is_empty() {
        return error(400, "pod and action are required");
    }
    if object
        .get("content")
        .is_some_and(|content| !content.is_object())
    {
        return error(400, "content must be an object");
    }
    if !matches!(action, "accept" | "decline" | "cancel") {
        return error(422, "action must be accept, decline or cancel");
    }
    let Some(index) = state.tasks.iter().position(|t| t.pod == pod && t.id == id) else {
        return error(404, "task not found in pod");
    };
    if state.tasks[index].status != "waiting_input" {
        return error(409, "task is not waiting_input");
    }
    if action == "accept" {
        let Gate::Consult { property, .. } = &state.tasks[index].script.gate else {
            return error(409, "task is not waiting_input");
        };
        if object["content"][property].as_str().is_none() {
            return error(422, &format!("{property} is required"));
        }
        finish(state, index);
    } else {
        state.tasks[index].gate_open = false;
        state.tasks[index].status = "blocked".to_owned();
        state.tasks[index].script.error = "the question was declined".to_owned();
    }
    reply(
        200,
        json!({"id": id, "pod": pod, "status": state.tasks[index].status,
               "a2aState": "WORKING", "description": "", "priority": "normal",
               "createdAt": "", "updatedAt": ""}),
    )
}
