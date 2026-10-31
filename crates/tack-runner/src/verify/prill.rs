//! The seam between the runner and prill, the verifier the `[verify]` table
//! names by default.
//!
//! prill's command line is `prill verify --repo DIR --evidence FILE [--brief
//! FILE] --out DIR --json [--image REF | --no-sandbox]`. It reads commits
//! only, never a work tree; it takes an `attempt-evidence/v0` document and
//! a `briefing/v1` document (both camelCase); and the pack it prints
//! (`mrp/v1`, camelCase, decisions `auto-merge-eligible` / `human-review` /
//! `reject`) is not the pack Tack reads (`docs/contracts/mrp-v1`,
//! snake_case, decisions `merge` / `review` / `reject`). Its exit code is 0
//! for a pack whose decision is not `reject`, 1 for a `reject` pack, and 2
//! when it could emit no pack, with an `error/v1` document on stdout.
//!
//! [`run`] therefore does, in order:
//!
//! 1. [`snapshot`]: commits what the harness left in the workspace, through
//!    a temporary index so the harness's own index and `HEAD` are untouched.
//!    The runner's markers (`.tack-runner/`, `.tack-attempt`,
//!    `.tack-checkout`) are kept out of that commit by pathspec.
//! 2. Writes `<evidence>/prill/attempt-evidence.json` from the staged
//!    `evidence.json` ([`attempt_evidence`]) and, when `brief.json` was
//!    staged, `<evidence>/prill/briefing.json` ([`briefing`]).
//! 3. Runs `<program> verify …` with the configured `args` last, so an
//!    `--image` or `--no-sandbox` reaches prill.
//! 4. Exit 0 or 1: the pack is translated ([`pack`]) and written where the
//!    caller stages it from. Exit 2: the failure carries prill's error code
//!    and message.
//!
//! prill's own `mrp.json` and `mrp.md` stay in `<evidence>/prill/`, beside
//! the documents it was given. The pack as prill printed it is also uploaded,
//! unchanged, as `mrp.prill.json` ([`ORIGINAL_FILE`]) next to the translated
//! `mrp.json`: the board hands that one, never the translation, to
//! `prill record mrp`.

use std::{fs, path::Path, process::Stdio, time::Duration};

use serde_json::{Value, json};
use tack_protocol::mrp::{
    Criterion, CriterionStatus, JudgeRubricEntry, JudgeSection, JudgeVerdict, MergeDecision,
    MergeReadinessPack, MutationSection, NextStep, ProducedBySection, RecommendationSection,
    RiskSection, RiskTier, SignatureSection, StageRun, UsageSection, VerifySection,
};
use tokio::process::Command;

use super::{VerifyFailure, failure, io_failure, json_failure, spawn_verifier, stderr_prefix};
use crate::{
    client::workspace::Workspace,
    config::VerifyConfig,
    evidence::AttemptEvidence,
    harness::{
        process::{ProcessExit, ProcessLimits},
        sha256::sha256_hex,
    },
};

/// The workspace paths that are the runner's, never the harness's work, and
/// so never enter the snapshot commit. `.tack-runner` names a directory;
/// a pathspec matches everything under it.
const RUNNER_MARKERS: [&str; 3] = [".tack-runner", ".tack-attempt", ".tack-checkout"];

/// How long one git invocation of the snapshot may take.
const GIT_TIMEOUT: Duration = Duration::from_secs(600);

/// The in-toto envelope prill writes beside its pack when it signs, and the
/// name it is staged under.
pub(super) const ENVELOPE_FILE: &str = "mrp.intoto.json";

/// The name prill's own pack is staged under, byte for byte as prill printed
/// it, beside the translated `mrp.json`. prill's `record mrp` takes only this
/// one: the translation is Tack's view of it, derived for display.
pub(super) const ORIGINAL_FILE: &str = "mrp.prill.json";

/// The media type of the staged pack as prill produced it.
pub(super) const ORIGINAL_MEDIA_TYPE: &str = "application/vnd.prill.mrp+json";

/// The name under `<evidence>` holding prill's documents and its own pack.
const PRILL_DIR: &str = "prill";

/// `kind = "prill"`: the whole sequence, from snapshot to the translated
/// pack at `output`.
pub(super) async fn run(
    config: &VerifyConfig,
    limits: &ProcessLimits,
    workspace: &Workspace,
    evidence: &Path,
    output: &Path,
    env: &crate::config::ProcessEnv,
    trajectory_uri: Option<&str>,
) -> Result<(), VerifyFailure> {
    let evidence_bytes = fs::read(evidence.join("evidence.json"))
        .map_err(|error| io_failure(None, "the staged evidence.json could not be read", &error))?;
    let tack_evidence: AttemptEvidence = serde_json::from_slice(&evidence_bytes)
        .map_err(|error| json_failure(None, "the staged evidence.json does not parse", &error))?;
    if !tack_evidence.captured {
        return Err(failure(
            None,
            format!(
                "the runner captured no evidence: {}",
                tack_evidence
                    .reason
                    .as_deref()
                    .unwrap_or("no reason recorded")
            ),
        ));
    }

    let out_dir = evidence.join(PRILL_DIR);
    fs::create_dir_all(&out_dir).map_err(|error| {
        io_failure(
            None,
            "prill's output directory could not be created",
            &error,
        )
    })?;

    // The commit the capture made of the work is the one verified. Only a
    // workspace that was not captured by the runner (no such file) is
    // snapshotted here.
    let head_commit = match fs::read_to_string(
        evidence
            .parent()
            .unwrap_or(evidence)
            .join(crate::evidence::CAPTURED_COMMIT_FILE),
    ) {
        Ok(recorded) => recorded.lines().next().unwrap_or_default().to_owned(),
        Err(_) => snapshot(
            Path::new("git"),
            &workspace.path,
            &out_dir.join("index"),
            workspace.attempt_id.as_str(),
        )
        .await
        .map_err(|reason| {
            failure(
                None,
                format!("the workspace could not be snapshotted: {reason}"),
            )
        })?,
    };

    // The run's steps, as the one artifact the engine staged and will upload: prill is told
    // where the board serves it and the digest of exactly these bytes.
    let trajectory = trajectory_uri.and_then(|uri| {
        fs::read(evidence.join(crate::evidence::TRAJECTORY_FILE))
            .ok()
            .map(|bytes| (uri, sha256_hex(&bytes)))
    });
    let evidence_document = out_dir.join("attempt-evidence.json");
    write_document(
        &evidence_document,
        &attempt_evidence(&tack_evidence, &head_commit, trajectory),
    )?;

    let mut args = vec![
        "verify".to_owned(),
        "--repo".to_owned(),
        workspace.path.display().to_string(),
        "--evidence".to_owned(),
        evidence_document.display().to_string(),
    ];
    let brief_path = evidence.join("brief.json");
    let brief_sha256 = match fs::read(&brief_path) {
        Ok(bytes) => {
            let brief: Value = serde_json::from_slice(&bytes).map_err(|error| {
                json_failure(None, "the staged brief.json does not parse", &error)
            })?;
            let brief_document = out_dir.join("briefing.json");
            write_document(&brief_document, &briefing(&brief))?;
            args.extend(["--brief".to_owned(), brief_document.display().to_string()]);
            Some(sha256_hex(&bytes))
        }
        Err(_) => None,
    };
    args.extend([
        "--out".to_owned(),
        out_dir.display().to_string(),
        "--json".to_owned(),
    ]);
    if let Some(key) = &config.sign_key {
        args.extend(["--sign".to_owned(), key.clone()]);
    }
    // prill reads its configuration from the commit `baseCommit` names in the
    // evidence document, which is the id fixed at provisioning, so a
    // `prill.yaml` the agent edited is never the one it applies.
    args.extend(config.args.iter().cloned());

    let result = spawn_verifier(config, limits, workspace, args, env).await?;
    let stderr = stderr_prefix(&result);
    let exit_code = match result.exit {
        ProcessExit::Exited(code @ (0 | 1)) => code,
        ProcessExit::Exited(2) => {
            let refusal = serde_json::from_str::<Value>(&result.stdout.text).ok();
            let (code, message) = refusal
                .as_ref()
                .map(|error| (text(&error["code"]), text(&error["message"])))
                .unwrap_or_default();
            return Err(failure(
                Some(2),
                if code.is_empty() {
                    format!("prill exited with code 2 without a pack: {stderr}")
                } else {
                    format!("prill refused: {code}: {message}")
                },
            ));
        }
        ProcessExit::Exited(code) => {
            return Err(failure(
                Some(code),
                format!("prill exited with code {code}: {stderr}"),
            ));
        }
        #[cfg(unix)]
        ProcessExit::Signaled(signal) => {
            return Err(failure(
                None,
                format!("prill was stopped by signal {signal}: {stderr}"),
            ));
        }
        ProcessExit::TimedOut => {
            return Err(failure(
                None,
                format!(
                    "prill did not finish within {} seconds",
                    config.timeout_seconds
                ),
            ));
        }
    };

    // Stdout is the pack, but the capture is bounded; the copy prill wrote
    // beside its Markdown is whole whatever the limit.
    let printed = if result.stdout.truncated {
        fs::read(out_dir.join("mrp.json")).unwrap_or_default()
    } else {
        result.stdout.text.into_bytes()
    };
    let prill_pack: Value = serde_json::from_slice(&printed).map_err(|error| {
        json_failure(
            Some(exit_code),
            format!("prill exited with a pack status but printed none: {stderr}"),
            &error,
        )
    })?;
    fs::write(evidence.join(ORIGINAL_FILE), &printed).map_err(|error| {
        io_failure(
            Some(exit_code),
            "prill's own pack could not be written for the upload",
            &error,
        )
    })?;
    let mut translated = pack(
        &prill_pack,
        &tack_evidence,
        &sha256_hex(&evidence_bytes),
        brief_sha256,
    );
    if config.sign_key.is_some() {
        let envelope = fs::read(out_dir.join(ENVELOPE_FILE)).map_err(|error| {
            io_failure(
                Some(exit_code),
                "prill was asked to sign but wrote no mrp.intoto.json",
                &error,
            )
        })?;
        translated.signature = SignatureSection::InToto {
            statement_sha256: sha256_hex(&envelope),
        };
        fs::write(evidence.join(ENVELOPE_FILE), envelope).map_err(|error| {
            io_failure(
                Some(exit_code),
                "the attestation could not be written",
                &error,
            )
        })?;
    }
    let mut bytes = serde_json::to_vec_pretty(&translated).map_err(|error| {
        json_failure(
            Some(exit_code),
            "the translated pack could not be serialized",
            &error,
        )
    })?;
    bytes.push(b'\n');
    fs::write(output, bytes).map_err(|error| {
        io_failure(
            Some(exit_code),
            "the translated pack could not be written",
            &error,
        )
    })
}

fn write_document(path: &Path, document: &Value) -> Result<(), VerifyFailure> {
    let mut bytes = serde_json::to_vec_pretty(document)
        .map_err(|error| json_failure(None, "a prill document could not be serialized", &error))?;
    bytes.push(b'\n');
    fs::write(path, bytes).map_err(|error| {
        io_failure(
            None,
            format!("{} could not be written", path.display()),
            &error,
        )
    })
}

// ---------------------------------------------------------------------
// Step 1: the snapshot.
// ---------------------------------------------------------------------

/// Commits the work tree of `workspace` and returns the commit, or `HEAD`
/// itself when the tree is unchanged. Staging happens in `index`, a file
/// that does not exist yet and is removed afterwards, so the harness's own
/// index is never read or written; `HEAD` is not moved and no ref is
/// written, so the commit is reachable by hash only, which is all prill
/// needs. Ignored files stay out, as they would of any commit.
pub(super) async fn snapshot(
    git: &Path,
    workspace: &Path,
    index: &Path,
    attempt_id: &str,
) -> Result<String, String> {
    let _ = fs::remove_file(index);
    let head = git_output(
        git,
        workspace,
        None,
        &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"],
    )
    .await
    .map_err(|reason| format!("HEAD is not a commit ({reason})"))?;
    let indexed = Some(index);
    git_output(git, workspace, indexed, &["read-tree", "HEAD"]).await?;
    let mut add = vec!["add", "-A", "--", "."];
    let excludes: Vec<String> = RUNNER_MARKERS
        .iter()
        .map(|marker| format!(":(exclude){marker}"))
        .collect();
    add.extend(excludes.iter().map(String::as_str));
    git_output(git, workspace, indexed, &add).await?;
    let tree = git_output(git, workspace, indexed, &["write-tree"]).await?;
    let _ = fs::remove_file(index);
    let head_tree = git_output(git, workspace, None, &["rev-parse", "HEAD^{tree}"]).await?;
    if tree == head_tree {
        return Ok(head);
    }
    let message = format!("tack: snapshot of attempt {attempt_id}");
    git_output(
        git,
        workspace,
        None,
        &["commit-tree", &tree, "-p", &head, "-m", &message],
    )
    .await
}

/// One bounded git invocation in `directory`: its trimmed stdout, or its
/// trimmed stderr as the error. The repository-selecting variables are
/// dropped for the reason `workspace::git` drops them — a runner started
/// inside a repository must not operate on that one — and the identity is
/// fixed so `commit-tree` never depends on the operator's `~/.gitconfig`.
async fn git_output(
    git: &Path,
    directory: &Path,
    index: Option<&Path>,
    args: &[&str],
) -> Result<String, String> {
    let mut command = Command::new(git);
    command
        .current_dir(directory)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "Tack Runner")
        .env("GIT_AUTHOR_EMAIL", "tack-runner@localhost")
        .env("GIT_COMMITTER_NAME", "Tack Runner")
        .env("GIT_COMMITTER_EMAIL", "tack-runner@localhost")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_CEILING_DIRECTORIES")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let child = command.spawn().map_err(|error| {
        format!(
            "git could not be started for `git {}` ({})",
            args[0],
            error.kind()
        )
    })?;
    let output = match tokio::time::timeout(GIT_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return Err(format!(
                "`git {}` could not be supervised ({})",
                args[0],
                error.kind()
            ));
        }
        Err(_) => return Err(format!("`git {}` did not finish in time", args[0])),
    };
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if output.status.success() {
        Ok(stdout)
    } else {
        let stderr: String = String::from_utf8_lossy(&output.stderr)
            .trim()
            .chars()
            .take(super::STDERR_PREFIX_CHARS)
            .collect();
        Err(format!("`git {}` failed: {stderr}", args[0]))
    }
}

// ---------------------------------------------------------------------
// Step 2: Tack's documents become prill's.
// ---------------------------------------------------------------------

/// A Tack `Measurement` carries its value only when it was measured.
fn measured(value: &Value) -> Option<&Value> {
    (value["source"] == "measured").then(|| &value["value"])
}

fn text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn non_empty(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| !text.is_empty())
}

/// `evidence.json` (evidence-v1) as an `attempt-evidence/v0` document whose
/// head is the snapshot commit.
pub(super) fn attempt_evidence(
    tack: &AttemptEvidence,
    head_commit: &str,
    trajectory: Option<(&str, String)>,
) -> Value {
    let usage = &tack.usage;
    let mut document = json!({
        "contract": "attempt-evidence/v0",
        "attemptId": tack.attempt_id,
        "harness": {
            "name": if tack.harness_kind.is_empty() { "tack-runner" } else { tack.harness_kind.as_str() },
        },
        "baseCommit": tack.base_commit,
        "headCommit": head_commit,
    });
    if let Some(version) = non_empty(&usage["harness_version"]) {
        document["harness"]["version"] = json!(version);
    }
    if let Some(model) = non_empty(&usage["model"]) {
        document["generator"] = json!({ "model": model });
    }
    // The family of the model the last implementer step ran on, when the harness recorded the
    // prompts of its steps. It is declared by the harness and never inferred from the model.
    let steps = tack.terminal_reason["steps"].as_array();
    if let Some(family) = steps
        .into_iter()
        .flatten()
        .rev()
        .filter(|step| step["role"] == "implementer")
        .find_map(|step| non_empty(&step["family"]))
    {
        document["generator"]["family"] = json!(family);
    }
    if let Some((uri, sha256)) = trajectory {
        document["trajectory"] = json!({ "uri": uri, "digest": format!("sha256:{sha256}") });
    }
    let tokens_in = measured(&usage["tokens_in"]).and_then(Value::as_u64);
    let tokens_out = measured(&usage["tokens_out"]).and_then(Value::as_u64);
    if let (Some(tokens_in), Some(tokens_out)) = (tokens_in, tokens_out) {
        let mut prill_usage = json!({ "inputTokens": tokens_in, "outputTokens": tokens_out });
        if let Some(duration_ms) = measured(&usage["duration_ms"]).and_then(Value::as_u64) {
            prill_usage["durationS"] = json!(duration_ms as f64 / 1000.0);
        }
        document["usage"] = prill_usage;
    }
    document
}

/// `brief.json` (brief-v1) as a `briefing/v1` document. A metric criterion
/// without both a command and a pattern, a `max_changed_files` constraint and
/// a `cwd` or `runner` prill has no field for are named in `riskHints` rather
/// than dropped silently.
pub fn briefing(brief: &Value) -> Value {
    let mut acceptance = Vec::new();
    let mut hints = Vec::new();
    for criterion in brief["acceptance"].as_array().into_iter().flatten() {
        let id = text(&criterion["id"]);
        let title = non_empty(&criterion["title"]).unwrap_or(&id).to_owned();
        match criterion["kind"].as_str() {
            Some("command") => {
                if let Some(cwd) = non_empty(&criterion["cwd"]) {
                    hints.push(format!(
                        "command criterion {id} ({title}) ran in `{cwd}`, which prill does not carry"
                    ));
                }
                acceptance.push(json!({
                    "id": id,
                    "description": title,
                    "type": "command",
                    "command": criterion["run"],
                    "expectExitCode": criterion["expect_exit"].as_i64().unwrap_or(0),
                }));
            }
            Some("test") => {
                if let Some(runner) = non_empty(&criterion["runner"]) {
                    hints.push(format!(
                        "test criterion {id} ({title}) names the runner `{runner}`, which prill does not carry"
                    ));
                }
                acceptance.push(json!({
                    "id": id,
                    "description": title,
                    "type": "test",
                    "name": criterion["name"],
                }));
            }
            Some(kind @ ("file" | "absent")) => acceptance.push(json!({
                "id": id,
                "description": title,
                "type": kind,
                "path": criterion["path"],
            })),
            Some("manual") => acceptance.push(json!({
                "id": id,
                "type": "manual",
                "question": title,
                "rubric": text(&criterion["text"]),
            })),
            Some("metric") => {
                let op = match criterion["op"].as_str() {
                    Some("gte") => ">=",
                    Some("lte") => "<=",
                    _ => "==",
                };
                match (non_empty(&criterion["command"]), non_empty(&criterion["pattern"])) {
                    (Some(command), Some(pattern)) => acceptance.push(json!({
                        "id": id,
                        "description": title,
                        "type": "metric",
                        "command": command,
                        "pattern": pattern,
                        "op": op,
                        "threshold": criterion["threshold"],
                    })),
                    _ => hints.push(format!(
                        "metric criterion {id} ({title}) has no command to measure it and was not checked"
                    )),
                }
            }
            _ => {}
        }
    }
    let mut forbidden = Vec::new();
    let mut allowed = Vec::new();
    for constraint in brief["constraints"].as_array().into_iter().flatten() {
        match constraint["kind"].as_str() {
            Some("forbidden_path") => forbidden.push(constraint["glob"].clone()),
            Some("allowed_dependency") => allowed.push(constraint["name"].clone()),
            Some("max_changed_files") => hints.push(format!(
                "at most {} changed files (not enforced by prill)",
                text(&constraint["n"])
            )),
            Some("note") => hints.push(text(&constraint["text"])),
            _ => {}
        }
    }
    let mut constraints = Vec::new();
    if !forbidden.is_empty() {
        constraints.push(json!({ "type": "forbidden_paths", "globs": forbidden }));
    }
    if !allowed.is_empty() {
        constraints.push(json!({ "type": "allowed_dependencies", "names": allowed }));
    }
    let item_id = text(&brief["item_id"]);
    let objective = non_empty(&brief["definition_of_done"])
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Item {item_id}").trim().to_owned());
    let mut document = json!({
        "contract": "briefing/v1",
        "id": if item_id.is_empty() { "tack-item".to_owned() } else { item_id },
        "objective": objective,
        "acceptance": acceptance,
    });
    if !constraints.is_empty() {
        document["constraints"] = Value::Array(constraints);
    }
    if !hints.is_empty() {
        document["riskHints"] = json!(hints);
    }
    document
}

// ---------------------------------------------------------------------
// Step 4: prill's pack becomes Tack's.
// ---------------------------------------------------------------------

/// A prill `mrp/v1` pack as an `mrp-v1` pack. `evidence_sha256` and
/// `brief_sha256` are the digests of the staged `evidence.json` and
/// `brief.json`, the documents the runner uploads beside the pack.
pub(super) fn pack(
    prill: &Value,
    tack: &AttemptEvidence,
    evidence_sha256: &str,
    brief_sha256: Option<String>,
) -> MergeReadinessPack {
    let decision = text(&prill["recommendation"]["decision"]);
    let reasons: Vec<String> = prill["recommendation"]["reasons"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|reason| {
            format!(
                "{}: {} — {}",
                text(&reason["severity"]),
                text(&reason["code"]),
                text(&reason["detail"])
            )
        })
        .collect();
    let flags = prill["flags"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|flag| format!("flag: {} — {}", text(&flag["code"]), text(&flag["detail"])));
    let risk = prill["risk"].as_object();
    let signals = risk
        .and_then(|risk| risk.get("signals"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|signal| {
            format!(
                "{}: {} {}",
                text(&signal["level"]),
                text(&signal["rule"]),
                text(&signal["detail"])
            )
            .trim()
            .to_owned()
        });

    let criteria = prill["criteria"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|criterion| Criterion {
            id: text(&criterion["id"]),
            kind: text(&criterion["type"]),
            status: match criterion["status"].as_str() {
                Some("pass") => CriterionStatus::Passed,
                Some("fail") => CriterionStatus::Failed,
                Some("manual") => CriterionStatus::Manual,
                _ => CriterionStatus::Skipped,
            },
            evidence_ref: None,
            note: non_empty(&criterion["reason"])
                .map(str::to_owned)
                .or_else(|| command_note(&criterion["evidence"])),
        })
        .collect();

    let verify = prill["suite"]["command"].as_object().map(|command| {
        let totals = &prill["suite"]["totals"];
        let mut tail = text(&command.get("outputTail").cloned().unwrap_or_default());
        if totals.is_object() {
            tail = format!(
                "{tail}\n[prill] {} passed, {} failed, {} errors, {} skipped",
                totals["passed"].as_u64().unwrap_or(0),
                totals["failed"].as_u64().unwrap_or(0),
                totals["errors"].as_u64().unwrap_or(0),
                totals["skipped"].as_u64().unwrap_or(0),
            )
            .trim()
            .to_owned();
        }
        VerifySection {
            command: command
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            exit_code: command
                .get("exitCode")
                .and_then(Value::as_i64)
                .map_or(-1, |code| code as i32),
            output_tail: (!tail.is_empty()).then_some(tail),
            duration_s: command.get("durationS").and_then(Value::as_f64),
        }
    });

    let mutation = prill["mutation"]["totals"]
        .as_object()
        .map(|totals| MutationSection {
            scope: "changed_files".to_owned(),
            score: prill["mutation"]["score"].as_f64().unwrap_or(0.0),
            killed: totals.get("killed").and_then(Value::as_u64).unwrap_or(0) as u32,
            survived: totals.get("survived").and_then(Value::as_u64).unwrap_or(0) as u32,
        });

    let judge = prill["judge"].as_object().map(|judge| JudgeSection {
        model_family: judge
            .get("family")
            .and_then(Value::as_str)
            .filter(|family| !family.is_empty())
            .unwrap_or("unknown")
            .to_owned(),
        blind: true,
        rubric: judge
            .get("verdicts")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|verdict| {
                let answer = text(&verdict["answer"]);
                let passed = answer == "yes";
                JudgeRubricEntry {
                    criterion_id: text(&verdict["id"]),
                    verdict: if passed {
                        JudgeVerdict::Pass
                    } else {
                        JudgeVerdict::Fail
                    },
                    reason: non_empty(&verdict["rationale"])
                        .map(str::to_owned)
                        .or_else(|| (!passed).then(|| format!("answer: {answer}"))),
                }
            })
            .collect(),
    });

    let (tokens_in, tokens_out) = prill["stages"].as_array().into_iter().flatten().fold(
        (0, 0),
        |(tokens_in, tokens_out), stage| {
            let tokens = &stage["tokens"];
            (
                tokens_in + tokens["input"].as_u64().unwrap_or(0),
                tokens_out + tokens["output"].as_u64().unwrap_or(0),
            )
        },
    );

    let merge_decision = match decision.as_str() {
        "auto-merge-eligible" => MergeDecision::Merge,
        "human-review" => MergeDecision::Review,
        _ => MergeDecision::Reject,
    };
    let tier = match risk
        .and_then(|risk| risk.get("level"))
        .and_then(Value::as_str)
    {
        Some("low") => RiskTier::Low,
        Some("medium") => RiskTier::Medium,
        Some("high") => RiskTier::High,
        _ => match merge_decision {
            MergeDecision::Merge => RiskTier::Low,
            MergeDecision::Review => RiskTier::Medium,
            MergeDecision::Reject => RiskTier::High,
        },
    };
    let mut rationale = format!("prill: {decision}");
    if !reasons.is_empty() {
        rationale.push_str(" — ");
        rationale.push_str(&reasons.join("; "));
    }
    let checked = tack
        .files
        .iter()
        .filter(|file| crate::evidence::checked_files::matches(&file.path))
        .map(|file| format!("{}: {}", crate::evidence::checked_files::SECTION, file.path));
    let risk_reasons = reasons
        .iter()
        .cloned()
        .chain(flags)
        .chain(signals)
        .chain(checked)
        .collect();

    MergeReadinessPack {
        v: "1".to_owned(),
        attempt_id: tack.attempt_id.clone(),
        evidence_sha256: evidence_sha256.to_owned(),
        brief_sha256,
        criteria,
        verify,
        mutation,
        // prill reports static analysis per tool and per tree, not as one
        // SARIF digest with counts; nothing honest fits Tack's section.
        static_analysis: None,
        judge,
        risk: RiskSection {
            tier,
            reasons: risk_reasons,
        },
        recommendation: RecommendationSection {
            decision: merge_decision,
            rationale,
        },
        usage: UsageSection {
            tokens_in,
            tokens_out,
        },
        signature: SignatureSection::None,
        produced_by: ProducedBySection {
            program: "prill".to_owned(),
            version: non_empty(&prill["prill"]["version"])
                .unwrap_or("unknown")
                .to_owned(),
        },
        next_steps: next_steps(&prill["nextSteps"]),
        stages: prill["stages"].as_array().map(|stages| {
            stages
                .iter()
                .map(|stage| StageRun {
                    name: text(&stage["name"]),
                    status: text(&stage["status"]),
                    reason: non_empty(&stage["reason"]).map(str::to_owned),
                })
                .collect()
        }),
    }
}

/// prill's `nextSteps`, `None` when it gave none.
fn next_steps(steps: &Value) -> Option<Vec<NextStep>> {
    let owned = |value: &Value| non_empty(value).map(str::to_owned);
    let steps: Vec<NextStep> = steps
        .as_array()
        .into_iter()
        .flatten()
        .map(|step| NextStep {
            code: text(&step["code"]),
            severity: text(&step["severity"]),
            stage: owned(&step["stage"]),
            criterion_id: owned(&step["criterionId"]),
            path: owned(&step["path"]),
            line: step["line"].as_u64().map(|line| line as u32),
            command: owned(&step["command"]),
            message: owned(&step["message"]),
        })
        .collect();
    (!steps.is_empty()).then_some(steps)
}

/// What a criterion's command or file evidence says when prill gave no
/// reason of its own.
fn command_note(evidence: &Value) -> Option<String> {
    match evidence["kind"].as_str()? {
        "command" => Some(if evidence["timedOut"] == true {
            "timed out".to_owned()
        } else {
            format!(
                "exit {}, expected {}",
                text(&evidence["exitCode"]),
                evidence["expectedExitCode"].as_i64().unwrap_or(0)
            )
        }),
        "file" => Some(if evidence["exists"] == true {
            "exists".to_owned()
        } else {
            "missing".to_owned()
        }),
        _ => None,
    }
}

#[cfg(test)]
#[path = "prill/tests.rs"]
mod tests;
