//! Claude Code's own claims: which requests it refuses, its command line,
//! and how it reads a `stream-json` transcript. Transcripts are files under
//! `fixtures/claude_code/`, each with its provenance beside it. The
//! lifecycle is proved in `local_process/tests.rs`.

use super::*;
use crate::harness::test_support::{finished, gateway, scratch, secret_store, spec};
use tack_orch::execution::{Approvals, RequestedModelProvider};

fn request(provider: Option<&str>, tools: &[&str], network: bool) -> crate::harness::ExecutionSpec {
    let mut request = spec(DESCRIPTOR.kind, std::path::Path::new("/unused"));
    request.work.request.requested_model_provider = provider.map(RequestedModelProvider::new);
    request.work.request.permission_policy.tools = tools.iter().map(|t| t.to_string()).collect();
    request.work.request.permission_policy.network = network;
    request
}

fn invocation(spec: &crate::harness::ExecutionSpec) -> Result<Invocation, HarnessError> {
    ClaudeCodeGrammar.invocation(&RunContext {
        spec,
        endpoint: None,
        scratch: std::path::Path::new("unused"),
    })
}

/// `(provider, tools, network allowed) -> accepted`. A native family is
/// matched case-insensitively; a configured provider is known because the
/// provider registry knows it, not because it is listed here.
#[test]
fn a_request_is_refused_for_its_provider_or_its_policy() {
    let gateway = crate::config::VERCEL_AI_GATEWAY_PROVIDER;
    let rows: [(Option<&str>, &[&str], bool, bool); 8] = [
        (None, &["Read"], true, true),
        (Some("anthropic"), &[], true, true),
        (Some("BEDROCK"), &[], true, true),
        (Some("Vertex"), &[], true, true),
        (Some(gateway), &[], true, true),
        (Some("openai"), &[], true, false),
        (None, &["WebFetch"], false, false),
        (None, &["WebFetch"], true, true),
    ];
    for (provider, tools, network, accepted) in rows {
        let outcome = invocation(&request(provider, tools, network));
        assert_eq!(
            outcome.is_ok(),
            accepted,
            "{provider:?} {tools:?} network={network}"
        );
        if let Err(error) = outcome {
            assert!(matches!(error, HarnessError::Rejected { .. }));
        }
    }
}

#[test]
fn the_policy_model_and_budget_reach_the_command_line() {
    let mut spec = request(Some("anthropic"), &["Read", "Edit"], true);
    spec.work.request.budgets = serde_json::json!({"cost_usd": 1.5});
    let args = invocation(&spec).expect("invocation").args;

    let value_of = |flag: &str| {
        let at = args.iter().position(|arg| arg == flag).expect(flag);
        args[at + 1].as_str()
    };
    assert_eq!(value_of("--tools"), "Read,Edit");
    assert_eq!(value_of("--model"), "opaque/model-alpha");
    assert_eq!(value_of("--max-budget-usd"), "1.5");
    assert_eq!(value_of("--output-format"), "stream-json");
    assert_eq!(value_of("--setting-sources"), "");

    spec.work.request.budgets = serde_json::json!({"cost_usd": null});
    spec.work.request.requested_model_id = None;
    let args = invocation(&spec).expect("invocation").args;
    assert!(
        !args
            .iter()
            .any(|arg| arg == "--max-budget-usd" || arg == "--model")
    );

    // `approvals` absent, or explicit `auto`, produces byte-for-byte the
    // same command line this adapter has always produced.
    let baseline = invocation(&request(Some("anthropic"), &["Read"], true))
        .expect("invocation")
        .args;
    let mut explicit_auto = request(Some("anthropic"), &["Read"], true);
    explicit_auto.work.request.permission_policy.approvals = Some(Approvals::Auto);
    let with_auto = invocation(&explicit_auto).expect("invocation").args;
    assert_eq!(with_auto, baseline);
    assert!(baseline.iter().any(|arg| arg == "bypassPermissions"));
    assert!(!baseline.iter().any(|arg| arg == "--permission-prompt-tool"));
}

/// `ask` swaps `bypassPermissions` for the flags measured against a real
/// `claude`, and sets `stdin_stays_open` so the core keeps the pipe open.
#[test]
fn an_ask_request_keeps_stdin_open_and_asks_the_host() {
    let mut spec = request(Some("anthropic"), &["Read"], true);
    spec.work.request.permission_policy.approvals = Some(Approvals::Ask);
    let built = invocation(&spec).expect("invocation");

    assert!(built.stdin_stays_open);
    let joined = built.args.join(" ");
    assert!(joined.contains("--input-format stream-json"));
    assert!(joined.contains("--permission-mode default"));
    assert!(joined.contains("--permission-prompt-tool stdio"));
    assert!(!built.args.iter().any(|arg| arg == "bypassPermissions"));

    let framed = ClaudeCodeGrammar.prompt("do it".to_owned(), built.stdin_stays_open);
    let message: serde_json::Value = serde_json::from_slice(&framed).expect("one JSON line");
    assert_eq!(message["type"], "user");
    assert_eq!(message["message"]["content"][0]["text"], "do it");
    assert_eq!(framed.last(), Some(&b'\n'));
    let plain = ClaudeCodeGrammar.prompt("do it".to_owned(), false);
    assert_eq!(plain, b"do it");
}

/// A `can_use_tool` control request becomes a question; a terminal `result`
/// line — even with stdin still open — tells the core the conversation is
/// over. Any other captured line means nothing here.
#[test]
fn a_permission_request_line_becomes_a_question() {
    let spec = request(Some("anthropic"), &["Bash"], true);
    let run = RunContext {
        spec: &spec,
        endpoint: None,
        scratch: std::path::Path::new("unused"),
    };
    let request_line =
        include_str!("../fixtures/claude_code/2.1.273/ask-tool-permission-request.jsonl");
    let signal = ClaudeCodeGrammar
        .signal(&run, request_line)
        .expect("a question");
    let StreamSignal::Question(question) = signal else {
        panic!("expected a question, got {signal:?}");
    };
    assert_eq!(question.vendor_id, "eb72d6ca-573f-48fe-aedb-002cfd31bb7c");
    assert_eq!(question.kind, "tool_permission");
    assert!(question.prompt.contains("Bash"));
    assert_eq!(
        question
            .options
            .iter()
            .map(|o| o.option_id.as_str())
            .collect::<Vec<_>>(),
        ["allow_once", "deny"]
    );

    let result_line = include_str!("../fixtures/claude_code/2.1.273/ask-result.jsonl");
    assert_eq!(
        ClaudeCodeGrammar.signal(&run, result_line),
        Some(StreamSignal::Finished)
    );

    assert_eq!(
        ClaudeCodeGrammar.signal(&run, "{\"type\":\"assistant\"}"),
        None
    );
}

/// `allow_once` writes `{"behavior":"allow"}`; anything else, including an
/// answer this CLI never offered, denies with a message — matching the
/// shape a real `claude` accepted for each.
#[test]
fn an_answer_becomes_a_control_response() {
    let question = Question {
        vendor_id: "eb72d6ca-573f-48fe-aedb-002cfd31bb7c".to_owned(),
        kind: "tool_permission".to_owned(),
        prompt: "Allow Bash?".to_owned(),
        options: vec![
            DecisionOption {
                option_id: "allow_once".to_owned(),
                label: "Allow once".to_owned(),
            },
            DecisionOption {
                option_id: "deny".to_owned(),
                label: "Deny".to_owned(),
            },
        ],
        metadata: serde_json::Map::new(),
    };

    let allow = DecisionAnswer {
        option_id: Some("allow_once".to_owned()),
        text: None,
    };
    let bytes = ClaudeCodeGrammar.answer(&question, &allow);
    let expected: Value = serde_json::from_str(include_str!(
        "../fixtures/claude_code/2.1.273/ask-allow-response.jsonl"
    ))
    .expect("fixture");
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).expect("valid json"),
        expected
    );

    let deny_question = Question {
        vendor_id: "f142b75c-e54c-4b01-9b00-58d521445d16".to_owned(),
        ..question
    };
    let deny = DecisionAnswer {
        option_id: Some("deny".to_owned()),
        text: Some("denied by test".to_owned()),
    };
    let bytes = ClaudeCodeGrammar.answer(&deny_question, &deny);
    let expected: Value = serde_json::from_str(include_str!(
        "../fixtures/claude_code/2.1.273/ask-deny-response.jsonl"
    ))
    .expect("fixture");
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).expect("valid json"),
        expected
    );
}

/// The credential itself is the core's to inject; the grammar only points
/// the CLI at the endpoint and blanks the variable that would outrank it.
#[test]
fn a_configured_endpoint_sets_the_base_url_and_nothing_secret() {
    let state = scratch("claude-endpoint");
    let secrets = secret_store(state.path());
    secrets
        .set("key", "a-resolvable-value")
        .expect("seed store");
    let provider = crate::config::VERCEL_AI_GATEWAY_PROVIDER;
    let endpoint =
        crate::provider::resolve_endpoint(&gateway("key"), &secrets, provider, DESCRIPTOR.wire)
            .expect("resolve")
            .expect("the gateway serves this wire");
    let spec = request(Some(provider), &[], true);

    let direct = invocation(&spec).expect("direct").env;
    assert!(direct.is_empty(), "{direct:?}");
    let routed = ClaudeCodeGrammar
        .invocation(&RunContext {
            spec: &spec,
            endpoint: Some(&endpoint),
            scratch: std::path::Path::new("unused"),
        })
        .expect("routed")
        .env;
    assert_eq!(routed["ANTHROPIC_BASE_URL"], endpoint.base_url);
    assert_eq!(routed["ANTHROPIC_API_KEY"], "");
    assert!(
        !routed
            .values()
            .any(|value| value == endpoint.credential.expose())
    );
}

fn read(exit: ProcessExit, stdout: &str) -> RunReport {
    let spec = request(None, &[], true);
    let run = RunContext {
        spec: &spec,
        endpoint: None,
        scratch: std::path::Path::new("unused"),
    };
    ClaudeCodeGrammar.report(&run, &finished(exit, stdout))
}

#[test]
fn a_success_transcript_yields_model_version_and_usage() {
    let transcript = include_str!("../fixtures/claude_code/2.1.223/success-with-usage.jsonl");
    let report = read(ProcessExit::Exited(0), transcript);
    assert!(report.succeeded);
    assert_eq!(report.observed_model.as_deref(), Some("claude-sonnet-5"));
    assert_eq!(report.harness_version.as_deref(), Some("2.1.223"));
    assert_eq!((report.tokens_in, report.tokens_out), (Some(2), Some(18)));
    assert_eq!(report.cost_usd, Some(0.0350687));

    let routed = include_str!("../fixtures/claude_code/2.1.261/gateway-routed-result.jsonl");
    let report = read(ProcessExit::Exited(1), routed);
    assert_eq!(
        report.observed_model.as_deref(),
        Some("anthropic/claude-opus-4.6")
    );
}

/// `(transcript, exit, the field of the reason that says why, its value)`.
/// `is_error` decides, not `subtype` and not the exit status.
#[test]
fn a_transcript_is_failed_by_is_error_or_by_a_missing_result() {
    let misleading =
        include_str!("../fixtures/claude_code/2.1.223/api-error-misleading-subtype.jsonl");
    let exhausted = include_str!("../fixtures/claude_code/2.1.223/budget-exhausted.jsonl");
    let truncated = include_str!("../fixtures/claude_code/2.1.223/truncated-mid-run.jsonl");
    let rows = [
        (misleading, 1, "subtype", "success"),
        (exhausted, 1, "subtype", "error_max_budget_usd"),
        (truncated, 0, "reason", "malformed_output"),
        (
            "{\"type\":\"result\",\"result\":\"no is_error\"}",
            0,
            "type",
            "result",
        ),
    ];
    for (transcript, exit, field, value) in rows {
        let report = read(ProcessExit::Exited(exit), transcript);
        assert!(!report.succeeded, "{value}");
        assert_eq!(report.terminal_reason[field], value);
    }
    assert_eq!(
        read(ProcessExit::Exited(1), exhausted).cost_usd,
        Some(0.013149)
    );
}

/// With no JSON line at all the exit status is all there is, and the
/// reason says the verdict was inferred.
#[test]
fn output_with_no_json_falls_back_to_the_exit_status() {
    let rows = [
        (ProcessExit::Exited(0), "fake-harness-ok", true),
        (ProcessExit::Exited(7), "", false),
        (ProcessExit::TimedOut, "", false),
    ];
    for (exit, stdout, succeeded) in rows {
        let report = read(exit, stdout);
        assert_eq!(report.succeeded, succeeded, "{exit:?}");
        let reason = report.terminal_reason["reason"].as_str().expect("reason");
        assert!(reason.contains("no structured result envelope"), "{reason}");
        assert_eq!((report.observed_model, report.tokens_in), (None, None));
    }
}

#[test]
fn claude_code_declares_advisory_cancel_artifacts_and_policy() {
    let declared = ClaudeCodeGrammar.capabilities();
    assert_eq!(declared.cancel.support, CapabilitySupport::Advisory);
    assert_eq!(declared.artifacts.support, CapabilitySupport::Advisory);
    assert_eq!(
        declared.additional["permission_policy"]["support"],
        "advisory"
    );
}
