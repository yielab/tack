//! Codex's own claims: its command line and how it reads an exit. The
//! lifecycle around them is proved in `local_process/tests.rs`.

use super::*;
use crate::client::AttemptState;
use crate::harness::HarnessAdapter;
use crate::harness::test_support::{
    finished, gateway, harness_for, scratch, script, secret_store, spec,
};
use tack_orch::execution::Approvals;

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
fn a_direct_request_passes_only_the_model() {
    let state = scratch("codex-direct");
    let request = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &request,
        endpoint: None,
        scratch: state.path(),
    };
    let invocation = CodexGrammar.invocation(&run).expect("invocation");
    assert_eq!(
        invocation.args,
        // The fixture request's own tools are `shell`/`filesystem`; `shell`
        // is write-capable, so this lands on `workspace-write`
        // (`the_permission_policy_maps_onto_the_sandbox_flag` covers the
        // rest of the mapping).
        [
            "exec",
            "--json",
            "--sandbox",
            "workspace-write",
            "--model",
            "opaque/model-alpha"
        ]
    );
    assert!(invocation.env.is_empty());
}

/// The overrides are global flags: Codex ignores one placed after `exec`.
#[test]
fn a_configured_endpoint_is_named_before_the_subcommand() {
    let state = scratch("codex-endpoint");
    let request = spec(DESCRIPTOR.kind, state.path());
    let endpoint = endpoint(state.path());
    let run = RunContext {
        spec: &request,
        endpoint: Some(&endpoint),
        scratch: state.path(),
    };
    let args = CodexGrammar.invocation(&run).expect("invocation").args;

    let exec = args.iter().position(|arg| arg == "exec").expect("exec");
    let overrides: Vec<&String> = args[..exec].iter().filter(|arg| *arg != "-c").collect();
    assert_eq!(overrides.len(), 5, "{args:?}");
    assert_eq!(overrides[0], "model_provider=tack_provider");
    let base_url = format!(
        "model_providers.tack_provider.base_url={:?}",
        endpoint.base_url
    );
    assert!(overrides.contains(&&base_url), "{args:?}");
    assert!(
        overrides
            .contains(&&"model_providers.tack_provider.env_key=\"AI_GATEWAY_API_KEY\"".to_owned())
    );
    assert!(
        !args
            .iter()
            .any(|arg| arg.contains(endpoint.credential.expose()))
    );
}

/// The exit decides, never the output: `exec --json`'s shape is unverified,
/// so output that is not JSON at all still succeeds on exit 0.
#[test]
fn the_exit_status_alone_decides_the_verdict() {
    let rows = [
        (ProcessExit::Exited(0), "fake-harness-ok", true, "completed"),
        (
            ProcessExit::Exited(0),
            "{\"incomplete\": \u{1} not-json }}}",
            true,
            "completed",
        ),
        (ProcessExit::Exited(17), "", false, "exit_code"),
        (ProcessExit::TimedOut, "", false, "timed_out"),
        #[cfg(unix)]
        (ProcessExit::Signaled(9), "", false, "signaled"),
    ];
    let state = scratch("codex-report");
    let request = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &request,
        endpoint: None,
        scratch: state.path(),
    };
    for (exit, stdout, succeeded, code) in rows {
        let report = CodexGrammar.report(&run, &finished(exit, stdout));
        assert_eq!(report.succeeded, succeeded, "{exit:?}");
        assert_eq!(report.terminal_reason["code"], code, "{exit:?}");
        assert_eq!(report.terminal_reason["stdout"]["text_preview"], stdout);
        assert_eq!(
            (report.observed_model, report.tokens_in, report.cost_usd),
            (None, None, None)
        );
    }
    let failed = CodexGrammar.report(&run, &finished(ProcessExit::Exited(17), ""));
    assert!(
        failed.terminal_reason["message"]
            .as_str()
            .expect("message")
            .contains("17")
    );
}

#[test]
fn codex_declares_what_it_does_not_enforce() {
    let declared = CodexGrammar.capabilities();
    assert_eq!(declared.cancel.support, CapabilitySupport::Advisory);
    assert_eq!(declared.usage.support, CapabilitySupport::Advisory);
    assert_eq!(declared.decisions.support, CapabilitySupport::Supported);
    assert_eq!(
        declared.additional["permission_policy"]["support"],
        "advisory"
    );
}

/// One row per policy value the mapping cares about: the tool list alone
/// selects the sandbox mode; `approvals` and `network` never change the
/// args (each is compared against the same expected args as the first row,
/// which fixes `tools: []`, `network: false` and `approvals: None`).
#[test]
fn the_permission_policy_maps_onto_the_sandbox_flag() {
    let state = scratch("codex-permission-policy");
    let rows: [(&[&str], bool, Option<Approvals>, &str); 6] = [
        (&[], false, None, "read-only"),
        (&["bash"], false, None, "workspace-write"),
        (&["Edit"], false, None, "workspace-write"),
        (&["read"], false, None, "read-only"),
        (&[], false, Some(Approvals::Auto), "read-only"),
        (&[], true, None, "read-only"),
    ];
    for (tools, network, approvals, sandbox) in rows {
        let mut request = spec(DESCRIPTOR.kind, state.path());
        request.work.request.permission_policy.tools =
            tools.iter().map(|tool| (*tool).to_owned()).collect();
        request.work.request.permission_policy.network = network;
        request.work.request.permission_policy.approvals = approvals;
        let run = RunContext {
            spec: &request,
            endpoint: None,
            scratch: state.path(),
        };
        let args = CodexGrammar.invocation(&run).expect("invocation").args;
        assert_eq!(
            args,
            [
                "exec",
                "--json",
                "--sandbox",
                sandbox,
                "--model",
                "opaque/model-alpha"
            ],
            "{tools:?} network={network} approvals={approvals:?}"
        );
    }
}

/// `(transcript, succeeded, tokens_in, tokens_out)`. Numbers for the real
/// capture are re-measured straight off its own `turn.completed` line
/// (`grep turn.completed exec-tool-call.jsonl`), not trusted from a brief.
/// `no_turn_completed` is the same capture with that line stripped,
/// standing in for a transcript that never reaches one.
#[test]
fn usage_is_read_from_the_turn_completed_line() {
    let exec_tool_call = include_str!("../fixtures/codex/0.149.1/exec-tool-call.jsonl");
    let no_turn_completed: String = exec_tool_call
        .lines()
        .filter(|line| !line.contains("turn.completed"))
        .collect::<Vec<_>>()
        .join("\n");
    let state = scratch("codex-usage");
    let request = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &request,
        endpoint: None,
        scratch: state.path(),
    };
    let rows = [
        (exec_tool_call, true, Some(57), Some(10)),
        (no_turn_completed.as_str(), true, None, None),
    ];
    for (transcript, succeeded, tokens_in, tokens_out) in rows {
        let report = CodexGrammar.report(&run, &finished(ProcessExit::Exited(0), transcript));
        assert_eq!(report.succeeded, succeeded, "{transcript:?}");
        assert_eq!(report.tokens_in, tokens_in, "{transcript:?}");
        assert_eq!(report.tokens_out, tokens_out, "{transcript:?}");
        assert_eq!(report.observed_model, None, "{transcript:?}");
        assert_eq!(report.cost_usd, None, "{transcript:?}");
    }
}

/// The prose transcript `app-server-approval.txt` keeps its two
/// `...`-elided lines readable but not machine-parseable; this pulls the
/// one full JSON line containing `needle`, dropping the leading
/// `stdout: `/`stdin:  ` label.
fn json_line(fixture: &str, needle: &str) -> String {
    let line = fixture
        .lines()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no line containing {needle:?} in fixture"));
    let start = line.find('{').expect("line carries a JSON object");
    line[start..].to_owned()
}

/// The thread id the fixture's own (elided) `id-2` result line carries,
/// read out of `"thread":{"id":"<...>"}` without parsing the line as JSON.
fn thread_id_in(fixture: &str) -> String {
    let marker = "\"thread\":{\"id\":\"";
    let start = fixture.find(marker).expect("thread id present") + marker.len();
    let rest = &fixture[start..];
    let end = rest.find('"').expect("closing quote");
    rest[..end].to_owned()
}

/// `(the args and stdin_stays_open an ask request builds, direct and with an
/// endpoint) -> (prompt()'s initialize line) -> (each stream line ->
/// signal()'s verdict)`, driven off the real `app-server-approval.txt` and
/// `app-server-turn.txt` captures.
#[test]
fn an_ask_request_becomes_an_app_server_conversation() {
    let state = scratch("codex-ask-invocation");
    let mut request = spec(DESCRIPTOR.kind, state.path());
    request.work.request.permission_policy.approvals = Some(Approvals::Ask);
    let run = RunContext {
        spec: &request,
        endpoint: None,
        scratch: state.path(),
    };
    let invocation = CodexGrammar.invocation(&run).expect("invocation");
    assert_eq!(invocation.args, ["app-server"]);
    assert!(invocation.stdin_stays_open);

    let endpoint = endpoint(state.path());
    let run_with_endpoint = RunContext {
        spec: &request,
        endpoint: Some(&endpoint),
        scratch: state.path(),
    };
    let with_endpoint = CodexGrammar
        .invocation(&run_with_endpoint)
        .expect("invocation");
    assert!(with_endpoint.stdin_stays_open);
    assert_eq!(with_endpoint.args.last(), Some(&"app-server".to_owned()));
    let overrides: Vec<&String> = with_endpoint.args[..with_endpoint.args.len() - 1]
        .iter()
        .filter(|arg| *arg != "-c")
        .collect();
    assert_eq!(overrides.len(), 5, "{:?}", with_endpoint.args);
    assert_eq!(overrides[0], "model_provider=tack_provider");

    // `prompt()`'s bytes: framed as the JSON-RPC `initialize` request when
    // stdin stays open, plain text otherwise.
    assert_eq!(
        CodexGrammar.prompt("ignored for an ask run".to_owned(), false),
        b"ignored for an ask run".to_vec()
    );
    let initialize_bytes = CodexGrammar.prompt("ignored for an ask run".to_owned(), true);
    assert_eq!(initialize_bytes.last(), Some(&b'\n'));
    let initialize: serde_json::Value =
        serde_json::from_slice(&initialize_bytes[..initialize_bytes.len() - 1])
            .expect("valid json");
    assert_eq!(
        initialize,
        serde_json::json!({
            "id": 1,
            "method": "initialize",
            "params": {"clientInfo": {"name": "tack", "version": env!("CARGO_PKG_VERSION")}},
        })
    );

    let approval = include_str!("../fixtures/codex/0.149.1/app-server-approval.txt");
    let turn = include_str!("../fixtures/codex/0.149.1/app-server-turn.txt");
    let id1_result = json_line(approval, "\"id\":1,\"result\"");
    let request_approval_line = json_line(approval, "item/commandExecution/requestApproval");
    let thread_id = thread_id_in(approval);
    let id2_result = format!(r#"{{"id":2,"result":{{"thread":{{"id":"{thread_id}"}}}}}}"#);
    let turn_completed_line = turn
        .lines()
        .find(|line| line.contains("turn/completed"))
        .expect("turn/completed line in fixture");

    match CodexGrammar.signal(&run, &id1_result) {
        Some(StreamSignal::Reply(bytes)) => {
            let actual: serde_json::Value = serde_json::from_slice(&bytes).expect("valid json");
            let expected = serde_json::json!({
                "id": 2,
                "method": "thread/start",
                "params": {
                    "cwd": state.path().display().to_string(),
                    "approvalPolicy": "on-request",
                    "sandbox": "workspace-write",
                    "model": "opaque/model-alpha",
                },
            });
            assert_eq!(actual, expected);
        }
        other => panic!("expected Reply for the id-1 result, got {other:?}"),
    }

    match CodexGrammar.signal(&run, &id2_result) {
        Some(StreamSignal::Reply(bytes)) => {
            let actual: serde_json::Value = serde_json::from_slice(&bytes).expect("valid json");
            let expected = serde_json::json!({
                "id": 3,
                "method": "turn/start",
                "params": {
                    "threadId": thread_id,
                    "approvalPolicy": "on-request",
                    "input": [{
                        "type": "text",
                        "text": "Implement the assigned item and run focused tests.",
                    }],
                },
            });
            assert_eq!(actual, expected);
        }
        other => panic!("expected Reply for the id-2 result, got {other:?}"),
    }

    match CodexGrammar.signal(&run, &request_approval_line) {
        Some(StreamSignal::Question(question)) => {
            assert_eq!(question.vendor_id, "0");
            assert_eq!(question.kind, "tool_permission");
            assert_eq!(
                question.prompt,
                "Allow command: /usr/bin/zsh -lc 'touch escalated-write.txt'? \
                 (need to write a file)"
            );
            assert_eq!(
                question.options,
                vec![
                    DecisionOption {
                        option_id: "accept".to_owned(),
                        label: "Allow once".to_owned(),
                    },
                    DecisionOption {
                        option_id: "decline".to_owned(),
                        label: "Deny".to_owned(),
                    },
                ]
            );
            assert_eq!(
                question.metadata["command"],
                "/usr/bin/zsh -lc 'touch escalated-write.txt'"
            );
            assert_eq!(question.metadata["cwd"], "/capture/wsrepo");
            assert_eq!(question.metadata["reason"], "need to write a file");
            assert_eq!(question.metadata["item_id"], "call_1");
            assert_eq!(
                question.metadata["turn_id"],
                "01a0c150-f680-75a0-b23c-91710faa7add"
            );
        }
        other => panic!("expected Question, got {other:?}"),
    }

    assert!(matches!(
        CodexGrammar.signal(&run, turn_completed_line),
        Some(StreamSignal::Finished)
    ));
    assert_eq!(
        CodexGrammar.signal(
            &run,
            r#"{"method":"item/started","params":{"item":{"id":"call_1"}}}"#
        ),
        None
    );
}

#[test]
fn an_answer_becomes_a_decision() {
    let question = Question {
        vendor_id: "0".to_owned(),
        kind: "tool_permission".to_owned(),
        prompt: "Allow command: touch x?".to_owned(),
        options: vec![
            DecisionOption {
                option_id: "accept".to_owned(),
                label: "Allow once".to_owned(),
            },
            DecisionOption {
                option_id: "decline".to_owned(),
                label: "Deny".to_owned(),
            },
        ],
        metadata: serde_json::Map::new(),
    };
    let rows = [
        (Some("accept"), "accept"),
        (Some("decline"), "decline"),
        (Some("an-option-never-offered"), "decline"),
    ];
    for (option_id, decision) in rows {
        let answer = DecisionAnswer {
            option_id: option_id.map(str::to_owned),
            text: None,
        };
        let bytes = CodexGrammar.answer(&question, &answer);
        let actual: serde_json::Value = serde_json::from_slice(&bytes).expect("valid json");
        assert_eq!(
            actual,
            serde_json::json!({"id": 0, "result": {"decision": decision}}),
            "{option_id:?}"
        );
        assert!(actual["id"].is_number(), "{option_id:?}: {actual:?}");
    }
}

/// `(stdout, exit) -> (succeeded, tokens_in, tokens_out)`, over the real
/// `app-server-turn.txt` capture and two synthetic variants of it: the same
/// turn marked `"failed"`, and a bare JSON-RPC error answering `turn/start`.
#[test]
fn the_app_server_stream_decides_the_verdict_and_usage() {
    let turn = include_str!("../fixtures/codex/0.149.1/app-server-turn.txt");
    let failed_turn = turn.replace("\"status\":\"completed\"", "\"status\":\"failed\"");
    let state = scratch("codex-appserver-report");
    let request = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &request,
        endpoint: None,
        scratch: state.path(),
    };

    let succeeded = CodexGrammar.report(&run, &finished(ProcessExit::Exited(0), turn));
    assert!(succeeded.succeeded, "{succeeded:?}");
    assert_eq!(succeeded.tokens_in, Some(57));
    assert_eq!(succeeded.tokens_out, Some(10));
    assert_eq!(succeeded.observed_model, None);

    let failed = CodexGrammar.report(&run, &finished(ProcessExit::Exited(0), &failed_turn));
    assert!(!failed.succeeded, "{failed:?}");

    let error_line =
        r#"{"id":3,"error":{"code":-32000,"message":"turn/start failed: bad thread id"}}"#;
    let errored = CodexGrammar.report(&run, &finished(ProcessExit::Exited(0), error_line));
    assert!(!errored.succeeded, "{errored:?}");
    assert!(
        errored.terminal_reason["message"]
            .as_str()
            .expect("message")
            .contains("bad thread id"),
        "{:?}",
        errored.terminal_reason
    );
}

/// Drives `CodexGrammar` through the real core (`start`, `decision_channels`,
/// `wait`) against a `/bin/sh` shim that replays the `app-server` protocol:
/// `initialize` -> `thread/start` -> `turn/start` -> one
/// `requestApproval` question -> `turn/completed`.
#[tokio::test]
async fn the_core_drives_codex_through_a_shim_that_asks() {
    let state = scratch("codex-ask-shim");
    let thread_id = "01a0c150-f620-7352-a5a0-bc78f70024c3";
    let body = format!(
        r#"read -r _initialize
echo '{{"id":1,"result":{{"userAgent":"tack-measure"}}}}'
read -r _thread_start
echo '{{"id":2,"result":{{"thread":{{"id":"{thread_id}"}}}}}}'
read -r _turn_start
echo '{{"method":"item/commandExecution/requestApproval","id":0,"params":{{"threadId":"{thread_id}","turnId":"turn-1","itemId":"call_1","reason":"need to write a file","command":"touch escalated-write.txt","cwd":"/workspace"}}}}'
read -r answer
echo "GOT:$answer"
echo '{{"method":"turn/completed","params":{{"turn":{{"status":"completed"}}}}}}'
exit 0"#
    );
    let harness = harness_for(CodexGrammar, script(state.path(), &body), state.path());
    let mut request = spec(DESCRIPTOR.kind, state.path());
    request.work.request.permission_policy.approvals = Some(Approvals::Ask);

    let handle = harness.start(&request).await.expect("start");
    let (mut questions_rx, answers_tx) = harness
        .decision_channels(&handle)
        .await
        .expect("an ask request exposes decision channels");

    let drive = async {
        let question = questions_rx.recv().await.expect("question");
        assert!(
            question.prompt.contains("touch escalated-write.txt"),
            "{question:?}"
        );
        let answer = DecisionAnswer {
            option_id: Some("accept".to_owned()),
            text: None,
        };
        let _ = answers_tx.send(answer).await;
    };
    let (outcome, ()) = tokio::join!(harness.wait(&handle), drive);
    let outcome = outcome.expect("wait");
    let stdout = outcome.terminal_reason["stdout"]["text_preview"]
        .as_str()
        .unwrap_or_default();
    assert!(
        stdout.contains("GOT:{\"id\":0,\"result\":{\"decision\":\"accept\"}}"),
        "{:?}",
        outcome.terminal_reason
    );
    assert_eq!(outcome.terminal_state, AttemptState::Succeeded);
}
