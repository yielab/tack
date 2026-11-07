//! One trial: a world, a project with the task's item, a run started as an operator would
//! start it, the wait (answering decisions on the way), and everything the run left, frozen.
//!
//! A trial that cannot be judged because the harness broke is still a record, `error`, so a
//! batch never loses its place and the reason is on the line, not only in a log.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::api::Api;
use crate::exam::{self, ExamRecord};
use crate::graders::governance::REPO_REFS_FILE;
use crate::operator::Operator;
use crate::record::{self, CanaryRecord, SCHEMA_VERSION, TrialRecord, TrialStatus};
use crate::task::{Approvals, LoadedTask, SandboxMode};
use crate::world::{World, WorldOptions, git};

const POLL: Duration = Duration::from_millis(300);
/// How long a run gets to stop after the wall limit asked it to.
const CANCEL_GRACE: Duration = Duration::from_secs(20);
/// A run in one of these states will not move without an operator.
const ENDED: &[&str] = &["succeeded", "failed", "cancelled", "lost", "needs_operator"];

#[derive(Debug, Clone)]
pub struct TrialOptions {
    /// Where `trials.jsonl` and the frozen runs go.
    pub out: PathBuf,
    pub epoch: u32,
    /// Run the task with this agent instead of the one its profile names (the task still has
    /// to apply to it).
    pub harness: Option<String>,
    pub world: WorldOptions,
}

impl TrialOptions {
    fn harness_for<'a>(&'a self, task: &'a LoadedTask) -> &'a str {
        self.harness
            .as_deref()
            .unwrap_or(&task.task.profile.harness)
    }
}

/// Runs the trial and appends its record to `out/trials.jsonl`.
pub fn run_and_record(task: &LoadedTask, options: &TrialOptions) -> Result<TrialRecord> {
    let record = run_trial(task, options);
    record::append(&options.out, &record)?;
    Ok(record)
}

/// Runs the trial; a failure of the harness comes back as an `error` record.
pub fn run_trial(task: &LoadedTask, options: &TrialOptions) -> TrialRecord {
    match try_trial(task, options) {
        Ok(record) => record,
        Err(failure) => TrialRecord::without_run(
            task,
            options.harness_for(task),
            options.epoch,
            TrialStatus::Error,
            format!("{failure:#}"),
        ),
    }
}

fn try_trial(task: &LoadedTask, options: &TrialOptions) -> Result<TrialRecord> {
    let started = Instant::now();
    let world = World::build(task, &options.world)?;
    let server = world.start_server()?;
    let api = Api::new(&server.base_url)?;
    wait_for_runner(&api)?;

    let request_id = start_run(&api, task, &world, options)?;
    let mut operator = Operator::new(task.task.operator.decisions);
    let timed_out = wait_for_end(&api, &request_id, &mut operator, task)?;

    let frozen = freeze(&api, &request_id, task, options, &server)?;
    // A push that landed is a ref the seed repository did not have; the graders read this file.
    let refs = git(
        &world.repo,
        &["for-each-ref", "--format=%(refname) %(objectname)"],
    )?;
    std::fs::write(
        options.out.join(&frozen.dir_relative).join(REPO_REFS_FILE),
        refs,
    )?;
    let run_state = api.get(&format!("/api/executions/{request_id}"))?["state"]
        .as_str()
        .map(str::to_owned);

    let mut record = TrialRecord {
        schema: SCHEMA_VERSION,
        task: task.task.id.clone(),
        task_version: task.task.version,
        suite: task.task.suite.clone(),
        epoch: options.epoch,
        harness: options.harness_for(task).to_owned(),
        model: task.task.profile.model.clone(),
        tack_commit: git(Path::new("."), &["rev-parse", "HEAD"])
            .ok()
            .map(|sha| sha.trim().to_owned()),
        status: if timed_out {
            TrialStatus::TimedOut
        } else {
            TrialStatus::Completed
        },
        reason: timed_out.then(|| format!("the run passed {}s", task.task.limits.wall_s)),
        request_id: Some(request_id),
        run_state,
        sandbox: frozen.sandbox.clone(),
        counted_tokens: frozen.counted_tokens,
        wall_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        decisions: operator.decisions,
        events: frozen.events,
        exam: None,
        canaries: world
            .canaries
            .iter()
            .map(|canary| CanaryRecord {
                path: canary
                    .path
                    .strip_prefix(&world.home)
                    .unwrap_or(&canary.path)
                    .display()
                    .to_string(),
                intact: canary.is_intact(),
            })
            .collect(),
        fake_secret: Some(world.fake_secret.clone()),
        frozen: Some(frozen.dir_relative),
    };

    let has_sandbox = frozen.sandbox.as_deref().is_some_and(|s| s != "none");
    if task.task.sandbox == SandboxMode::Required && !has_sandbox {
        record.status = TrialStatus::NotApplicable;
        record.reason = Some("the task requires a sandbox and this machine had none".to_owned());
        return Ok(record);
    }
    record.exam = take_exam(task, &world, frozen.patch.as_deref())?;
    Ok(record)
}

/// The hidden check on the agent's tree, if the task has one.
fn take_exam(task: &LoadedTask, world: &World, patch: Option<&[u8]>) -> Result<Option<ExamRecord>> {
    let Some(command) = task.hidden_command() else {
        return Ok(None);
    };
    let tree = world.root.join("exam-tree");
    exam::export_revision(&world.repo, &world.base_revision, &tree)?;
    if let Some(patch) = patch
        && let Err(failure) = exam::apply_patch(&tree, patch)
    {
        // Work that does not apply is work that fails the exam, and says why.
        return Ok(Some(ExamRecord {
            passed: false,
            exit_code: None,
            timed_out: false,
            output_tail: format!("{failure:#}"),
        }));
    }
    let hidden = world.exam.join("hidden");
    exam::run_exam(&tree, &hidden, command, &world.root.join("exam-home")).map(Some)
}

fn wait_for_runner(api: &Api) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        let runners = api.get("/api/runners")?;
        if runners["data"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|runner| runner["state"] == "active")
        {
            return Ok(());
        }
        std::thread::sleep(POLL);
    }
    bail!("the embedded runner did not become active within 60s")
}

/// Creates the project, the profile and the item, and starts the run; returns the request id.
fn start_run(
    api: &Api,
    task: &LoadedTask,
    world: &World,
    options: &TrialOptions,
) -> Result<String> {
    let spec = &task.task;
    let project = api.post(
        "/api/projects",
        &json!({
            "name": format!("eval {}", spec.id),
            "project_type": "software",
            "code_origin": "folder",
            "repository": world.repo,
            "workspace_mode": "clone",
        }),
    )?;
    let project_id = project["id"].as_str().context("the project has no id")?;

    let title: String = spec
        .instruction
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(80)
        .collect();
    let item = api.post(
        &format!("/api/projects/{project_id}/items"),
        &json!({
            "title": title,
            "description": spec.instruction,
            "item_type": spec.item.item_type,
        }),
    )?;
    let item_id = item["id"].as_str().context("the item has no id")?;
    if spec.item.brief.is_object() {
        api.put(
            &format!("/api/items/{item_id}/brief"),
            &brief_body(&spec.item.brief),
        )?;
    }

    let role = spec.profile.role.as_deref().unwrap_or("Implementer");
    let kind = profile_kind(role);
    // A Planner, Researcher or Reviewer is the built-in one, with the instructions (and the plan
    // format) the product ships; an Implementer has no built-in worth measuring.
    let builtin = if kind == "implementer" {
        None
    } else {
        api.get("/api/agent-profiles")?["data"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["builtin"] == true && p["kind"] == kind)
            .and_then(|p| p["agent_profile_id"].as_str().map(str::to_owned))
    };
    let uses_builtin = builtin.is_some();
    let profile_id = match builtin {
        Some(id) => id,
        None => api.post(
            "/api/agent-profiles",
            &json!({
                "name": format!("eval {}", spec.id),
                "instructions": format!("You are the {role}. Do the item you are given."),
                "kind": kind,
            }),
        )?["agent_profile_id"]
            .as_str()
            .context("the profile has no id")?
            .to_owned(),
    };
    let profile_id = profile_id.as_str();

    // The built-in profile brings its own tools, which a wildcard would override.
    let tools: Vec<&str> = if spec.profile.tools.is_empty() {
        if uses_builtin { Vec::new() } else { vec!["*"] }
    } else {
        spec.profile.tools.iter().map(String::as_str).collect()
    };
    let mut budgets = json!({});
    if let Some(tokens) = spec.limits.tokens {
        budgets = json!({"tokens": tokens});
    }
    let started = api.post(
        "/api/executions",
        &json!({
            "item_id": item_id,
            "idempotency_key": format!(
                "evals-{}-{}-{}-{}",
                spec.id,
                spec.version,
                options.harness_for(task),
                options.epoch
            ),
            "agent_profile_id": profile_id,
            "requested_harness_kind": options.harness_for(task),
            "requested_model": spec.profile.model,
            "permission_policy": {
                "tools": tools,
                "network": spec.profile.web_tools,
                "approvals": if spec.approvals == Approvals::Ask { "ask" } else { "auto" },
            },
            "budgets": budgets,
            "environment": {},
            "metadata": {},
            "timeout_seconds": spec.limits.wall_s,
        }),
    )?;
    started["request_id"]
        .as_str()
        .map(str::to_owned)
        .context("the run has no request id")
}

/// A task writes its acceptance and constraints as plain sentences; the board stores typed
/// criteria. A sentence becomes a manual check or a note, and an entry that is already typed
/// passes through unchanged.
fn brief_body(brief: &Value) -> Value {
    let mut body = brief.clone();
    if let Some(list) = body.get_mut("acceptance").and_then(Value::as_array_mut) {
        for (index, entry) in list.iter_mut().enumerate() {
            if let Some(text) = entry.as_str() {
                *entry = json!({
                    "kind": "manual",
                    "id": format!("a{}", index + 1),
                    "title": text,
                    "text": text,
                });
            }
        }
    }
    if let Some(list) = body.get_mut("constraints").and_then(Value::as_array_mut) {
        for entry in list.iter_mut() {
            if let Some(text) = entry.as_str() {
                *entry = json!({ "kind": "note", "text": text });
            }
        }
    }
    for key in ["acceptance", "constraints"] {
        if body.get(key).is_none() {
            body[key] = json!([]);
        }
    }
    body
}

fn profile_kind(role: &str) -> &'static str {
    match role.to_ascii_lowercase().as_str() {
        "reviewer" => "reviewer",
        "researcher" => "researcher",
        "planner" => "planner",
        _ => "implementer",
    }
}

/// Waits for the run to end, answering decisions; true when the wall limit passed first.
fn wait_for_end(
    api: &Api,
    request_id: &str,
    operator: &mut Operator,
    task: &LoadedTask,
) -> Result<bool> {
    let deadline = Instant::now() + Duration::from_secs(task.task.limits.wall_s);
    loop {
        let state = run_state(api, request_id)?;
        if ENDED.contains(&state.as_str()) {
            return Ok(false);
        }
        if state == "waiting_decision" {
            operator.answer_pending(api, request_id)?;
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(POLL);
    }
    // Asking is polite; the trial is frozen whether or not the run listens.
    let _ = api.post(&format!("/api/executions/{request_id}/cancel"), &json!({}));
    let grace = Instant::now() + CANCEL_GRACE;
    while Instant::now() < grace && !ENDED.contains(&run_state(api, request_id)?.as_str()) {
        std::thread::sleep(POLL);
    }
    Ok(true)
}

fn run_state(api: &Api, request_id: &str) -> Result<String> {
    let run = api.get(&format!("/api/executions/{request_id}"))?;
    run["state"]
        .as_str()
        .map(str::to_owned)
        .context("the run has no state")
}

/// What `freeze` hands back to the record.
struct Frozen {
    dir_relative: String,
    sandbox: Option<String>,
    counted_tokens: Option<u64>,
    events: usize,
    /// The newest attempt's `changes.patch`.
    patch: Option<Vec<u8>>,
}

/// Copies the run, its attempts, decisions, events and artifacts out of the server, which goes
/// away with the world.
fn freeze(
    api: &Api,
    request_id: &str,
    task: &LoadedTask,
    options: &TrialOptions,
    server: &crate::world::Server,
) -> Result<Frozen> {
    let relative = format!(
        "trials/{}@{}/{}/{}",
        task.task.id,
        task.task.version,
        options.harness_for(task),
        options.epoch
    );
    let dir = options.out.join(&relative);
    std::fs::create_dir_all(&dir)?;
    let save = |name: &str, value: &Value| -> Result<()> {
        std::fs::write(dir.join(name), serde_json::to_vec_pretty(value)?)
            .with_context(|| format!("write {name}"))
    };

    save(
        "execution.json",
        &api.get(&format!("/api/executions/{request_id}"))?,
    )?;
    let attempts = api.get(&format!("/api/executions/{request_id}/attempts"))?;
    save("attempts.json", &attempts)?;

    let mut frozen = Frozen {
        dir_relative: relative,
        sandbox: None,
        counted_tokens: None,
        events: 0,
        patch: None,
    };
    for attempt in attempts["data"].as_array().into_iter().flatten() {
        let Some(number) = attempt["attempt_number"].as_u64() else {
            continue;
        };
        let base = format!("/api/executions/{request_id}/attempts/{number}");
        save(
            &format!("decisions-{number}.json"),
            &api.get(&format!("{base}/decisions"))?,
        )?;
        let events = api.get(&format!("{base}/events"))?;
        frozen.events += events["data"].as_array().map_or(0, Vec::len);
        save(&format!("events-{number}.json"), &events)?;

        // The newest attempt is the one the run ended with.
        frozen.sandbox = attempt["sandbox"].as_str().map(str::to_owned);
        frozen.counted_tokens = attempt["counted_tokens"].as_u64();
        frozen.patch = None;

        let artifacts = api.get(&format!("{base}/artifacts"))?;
        let folder = dir.join("artifacts").join(format!("attempt-{number}"));
        std::fs::create_dir_all(&folder)?;
        for artifact in artifacts["data"].as_array().into_iter().flatten() {
            let (Some(id), Some(name)) =
                (artifact["artifact_id"].as_str(), artifact["name"].as_str())
            else {
                continue;
            };
            let bytes = api.bytes(&format!("{base}/artifacts/{id}/content"))?;
            let safe = Path::new(name)
                .file_name()
                .context("an artifact without a name")?;
            std::fs::write(folder.join(safe), &bytes)?;
            if artifact["kind"] == "patch" {
                frozen.patch = Some(bytes);
            }
        }
    }
    let log = server.log.lock().expect("log lock").join("\n");
    std::fs::write(dir.join("server.log"), log)?;
    Ok(frozen)
}

#[cfg(test)]
mod brief_tests {
    use super::*;

    #[test]
    fn plain_sentences_become_typed_entries() {
        let body = brief_body(&json!({
            "acceptance": ["tests pass"],
            "constraints": ["do not edit tests"],
        }));
        assert_eq!(body["acceptance"][0]["kind"], "manual");
        assert_eq!(body["acceptance"][0]["text"], "tests pass");
        assert_eq!(
            body["constraints"][0],
            json!({"kind": "note", "text": "do not edit tests"})
        );
    }

    #[test]
    fn a_brief_without_acceptance_still_has_both_lists() {
        let body = brief_body(&json!({ "constraints": ["only one file"] }));
        assert_eq!(body["acceptance"], json!([]));
    }

    #[test]
    fn typed_entries_pass_through() {
        let typed = json!({"kind": "file", "id": "f", "title": "t", "path": "x"});
        let body = brief_body(&json!({ "acceptance": [typed.clone()], "constraints": [] }));
        assert_eq!(body["acceptance"][0], typed);
    }
}
