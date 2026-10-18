//! Claude Code as a harness: its command line, how its `stream-json`
//! transcript is read, and what it supports. The lifecycle is
//! [`crate::harness::local_process`].
//!
//! Vendor findings — what is measured, what is a documented guess, and at
//! which version — are in `fixtures/claude_code/README.md`.

use std::collections::BTreeMap;

use serde_json::Value;
use tack_orch::execution::{
    Approvals, CapabilitySupport, FeatureCapabilities, PermissionPolicy, Usage,
};

use crate::harness::{
    DecisionAnswer, DecisionOption, HarnessError, Question, StreamSignal,
    local_process::{
        HarnessDescriptor, HarnessGrammar, Invocation, LocalProcessHarness, ModelSelection,
        ModelUsage, RunContext, RunReport, UsageDetail, call_usage, capability, policy_capability,
    },
    process::{ProcessExit, ProcessResult},
};

mod readiness;

pub static DESCRIPTOR: HarnessDescriptor = HarnessDescriptor {
    kind: "claude-code",
    program: "claude",
    model_selection: ModelSelection::Optional,
    // `claude` finds its login session under `HOME`, and its Bash tool runs
    // a real shell that needs `PATH`. Nothing else is inherited.
    inherited_env: &["HOME", "PATH"],
    probe_notes: &[(
        "model_discovery_note",
        "Claude Code's CLI has no list-models command; model availability is only observable \
         via a live, billed invocation, so this probe lists none.",
    )],
    credential_note: "Claude Code signs in with its own login; Tack holds no model credential.",
    reports_process_groups: false,
    cancels_over_api: false,
    reports_progress: true,
    sandbox_binds: &[".claude"],
    readiness: Some(readiness::check),
    models_docs: "https://code.claude.com/docs/en/model-config",
    sign_in_docs: "https://code.claude.com/docs/en/authentication",
};

/// Tools that reach the network, matched case-insensitively against the run's tools.
const NETWORK_TOOLS: &[&str] = &["webfetch", "websearch"];

/// The tools this run gives Claude Code: the run's own list, else the profile's for this
/// agent (`tool_policy.tools["claude-code"]`, the shape the Run dialog and auto-start read),
/// else none. The run's list wins because the person typed it for this run.
fn tools(run: &RunContext<'_>) -> Vec<String> {
    let request = &run.spec.work.request;
    if !request.permission_policy.tools.is_empty() {
        return request.permission_policy.tools.clone();
    }
    request.resolved_agent_profile.tool_policy["tools"][DESCRIPTOR.kind]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

pub struct ClaudeCodeGrammar;

pub type ClaudeCodeAdapter<C = crate::SystemClock> = LocalProcessHarness<ClaudeCodeGrammar, C>;

fn rejected(reason: String) -> HarnessError {
    tracing::warn!(
        reason,
        harness = DESCRIPTOR.kind,
        "request rejected before spawn"
    );
    HarnessError::Rejected { reason }
}

fn bounded_prefix(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

/// Reads a terminal `{"type":"result"}` object. `is_error` alone decides
/// success: `subtype` was observed reporting `"success"` beside
/// `"is_error":true` for an invalid-model API error. A missing `is_error`
/// fails closed.
///
/// `usage.input_tokens` counts only input the cache did not serve (12 for a
/// run that read 1.08M tokens, measured on 2.1.273), so `tokens_in` adds the
/// cache reads and writes to it: the input the model actually read.
/// `modelUsage` covers every model the run used, auxiliary ones included, so
/// its totals win over `usage` when it is present.
fn report_from_result_line(
    result: &Value,
    init_model: Option<String>,
    harness_version: Option<String>,
) -> RunReport {
    let is_error = result
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let usage = result_usage(result, init_model.as_deref());
    let cached = |tokens: Option<u64>| tokens.unwrap_or(0);
    RunReport {
        succeeded: !is_error,
        terminal_reason: result.clone(),
        harness_version,
        observed_model: init_model,
        tokens_in: usage.input.map(|input| {
            input + cached(usage.detail.cache_read_tokens) + cached(usage.detail.cache_write_tokens)
        }),
        tokens_out: usage.output,
        duration_ms: result.get("duration_ms").and_then(Value::as_u64),
        cost_usd: result.get("total_cost_usd").and_then(Value::as_f64),
        usage_detail: usage.detail,
    }
}

struct ResultUsage {
    input: Option<u64>,
    output: Option<u64>,
    detail: UsageDetail,
}

fn result_usage(result: &Value, init_model: Option<&str>) -> ResultUsage {
    let model_calls = result.get("num_turns").and_then(Value::as_u64);
    if let Some(per_model) = result.get("modelUsage").and_then(Value::as_object)
        && !per_model.is_empty()
    {
        let sum = |key: &str| {
            per_model
                .values()
                .filter_map(|usage| usage.get(key).and_then(Value::as_u64))
                .reduce(|a, b| a + b)
        };
        let mut models: Vec<String> = per_model.keys().cloned().collect();
        // The session's own model first, then the rest by output size.
        models.sort_by_key(|name| {
            let output = per_model[name]
                .get("outputTokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            (Some(name.as_str()) != init_model, std::cmp::Reverse(output))
        });
        let by_model = per_model
            .iter()
            .map(|(name, usage)| {
                let field = |key: &str| usage.get(key).and_then(Value::as_u64);
                let entry = ModelUsage {
                    input: field("inputTokens").unwrap_or(0),
                    output: field("outputTokens").unwrap_or(0),
                    cache_read: field("cacheReadInputTokens"),
                    cache_write: field("cacheCreationInputTokens"),
                };
                (name.clone(), entry)
            })
            .collect();
        return ResultUsage {
            input: sum("inputTokens"),
            output: sum("outputTokens"),
            detail: UsageDetail {
                cache_read_tokens: sum("cacheReadInputTokens"),
                cache_write_tokens: sum("cacheCreationInputTokens"),
                model_calls,
                models,
                by_model,
            },
        };
    }
    let field = |key: &str| {
        result
            .pointer(&format!("/usage/{key}"))
            .and_then(Value::as_u64)
    };
    ResultUsage {
        input: field("input_tokens"),
        output: field("output_tokens"),
        detail: UsageDetail {
            cache_read_tokens: field("cache_read_input_tokens"),
            cache_write_tokens: field("cache_creation_input_tokens"),
            model_calls,
            models: init_model.map(str::to_owned).into_iter().collect(),
            by_model: BTreeMap::new(),
        },
    }
}

/// No JSON line at all: the exit status is the only signal left, and the
/// reason says the verdict was inferred rather than observed.
fn report_from_exit(result: &ProcessResult) -> RunReport {
    let (succeeded, note) = match result.exit {
        ProcessExit::Exited(0) => (
            true,
            "no structured result envelope was produced; inferred success from exit code 0",
        ),
        ProcessExit::Exited(_) => (
            false,
            "no structured result envelope was produced; inferred failure from a non-zero exit code",
        ),
        ProcessExit::TimedOut => (
            false,
            "process exceeded its timeout with no structured result envelope",
        ),
        #[cfg(unix)]
        ProcessExit::Signaled(_) => (
            false,
            "process terminated by signal with no structured result envelope",
        ),
    };
    let reason = if matches!(result.exit, ProcessExit::Exited(code) if code != 0) {
        "exit_status"
    } else {
        note
    };
    RunReport::verdict(
        succeeded,
        serde_json::json!({
            "reason": reason,
            "detail": note,
            "exit": format!("{:?}", result.exit),
            "stderr_prefix": bounded_prefix(&result.stderr.text, 500),
        }),
    )
}

/// Scans the transcript for the two lines that matter: `system`/`init`
/// (the session's model and `claude_code_version`) and the terminal
/// `result`. A line that does not parse is skipped, so one corrupted line
/// never discards an otherwise good stream. JSON lines with no `result` —
/// a truncated stream — is a failed run.
fn parse_run_output(result: &ProcessResult) -> RunReport {
    let mut init_model = None;
    let mut harness_version = None;
    let mut result_line = None;
    let mut any_json_line = false;

    for line in result.stdout.text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
            continue;
        };
        any_json_line = true;
        let field = |name: &str| value.get(name).and_then(Value::as_str);
        match field("type") {
            Some("system") if field("subtype") == Some("init") => {
                init_model = field("model").map(str::to_owned);
                harness_version = field("claude_code_version").map(str::to_owned);
            }
            Some("result") if result_line.is_none() => result_line = Some(value),
            Some("result") => tracing::debug!(harness = DESCRIPTOR.kind, "terminal_line_ignored"),
            _ => {}
        }
    }

    if let Some(result_line) = result_line {
        return report_from_result_line(&result_line, init_model, harness_version);
    }
    if !any_json_line {
        return report_from_exit(result);
    }
    RunReport::verdict(
        false,
        serde_json::json!({
            "reason": "malformed_output",
            "detail": "harness produced JSON output but no parseable terminal `result` object",
            "exit": format!("{:?}", result.exit),
            "stdout_prefix": bounded_prefix(&result.stdout.text, 500),
        }),
    )
}

impl HarnessGrammar for ClaudeCodeGrammar {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        &DESCRIPTOR
    }

    fn invocation(&self, run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
        let request = &run.spec.work.request;
        let policy = &request.permission_policy;
        let tools = tools(run);
        let all_tools = tools.iter().any(|tool| tool == PermissionPolicy::ALL_TOOLS);
        let names_network_tool = tools
            .iter()
            .any(|tool| NETWORK_TOOLS.contains(&tool.to_ascii_lowercase().as_str()));
        if !policy.network && names_network_tool {
            return Err(rejected(
                "permission_policy denies network but names a network tool \
                 (WebFetch/WebSearch), a self-contradictory request"
                    .to_owned(),
            ));
        }

        // `ask` swaps only the permission surface for the flags measured
        // against a real `claude`: `--input-format stream-json` so the
        // prompt can arrive as a control-protocol message, and
        // `--permission-mode default --permission-prompt-tool stdio` so a
        // tool call pauses for a `control_request` instead of running.
        // `auto` (absent counts as `auto`) keeps every flag exactly as
        // measured before this ever existed.
        let ask = matches!(policy.approvals, Some(Approvals::Ask));

        let mut args: Vec<String> = vec!["-p".to_owned()];
        if ask {
            args.extend(["--input-format".to_owned(), "stream-json".to_owned()]);
        }
        args.extend(
            [
                "--output-format",
                "stream-json",
                "--verbose",
                "--no-session-persistence",
            ]
            .map(str::to_owned),
        );
        if ask {
            args.extend(
                [
                    "--permission-mode",
                    "default",
                    "--permission-prompt-tool",
                    "stdio",
                ]
                .map(str::to_owned),
            );
        } else {
            args.extend(["--permission-mode", "bypassPermissions"].map(str::to_owned));
        }
        // No MCP server at all, the account's claude.ai connectors included:
        // their tools load mid-run and, with no ToolSearch in a fixed tool
        // list, every schema lands in the prompt — one run went from 41K to
        // 255K tokens of context per call on 2.1.291.
        args.push("--strict-mcp-config".to_owned());
        args.extend(["--effort", "high", "--setting-sources", "", "--tools"].map(str::to_owned));
        // `""` would turn every tool off; `default` is claude's word for all of them.
        if all_tools {
            args.push("default".to_owned());
            if !policy.network {
                args.extend([
                    "--disallowedTools".to_owned(),
                    "WebFetch,WebSearch".to_owned(),
                ]);
            }
        } else {
            args.push(tools.join(","));
        }
        if let Some(model) = &request.requested_model {
            args.extend(["--model".to_owned(), model.clone()]);
        }

        let mut env = BTreeMap::new();
        // The auto-memory index reaches the prompt past `--setting-sources ""`;
        // this removes it (measured on 2.1.302).
        env.insert("CLAUDE_CODE_DISABLE_AUTO_MEMORY".to_owned(), "1".to_owned());
        // The same opt-out for claude.ai connectors, for a CLI that predates
        // `--strict-mcp-config` covering them.
        env.insert("ENABLE_CLAUDEAI_MCP_SERVERS".to_owned(), "false".to_owned());
        Ok(Invocation {
            args,
            env,
            stdin_stays_open: ask,

            ..Default::default()
        })
    }

    fn report(&self, _run: &RunContext<'_>, result: &ProcessResult) -> RunReport {
        let mut report = parse_run_output(result);
        if report.succeeded && !matches!(result.exit, ProcessExit::Exited(0)) {
            report.succeeded = false;
            report.terminal_reason = serde_json::json!({
                "reason": "exit_status",
                "exit": format!("{:?}", result.exit),
                "stream": report.terminal_reason,
            });
        }
        report
    }

    fn capabilities(&self) -> FeatureCapabilities {
        FeatureCapabilities {
            cancel: capability(
                CapabilitySupport::Advisory,
                "The `claude` process is always signalled (it leads its own process group). A \
                 Bash-tool subprocess was observed (via `ps`) in its own session; it is only \
                 cleaned up if Claude Code exits within the SIGTERM grace period, since SIGKILL \
                 cannot reach another session's process group.",
            ),
            resume: capability(
                CapabilitySupport::Unsupported,
                "Headless (--print) invocation is one ephemeral process with no reattachment \
                 interface. `--resume <session-id>` starts a new process that continues stored \
                 history, which is not reattaching to an in-flight execution after a restart.",
            ),
            decisions: capability(
                CapabilitySupport::Supported,
                "With --permission-mode default --permission-prompt-tool stdio and stdin kept \
                 open, a tool call pauses on a can_use_tool control_request until this adapter \
                 answers it with a control_response; the CLI keeps running past its own result \
                 line until stdin closes.",
            ),
            artifacts: capability(
                CapabilitySupport::Advisory,
                "Write/Edit output lands in the workspace, but only the redacted stdout/stderr \
                 transcript is staged as a log artifact; no per-file artifact discovery is \
                 implemented.",
            ),
            usage: capability(
                CapabilitySupport::Advisory,
                "Token totals, cache reads and writes, model calls and the models used are read \
                 from the result line's `modelUsage` (every model, auxiliary ones included) or, \
                 without it, `usage`; `total_cost_usd` is Claude Code's own estimate at list \
                 price, not a bill. While it runs, each model call's tokens are read from its \
                 `assistant` line, once per message id, with the output low until the result.",
            ),
            additional: policy_capability(
                CapabilitySupport::Advisory,
                "the tool list is enforced through --tools; `network: false` only rejects a request \
                 naming WebFetch or WebSearch, and does not stop the Bash tool from reaching the \
                 network",
            ),
        }
    }

    /// An `ask` run reads stream-json on stdin, so its prompt is one user
    /// message in that format; any other run takes the prompt as plain text.
    fn prompt(&self, prompt: String, stdin_stays_open: bool) -> Vec<u8> {
        if !stdin_stays_open {
            return prompt.into_bytes();
        }
        let message = serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": [{"type": "text", "text": prompt}]},
        });
        let mut line = message.to_string().into_bytes();
        line.push(b'\n');
        line
    }

    /// A `can_use_tool` control request becomes a question; a terminal
    /// `result` line is what tells the core the conversation is over, so it
    /// can close stdin and let the CLI exit. Any other line — `system`,
    /// `assistant`, `user`/tool-result — means nothing to the core here.
    fn signal(&self, _run: &RunContext<'_>, line: &str) -> Option<StreamSignal> {
        let value: Value = serde_json::from_str(line.trim()).ok()?;
        match value.get("type").and_then(Value::as_str) {
            Some("control_request")
                if value.pointer("/request/subtype").and_then(Value::as_str)
                    == Some("can_use_tool") =>
            {
                let vendor_id = value.get("request_id")?.as_str()?.to_owned();
                let tool_name = value
                    .pointer("/request/tool_name")
                    .and_then(Value::as_str)
                    .unwrap_or("a tool");
                let description = value
                    .pointer("/request/description")
                    .and_then(Value::as_str)
                    .filter(|description| !description.is_empty());
                let prompt = match description {
                    Some(description) => format!("Allow {tool_name}: {description}?"),
                    None => format!("Allow {tool_name}?"),
                };
                let mut metadata = serde_json::Map::new();
                metadata.insert("tool_name".to_owned(), Value::String(tool_name.to_owned()));
                if let Some(input) = value.pointer("/request/input") {
                    metadata.insert("input".to_owned(), input.clone());
                }
                Some(StreamSignal::Question(Question {
                    vendor_id,
                    kind: "tool_permission".to_owned(),
                    prompt,
                    options: vec![
                        DecisionOption {
                            option_id: "allow_once".to_owned(),
                            label: "Allow once".to_owned(),
                            description: description.map(str::to_owned),
                            risks: None,
                            estimated_tokens: None,
                        },
                        DecisionOption {
                            option_id: "deny".to_owned(),
                            label: "Deny".to_owned(),
                            description: None,
                            risks: None,
                            estimated_tokens: None,
                        },
                    ],
                    metadata,
                    recommendation: None,
                }))
            }
            Some("result") => Some(StreamSignal::Finished),
            _ => None,
        }
    }

    /// An `assistant` line carries `message.usage` as that API call ends, and a call split
    /// over several lines (a thinking block, then its tool_use) repeats the id and the usage,
    /// so the id names the call. `output_tokens` is low until the `result` line, which settles
    /// the attempt's figure (`fixtures/claude_code/README.md`, 2.1.302).
    fn live_usage(&self, line: &str) -> Option<(String, Usage)> {
        let value: Value = serde_json::from_str(line.trim()).ok()?;
        if value.get("type").and_then(Value::as_str) != Some("assistant") {
            return None;
        }
        let call = value.pointer("/message/id")?.as_str()?.to_owned();
        let field = |key: &str| {
            value
                .pointer(&format!("/message/usage/{key}"))
                .and_then(Value::as_u64)
        };
        let (input, read, write) = (
            field("input_tokens")?,
            field("cache_read_input_tokens").unwrap_or(0),
            field("cache_creation_input_tokens").unwrap_or(0),
        );
        let detail = UsageDetail {
            cache_read_tokens: Some(read),
            cache_write_tokens: Some(write),
            ..Default::default()
        };
        Some((
            call,
            call_usage(
                input + read + write,
                field("output_tokens").unwrap_or(0),
                detail,
            ),
        ))
    }

    /// `allow_once` is the only option this grammar spends on letting the
    /// tool run; every other answer — including one this CLI never offered —
    /// is a denial, which `control_response` always accepts with a message.
    fn answer(&self, question: &Question, answer: &DecisionAnswer) -> Vec<u8> {
        let allow = answer.option_id.as_deref() == Some("allow_once");
        let response = if allow {
            serde_json::json!({"behavior": "allow"})
        } else {
            serde_json::json!({
                "behavior": "deny",
                "message": answer
                    .text
                    .clone()
                    .unwrap_or_else(|| "denied by the operator".to_owned()),
            })
        };
        let payload = serde_json::json!({
            "type": "control_response",
            "response": {
                "subtype": "success",
                "request_id": question.vendor_id,
                "response": response,
            },
        });
        let mut bytes = serde_json::to_vec(&payload).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }
}

#[cfg(test)]
#[path = "claude_code/tests.rs"]
mod tests;
