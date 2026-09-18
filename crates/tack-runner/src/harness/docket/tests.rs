//! docket's own claims: its command line, how a result line becomes a
//! report, and the real binary against a fake model server. Fixtures are
//! captured transcripts under `fixtures/docket/`. The lifecycle is proved
//! in `local_process/tests.rs`.

use std::time::Duration;

use super::*;
use crate::harness::HarnessAdapter;
use crate::harness::local_process::LocalProcessHarness;
use crate::harness::process::ProcessLimits;
use crate::harness::test_support::{finished, gateway, scratch, secret_store, set_env, spec};
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

#[test]
fn a_request_becomes_a_harness_run_command() {
    let state = scratch("docket-invocation");
    let endpoint = endpoint(state.path());
    let with_endpoint = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &with_endpoint,
        endpoint: Some(&endpoint),
    };
    let invocation = DocketGrammar.invocation(&run).expect("invocation");
    assert_eq!(
        invocation.args,
        [
            "harness",
            "run",
            "--workspace",
            &state.path().display().to_string(),
            "--task-file",
            "/dev/stdin",
            "--model",
            "openai/opaque/model-alpha",
            "--agent-id",
            "attempt",
            "--timeout",
            "30",
        ]
    );
    assert_eq!(invocation.env[BASE_URL_ENV], endpoint.base_url);
    assert!(invocation.env["DOCKET_HOME"].ends_with("docket-home"));

    let mut own_env = spec(DESCRIPTOR.kind, state.path());
    set_env(&mut own_env, &[(BASE_URL_ENV, "http://127.0.0.1:9/v1")]);
    let run = RunContext {
        spec: &own_env,
        endpoint: None,
    };
    let invocation = DocketGrammar.invocation(&run).expect("invocation");
    assert!(!invocation.env.contains_key(BASE_URL_ENV));
    assert!(invocation.env.contains_key("DOCKET_HOME"));
}

#[test]
fn a_request_with_no_model_endpoint_is_refused() {
    let state = scratch("docket-refused-invocation");
    let request = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &request,
        endpoint: None,
    };
    let error = DocketGrammar
        .invocation(&run)
        .expect_err("no endpoint and no request-level base url");
    assert!(matches!(&error, HarnessError::Rejected { reason } if reason.contains(BASE_URL_ENV)));
}

fn read(stdout: &str) -> RunReport {
    let state = scratch("docket-report");
    let request = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &request,
        endpoint: None,
    };
    DocketGrammar.report(
        &run,
        &finished(crate::harness::process::ProcessExit::Exited(0), stdout),
    )
}

/// `(fixture, succeeded, served model, tokens in, tokens out)` over the
/// four captured result shapes — `blocked`'s own detail is checked
/// separately below since it is the one non-null case.
#[test]
fn a_result_line_becomes_a_report() {
    let ok = include_str!("../fixtures/docket/0.2.0b1/ok.ndjson");
    let refused = include_str!("../fixtures/docket/0.2.0b1/refused.ndjson");
    let blocked = include_str!("../fixtures/docket/0.2.0b1/blocked.ndjson");
    let cancelled = include_str!("../fixtures/docket/0.2.0b1/cancelled.ndjson");
    let rows = [
        (ok, true, Some("anthropic/claude-x"), 102, 20),
        (refused, false, None, 0, 0),
        (blocked, false, Some("anthropic/claude-x"), 40, 8),
        (cancelled, false, None, 0, 0),
    ];
    for (transcript, succeeded, model, tokens_in, tokens_out) in rows {
        let report = read(transcript);
        assert_eq!(report.succeeded, succeeded, "{transcript}");
        assert_eq!(report.observed_model.as_deref(), model, "{transcript}");
        assert_eq!(
            (report.tokens_in, report.tokens_out),
            (Some(tokens_in), Some(tokens_out)),
            "{transcript}"
        );
        assert_eq!(report.cost_usd, None, "{transcript}");
    }
    let blocked_report = read(blocked);
    assert_eq!(blocked_report.terminal_reason["code"], "blocked");
    assert_eq!(blocked_report.terminal_reason["blocked"]["tool"], "bash");
}

#[test]
fn output_without_a_result_line_is_a_failed_run() {
    for stdout in ["", "not json at all", "{\"incomplete\": true"] {
        let report = read(stdout);
        assert!(!report.succeeded, "{stdout:?}");
        assert_eq!(
            report.terminal_reason["code"], "malformed_output",
            "{stdout:?}"
        );
    }
}

/// One JSON chat-completions response per call: a `write` tool call, then a
/// final message. The served model id is fixed and distinct from what was
/// requested, so an assertion that it reached the outcome proves it came
/// from the response body, not an echo of the request.
const SERVED_MODEL: &str = "served/observed-model-x";
const FAKE_KEY: &str = "sk-fake-gateway-canary-71c2";

#[derive(Default)]
struct SequencedToolCall {
    calls: std::sync::atomic::AtomicUsize,
}

impl wiremock::Respond for SequencedToolCall {
    /// The tool call's own `content` argument carries the fake credential —
    /// standing in for a worst-case model response that echoes a value it
    /// was never even given — so a real captured line on docket's own
    /// stdout is what the redaction assertion below actually has to catch.
    fn respond(&self, _request: &wiremock::Request) -> wiremock::ResponseTemplate {
        let first = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0;
        let message = if first {
            serde_json::json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "write",
                        "arguments": serde_json::json!({
                            "path": "greeting.txt",
                            "content": format!("hello from the fake model server; canary={FAKE_KEY}\n"),
                        })
                        .to_string(),
                    },
                }],
            })
        } else {
            serde_json::json!({"role": "assistant", "content": "Done."})
        };
        let finish_reason = if first { "tool_calls" } else { "stop" };
        wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "chatcmpl-live",
            "model": SERVED_MODEL,
            "choices": [{"index": 0, "message": message, "finish_reason": finish_reason}],
            "usage": {"prompt_tokens": 11, "completion_tokens": 5, "total_tokens": 16},
        }))
    }
}

/// Builds the request a live docket run is spawned with: the contract's own
/// claim fixture, pointed at the gateway provider and a fixed model.
fn live_request(workspace: &std::path::Path) -> crate::harness::ExecutionSpec {
    let mut request = spec(DESCRIPTOR.kind, workspace);
    request.work.request.requested_model_provider = Some(RequestedModelProvider::new(
        crate::config::VERCEL_AI_GATEWAY_PROVIDER,
    ));
    request.work.request.requested_model_id = Some(RequestedModelId::new("anthropic/claude-x"));
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

async fn mount_sequenced_tool_call(server: &wiremock::MockServer) {
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/chat/completions"))
        .respond_with(SequencedToolCall::default())
        .mount(server)
        .await;
}

fn live_harness(staging_root: std::path::PathBuf, secrets: SecretStore) -> DocketAdapter {
    LocalProcessHarness::discover(
        DocketGrammar,
        ProcessLimits::new(1_048_576, 1_048_576, Duration::from_secs(30)),
        staging_root,
        secrets,
    )
    .expect("docket located above")
    .with_providers(gateway("key"))
}

fn assert_live_outcome(outcome: &crate::harness::HarnessOutcome, workspace: &std::path::Path) {
    assert_eq!(
        outcome.terminal_state,
        crate::client::AttemptState::Succeeded,
        "{:?}",
        outcome.terminal_reason
    );
    let written = std::fs::read_to_string(workspace.join("greeting.txt")).expect("docket wrote it");
    assert!(written.contains("hello from the fake model server"));
    assert_eq!(outcome.actual_execution.model_id.as_str(), SERVED_MODEL);
    assert_eq!(
        outcome.actual_execution.model_observation_source,
        "harness_reported"
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
async fn the_real_docket_edits_a_file_against_a_fake_model_server() {
    if crate::harness::locate::locate_installed(DESCRIPTOR.program).is_err() {
        eprintln!("skipping live docket test: `docket` not found on PATH");
        return;
    }
    let state = scratch("docket-live");
    let workspace = scratch("docket-live-workspace");
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
}
