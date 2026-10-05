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
use tack_orch::execution::{Approvals, CapabilityValue, RequestedModelId, RequestedModelProvider};

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
    let scratch_dir = scratch("docket-invocation-scratch");
    let endpoint = endpoint(state.path());
    let with_endpoint = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &with_endpoint,
        endpoint: Some(&endpoint),
        scratch: scratch_dir.path(),
    };
    let invocation = invocation(&DocketFeatures::default(), &run).expect("invocation");
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
    let docket_home = &invocation.env["DOCKET_HOME"];
    assert!(docket_home.starts_with(&scratch_dir.path().display().to_string()));
    assert!(!docket_home.starts_with(&state.path().display().to_string()));
    assert!(docket_home.ends_with("docket-home"));

    let mut own_env = spec(DESCRIPTOR.kind, state.path());
    set_env(&mut own_env, &[(BASE_URL_ENV, "http://127.0.0.1:9/v1")]);
    let run = RunContext {
        spec: &own_env,
        endpoint: None,
        scratch: scratch_dir.path(),
    };
    let invocation = super::invocation(&DocketFeatures::default(), &run).expect("invocation");
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
        scratch: state.path(),
    };
    let error = invocation(&DocketFeatures::default(), &run)
        .expect_err("no endpoint and no request-level base url");
    assert!(matches!(&error, HarnessError::Rejected { reason } if reason.contains(BASE_URL_ENV)));
}

#[test]
fn contract_1_1_passes_the_task_as_a_file() {
    let state = scratch("docket-invocation-1-1");
    let scratch_dir = scratch("docket-invocation-1-1-scratch");
    let endpoint = endpoint(state.path());
    let request = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &request,
        endpoint: Some(&endpoint),
        scratch: scratch_dir.path(),
    };
    let features = DocketFeatures {
        contract: Contract::V1_1,
        ..DocketFeatures::default()
    };
    let task_file = scratch_dir.path().join("task.md");
    let invocation = invocation(&features, &run).expect("invocation");
    assert!(!invocation.stdin_stays_open);
    assert_eq!(
        invocation.args,
        [
            "harness",
            "run",
            "--workspace",
            &state.path().display().to_string(),
            "--task-file",
            &task_file.display().to_string(),
            "--model",
            "openai/opaque/model-alpha",
            "--agent-id",
            "attempt",
            "--timeout",
            "30",
            "--contract",
            "1.1",
        ]
    );
    assert_eq!(
        std::fs::read_to_string(task_file).expect("task file"),
        request.work.request.resolved_agent_profile.instructions
    );
}

#[test]
fn capability_lines_follow_what_was_negotiated() {
    let policy_line = |features: &FeatureCapabilities| {
        features.additional["permission_policy"]["reason"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    let unprobed = capabilities(&DocketFeatures::default());
    let lines = [
        (
            &unprobed.resume.reason.clone().unwrap_or_default(),
            "harness mode is one synchronous run to completion; no reattachment interface is documented or observed",
        ),
        (
            &unprobed.decisions.reason.clone().unwrap_or_default(),
            "harness mode's approval posture is fixed to non-interactive refusal: a tool call that would otherwise wait for a human is denied immediately as `blocked` rather than pausing the run to ask one",
        ),
        (
            &unprobed.artifacts.reason.clone().unwrap_or_default(),
            "this docket does not report the files a run wrote, so only the captured stdout/stderr is staged as a log",
        ),
        (
            &unprobed.usage.reason.clone().unwrap_or_default(),
            "the result line's usage.input_tokens/output_tokens are real measurements, but cost_usd is always null on the installed version, so cost is never reported",
        ),
        (
            &policy_line(&unprobed),
            "this docket does not accept --policy, so the request's tool list and network flag are not passed to it",
        ),
    ];
    for (line, expected) in lines {
        assert_eq!(line, expected);
    }

    let negotiated = capabilities(&DocketFeatures {
        contract: Contract::V1_1,
        answers: true,
        token_file: true,
        max_tokens: true,
        policy: true,
        version: "9.9.9".to_owned(),
        ..DocketFeatures::default()
    });
    let reason = |line: &CapabilityValue| line.reason.clone().unwrap_or_default();
    let policy = policy_line(&negotiated);
    assert_eq!(negotiated.decisions.support, CapabilitySupport::Supported);
    assert_eq!(unprobed.decisions.support, CapabilitySupport::Unsupported);
    assert!(reason(&negotiated.decisions).contains("--answers"));
    assert!(reason(&negotiated.decisions).contains("negotiated contract 1.1 against docket 9.9.9"));
    assert_eq!(negotiated.artifacts.support, CapabilitySupport::Supported);
    assert_eq!(unprobed.artifacts.support, CapabilitySupport::Unsupported);
    // A docket that speaks 1.1 but predates the files it reports.
    let early = capabilities(&DocketFeatures {
        contract: Contract::V1_1,
        ..DocketFeatures::default()
    });
    assert_eq!(early.artifacts.support, CapabilitySupport::Unsupported);
    assert!(reason(&negotiated.artifacts).contains("1.1 result"));
    assert!(reason(&negotiated.usage).contains("--max-tokens"));
    assert!(policy.contains("--policy"));
    assert_eq!(
        negotiated.additional["permission_policy"]["support"],
        "supported"
    );
    assert_eq!(
        unprobed.additional["permission_policy"]["support"],
        "unsupported"
    );
    assert!(policy.contains("negotiated contract 1.1 against docket 9.9.9"));
    // Only a docket that speaks 1.1 reports its process groups.
    assert_eq!(unprobed.cancel.support, CapabilitySupport::Advisory);
    assert_eq!(negotiated.cancel.support, CapabilitySupport::Supported);
}

/// The captured cancelled run names one group, started then gone, and ends
/// on its result line; no other line of it means anything to the core.
#[test]
fn process_lines_become_group_signals() {
    let state = scratch("docket-signal");
    let request = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &request,
        endpoint: None,
        scratch: state.path(),
    };
    let signals: Vec<_> = include_str!("../fixtures/docket/contract-1.1/cancelled-process.ndjson")
        .lines()
        .filter_map(|line| DocketGrammar.signal(&run, line))
        .collect();
    assert_eq!(
        signals,
        [
            StreamSignal::ProcessStarted { pgid: 2_169_014 },
            StreamSignal::ProcessExited { pgid: 2_169_014 },
            StreamSignal::Finished,
        ]
    );
}

/// `ask` on a docket that offers `--answers` keeps stdin open for the answer
/// lines; without the flag, or for any other posture, the refusal posture
/// is untouched.
#[test]
fn ask_drives_the_answer_channel_only_where_docket_offers_it() {
    let state = scratch("docket-invocation-ask");
    let scratch_dir = scratch("docket-invocation-ask-scratch");
    let endpoint = endpoint(state.path());
    let mut request = spec(DESCRIPTOR.kind, state.path());
    let offered = DocketFeatures {
        contract: Contract::V1_1,
        answers: true,
        ..DocketFeatures::default()
    };
    let rows = [
        (Some(Approvals::Ask), &offered, true),
        (Some(Approvals::Ask), &DocketFeatures::default(), false),
        (None, &offered, false),
    ];
    for (approvals, features, asks) in rows {
        request.work.request.permission_policy.approvals = approvals;
        let run = RunContext {
            spec: &request,
            endpoint: Some(&endpoint),
            scratch: scratch_dir.path(),
        };
        let invocation = invocation(features, &run).expect("invocation");
        assert_eq!(invocation.stdin_stays_open, asks, "{approvals:?}");
        assert_eq!(
            invocation
                .args
                .ends_with(&["--answers".to_owned(), "stdin".to_owned()]),
            asks,
            "{approvals:?}"
        );
    }
}

/// The captured asked-and-answered run names one gated call: its
/// `approval_requested` event is the only question, the result line the only
/// end, and an answer is one exact `AnswerLine`.
#[test]
fn an_approval_request_becomes_a_question_and_its_answer_is_one_line() {
    let state = scratch("docket-ask-signal");
    let request = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &request,
        endpoint: None,
        scratch: state.path(),
    };
    let signals: Vec<_> = include_str!("../fixtures/docket/contract-1.1/asked-answered.ndjson")
        .lines()
        .filter_map(|line| DocketGrammar.signal(&run, line))
        .collect();
    let [
        StreamSignal::Question(question),
        StreamSignal::ProcessStarted { .. },
        StreamSignal::ProcessExited { .. },
        StreamSignal::Finished,
    ] = signals.as_slice()
    else {
        panic!("{signals:?}");
    };
    assert_eq!(
        question.vendor_id,
        "apr-957ee731-4440-4419-b4e0-da5b18fabf67"
    );
    assert_eq!(question.kind, "tool_permission");
    assert_eq!(question.prompt, "Allow bash (call call-1)?");
    assert_eq!(
        question
            .options
            .iter()
            .map(|o| o.option_id.as_str())
            .collect::<Vec<_>>(),
        ["accept", "decline"]
    );
    assert_eq!(
        question.metadata["token"],
        "run-721a5ebd-30b1-4634-b1b2-197989d99934"
    );
    assert_eq!(question.metadata["tool"], "bash");
    assert_eq!(question.metadata["callId"], "call-1");

    let answer_for = |option: Option<&str>| {
        let answer = DecisionAnswer {
            option_id: option.map(str::to_owned),
            text: None,
        };
        String::from_utf8(DocketGrammar.answer(question, &answer)).expect("utf-8")
    };
    let line = |action: &str| {
        format!(
            "{{\"answer\":{{\"action\":\"{action}\",\"approvalToken\":\"apr-957ee731-4440-4419-b4e0-da5b18fabf67\",\"content\":null}},\"token\":\"run-721a5ebd-30b1-4634-b1b2-197989d99934\",\"v\":\"1.1.0\"}}"
        )
    };
    assert_eq!(answer_for(Some("accept")), line("accept"));
    assert_eq!(answer_for(Some("decline")), line("decline"));
    assert_eq!(answer_for(None), line("decline"));
}

#[test]
fn a_contract_1_1_result_line_is_finished() {
    let report = read(include_str!("../fixtures/docket/contract-1.1/ok.ndjson"));
    assert!(report.succeeded);
    assert_eq!(report.observed_model.as_deref(), Some("anthropic/claude-x"));
    assert_eq!((report.tokens_in, report.tokens_out), (Some(22), Some(10)));

    // The captured `ok-files` run wrote `greeting.txt` and
    // `.tack-runner/state.json`; only the first is the request's own.
    let files = read(include_str!(
        "../fixtures/docket/contract-1.1/ok-files.ndjson"
    ));
    assert_eq!(
        files.terminal_reason["files"],
        serde_json::json!([{"path": "greeting.txt", "op": "write"}])
    );
    assert_eq!(files.terminal_reason["max_tokens"], 5000);
    assert_eq!(
        report.terminal_reason["max_tokens"],
        serde_json::Value::Null
    );

    // A recipe run includes a task block with recipe status and output.
    let recipe = read(include_str!(
        "../fixtures/docket/contract-1.1/recipe-ok.ndjson"
    ));
    assert!(recipe.succeeded);
    // A recipe has no single turn, so docket reports no served model.
    assert_eq!(recipe.observed_model, None);
    assert_eq!((recipe.tokens_in, recipe.tokens_out), (Some(250), Some(50)));
    let task = &recipe.terminal_reason["task"];
    assert_eq!(task["status"], "done");
    let roles = task["hops"]
        .as_array()
        .expect("hops")
        .iter()
        .map(|hop| hop["role"].as_str().expect("role"))
        .collect::<Vec<_>>();
    assert_eq!(roles, ["lead", "researcher", "analyst", "writer", "critic"]);
}

/// One request, `network: false` and two tools, under every probe outcome:
/// the flags are passed only where their probe is true, the policy document
/// is exact, and a request the document cannot express never reaches docket.
#[test]
fn limits_and_policy_are_passed_only_where_probed() {
    let state = scratch("docket-invocation-limits");
    let scratch_dir = scratch("docket-invocation-limits-scratch");
    let endpoint = endpoint(state.path());
    let mut request = spec(DESCRIPTOR.kind, state.path());
    request.work.request.permission_policy.tools = vec!["read".to_owned(), "BASH".to_owned()];
    request.work.request.permission_policy.network = false;
    request.work.request.budgets = serde_json::json!({"tokens": 200_000});
    let token = scratch_dir.path().join("token.json").display().to_string();
    let policy = scratch_dir.path().join("policy.yaml").display().to_string();
    let everything = DocketFeatures {
        contract: Contract::V1_1,
        token_file: true,
        max_tokens: true,
        policy: true,
        ..DocketFeatures::default()
    };
    let without_policy = DocketFeatures {
        policy: false,
        ..everything.clone()
    };
    let flags = |args: Vec<String>| args[12..].to_vec();
    let run = RunContext {
        spec: &request,
        endpoint: Some(&endpoint),
        scratch: scratch_dir.path(),
    };
    let args = |features: &DocketFeatures| flags(invocation(features, &run).expect("args").args);
    assert_eq!(
        args(&everything),
        [
            "--contract",
            "1.1",
            "--token-file",
            &token,
            "--max-tokens",
            "200000",
            "--policy",
            &policy
        ]
    );
    assert_eq!(
        args(&without_policy),
        [
            "--contract",
            "1.1",
            "--token-file",
            &token,
            "--max-tokens",
            "200000"
        ]
    );
    assert!(args(&DocketFeatures::default()).is_empty());
    assert_eq!(
        std::fs::read_to_string(&policy).expect("policy file"),
        "kind: policy\n\
         name: tack-permission-policy\n\
         description: The tools this Tack request does not allow.\n\
         appliesTo:\n  - \"*\"\n\
         on: toolCall\n\
         when:\n  anyOf:\n    - tool: write\n    - tool: edit\n    - tool: glob\n    \
         - tool: grep\n    - tool: fetch\n    - tool: skill\n    - tool: consult\n\
         then: block\n\
         message: this tool is not allowed by the Tack request's permission_policy\n"
    );

    // A budget that is not a positive integer passes no `--max-tokens`.
    for budgets in [serde_json::json!({}), serde_json::json!({"tokens": 0})] {
        request.work.request.budgets = budgets;
        let run = RunContext {
            spec: &request,
            endpoint: Some(&endpoint),
            scratch: scratch_dir.path(),
        };
        let args = invocation(&everything, &run).expect("args").args;
        assert!(!args.contains(&"--max-tokens".to_owned()), "{args:?}");
    }

    // Refused before spawn, naming the field.
    let refusals = [
        (vec!["shell"], false, "permission_policy.tools"),
        (vec!["read", "fetch"], false, "permission_policy.network"),
    ];
    for (tools, network, field) in refusals {
        request.work.request.permission_policy.tools =
            tools.into_iter().map(str::to_owned).collect();
        request.work.request.permission_policy.network = network;
        let run = RunContext {
            spec: &request,
            endpoint: Some(&endpoint),
            scratch: scratch_dir.path(),
        };
        let error = invocation(&everything, &run).expect_err("refused");
        assert!(
            matches!(&error, HarnessError::Rejected { reason } if reason.contains(field)),
            "{error:?}"
        );
        // The same request on a docket with no `--policy` is not refused.
        invocation(&without_policy, &run).expect("nothing is passed");
    }
    request.work.request.permission_policy.tools = Vec::new();
    request.work.request.permission_policy.additional =
        [("sandbox".to_owned(), serde_json::json!(true))].into();
    let run = RunContext {
        spec: &request,
        endpoint: Some(&endpoint),
        scratch: scratch_dir.path(),
    };
    let error = invocation(&everything, &run).expect_err("refused");
    assert!(
        matches!(&error, HarnessError::Rejected { reason } if reason.contains("permission_policy.sandbox"))
    );
}

#[test]
fn recipe_is_passed_only_when_probed_and_specified() {
    let state = scratch("docket-invocation-recipe");
    let scratch_dir = scratch("docket-invocation-recipe-scratch");
    let endpoint = endpoint(state.path());
    let with_recipe = DocketFeatures {
        contract: Contract::V1_1,
        recipe: true,
        ..DocketFeatures::default()
    };
    let without_recipe = DocketFeatures {
        contract: Contract::V1_1,
        recipe: false,
        ..DocketFeatures::default()
    };
    let flags = |args: Vec<String>| args[12..].to_vec();

    // Recipe is passed when probed and specified.
    {
        let mut request = spec(DESCRIPTOR.kind, state.path());
        request.work.request.resolved_agent_profile.tool_policy =
            serde_json::json!({"docket": {"recipe": "my-recipe"}});
        let run = RunContext {
            spec: &request,
            endpoint: Some(&endpoint),
            scratch: scratch_dir.path(),
        };
        let all = |features: &DocketFeatures| invocation(features, &run).expect("args").args;
        // docket refuses `--recipe` with `--agent-id` or `--max-tokens`, so a
        // recipe run carries neither, even with a token budget on the request.
        let recipe_args = all(&with_recipe);
        assert!(!recipe_args.contains(&"--agent-id".to_owned()));
        assert!(!recipe_args.contains(&"--max-tokens".to_owned()));
        assert_eq!(
            recipe_args[8..],
            [
                "--timeout",
                "30",
                "--contract",
                "1.1",
                "--recipe",
                "my-recipe"
            ]
        );
        assert_eq!(flags(all(&without_recipe)), ["--contract", "1.1"]);
        let budgeted = DocketFeatures {
            max_tokens: true,
            ..with_recipe.clone()
        };
        assert!(!all(&budgeted).contains(&"--max-tokens".to_owned()));
    }

    // Recipe is not passed when empty.
    {
        let mut request = spec(DESCRIPTOR.kind, state.path());
        request.work.request.resolved_agent_profile.tool_policy =
            serde_json::json!({"docket": {"recipe": ""}});
        let run = RunContext {
            spec: &request,
            endpoint: Some(&endpoint),
            scratch: scratch_dir.path(),
        };
        let args =
            |features: &DocketFeatures| flags(invocation(features, &run).expect("args").args);
        assert!(!args(&with_recipe).contains(&"--recipe".to_owned()));
    }

    // Recipe is not passed when absent.
    {
        let mut request = spec(DESCRIPTOR.kind, state.path());
        request.work.request.resolved_agent_profile.tool_policy = serde_json::json!({"docket": {}});
        let run = RunContext {
            spec: &request,
            endpoint: Some(&endpoint),
            scratch: scratch_dir.path(),
        };
        let args =
            |features: &DocketFeatures| flags(invocation(features, &run).expect("args").args);
        assert!(!args(&with_recipe).contains(&"--recipe".to_owned()));
    }
}

#[test]
fn the_real_docket_is_negotiated_not_assumed() {
    let Ok(program) = crate::harness::locate::locate_installed(DESCRIPTOR.program) else {
        eprintln!("skipping: `docket` not found on PATH");
        return;
    };
    let found = DocketFeatures::probe(&program);
    assert!(!found.version.is_empty(), "docket --version was not read");
    let state = scratch("docket-negotiated");
    let scratch_dir = scratch("docket-negotiated-scratch");
    let endpoint = endpoint(state.path());
    let request = live_request(state.path());
    let run = RunContext {
        spec: &request,
        endpoint: Some(&endpoint),
        scratch: scratch_dir.path(),
    };
    let args = invocation(&found, &run).expect("invocation").args;
    assert_eq!(
        args.iter().any(|arg| arg == "--contract"),
        found.contract == Contract::V1_1,
        "{found:?} {args:?}"
    );

    // The generated policy is one docket itself accepts: it validates every
    // `--policy` file before any run exists and refuses the call on an
    // invalid one, so a refusal naming the policy is the failure. The
    // endpoint is a closed loopback port, so a valid file ends the run
    // there, with no spend.
    if !found.policy {
        return;
    }
    let policy_path = scratch_dir.path().join("policy.yaml");
    assert!(policy_path.exists(), "{args:?}");
    let invalid = scratch_dir.path().join("invalid.yaml");
    std::fs::write(&invalid, "kind: policy\nthen: block\n").expect("write");
    let refusal = |policy: &std::path::Path| {
        let output = std::process::Command::new(&program)
            .args([
                "harness",
                "run",
                "--workspace",
                ".",
                "--task",
                "x",
                "--model",
                "a/b",
                "--contract",
                "1.1",
                "--policy",
            ])
            .arg(policy)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("DOCKET_HOME", scratch_dir.path().join("docket-home"))
            .env("DOCKET_LLM_BASE_URL", "http://127.0.0.1:9/v1")
            .env("DOCKET_LLM_API_KEY", "not-a-key")
            .output()
            .expect("run docket");
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    assert!(refusal(&invalid).contains("invalid --policy"));
    assert!(!refusal(&policy_path).contains("invalid --policy"));
}

fn read(stdout: &str) -> RunReport {
    let state = scratch("docket-report");
    let request = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &request,
        endpoint: None,
        scratch: state.path(),
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
    // docket's own tool names: the policy document can only name those.
    request.work.request.permission_policy.tools =
        ["read", "write", "edit", "bash"].map(str::to_owned).into();
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

/// Gates one `bash` call (`git push origin production` is a `prod-deploy`
/// action class for docket) on a real docket that offers `--answers` and
/// accepts it. Run by hand: `cargo nextest run ... --run-ignored only`.
#[tokio::test]
#[ignore = "needs a docket with --answers on PATH"]
async fn the_real_docket_asks_and_a_gated_bash_call_is_accepted() {
    let state = scratch("docket-live-ask");
    let workspace = scratch("docket-live-ask-workspace");
    let secrets = secret_store(state.path());
    secrets.set("key", FAKE_KEY).expect("seed store");

    let calls = std::sync::atomic::AtomicUsize::new(0);
    let server = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/chat/completions"))
        .respond_with(move |_: &wiremock::Request| {
            let first = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0;
            let message = if first {
                serde_json::json!({
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "bash",
                            "arguments": serde_json::json!({"command": "git push origin production"}).to_string(),
                        },
                    }],
                })
            } else {
                serde_json::json!({"role": "assistant", "content": "Done."})
            };
            wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "chatcmpl-live",
                "model": SERVED_MODEL,
                "choices": [{"index": 0, "message": message,
                    "finish_reason": if first { "tool_calls" } else { "stop" }}],
                "usage": {"prompt_tokens": 11, "completion_tokens": 5, "total_tokens": 16},
            }))
        })
        .mount(&server)
        .await;
    let _base_url_override = BaseUrlOverride::set(&server.uri());

    let harness = live_harness(state.path().join("staging"), secrets);
    let mut request = live_request(workspace.path());
    request.work.request.permission_policy.approvals = Some(Approvals::Ask);
    let handle = harness.start(&request).await.expect("start");
    let (mut questions, answers) = harness
        .decision_channels(&handle)
        .await
        .expect("an ask request exposes decision channels");
    let drive = async {
        let question = questions.recv().await.expect("a gated call asks");
        assert!(question.prompt.contains("bash"), "{question:?}");
        let accept = DecisionAnswer {
            option_id: Some("accept".to_owned()),
            text: None,
        };
        let _ = answers.send(accept).await;
    };
    let (outcome, ()) = tokio::join!(harness.wait(&handle), drive);
    let outcome = outcome.expect("wait");
    assert_eq!(
        outcome.terminal_state,
        crate::client::AttemptState::Succeeded,
        "{:?}",
        outcome.terminal_reason
    );
}
