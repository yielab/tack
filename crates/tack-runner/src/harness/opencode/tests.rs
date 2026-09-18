//! opencode's own claims: its command line and environment, the permission
//! block as a mapping from policy, refusals, and how its event stream
//! becomes a report. Fixtures are captured transcripts under
//! `fixtures/opencode/`. The lifecycle is proved in `local_process/tests.rs`.

use std::time::Duration;

use super::*;
use crate::harness::HarnessAdapter;
use crate::harness::local_process::LocalProcessHarness;
use crate::harness::process::{ProcessExit, ProcessLimits};
use crate::harness::test_support::{finished, gateway, scratch, secret_store, spec};
use crate::secrets::SecretStore;
use tack_orch::execution::{RequestedModelId, RequestedModelProvider};

fn endpoint(state: &std::path::Path) -> crate::provider::ProviderEndpoint {
    let secrets = secret_store(state);
    secrets
        .set("key", "a-resolvable-value")
        .expect("seed store");
    let provider = crate::config::VERCEL_AI_GATEWAY_PROVIDER;
    crate::provider::resolve_endpoint(&gateway("key"), &secrets, provider, DESCRIPTOR.wire)
        .expect("resolve")
        .expect("the gateway serves this wire")
}

/// The contract's own request, with `network` granted — every real
/// invocation needs it, since `invocation()` refuses the contract's own
/// default (`false`) outright.
fn networked(state: &std::path::Path) -> crate::harness::ExecutionSpec {
    let mut request = spec(DESCRIPTOR.kind, state);
    request.work.request.permission_policy.network = true;
    request
}

#[test]
fn a_request_becomes_a_run_command_and_environment() {
    let state = scratch("opencode-invocation");
    let scratch_dir = scratch("opencode-invocation-scratch");
    let endpoint = endpoint(state.path());
    let request = networked(state.path());
    let run = RunContext {
        spec: &request,
        endpoint: Some(&endpoint),
        scratch: scratch_dir.path(),
    };
    let invocation = OpencodeGrammar.invocation(&run).expect("invocation");
    assert_eq!(
        invocation.args,
        [
            "run",
            "--pure",
            "--format",
            "json",
            "--title",
            "tack",
            "-m",
            "tack/opaque/model-alpha"
        ]
    );
    let home = &invocation.env["HOME"];
    assert!(home.starts_with(&scratch_dir.path().display().to_string()));
    assert!(!home.starts_with(&state.path().display().to_string()));
    assert!(home.ends_with("opencode-home"));
    assert!(invocation.env["OPENCODE_CONFIG_DIR"].ends_with("opencode-home/config"));
    for flag in [
        "OPENCODE_DISABLE_PROJECT_CONFIG",
        "OPENCODE_DISABLE_AUTOUPDATE",
        "OPENCODE_DISABLE_MODELS_FETCH",
        "OPENCODE_DISABLE_SHARE",
    ] {
        assert_eq!(invocation.env[flag], "1", "{flag}");
    }
    let content: serde_json::Value =
        serde_json::from_str(&invocation.env["OPENCODE_CONFIG_CONTENT"]).expect("valid json");
    assert_eq!(
        content["provider"]["tack"]["options"]["baseURL"],
        endpoint.base_url
    );
    assert_eq!(
        content["provider"]["tack"]["options"]["apiKey"],
        format!("{{env:{}}}", endpoint.credential_env_var)
    );
    assert!(content["provider"]["tack"]["models"]["opaque/model-alpha"].is_object());
}

#[test]
fn the_permission_block_is_a_table_over_policies() {
    let state = scratch("opencode-permission");
    let endpoint = endpoint(state.path());
    // `network` is always granted: a denied one is rejected before spawn
    // (`the_refusals_are_a_table`), so `webfetch` can only ever be observed
    // here as "allow".
    let rows: [(&[&str], [&str; 4]); 2] = [
        (&[], ["allow", "deny", "deny", "deny"]),
        (
            &["edit", "bash", "task"],
            ["allow", "allow", "allow", "allow"],
        ),
    ];
    for (tools, expected) in rows {
        let mut request = spec(DESCRIPTOR.kind, state.path());
        request.work.request.permission_policy.network = true;
        request.work.request.permission_policy.tools =
            tools.iter().map(|tool| (*tool).to_owned()).collect();
        let run = RunContext {
            spec: &request,
            endpoint: Some(&endpoint),
            scratch: state.path(),
        };
        let invocation = OpencodeGrammar.invocation(&run).expect("invocation");
        let content: serde_json::Value =
            serde_json::from_str(&invocation.env["OPENCODE_CONFIG_CONTENT"]).expect("json");
        let permission = &content["permission"];
        for (key, want) in ["webfetch", "task", "edit", "bash"].iter().zip(expected) {
            assert_eq!(permission[key], want, "{tools:?} {key}");
        }
    }
}

#[test]
fn the_refusals_are_a_table() {
    let state = scratch("opencode-refusals");
    let endpoint = endpoint(state.path());
    let request = spec(DESCRIPTOR.kind, state.path());
    let rows = [
        (
            RunContext {
                spec: &request,
                endpoint: None,
                scratch: state.path(),
            },
            "endpoint",
        ),
        (
            RunContext {
                spec: &request,
                endpoint: Some(&endpoint),
                scratch: state.path(),
            },
            "network",
        ),
    ];
    for (run, needle) in rows {
        let error = OpencodeGrammar.invocation(&run).expect_err("rejected");
        assert!(
            matches!(&error, HarnessError::Rejected { reason } if reason.contains(needle)),
            "{needle}"
        );
    }
}

fn read(stdout: &str, exit: ProcessExit) -> RunReport {
    let state = scratch("opencode-report");
    let request = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &request,
        endpoint: None,
        scratch: state.path(),
    };
    OpencodeGrammar.report(&run, &finished(exit, stdout))
}

/// `(fixture, exit, succeeded, tokens in, tokens out)`. `tool_denied` sums
/// tokens across both its `step_finish` events; `cut_short` has none.
#[test]
fn events_become_a_report_over_captured_fixtures() {
    let completed = include_str!("../fixtures/opencode/1.18.30/completed.ndjson");
    let tool_denied = include_str!("../fixtures/opencode/1.18.30/tool_denied.ndjson");
    let cut_short = include_str!("../fixtures/opencode/1.18.30/cut_short.ndjson");
    let rows = [
        (completed, ProcessExit::Exited(0), true, 7, 3),
        (tool_denied, ProcessExit::Exited(0), true, 14, 6),
        (cut_short, ProcessExit::Exited(143), false, 0, 0),
    ];
    for (transcript, exit, succeeded, tokens_in, tokens_out) in rows {
        let report = read(transcript, exit);
        assert_eq!(report.succeeded, succeeded, "{transcript:?}");
        assert_eq!(report.observed_model, None, "{transcript:?}");
        assert_eq!(report.cost_usd, None, "{transcript:?}");
        if succeeded {
            assert_eq!(
                (report.tokens_in, report.tokens_out),
                (Some(tokens_in), Some(tokens_out)),
                "{transcript:?}"
            );
        }
    }
    let cancelled = read(cut_short, ProcessExit::Exited(143));
    assert_eq!(cancelled.terminal_reason["reason"], "cancelled");
}

/// One JSON chat-completions chunk stream per call, over `text/event-stream`:
/// a `write` tool call, then a final message. The served model id is fixed
/// and never read by this adapter (ADR 0067 decision 3) — every event
/// stream lacks a model field at all, so there is nothing to echo.
const SERVED_MODEL: &str = "served/observed-model-x";
const FAKE_KEY: &str = "sk-fake-canary-71c2";

fn sse(chunks: &[serde_json::Value]) -> Vec<u8> {
    let mut body = String::new();
    for chunk in chunks {
        body.push_str(&format!("data: {chunk}\n\n"));
    }
    body.push_str("data: [DONE]\n\n");
    body.into_bytes()
}

#[derive(Default)]
struct SequencedToolCall {
    calls: std::sync::atomic::AtomicUsize,
}

impl wiremock::Respond for SequencedToolCall {
    /// The tool call's own `arguments` string carries the fake credential —
    /// standing in for a worst-case model response that echoes a value it
    /// was never even given — so a real captured line on opencode's own
    /// stdout is what the redaction assertion below actually has to catch.
    fn respond(&self, _request: &wiremock::Request) -> wiremock::ResponseTemplate {
        let first = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0;
        let body = if first {
            let arguments = serde_json::json!({
                "filePath": "greeting.txt",
                "content": format!("hello from the fake model server; canary={FAKE_KEY}\n"),
            })
            .to_string();
            sse(&[
                serde_json::json!({"id": "c1", "model": SERVED_MODEL, "choices": [{"index": 0,
                    "delta": {"role": "assistant", "tool_calls": [{"index": 0, "id": "call_1",
                    "type": "function", "function": {"name": "write", "arguments": arguments}}]},
                    "finish_reason": null}]}),
                serde_json::json!({"id": "c1", "model": SERVED_MODEL, "choices": [{"index": 0,
                    "delta": {}, "finish_reason": "tool_calls"}],
                    "usage": {"prompt_tokens": 11, "completion_tokens": 5, "total_tokens": 16}}),
            ])
        } else {
            sse(&[
                serde_json::json!({"id": "c2", "model": SERVED_MODEL, "choices": [{"index": 0,
                    "delta": {"role": "assistant", "content": "Done."}, "finish_reason": null}]}),
                serde_json::json!({"id": "c2", "model": SERVED_MODEL, "choices": [{"index": 0,
                    "delta": {}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 7}}),
            ])
        };
        wiremock::ResponseTemplate::new(200).set_body_raw(body, "text/event-stream")
    }
}

async fn mount_sequenced_tool_call(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/chat/completions"))
        .respond_with(SequencedToolCall::default())
        .mount(server)
        .await;
}

fn live_request(workspace: &std::path::Path) -> crate::harness::ExecutionSpec {
    let mut request = spec(DESCRIPTOR.kind, workspace);
    request.work.request.requested_model_provider = Some(RequestedModelProvider::new(
        crate::config::VERCEL_AI_GATEWAY_PROVIDER,
    ));
    request.work.request.requested_model_id = Some(RequestedModelId::new("fake-model"));
    request.work.request.permission_policy.network = true;
    request.work.request.permission_policy.tools = vec!["edit".to_owned(), "bash".to_owned()];
    request.work.request.resolved_agent_profile.instructions =
        "Write a short greeting to greeting.txt".to_owned();
    request
}

/// Sets the loopback override for the lifetime of the guard, so a panic
/// mid-test still clears a process-wide variable the next test would
/// otherwise inherit.
struct BaseUrlOverride;

impl BaseUrlOverride {
    fn set(uri: &str) -> Self {
        // SAFETY-ish: process-wide, but safe under `cargo nextest` (one
        // process per test), never under a bare `cargo test`.
        unsafe { std::env::set_var("TACK_RUNNER_VERCEL_AI_GATEWAY_TEST_BASE_URL", uri) };
        Self
    }
}

impl Drop for BaseUrlOverride {
    fn drop(&mut self) {
        unsafe { std::env::remove_var("TACK_RUNNER_VERCEL_AI_GATEWAY_TEST_BASE_URL") };
    }
}

fn live_harness(staging_root: std::path::PathBuf, secrets: SecretStore) -> OpencodeAdapter {
    LocalProcessHarness::discover(
        OpencodeGrammar,
        ProcessLimits::new(1_048_576, 1_048_576, Duration::from_secs(30)),
        staging_root,
        secrets,
    )
    .expect("opencode located above")
    .with_providers(gateway("key"))
}

/// The real machine's own opencode state directory, untouched by a spawn
/// that always points `HOME` at the attempt's own workspace instead.
fn real_home_config_mtime() -> Option<std::time::SystemTime> {
    let home = std::env::var("HOME").ok()?;
    std::fs::metadata(std::path::Path::new(&home).join(".config/opencode"))
        .ok()?
        .modified()
        .ok()
}

fn assert_live_outcome(outcome: &crate::harness::HarnessOutcome, workspace: &std::path::Path) {
    assert_eq!(
        outcome.terminal_state,
        crate::client::AttemptState::Succeeded,
        "{:?}",
        outcome.terminal_reason
    );
    let written =
        std::fs::read_to_string(workspace.join("greeting.txt")).expect("opencode wrote it");
    assert!(written.contains("hello from the fake model server"));
    assert_eq!(outcome.actual_execution.model_id.as_str(), "fake-model");
    assert_eq!(
        outcome.actual_execution.model_observation_source,
        "requested_not_confirmed"
    );
    assert!(outcome.usage.tokens_in.value.unwrap_or(0) > 0);
}

/// The gateway credential reached the fake server, but never the staged log.
async fn assert_key_handling(
    server: &wiremock::MockServer,
    outcome: &crate::harness::HarnessOutcome,
) {
    let requests = server.received_requests().await.unwrap_or_default();
    let auth = requests[0]
        .headers
        .get("Authorization")
        .expect("auth header sent")
        .to_str()
        .expect("header is ascii");
    assert_eq!(auth, format!("Bearer {FAKE_KEY}"));

    let staged = outcome.terminal_reason["artifact"]["staged_path"]
        .as_str()
        .expect("run log staged");
    let log = std::fs::read_to_string(staged).expect("read staged log");
    assert!(!log.contains(FAKE_KEY));
}

#[tokio::test]
async fn the_real_opencode_edits_a_file_against_a_fake_model_server() {
    if crate::harness::locate::locate_installed(DESCRIPTOR.program).is_err() {
        eprintln!("skipping live opencode test: `opencode` not found on PATH");
        return;
    }
    let real_home_before = real_home_config_mtime();
    let state = scratch("opencode-live");
    let workspace = scratch("opencode-live-workspace");
    let secrets = secret_store(state.path());
    secrets.set("key", FAKE_KEY).expect("seed store");

    let server = wiremock::MockServer::start().await;
    mount_sequenced_tool_call(&server).await;
    let _base_url_override = BaseUrlOverride::set(&server.uri());

    let harness = live_harness(state.path().join("staging"), secrets);
    let request = live_request(workspace.path());
    let handle = harness.start(&request).await.expect("start");
    let outcome = harness.wait(&handle).await.expect("wait");

    assert_live_outcome(&outcome, workspace.path());
    assert_key_handling(&server, &outcome).await;
    assert_eq!(
        real_home_config_mtime(),
        real_home_before,
        "real HOME must never be touched"
    );
}
