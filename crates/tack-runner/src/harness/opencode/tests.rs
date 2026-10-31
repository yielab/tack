//! opencode's own claims: how its event stream becomes a
//! report. Fixtures are captured transcripts under `fixtures/opencode/`. The
//! lifecycle is proved in `local_process/tests.rs`.

use super::*;
use crate::harness::process::ProcessExit;
use crate::harness::test_support::{finished, scratch, spec};

/// The contract's own request, with `network` granted — every real
/// invocation needs it, since `invocation()` refuses the contract's own
/// default (`false`) outright.
fn networked(state: &std::path::Path) -> crate::harness::ExecutionSpec {
    let mut request = spec(DESCRIPTOR.kind, state);
    request.work.request.permission_policy.network = true;
    request
}

#[test]
fn opencode_declares_decisions_supported_over_acp() {
    let declared = OpencodeGrammar.capabilities();
    assert_eq!(declared.decisions.support, CapabilitySupport::Supported);
    let reason = declared.decisions.reason.as_deref().unwrap_or_default();
    assert!(reason.contains("acp"), "{reason}");
    assert!(reason.contains("session/request_permission"), "{reason}");
}

fn read(stdout: &str, exit: ProcessExit) -> RunReport {
    let state = scratch("opencode-report");
    let request = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &request,
        scratch: state.path(),
        program: None,
        host: &crate::config::ProcessEnv::default(),
    };
    OpencodeGrammar.report(&run, &finished(exit, stdout))
}

/// `(fixture, exit, succeeded, tokens in, tokens out)`. `tool_denied` sums
/// tokens across both its `step_finish` events; `cut_short` has none.
/// `served_model` proves the served-model measurement itself: the fake
/// server answered every chunk's `model` as `served-model-xyz` while the
/// request named `tack/requested-model`, and `observed_model` still reads
/// `None` below.
#[test]
fn events_become_a_report_over_captured_fixtures() {
    let completed = include_str!("../fixtures/opencode/1.18.30/completed.ndjson");
    let tool_denied = include_str!("../fixtures/opencode/1.18.30/tool_denied.ndjson");
    let cut_short = include_str!("../fixtures/opencode/1.18.30/cut_short.ndjson");
    let served_model = include_str!("../fixtures/opencode/1.18.30/served_model.ndjson");
    let rows = [
        (completed, ProcessExit::Exited(0), true, 7, 3),
        (tool_denied, ProcessExit::Exited(0), true, 14, 6),
        (cut_short, ProcessExit::Exited(143), false, 0, 0),
        (served_model, ProcessExit::Exited(0), true, 16, 7),
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

/// The `[t=…] STDOUT: ` prefix a captured-conversation fixture uses, stripped
/// so the JSON itself is what a test reads. `SEND:`/`AUTO-RESPOND:` lines
/// are a different marker and never match.
fn stdout_json_lines(fixture: &str) -> Vec<&str> {
    fixture
        .lines()
        .filter_map(|line| line.split_once("STDOUT: ").map(|(_, json)| json))
        .collect()
}

/// The fixture's own `/* elided: ... */` comment — present on exactly one
/// captured line, marking data the capture session left out for a human
/// reader — is not valid JSON and never appears in a real vendor line;
/// stripped here so that one line still parses like every other.
fn strip_elided_comment(line: &str) -> std::borrow::Cow<'_, str> {
    match (line.find("/*"), line.find("*/")) {
        (Some(start), Some(end)) if end > start => {
            let mut owned = String::with_capacity(line.len());
            owned.push_str(&line[..start]);
            owned.push_str(&line[end + 2..]);
            std::borrow::Cow::Owned(owned)
        }
        _ => std::borrow::Cow::Borrowed(line),
    }
}

/// The one line of `fixture` whose parsed JSON satisfies `matches`.
fn find_json<'a>(lines: &[&'a str], matches: impl Fn(&Value) -> bool) -> &'a str {
    lines
        .iter()
        .copied()
        .find(|line| {
            serde_json::from_str::<Value>(&strip_elided_comment(line))
                .map(|value| matches(&value))
                .unwrap_or(false)
        })
        .expect("a line matching the predicate")
}

/// `prompt()`'s handshake bytes, then every line of `asking_acp.txt`'s own
/// captured conversation read back through `signal()`: the id-1 and id-2
/// results drive the rest of the handshake from their own replies, the
/// `session/request_permission` line becomes a question, the id-3 result
/// ends the run, and a `session/update` notification means nothing here.
#[test]
fn an_ask_request_becomes_an_acp_conversation() {
    let state = scratch("opencode-ask-conversation");
    let request = networked(state.path());
    let run = RunContext {
        spec: &request,
        scratch: state.path(),
        program: None,
        host: &crate::config::ProcessEnv::default(),
    };

    let framed = OpencodeGrammar.prompt("do it".to_owned(), true);
    let message: Value = serde_json::from_slice(&framed).expect("one JSON line");
    assert_eq!(message["method"], "initialize");
    assert_eq!(message["id"], 1);
    assert_eq!(message["params"]["protocolVersion"], 1);
    assert_eq!(framed.last(), Some(&b'\n'));
    let plain = OpencodeGrammar.prompt("do it".to_owned(), false);
    assert_eq!(plain, b"do it");

    let fixture = include_str!("../fixtures/opencode/1.18.30/asking_acp.txt");
    let lines = stdout_json_lines(fixture);

    let reply_bytes = |signal: StreamSignal| -> Value {
        let StreamSignal::Reply(bytes) = signal else {
            panic!("expected a reply, got {signal:?}");
        };
        serde_json::from_slice(&bytes).expect("valid json")
    };

    // the id-1 result -> the session/new request.
    let id1_result = find_json(&lines, |v| v.get("id").and_then(Value::as_i64) == Some(1));
    let got = reply_bytes(OpencodeGrammar.signal(&run, id1_result).expect("a reply"));
    assert_eq!(
        got,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "session/new",
            "params": {
                "cwd": working_directory(&run).expect("a confined cwd").display().to_string(),
                "mcpServers": [],
            },
        })
    );

    // the id-2 result -> the session/prompt request, carrying its session
    // id and this request's own prompt. The fixture's own line elides its
    // built-in model catalog with a `/* ... */` comment for a human
    // reader; stripped so this parses like the real vendor line would.
    let id2_result = find_json(&lines, |v| v.get("id").and_then(Value::as_i64) == Some(2));
    let id2_result = strip_elided_comment(id2_result);
    let got = reply_bytes(OpencodeGrammar.signal(&run, &id2_result).expect("a reply"));
    assert_eq!(
        got,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "session/prompt",
            "params": {
                "sessionId": "ses_f3eab904affeINRQ0ml40TzJph",
                "prompt": [{"type": "text", "text": request.work.request.resolved_agent_profile.instructions}],
            },
        })
    );

    // session/request_permission -> a question.
    let permission_line = find_json(&lines, |v| {
        v.get("method").and_then(Value::as_str) == Some("session/request_permission")
    });
    let signal = OpencodeGrammar
        .signal(&run, permission_line)
        .expect("a question");
    let StreamSignal::Question(question) = signal else {
        panic!("expected a question, got {signal:?}");
    };
    assert_eq!(question.vendor_id, "0");
    assert_eq!(question.kind, "tool_permission");
    assert!(question.prompt.contains("echo hello-from-bash-tool"));
    assert_eq!(
        question
            .options
            .iter()
            .map(|option| option.option_id.as_str())
            .collect::<Vec<_>>(),
        ["once", "reject"]
    );
    assert_eq!(question.metadata["tool_call_id"], "call_1");
    assert_eq!(question.metadata["kind"], "execute");

    // the id-3 result -> Finished.
    let id3_result = find_json(&lines, |v| v.get("id").and_then(Value::as_i64) == Some(3));
    assert_eq!(
        OpencodeGrammar.signal(&run, id3_result),
        Some(StreamSignal::Finished)
    );

    // a session/update notification -> nothing.
    let update_line = find_json(&lines, |v| {
        v.get("method").and_then(Value::as_str) == Some("session/update")
    });
    assert_eq!(OpencodeGrammar.signal(&run, update_line), None);
}

/// `once` writes the fixture's own AUTO-RESPOND line, `id` a JSON number;
/// anything else, including an answer this grammar never offered, rejects.
#[test]
fn an_answer_becomes_a_permission_outcome() {
    let question = Question {
        vendor_id: "0".to_owned(),
        kind: "tool_permission".to_owned(),
        prompt: "Allow echo hello-from-bash-tool?".to_owned(),
        options: vec![
            DecisionOption {
                option_id: "once".to_owned(),
                label: "Allow once".to_owned(),
                description: None,
                risks: None,
                estimated_tokens: None,
            },
            DecisionOption {
                option_id: "reject".to_owned(),
                label: "Deny".to_owned(),
                description: None,
                risks: None,
                estimated_tokens: None,
            },
        ],
        metadata: serde_json::Map::new(),
        recommendation: None,
    };

    let fixture = include_str!("../fixtures/opencode/1.18.30/asking_acp.txt");
    let auto_respond = fixture
        .lines()
        .find_map(|line| line.split_once("AUTO-RESPOND: ").map(|(_, json)| json))
        .expect("the fixture's own auto-respond line");
    let expected: Value = serde_json::from_str(auto_respond).expect("fixture json");
    assert!(expected["id"].is_number());

    let allow = DecisionAnswer {
        option_id: Some("once".to_owned()),
        text: None,
    };
    let bytes = OpencodeGrammar.answer(&question, &allow);
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).expect("valid json"),
        expected
    );

    for option_id in ["reject", "an-option-this-grammar-never-offered"] {
        let deny = DecisionAnswer {
            option_id: Some(option_id.to_owned()),
            text: None,
        };
        let bytes = OpencodeGrammar.answer(&question, &deny);
        let got: Value = serde_json::from_slice(&bytes).expect("valid json");
        assert_eq!(
            got["result"]["outcome"]["optionId"], "reject",
            "{option_id}"
        );
        assert_eq!(got["id"], 0);
    }
}

/// The id-3 result decides the verdict and usage over the same captured
/// conversation; a `stopReason` other than `end_turn`, or a JSON-RPC
/// `error` on id 1-3, fails the run.
#[test]
fn acp_events_become_a_report_over_a_captured_conversation() {
    let fixture = include_str!("../fixtures/opencode/1.18.30/asking_acp.txt");
    let lines = stdout_json_lines(fixture);
    let id3_result = find_json(&lines, |v| v.get("id").and_then(Value::as_i64) == Some(3));

    let report = read(id3_result, ProcessExit::Exited(0));
    assert!(report.succeeded, "{:?}", report.terminal_reason);
    assert_eq!((report.tokens_in, report.tokens_out), (Some(5), Some(2)));

    let cancelled = id3_result.replace("\"end_turn\"", "\"cancelled\"");
    let report = read(&cancelled, ProcessExit::Exited(0));
    assert!(!report.succeeded, "{:?}", report.terminal_reason);

    let errored = read(
        r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32000,"message":"boom"}}"#,
        ProcessExit::Exited(1),
    );
    assert!(!errored.succeeded);
    assert_eq!(errored.terminal_reason["message"], "boom");
}

/// `tokens.input` leaves the cache reads out (opencode 1.18.30): `tokens_in` adds them and the
/// writes back and names them, so the counted tokens are input + cache writes + output. The
/// same steps read live, one per `step_finish`, add up to the finished run's figure.
#[test]
fn cache_reads_are_added_back_to_the_input_and_named() {
    use crate::harness::local_process::live_usage_step;

    let transcript = include_str!("../fixtures/opencode/1.18.30/cache_read.ndjson");
    let report = read(transcript, ProcessExit::Exited(0));
    assert!(report.succeeded);
    // (201 + 800) + (202 + 800) in; 7 + 7 out.
    assert_eq!(report.tokens_in, Some(2_003));
    assert_eq!(report.tokens_out, Some(14));
    assert_eq!(report.usage_detail.cache_read_tokens, Some(1_600));
    assert_eq!(report.usage_detail.cache_write_tokens, Some(0));

    let mut calls = std::collections::BTreeSet::new();
    let steps: Vec<Usage> = transcript
        .lines()
        .filter_map(|line| live_usage_step(&OpencodeGrammar, line, &mut calls))
        .map(|signal| match signal {
            StreamSignal::Progress(step) => step.usage.expect("a step's usage"),
            other => panic!("expected progress, got {other:?}"),
        })
        .collect();
    let counted: Vec<u64> = steps.iter().filter_map(Usage::counted).collect();
    assert_eq!(counted, [208, 209]);
    assert_eq!(steps[0].tokens_in.value, Some(1_001));
    assert_eq!(steps[0].additional["cache_read_tokens"], 800);

    // A step is counted once however often its line is read.
    let first = transcript
        .lines()
        .find(|line| line.contains("step_finish"))
        .expect("a step_finish line");
    assert!(live_usage_step(&OpencodeGrammar, first, &mut calls).is_none());
    assert_eq!(OpencodeGrammar.live_usage("{\"type\":\"text\"}"), None);
}

/// An expired key prints one `error` event, no `step_finish`, and exits 0 (1.18.30). The
/// attempt fails with the error's own words, not as malformed output.
#[test]
fn an_error_event_fails_the_attempt_with_its_message() {
    let transcript = include_str!("../fixtures/opencode/1.18.30/api_key_expired.ndjson");
    let report = read(transcript, ProcessExit::Exited(0));
    assert!(!report.succeeded);
    assert_eq!(report.terminal_reason["reason"], "error");
    let message = report.terminal_reason["message"].as_str().expect("message");
    assert!(
        message.starts_with("This API key has expired."),
        "{message}"
    );
    assert!(message.chars().count() <= ERROR_MESSAGE_CHARS);

    // Bounded, and named by the error when it has no message.
    let long = format!(
        "{{\"type\":\"error\",\"error\":{{\"name\":\"APIError\",\"data\":{{\"message\":\"{}\"}}}}}}",
        "x".repeat(2 * ERROR_MESSAGE_CHARS)
    );
    let bounded = read(&long, ProcessExit::Exited(0));
    assert_eq!(
        bounded.terminal_reason["message"]
            .as_str()
            .map(|m| m.chars().count()),
        Some(ERROR_MESSAGE_CHARS)
    );
    let nameless = read(
        "{\"type\":\"error\",\"error\":{\"name\":\"UnknownError\"}}",
        ProcessExit::Exited(0),
    );
    assert_eq!(nameless.terminal_reason["message"], "UnknownError");
}
