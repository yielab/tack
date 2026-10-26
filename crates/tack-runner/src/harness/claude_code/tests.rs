//! Claude Code's own claims: which requests it refuses, its command line,
//! and how it reads a `stream-json` transcript. Transcripts are files under
//! `fixtures/claude_code/`, each with its provenance beside it. The
//! lifecycle is proved in `local_process/tests.rs`.

use super::*;
use crate::harness::test_support::{finished, spec};
use tack_protocol::execution::Approvals;

fn request(tools: &[&str], network: bool) -> crate::harness::ExecutionSpec {
    let mut request = spec(DESCRIPTOR.kind, std::path::Path::new("/unused"));
    request.work.request.permission_policy.tools = tools.iter().map(|t| t.to_string()).collect();
    request.work.request.permission_policy.network = network;
    request
}

fn invocation(spec: &crate::harness::ExecutionSpec) -> Result<Invocation, HarnessError> {
    ClaudeCodeGrammar.invocation(&RunContext {
        spec,
        scratch: std::path::Path::new("unused"),
        program: None,
    })
}

/// `(tools, network allowed) -> accepted`.
#[test]
fn a_request_is_refused_for_its_policy() {
    let rows: [(&[&str], bool, bool); 3] = [
        (&["Read"], true, true),
        (&["WebFetch"], false, false),
        (&["WebFetch"], true, true),
    ];
    for (tools, network, accepted) in rows {
        let outcome = invocation(&request(tools, network));
        assert_eq!(outcome.is_ok(), accepted, "{tools:?} network={network}");
        if let Err(error) = outcome {
            assert!(matches!(error, HarnessError::Rejected { .. }));
        }
    }
}

/// `*` (every tool, as the built-in profiles write it) is the eight file,
/// shell and web tools, never `--tools "*"` or claude's `default`; an empty
/// list stays `""`, every tool off.
#[test]
fn every_tool_is_the_eight_and_no_network_drops_the_web_tools() {
    let args = |tools: &[&str], network: bool| {
        invocation(&request(tools, network))
            .expect("invocation")
            .args
    };
    let value_of = |args: &[String], flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .map(|at| args[at + 1].clone())
    };

    let networked = args(&["*"], true);
    assert_eq!(
        value_of(&networked, "--tools").as_deref(),
        Some("Bash,Edit,Glob,Grep,Read,WebFetch,WebSearch,Write")
    );
    assert_eq!(value_of(&networked, "--disallowedTools"), None);

    let offline = args(&["*"], false);
    assert_eq!(
        value_of(&offline, "--tools").as_deref(),
        Some("Bash,Edit,Glob,Grep,Read,Write")
    );
    assert_eq!(value_of(&offline, "--disallowedTools"), None);

    assert_eq!(value_of(&args(&[], true), "--tools").as_deref(), Some(""));
}

#[test]
fn no_mcp_server_loads_not_even_the_accounts_claude_ai_connectors() {
    let invocation = invocation(&request(&["Read"], true)).expect("invocation");
    assert!(
        invocation
            .args
            .iter()
            .any(|arg| arg == "--strict-mcp-config")
    );
    assert!(!invocation.args.iter().any(|arg| arg == "--mcp-config"));
    assert_eq!(
        invocation
            .env
            .get("ENABLE_CLAUDEAI_MCP_SERVERS")
            .map(String::as_str),
        Some("false")
    );
}

#[test]
fn the_policy_and_model_reach_the_command_line() {
    let mut spec = request(&["Read", "Edit"], true);
    spec.work.request.budgets = serde_json::json!({"cost_usd": 1.5});
    let args = invocation(&spec).expect("invocation").args;

    let value_of = |flag: &str| {
        let at = args.iter().position(|arg| arg == flag).expect(flag);
        args[at + 1].as_str()
    };
    assert_eq!(value_of("--tools"), "Read,Edit");
    assert_eq!(value_of("--model"), "opaque/model-alpha");
    // A dollar budget in an old snapshot is read and ignored: no flag carries it.
    assert!(!args.iter().any(|arg| arg == "--max-budget-usd"));
    assert_eq!(value_of("--output-format"), "stream-json");
    assert_eq!(value_of("--setting-sources"), "");
    // The attribution reminder is switched off (measured on 2.1.302).
    assert_eq!(value_of("--settings"), r#"{"includeCoAuthoredBy":false}"#);

    spec.work.request.requested_model = None;
    let args = invocation(&spec).expect("invocation").args;
    assert!(!args.iter().any(|arg| arg == "--model"));

    // `approvals` absent, or explicit `auto`, produces byte-for-byte the
    // same command line this adapter has always produced.
    let baseline = invocation(&request(&["Read"], true))
        .expect("invocation")
        .args;
    let mut explicit_auto = request(&["Read"], true);
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
    let mut spec = request(&["Read"], true);
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
    let spec = request(&["Bash"], true);
    let run = RunContext {
        spec: &spec,
        scratch: std::path::Path::new("unused"),
        program: None,
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
    assert_eq!(
        question.options[0].description.as_deref(),
        Some("list files")
    );
    assert_eq!(question.options[1].description, None);

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
                description: None,
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
        metadata: serde_json::Map::new(),
        recommendation: None,
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

fn read(exit: ProcessExit, stdout: &str) -> RunReport {
    let spec = request(&[], true);
    let run = RunContext {
        spec: &spec,
        scratch: std::path::Path::new("unused"),
        program: None,
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

/// The figures of a real 2.1.273 run that read 1.08M tokens: `usage.input_tokens`
/// alone said 12.
#[test]
fn tokens_in_counts_the_cache_and_model_usage_names_every_model() {
    let init = r#"{"type":"system","subtype":"init","model":"claude-sonnet-5-5","claude_code_version":"2.1.273"}"#;
    let result = r#"{"type":"result","is_error":false,"num_turns":6,"total_cost_usd":1.3368094,
        "usage":{"input_tokens":12,"cache_creation_input_tokens":286414,"cache_read_input_tokens":791447,"output_tokens":3284},
        "modelUsage":{
          "claude-haiku-4-5":{"inputTokens":300,"outputTokens":40,"cacheReadInputTokens":0,"cacheCreationInputTokens":0},
          "claude-sonnet-5-5":{"inputTokens":12,"outputTokens":3284,"cacheReadInputTokens":791447,"cacheCreationInputTokens":286414}}}"#
        .replace('\n', "");
    let report = read(ProcessExit::Exited(0), &format!("{init}\n{result}\n"));
    assert_eq!(report.tokens_in, Some(312 + 791_447 + 286_414));
    assert_eq!(report.tokens_out, Some(3_324));
    assert_eq!(
        report.usage_detail,
        UsageDetail {
            cache_read_tokens: Some(791_447),
            cache_write_tokens: Some(286_414),
            model_calls: Some(6),
            models: vec!["claude-sonnet-5-5".into(), "claude-haiku-4-5".into()],
            by_model: BTreeMap::from([
                (
                    "claude-haiku-4-5".to_owned(),
                    ModelUsage {
                        input: 300,
                        output: 40,
                        cache_read: Some(0),
                        cache_write: Some(0),
                    }
                ),
                (
                    "claude-sonnet-5-5".to_owned(),
                    ModelUsage {
                        input: 12,
                        output: 3_284,
                        cache_read: Some(791_447),
                        cache_write: Some(286_414),
                    }
                ),
            ]),
        }
    );

    // Without `modelUsage`, `usage` carries the same breakdown.
    let plain = r#"{"type":"result","is_error":false,"num_turns":2,"usage":{"input_tokens":3,"cache_read_input_tokens":100,"output_tokens":9}}"#;
    let report = read(ProcessExit::Exited(0), &format!("{init}\n{plain}\n"));
    assert_eq!((report.tokens_in, report.tokens_out), (Some(103), Some(9)));
    assert_eq!(report.usage_detail.cache_write_tokens, None);
    assert_eq!(
        report.usage_detail.models,
        vec!["claude-sonnet-5-5".to_owned()]
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
        let detail = report.terminal_reason["detail"].as_str().expect("detail");
        assert!(detail.contains("no structured result envelope"), "{detail}");
        if matches!(exit, ProcessExit::Exited(7)) {
            assert_eq!(report.terminal_reason["reason"], "exit_status");
        }
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

/// The auto-memory index reaches the prompt past `--setting-sources ""`;
/// `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` removes it (measured on 2.1.302).
#[test]
fn the_auto_memory_index_is_switched_off() {
    let invocation = invocation(&request(&["Read"], true)).expect("invocation");
    assert_eq!(
        invocation
            .env
            .get("CLAUDE_CODE_DISABLE_AUTO_MEMORY")
            .map(String::as_str),
        Some("1")
    );
}

/// The run's own tool list wins; a blank one takes the profile's for this agent
/// (`tool_policy.tools["claude-code"]`); a profile without it leaves the
/// command line exactly as it was.
#[test]
fn a_profiles_tools_fill_a_run_that_names_none() {
    let tools_arg = |tools: &[&str], network: bool, policy: serde_json::Value| {
        let mut spec = request(tools, network);
        spec.work.request.resolved_agent_profile.tool_policy = policy;
        invocation(&spec).map(|built| built.args)
    };
    let value_of = |args: &[String], flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .map(|at| args[at + 1].clone())
    };
    let profile = |tools: serde_json::Value| serde_json::json!({"tools": {"claude-code": tools}});

    let from_profile =
        tools_arg(&[], true, profile(serde_json::json!(["Read", "Grep"]))).expect("invocation");
    assert_eq!(
        value_of(&from_profile, "--tools").as_deref(),
        Some("Read,Grep")
    );

    let run_wins = tools_arg(
        &["Edit"],
        true,
        profile(serde_json::json!(["Read", "Grep"])),
    )
    .expect("invocation");
    assert_eq!(value_of(&run_wins, "--tools").as_deref(), Some("Edit"));

    // Another agent's tools, or none, are not Claude Code's.
    for policy in [
        serde_json::json!({}),
        serde_json::json!({"tools": {"codex": ["Read"]}}),
        profile(serde_json::json!("Read")),
    ] {
        let args = tools_arg(&[], true, policy.clone()).expect("invocation");
        let untouched = invocation(&request(&[], true)).expect("invocation").args;
        assert_eq!(args, untouched, "{policy}");
        assert_eq!(value_of(&args, "--tools").as_deref(), Some(""), "{policy}");
    }

    // `*` is every tool, and a run without network still drops the web ones.
    let all = tools_arg(&[], false, profile(serde_json::json!(["*"]))).expect("invocation");
    assert_eq!(
        value_of(&all, "--tools").as_deref(),
        Some("Bash,Edit,Glob,Grep,Read,Write")
    );
    // A profile that names a web tool for a run without network is refused as a run's would be.
    assert!(matches!(
        tools_arg(&[], false, profile(serde_json::json!(["WebFetch"]))),
        Err(HarnessError::Rejected { .. })
    ));
}

/// `live-usage.jsonl` (2.1.302) read line by line the way `wait` reads it: each API call
/// counts once though its thinking block and its tool_use repeat the message id, and the
/// calls' counted tokens (input + cache writes + output) add up to the running sum.
#[test]
fn live_usage_counts_each_call_once_and_the_result_settles_the_attempt() {
    use crate::harness::local_process::live_usage_step;

    let transcript = include_str!("../fixtures/claude_code/2.1.302/live-usage.jsonl");
    let mut calls = std::collections::BTreeSet::new();
    let steps: Vec<Usage> = transcript
        .lines()
        .filter_map(|line| live_usage_step(&ClaudeCodeGrammar, line, &mut calls))
        .map(|signal| match signal {
            StreamSignal::Progress(step) => step.usage.expect("a call's usage"),
            other => panic!("expected progress, got {other:?}"),
        })
        .collect();

    // Four calls in five `assistant` lines: (2 + 1 666 + 16), (2 + 1 104 + 9), (2 + 1 227 + 16), (2 + 1 104 + 58).
    let counted: Vec<u64> = steps.iter().filter_map(Usage::counted).collect();
    assert_eq!(counted, [1_684, 1_115, 1_245, 1_164]);
    let running: Vec<u64> = counted
        .iter()
        .scan(0, |sum, call| {
            *sum += call;
            Some(*sum)
        })
        .collect();
    assert_eq!(running, [1_684, 2_799, 4_044, 5_208]);

    // `tokens_in` holds the cache reads and writes, as it does for a finished run.
    assert_eq!(steps[0].tokens_in.value, Some(2 + 3_223 + 1_666));
    assert_eq!(steps[0].additional["cache_read_tokens"], 3_223);
    assert_eq!(steps[0].additional["cache_write_tokens"], 1_666);

    // The result line is the attempt's settled figure: its output (400) is not the live sum's (99).
    let settled = read(ProcessExit::Exited(0), transcript);
    assert_eq!(settled.tokens_out, Some(400));
    assert_eq!(settled.tokens_in, Some(8 + 21_325 + 5_101));
    let live_output: u64 = steps.iter().filter_map(|step| step.tokens_out.value).sum();
    assert_eq!(live_output, 99);

    // A line that is not an `assistant` call says nothing.
    assert_eq!(ClaudeCodeGrammar.live_usage("{\"type\":\"user\"}"), None);
    assert_eq!(ClaudeCodeGrammar.live_usage("not json"), None);
}
