//! An in-process docket that answers from the files in
//! `src/harness/fixtures/docket/v5/`: what a real docket 0.2.0b6 sent
//! (`captured`, recorded by `scripts/capture-docket-fixtures.sh`) and the
//! answers a model has to run for (`constructed`, marked so). A route's answer
//! is its file with this run's names and numbers put in, so the fake has the
//! keys docket has and no others; a request body is checked against the keys
//! of the captured requests Tack sends, and an unknown or missing key is a
//! `400` that names it, so the fake is never more lenient than the capture.
//! What it leaves out is in `src/harness/fixtures/docket/v5/README.md`.
//!
//! It serves on a loopback port the OS picked, so nothing is ever spawned and
//! no real model is ever called. A test scripts a task before the adapter posts
//! it; the fake records every request for assertion.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
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

/// Where the captures and the constructed answers are.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/harness/fixtures/docket/v5")
}

/// One fixture's answer, as the file holds it.
pub fn fixture(name: &str) -> Value {
    let path = fixtures_dir().join(format!("{name}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|why| panic!("fixture {}: {why}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|why| panic!("fixture {}: {why}", path.display()))
}

/// A fixture with this run's names where the file has its own. The swap is on
/// the text, so a name inside a sentence or a path changes with the key.
fn fixture_with(name: &str, swaps: &[(&str, &str)]) -> Value {
    let mut text = fixture(name).to_string();
    for (from, to) in swaps {
        text = text.replace(&json_text(from), &json_text(to));
    }
    serde_json::from_str(&text).expect("a swapped fixture is still JSON")
}

/// `text` as it sits inside a JSON string.
fn json_text(text: &str) -> String {
    let quoted = json!(text).to_string();
    quoted[1..quoted.len() - 1].to_owned()
}

/// What a `<name>.json.provenance` says: its first line, then `key: value`.
#[derive(Clone, Debug)]
pub struct Provenance {
    pub name: String,
    pub state: String,
    pub fields: BTreeMap<String, String>,
}

impl Provenance {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(String::as_str)
    }
}

/// Every provenance file of the directory, by fixture name.
pub fn provenances() -> Vec<Provenance> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(fixtures_dir()).expect("the fixtures directory") {
        let path = entry.expect("entry").path();
        let Some(file) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(name) = file.strip_suffix(".json.provenance") else {
            continue;
        };
        let text = std::fs::read_to_string(&path).expect("provenance");
        let mut lines = text.lines();
        let state = lines.next().unwrap_or_default().to_owned();
        let fields = lines
            .filter_map(|line| line.split_once(": "))
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect();
        found.push(Provenance {
            name: name.to_owned(),
            state,
            fields,
        });
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// The status docket answered with, from the provenance.
fn fixture_status(name: &str) -> u16 {
    provenances()
        .iter()
        .find(|found| found.name == name)
        .and_then(|found| found.get("status")?.parse().ok())
        .unwrap_or_else(|| panic!("fixture {name} names no status"))
}

/// The keys a request to one route may carry, and must.
#[derive(Clone, Debug, Default)]
struct KeySpec {
    allowed: BTreeSet<String>,
    required: Option<BTreeSet<String>>,
}

/// The request keys of every route, from the captured requests Tack sends: any
/// key one of them has is allowed, and a key every one of them has is required
/// unless a provenance lists it as optional.
fn request_keys() -> HashMap<String, KeySpec> {
    let mut routes: HashMap<String, KeySpec> = HashMap::new();
    let mut optional: HashMap<String, BTreeSet<String>> = HashMap::new();
    for found in provenances() {
        let (Some(route), Some(request)) = (found.get("route"), found.get("request")) else {
            continue;
        };
        if found.state != "captured" || found.get("tack-sends") != Some("yes") {
            continue;
        }
        let text = std::fs::read_to_string(fixtures_dir().join(request)).expect("request");
        let Ok(Value::Object(body)) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let keys: BTreeSet<String> = body.keys().cloned().collect();
        let spec = routes.entry(route.to_owned()).or_default();
        spec.allowed.extend(keys.iter().cloned());
        spec.required = Some(match spec.required.take() {
            Some(required) => required.intersection(&keys).cloned().collect(),
            None => keys,
        });
        optional.entry(route.to_owned()).or_default().extend(
            found
                .get("optional-keys")
                .unwrap_or_default()
                .split_whitespace()
                .map(str::to_owned),
        );
    }
    for (route, spec) in &mut routes {
        if let (Some(required), Some(optional)) = (&mut spec.required, optional.get(route)) {
            required.retain(|key| !optional.contains(key));
        }
    }
    routes
}

/// What a task waits on before it can finish.
#[derive(Clone, Debug)]
pub enum Gate {
    None,
    /// A `pre_tool_call` policy parked the task on an approval. The task holds
    /// the token: docket sends it as the task's inbox item (`waiting_approval`,
    /// `approvalToken`) and lists the record in `GET /approvals`.
    Approval {
        token: String,
        tool: String,
        action: String,
    },
    /// An approval no task holds: docket sends it as its own inbox item
    /// (`ApprovalView`), and the record is in `GET /approvals` too.
    UnboundApproval {
        token: String,
        tool: String,
        action: String,
    },
    /// A step parked the task on a question (`waiting_input`). With `options`
    /// it is a decision: the task view lists them (the first recommended), the
    /// inbox's question does not, and an accept must name one in
    /// `content.optionId`, as docket's `validate_answer_v11` requires.
    Consult {
        question: String,
        property: String,
        options: Vec<String>,
    },
    /// Never finishes by itself; only a cancel ends it.
    Hold,
}

impl Gate {
    /// The token, tool and action of either kind of approval.
    fn approval(&self) -> Option<(&str, &str, &str)> {
        match self {
            Self::Approval {
                token,
                tool,
                action,
            }
            | Self::UnboundApproval {
                token,
                tool,
                action,
            } => Some((token, tool, action)),
            _ => None,
        }
    }
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
    /// The sentence docket's task record carries in `reason`.
    pub reason: String,
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
            reason: String::new(),
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
    /// What `readOnly` answers instead of the capture's: `Some(None)` leaves the key out, as
    /// docket did before it granted the sandbox a toolchain.
    read_only: Option<Option<Vec<String>>>,
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
    keys: HashMap<String, KeySpec>,
    /// `GET /approvals` answers `500`.
    approvals_down: bool,
    /// How many of the next answers (to a question or an approval) docket refuses with `422`.
    answers_refused: u32,
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
            keys: request_keys(),
            approvals_down: false,
            answers_refused: 0,
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
                read_only: None,
            },
        );
    }

    /// What the pod's `readOnly` answers: the folders its sandbox reads, `[]` for none, or
    /// `None` for a docket that sends no such key.
    pub fn set_read_only(&self, name: &str, paths: Option<&[&str]>) {
        let mut world = self.shared.lock().unwrap();
        let pod = world.pods.get_mut(name).expect("a pod added first");
        pod.read_only = Some(paths.map(|p| p.iter().map(|s| (*s).to_owned()).collect()));
    }

    /// The repository docket commits a task's branch in.
    pub fn set_repository(&self, path: &Path) {
        self.shared.lock().unwrap().repository = Some(path.to_path_buf());
    }

    /// Makes `GET /approvals` answer `500`, as when docket cannot read its records.
    pub fn fail_approvals_list(&self) {
        self.shared.lock().unwrap().approvals_down = true;
    }

    /// Makes docket refuse the next `count` answers to a question or an approval,
    /// as it does one a policy blocks (`422`).
    pub fn refuse_answers(&self, count: u32) {
        self.shared.lock().unwrap().answers_refused = count;
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

/// An error docket has no capture of: its `{"ok": false, "error"}` shape, a
/// sentence of the fake's own.
fn error(status: u16, sentence: &str) -> Response {
    reply(status, json!({"ok": false, "error": sentence}))
}

/// A refusal as docket gave it, with this run's names where the capture has
/// its own.
fn refusal(name: &str, swaps: &[(&str, &str)]) -> Response {
    reply(fixture_status(name), fixture_with(name, swaps))
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

/// One trace line: the envelope of a captured event, with this event's own
/// payload.
fn trace_event(state: &mut World, pod: &str, task: &str, event_type: &str, payload: Value) {
    let mut payload = payload;
    payload["task"] = json!(task);
    let template = fixture("traces-events");
    let mut line: Value =
        serde_json::from_str(template["events"][0].as_str().expect("an event line"))
            .expect("an event is JSON");
    line["ts"] = json!(timestamp(state.trace.len()));
    line["project"] = json!(pod);
    line["session_id"] = json!(format!("agent:{pod}:{task}"));
    line["agent_role"] = json!(payload["role"].as_str().unwrap_or("implementer"));
    line["event_type"] = json!(event_type);
    line["payload"] = payload;
    state.trace.push((pod.to_owned(), line));
}

/// `400` for a key the captured requests to this route never carry, or one
/// every one of them carries and this one lacks.
fn check_keys(state: &World, route: &str, body: &Value) -> Option<Response> {
    let spec = state.keys.get(route)?;
    let object = body.as_object()?;
    for key in object.keys() {
        if !spec.allowed.contains(key) {
            return Some(error(
                400,
                &format!("the fake: {route} has no key {key:?} in any captured request"),
            ));
        }
    }
    for key in spec.required.iter().flatten() {
        if !object.contains_key(key) {
            return Some(error(
                400,
                &format!("the fake: {route} lacks the key {key:?} every captured request has"),
            ));
        }
    }
    None
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
            return refusal("unauthorized", &[]);
        }
    }
    let raw_body = !body.is_empty() && parsed.is_null();
    let keyed = |route: &str| {
        if raw_body {
            None
        } else {
            check_keys(&state, route, &parsed)
        }
    };
    let refused_keys = match (method.as_str(), segments.as_slice()) {
        ("POST", ["pods"]) => keyed("POST /pods"),
        ("POST", ["tasks", _, _, "cancel"]) => keyed("POST /tasks/<pod>/<id>/cancel"),
        ("POST", ["tasks", _, "answer"]) => keyed("POST /tasks/<id>/answer"),
        ("POST", ["tasks", _]) => keyed("POST /tasks/<pod>"),
        ("POST", ["approvals", _]) => keyed("POST /approvals/<token>"),
        _ => None,
    };
    if let Some(refused) = refused_keys {
        return refused;
    }
    match (method.as_str(), segments.as_slice()) {
        ("GET", ["status.json"]) => status(&state),
        ("GET", ["health"]) => {
            let mut answer = fixture("health");
            answer["apiVersion"] = json!(state.api_version);
            reply(200, answer)
        }
        ("GET", ["pods"]) => pods(&state),
        ("GET", ["pods", pod]) => pod_report(&state, pod),
        ("POST", ["pods"]) if raw_body => refusal("post-tasks-invalid-json", &[]),
        ("POST", ["pods"]) => post_pod(&mut state, &parsed),
        ("POST", ["tasks", _]) if raw_body => refusal("post-tasks-invalid-json", &[]),
        ("POST", ["tasks", pod]) => post_task(&mut state, pod, &parsed),
        ("GET", ["tasks", pod, id]) => get_task(&state, pod, id),
        ("POST", ["tasks", pod, id, "cancel"]) => cancel_task(&mut state, pod, id),
        ("GET", ["tasks", pod, id, "diff"]) => diff(&state, pod, id),
        ("POST", ["tasks", id, "answer"]) => answer(&mut state, id, &parsed),
        ("GET", ["traces", pod]) => traces(&state, pod, &query),
        ("GET", ["inbox"]) => inbox(&state),
        ("GET", ["approvals"]) => approvals(&state),
        ("POST", ["approvals", token]) => approve(&mut state, token, &parsed),
        _ => error(404, "not found"),
    }
}

fn status(state: &World) -> Response {
    let template = fixture("status");
    let agents: Vec<Value> = state
        .models
        .iter()
        .enumerate()
        .map(|(index, model)| {
            let mut agent = template["agents"][0].clone();
            agent["id"] = json!(format!("member-{index}"));
            agent["model"] = json!(model);
            agent
        })
        .collect();
    let mut answer = template;
    answer["apiVersion"] = json!(state.api_version);
    answer["agents"] = json!(agents);
    reply(200, answer)
}

fn pods(state: &World) -> Response {
    let template = fixture("pods")["pods"][0].clone();
    let pods: Vec<Value> = state
        .pods
        .iter()
        .map(|(name, pod)| {
            let count = |status: &str| {
                state
                    .tasks
                    .iter()
                    .filter(|task| &task.pod == name && task.status == status)
                    .count()
            };
            let mut entry = template.clone();
            entry["name"] = json!(name);
            entry["codebase"] = json!(pod.codebase);
            entry["profile"] = json!(pod.profile);
            entry["queue"] = json!({
                "pending": count("pending"), "running": count("running"),
                "waitingApproval": count("waiting_approval"),
                "waitingInput": count("waiting_input"),
                "failed": count("failed"), "completed": count("done"),
            });
            entry
        })
        .collect();
    reply(200, json!({"pods": pods}))
}

fn pod_report(state: &World, name: &str) -> Response {
    let Some(pod) = state.pods.get(name) else {
        return refusal("pod-unknown", &[("no-such-team", name)]);
    };
    let mut report = fixture_with("pod-attended", &[("fab-dev", name)]);
    let violations: Vec<Value> = pod
        .violations
        .iter()
        .map(|(requirement, fix)| json!({"requirement": requirement, "fix": fix}))
        .collect();
    report["profile"] = json!({"name": pod.profile, "violations": violations});
    match &pod.read_only {
        Some(Some(paths)) => report["readOnly"] = json!(paths),
        Some(None) => {
            report.as_object_mut().unwrap().remove("readOnly");
        }
        None => {}
    }
    if let Some(model) = state.models.first() {
        for member in report["members"].as_array_mut().into_iter().flatten() {
            member["model"]["value"] = json!(model);
        }
    }
    reply(200, report)
}

fn post_pod(state: &mut World, body: &Value) -> Response {
    let Some(name) = body["name"].as_str().filter(|name| !name.is_empty()) else {
        return refusal("post-pods-old-fields", &[]);
    };
    let ensure = body["ensure"] == json!(true);
    if state.pods.contains_key(name) {
        return if ensure {
            reply(
                200,
                fixture_with("post-pods-existing", &[("fab-dev", name)]),
            )
        } else {
            refusal("post-pods-conflict", &[("fab-dev", name)])
        };
    }
    state.pods.insert(
        name.to_owned(),
        Pod {
            codebase: body["location"].as_str().unwrap_or_default().to_owned(),
            profile: "lights-out".to_owned(),
            violations: Vec::new(),
            read_only: None,
        },
    );
    reply(201, fixture_with("post-pods-created", &[("fab-dev", name)]))
}

/// The body rules of `POST /tasks/<pod>` that docket's captured refusals show:
/// `400` for a wrong shape, `422` for a restriction that would widen. What
/// docket accepts without comment (a blank description, any priority) is
/// accepted here too.
fn check_task_body(body: &Value) -> Option<Response> {
    let Some(object) = body.as_object() else {
        return Some(error(400, "Request body must be a JSON object"));
    };
    if object
        .get("description")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Some(refusal("post-tasks-no-description", &[]));
    }
    for key in ["externalRef", "idempotencyKey"] {
        if let Some(value) = object.get(key)
            && value.as_str().is_none_or(str::is_empty)
        {
            return Some(refusal(
                "post-tasks-empty-external-ref",
                &[("externalRef", key)],
            ));
        }
    }
    if let Some(gate) = object.get("gate")
        && !gate["command"].as_str().is_some_and(|c| !c.is_empty())
    {
        return Some(refusal("post-tasks-gate-without-command", &[]));
    }
    if let Some(max) = object.get("maxTokens")
        && max.as_u64().is_none_or(|n| n == 0)
    {
        return Some(refusal("post-tasks-max-tokens-zero", &[]));
    }
    if let Some(brief) = object.get("brief")
        && !brief["objective"]
            .as_str()
            .is_some_and(|text| !text.trim().is_empty())
    {
        return Some(refusal("post-tasks-brief-without-objective", &[]));
    }
    if let Some(restrictions) = object.get("restrictions") {
        if restrictions["network"]
            .as_str()
            .is_some_and(|n| n != "none")
        {
            return Some(refusal("post-tasks-network-open", &[]));
        }
        // The tools `restrictions.deniedTools` accepts are in the capture's own sentence.
        let known = fixture("post-tasks-denied-tool-unknown")["error"]
            .as_str()
            .and_then(|sentence| sentence.split_once('('))
            .map(|(_, list)| list.trim_end_matches(')').to_owned())
            .unwrap_or_default();
        for tool in restrictions["deniedTools"].as_array().into_iter().flatten() {
            let name = tool.as_str().unwrap_or_default();
            if !known.split(", ").any(|known| known == name) {
                return Some(refusal(
                    "post-tasks-denied-tool-unknown",
                    &[("no-such-tool", name)],
                ));
            }
        }
    }
    None
}

fn post_task(state: &mut World, pod: &str, body: &Value) -> Response {
    if !state.pods.contains_key(pod) {
        return refusal("post-tasks-unknown-pod", &[("no-such-team", pod)]);
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
        let mut answer = fixture("post-task-existing");
        answer["task"] = json!(task.id);
        answer["project"] = json!(pod);
        answer["status"] = json!(task.status);
        return reply(200, answer);
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
    } else if state.tasks[index].script.gate.approval().is_some() {
        state.tasks[index].status = "waiting_approval".to_owned();
    }
    let mut answer = fixture("post-task-created");
    answer["task"] = json!(id);
    answer["project"] = json!(pod);
    reply(201, answer)
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

/// A task as `GET /tasks/<pod>/<id>` sends it: the captured record of a task
/// that failed (or the constructed one of a task that finished), with this
/// task's own values. The keys of the request that docket stores come back
/// only when the request had them; the sentence is `reason`, and a task has no
/// `error` key.
fn task_record(task: &Task) -> Value {
    let body = &task.body;
    let parked = matches!(&task.script.gate, Gate::Consult { .. })
        && task.gate_open
        && task.status == "waiting_input";
    let mut record = fixture(match task.status.as_str() {
        _ if parked => "task-waiting-input-options",
        "done" => "task-done",
        _ => "task-failed-pipeline",
    });
    record["id"] = json!(task.id);
    record["description"] = body["description"].clone();
    record["status"] = json!(task.status);
    record["priority"] = json!(body["priority"].as_str().unwrap_or("normal"));
    for key in [
        "externalRef",
        "idempotencyKey",
        "baseRef",
        "pipeline",
        "gate",
        "maxTokens",
        "restrictions",
        "brief",
    ] {
        match body.get(key) {
            // docket fills a brief out to the whole document.
            Some(Value::Object(given)) if key == "brief" => {
                for (name, value) in given {
                    record["brief"][name] = value.clone();
                }
            }
            Some(value) => record[key] = value.clone(),
            None => {
                record.as_object_mut().expect("record").remove(key);
            }
        }
    }
    if parked {
        record["question"] = consult_question(task);
    }
    record["worktree"]["branch"] = json!(task.branch);
    record["worktree"]["baseCommit"] = json!(task.base_commit);
    record["worktree"]["baseRef"] = body["baseRef"].clone();
    record["reason"] = json!(task.script.reason);
    record["hops"] = json!(task.hops);
    if task.status == "done" {
        record["usage"] = task.script.usage.clone();
    }
    let object = record.as_object_mut().expect("record");
    match &task.script.failure_kind {
        Some(kind) if matches!(task.status.as_str(), "failed" | "blocked") => {
            object.insert("failureKind".to_owned(), json!(kind));
        }
        _ => {
            object.remove("failureKind");
        }
    }
    record
}

/// What a parked question asks for: its one property, required for a plain
/// question and optional beside docket's options, as the capture has it.
fn consult_schema(property: &str, options: &[String]) -> Value {
    let required: Vec<&str> = if options.is_empty() {
        vec![property]
    } else {
        Vec::new()
    };
    json!({"type": "object", "properties": {property: {"type": "string"}}, "required": required})
}

/// The parked question as the task view sends it: the captured decision with
/// this task's names, the gate's message, schema and options.
fn consult_question(task: &Task) -> Value {
    let Gate::Consult {
        question,
        property,
        options,
    } = &task.script.gate
    else {
        return Value::Null;
    };
    let mut asked = fixture("task-waiting-input-options")["question"].clone();
    asked["id"] = json!(format!("q-{}", task.id));
    asked["taskId"] = json!(task.id);
    asked["pod"] = json!(task.pod);
    asked["message"] = json!(question);
    asked["requestedSchema"] = consult_schema(property, options);
    if options.is_empty() {
        asked["kind"] = json!("clarification");
        asked["options"] = json!([]);
        asked
            .as_object_mut()
            .expect("question")
            .remove("recommendation");
    } else {
        let listed: Vec<Value> = options
            .iter()
            .map(|id| {
                let mut option = asked["options"][0].clone();
                option["id"] = json!(id);
                option["label"] = json!(format!("Option {id}"));
                option
            })
            .collect();
        asked["options"] = json!(listed);
        asked["recommendation"]["optionId"] = json!(options[0]);
    }
    asked
}

fn get_task(state: &World, pod: &str, id: &str) -> Response {
    match state.tasks.iter().find(|t| t.pod == pod && t.id == id) {
        Some(task) => reply(200, task_record(task)),
        None => refusal("task-unknown", &[("fab-dev", pod), ("task-unknown", id)]),
    }
}

fn cancel_task(state: &mut World, pod: &str, id: &str) -> Response {
    let Some(index) = state.tasks.iter().position(|t| t.pod == pod && t.id == id) else {
        return refusal(
            "task-cancel-unknown",
            &[("fab-dev", pod), ("task-unknown", id)],
        );
    };
    let status = state.tasks[index].status.clone();
    if !matches!(
        status.as_str(),
        "running" | "waiting_input" | "waiting_approval" | "pending"
    ) {
        return refusal(
            "task-cancel-finished",
            &[
                ("task-00000000-0000-0000-0000-000000000000", id),
                (" failed;", &format!(" {status};")),
            ],
        );
    }
    state.tasks[index].status = "cancelled".to_owned();
    state.tasks[index].gate_open = false;
    let mut answer = fixture("cancel-accepted");
    answer["task"] = json!(id);
    reply(200, answer)
}

fn diff(state: &World, pod: &str, id: &str) -> Response {
    let Some(task) = state.tasks.iter().find(|t| t.pod == pod && t.id == id) else {
        return refusal(
            "task-diff-unknown",
            &[("fab-dev", pod), ("task-unknown", id)],
        );
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
    let mut answer = fixture("task-diff-failed");
    answer["task"] = json!(id);
    answer["branch"] = json!(task.branch);
    answer["baseCommit"] = json!(task.base_commit);
    answer["head"] = json!(head);
    answer["patch"] = json!(patch);
    answer["untracked"] = json!(task.script.untracked);
    reply(200, answer)
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
    let template = fixture("inbox-busy");
    let waiting = fixture("inbox-waiting-approval");
    let mut needs_you = Vec::new();
    let mut running = Vec::new();
    for task in &state.tasks {
        match (&task.script.gate, task.gate_open, task.status.as_str()) {
            (
                Gate::Approval {
                    token,
                    tool: _,
                    action: _,
                },
                true,
                "waiting_approval",
            ) => {
                let mut item = waiting["needsYou"][0].clone();
                item["id"] = json!(task.id);
                item["pod"] = json!(task.pod);
                item["approvalToken"] = json!(token);
                item["reason"] = json!(format!(
                    "implementer hop parked for approval (token={token})"
                ));
                needs_you.push(item);
            }
            (
                Gate::UnboundApproval {
                    token,
                    tool,
                    action,
                },
                true,
                "waiting_approval",
            ) => {
                let mut item = template["needsYou"][0].clone();
                item["token"] = json!(token);
                item["pod"] = json!(task.pod);
                item["taskId"] = json!(task.id);
                item["tool"] = json!(tool);
                item["action"] = json!(action);
                needs_you.push(item);
            }
            (
                Gate::Consult {
                    question,
                    property,
                    options,
                },
                true,
                "waiting_input",
            ) => {
                let mut item = fixture("inbox-waiting-input-options")["needsYou"][0].clone();
                item["id"] = json!(task.id);
                item["pod"] = json!(task.pod);
                item["description"] = task.body["description"].clone();
                item["question"]["id"] = json!(format!("q-{}", task.id));
                item["question"]["taskId"] = json!(task.id);
                item["question"]["pod"] = json!(task.pod);
                item["question"]["message"] = json!(question);
                item["question"]["requestedSchema"] = consult_schema(property, options);
                needs_you.push(item);
            }
            (_, _, "running") => {
                let mut item = template["running"][0].clone();
                item["id"] = json!(task.id);
                item["pod"] = json!(task.pod);
                item["description"] = task.body["description"].clone();
                running.push(item);
            }
            _ => {}
        }
    }
    let mut answer = template;
    answer["needsYou"] = json!(needs_you);
    answer["running"] = json!(running);
    reply(200, answer)
}

/// `GET /approvals`: a record for each approval a task waits on, in the
/// captured record's shape.
fn approvals(state: &World) -> Response {
    if state.approvals_down {
        return error(500, "approvals could not be read");
    }
    let template = fixture("approvals-pending");
    let pending: Vec<Value> = state
        .tasks
        .iter()
        .filter(|task| task.gate_open && task.status == "waiting_approval")
        .filter_map(|task| {
            let (token, tool, action) = task.script.gate.approval()?;
            let mut record = template["pending"][0].clone();
            record["token"] = json!(token);
            record["project"] = json!(task.pod);
            record["action"] = json!(action);
            record["context"]["tool"] = json!(tool);
            if matches!(task.script.gate, Gate::UnboundApproval { .. }) {
                record["context"]["taskId"] = Value::Null;
            } else {
                record["context"]["taskId"] = json!(task.id);
            }
            Some(record)
        })
        .collect();
    reply(200, json!({"pending": pending}))
}

fn approve(state: &mut World, token: &str, body: &Value) -> Response {
    let Some(object) = body.as_object() else {
        return refusal("post-tasks-invalid-json", &[]);
    };
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(action, "grant" | "deny") {
        return refusal("approval-bad-action", &[]);
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
    if state.answers_refused > 0 {
        state.answers_refused -= 1;
        return error(422, "blocked by policy 'pre_input'");
    }
    let Some(index) = state
        .tasks
        .iter()
        .position(|task| matches!(task.script.gate.approval(), Some((held, ..)) if held == token))
    else {
        return refusal("approval-unknown", &[]);
    };
    if !state.tasks[index].gate_open {
        return error(409, "approval already resolved");
    }
    if action == "grant" {
        finish(state, index);
    } else {
        state.tasks[index].gate_open = false;
        state.tasks[index].status = "blocked".to_owned();
        state.tasks[index].script.reason = "the approval was denied".to_owned();
    }
    let mut answer = fixture("approval-granted");
    answer["state"] = json!(if action == "grant" {
        "granted"
    } else {
        "denied"
    });
    reply(200, answer)
}

fn answer(state: &mut World, id: &str, body: &Value) -> Response {
    let Some(object) = body.as_object() else {
        return refusal("post-tasks-invalid-json", &[]);
    };
    let pod = object
        .get("pod")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if pod.is_empty() {
        return refusal("answer-without-pod", &[]);
    }
    if object
        .get("content")
        .is_some_and(|content| !content.is_object())
    {
        return error(400, "content must be an object");
    }
    if !matches!(action, "accept" | "decline" | "cancel") {
        return refusal("answer-bad-action", &[("maybe", action)]);
    }
    let Some(index) = state.tasks.iter().position(|t| t.pod == pod && t.id == id) else {
        return refusal("answer-unknown", &[("fab-dev", pod), ("task-unknown", id)]);
    };
    if state.tasks[index].status != "waiting_input" {
        return error(409, "task is not waiting_input");
    }
    if state.answers_refused > 0 {
        state.answers_refused -= 1;
        return error(422, "blocked by policy 'pre_input'");
    }
    if action == "accept" {
        let Gate::Consult {
            property, options, ..
        } = &state.tasks[index].script.gate
        else {
            return error(409, "task is not waiting_input");
        };
        if !options.is_empty() {
            // docket's own refusals, from `validate_answer_v11`.
            match object["content"]["optionId"].as_str() {
                None => {
                    return error(
                        422,
                        "accept requires optionId when the question has options",
                    );
                }
                Some(id) if !options.iter().any(|own| own == id) => {
                    return error(422, &format!("optionId '{id}' names no option"));
                }
                Some(_) => {}
            }
        }
        if options.is_empty() && object["content"][property].as_str().is_none() {
            return error(422, &format!("{property} is required"));
        }
        finish(state, index);
    } else {
        state.tasks[index].gate_open = false;
        state.tasks[index].status = "blocked".to_owned();
        state.tasks[index].script.reason = "the question was declined".to_owned();
    }
    let mut answer = fixture("answer-accepted");
    answer["id"] = json!(id);
    answer["pod"] = json!(pod);
    answer["status"] = json!(state.tasks[index].status);
    reply(200, answer)
}
