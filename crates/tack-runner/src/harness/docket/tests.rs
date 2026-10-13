//! docket's own claims: its command line, how a result line becomes a
//! report, and the real binary's negotiated features. Fixtures are
//! captured transcripts under `fixtures/docket/`. The lifecycle is proved
//! in `local_process/tests.rs`.

use super::*;
use crate::harness::test_support::{finished, scratch, set_env, spec};
use tack_orch::execution::{Approvals, CapabilityValue};

/// A request whose own environment names a model server.
fn spec_with_base_url(state: &std::path::Path) -> crate::harness::ExecutionSpec {
    let mut request = spec(DESCRIPTOR.kind, state);
    set_env(&mut request, &[(BASE_URL_ENV, "http://127.0.0.1:9/v1")]);
    request
}

#[test]
fn a_request_becomes_a_harness_run_command() {
    let state = scratch("docket-invocation");
    let scratch_dir = scratch("docket-invocation-scratch");
    let request = spec_with_base_url(state.path());
    let run = RunContext {
        spec: &request,
        scratch: scratch_dir.path(),
    };
    let invocation = invocation(&DocketFeatures::default(), &run).expect("invocation");
    assert_eq!(
        invocation.args,
        [
            "exec",
            "--workspace",
            &state.path().display().to_string(),
            "--task-file",
            "/dev/stdin",
            "--model",
            "opaque/model-alpha",
            "--agent-id",
            "attempt",
            "--timeout",
            "30",
        ]
    );
    assert!(!invocation.env.contains_key(BASE_URL_ENV));
    let docket_home = &invocation.env["DOCKET_HOME"];
    assert!(docket_home.starts_with(&scratch_dir.path().display().to_string()));
    assert!(!docket_home.starts_with(&state.path().display().to_string()));
    assert!(docket_home.ends_with("docket-home"));
}

#[test]
fn a_request_naming_no_model_passes_no_model_flag() {
    let state = scratch("docket-no-model");
    let scratch_dir = scratch("docket-no-model-scratch");
    let mut request = spec_with_base_url(state.path());
    request.work.request.requested_model = None;
    let run = RunContext {
        spec: &request,
        scratch: scratch_dir.path(),
    };
    let invocation = invocation(&DocketFeatures::default(), &run).expect("invocation");
    assert!(
        !invocation.args.iter().any(|arg| arg == "--model"),
        "{:?}",
        invocation.args
    );
}

#[test]
fn a_request_naming_no_base_url_is_refused() {
    let state = scratch("docket-refused-invocation");
    let request = spec(DESCRIPTOR.kind, state.path());
    let run = RunContext {
        spec: &request,
        scratch: state.path(),
    };
    let error =
        invocation(&DocketFeatures::default(), &run).expect_err("no request-level base url");
    assert!(matches!(&error, HarnessError::Rejected { reason } if reason.contains(BASE_URL_ENV)));
}

#[test]
fn contract_1_1_passes_the_task_as_a_file() {
    let state = scratch("docket-invocation-1-1");
    let scratch_dir = scratch("docket-invocation-1-1-scratch");
    let request = spec_with_base_url(state.path());
    let run = RunContext {
        spec: &request,
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
            "exec",
            "--workspace",
            &state.path().display().to_string(),
            "--task-file",
            &task_file.display().to_string(),
            "--model",
            "opaque/model-alpha",
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

/// The captured cancelled run makes one model call and one bash call, names
/// that call's group, started then gone, and ends on its result line; no
/// other line of it means anything to the core.
#[test]
fn process_lines_become_group_signals() {
    let state = scratch("docket-signal");
    let request = spec_with_base_url(state.path());
    let run = RunContext {
        spec: &request,
        scratch: state.path(),
    };
    let signals: Vec<_> = include_str!("../fixtures/docket/contract-1.1/cancelled-process.ndjson")
        .lines()
        .filter_map(|line| DocketGrammar.signal(&run, line))
        .collect();
    assert_eq!(
        signals,
        [
            step("agent.model_turn", "Model turn 1: 10 tokens in, 5 out"),
            step(
                "agent.tool_call",
                "bash: python3 -c \"import time; time.sleep(30)\""
            ),
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
    let mut request = spec_with_base_url(state.path());
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
    let request = spec_with_base_url(state.path());
    let run = RunContext {
        spec: &request,
        scratch: state.path(),
    };
    let signals: Vec<_> = include_str!("../fixtures/docket/contract-1.1/asked-answered.ndjson")
        .lines()
        .filter_map(|line| DocketGrammar.signal(&run, line))
        .collect();
    let [
        first_turn,
        tool_call,
        StreamSignal::Question(question),
        StreamSignal::ProcessStarted { .. },
        StreamSignal::ProcessExited { .. },
        second_turn,
        StreamSignal::Finished,
    ] = signals.as_slice()
    else {
        panic!("{signals:?}");
    };
    assert_eq!(
        [first_turn, tool_call, second_turn],
        [
            &step("agent.model_turn", "Model turn 1: 20 tokens in, 8 out"),
            &step("agent.tool_call", "bash: git push origin production"),
            &step("agent.model_turn", "Model turn 2: 50 tokens in, 10 out"),
        ]
    );
    assert_eq!(
        question.vendor_id,
        "apr-957ee731-4440-4419-b4e0-da5b18fabf67"
    );
    assert_eq!(question.kind, "tool_permission");
    assert_eq!(
        question.prompt,
        "Allow bash to run \"git push origin production\"? docket's policy: matches high-risk \
         action class 'prod-deploy': Production deploys and release pushes."
    );
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
    let mut request = spec_with_base_url(state.path());
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
    let flags = |args: Vec<String>| args[11..].to_vec();
    let run = RunContext {
        spec: &request,
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
    let flags = |args: Vec<String>| args[11..].to_vec();

    // Recipe is passed when probed and specified.
    {
        let mut request = spec_with_base_url(state.path());
        request.work.request.resolved_agent_profile.tool_policy =
            serde_json::json!({"docket": {"recipe": "my-recipe"}});
        let run = RunContext {
            spec: &request,
            scratch: scratch_dir.path(),
        };
        let all = |features: &DocketFeatures| invocation(features, &run).expect("args").args;
        // docket refuses `--recipe` with `--agent-id` or `--max-tokens`, so a
        // recipe run carries neither, even with a token budget on the request.
        let recipe_args = all(&with_recipe);
        assert!(!recipe_args.contains(&"--agent-id".to_owned()));
        assert!(!recipe_args.contains(&"--max-tokens".to_owned()));
        assert_eq!(
            recipe_args[7..],
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
        let mut request = spec_with_base_url(state.path());
        request.work.request.resolved_agent_profile.tool_policy =
            serde_json::json!({"docket": {"recipe": ""}});
        let run = RunContext {
            spec: &request,
            scratch: scratch_dir.path(),
        };
        let args =
            |features: &DocketFeatures| flags(invocation(features, &run).expect("args").args);
        assert!(!args(&with_recipe).contains(&"--recipe".to_owned()));
    }

    // Recipe is not passed when absent.
    {
        let mut request = spec_with_base_url(state.path());
        request.work.request.resolved_agent_profile.tool_policy = serde_json::json!({"docket": {}});
        let run = RunContext {
            spec: &request,
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
    let request = live_request(state.path(), "http://127.0.0.1:9");
    let run = RunContext {
        spec: &request,
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
                "exec",
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
    let request = spec_with_base_url(state.path());
    let run = RunContext {
        spec: &request,
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

const FAKE_KEY: &str = "sk-fake-canary-71c2";

/// Builds the request a live docket run is spawned with: the contract's own
/// claim fixture, pointed at the fake model server named in its own
/// environment and a fixed model.
fn live_request(workspace: &std::path::Path, server_uri: &str) -> crate::harness::ExecutionSpec {
    let mut request = spec(DESCRIPTOR.kind, workspace);
    request.work.request.requested_model = Some("anthropic/claude-x".to_owned());
    set_env(
        &mut request,
        &[
            (BASE_URL_ENV, &format!("{server_uri}/v1")),
            ("DOCKET_LLM_API_KEY", FAKE_KEY),
        ],
    );
    request.work.request.resolved_agent_profile.instructions =
        "Write a short greeting to greeting.txt".to_owned();
    // docket's own tool names: the policy document can only name those.
    request.work.request.permission_policy.tools =
        ["read", "write", "edit", "bash"].map(str::to_owned).into();
    request
}

/// `*` blocks nothing, except `fetch` when the network is denied.
#[test]
fn every_tool_blocks_only_fetch_when_offline() {
    let policy = |network: bool| PermissionPolicy {
        tools: vec!["*".to_owned()],
        network,
        approvals: None,
        additional: Default::default(),
    };
    assert_eq!(policy_document(&policy(true)).expect("document"), None);
    let offline = policy_document(&policy(false))
        .expect("document")
        .expect("a document");
    assert!(offline.contains("fetch"), "{offline}");
    assert!(!offline.contains("bash"), "{offline}");
}

#[test]
fn an_approval_prompt_names_the_call_the_reason_and_the_agents_own_words() {
    assert_eq!(
        approval_prompt(
            Some("bash"),
            Some("c1"),
            Some(
                "tool call 'bash': 'sh' is not on the curated allowlist; call=bash \
                 command=\"sh -c 'echo done > DONE.txt'\""
            ),
            Some("I'll run the exact shell command as requested."),
        ),
        "Allow bash to run \"sh -c 'echo done > DONE.txt'\"? docket's policy: 'sh' is not on the \
         curated allowlist. The agent says: \"I'll run the exact shell command as requested.\""
    );
    assert_eq!(
        approval_prompt(
            Some("write_file"),
            None,
            Some(
                "tool call 'write_file': outside the workspace; call=write_file path=\"/etc/x\" content=\"y\""
            ),
            None,
        ),
        "Allow write_file with path=\"/etc/x\" content=\"y\"? docket's policy: outside the workspace."
    );
    assert_eq!(
        approval_prompt(Some("bash"), None, Some("something else entirely"), None),
        "Allow bash? something else entirely"
    );
    assert_eq!(
        approval_prompt(Some("bash"), Some("c1"), None, None),
        "Allow bash (call c1)?"
    );
    assert_eq!(
        approval_prompt(None, None, None, None),
        "Allow a gated tool call?"
    );
}

fn step(kind: &str, text: &str) -> StreamSignal {
    StreamSignal::Progress(Progress {
        kind: kind.to_owned(),
        text: text.to_owned(),
    })
}

/// A call docket refused and a model call that failed are steps too; a call
/// it allowed adds nothing past the call itself, and arguments that are not
/// a command are shown as the model sent them.
#[test]
fn refused_calls_failed_model_calls_and_other_tools_are_steps() {
    let state = scratch("docket-steps");
    let request = spec_with_base_url(state.path());
    let run = RunContext {
        spec: &request,
        scratch: state.path(),
    };
    let line = |event_type: &str, payload: serde_json::Value| {
        serde_json::json!({
            "v": "1.1.0",
            "token": "run-1",
            "event": {"event_type": event_type, "payload": payload},
        })
        .to_string()
    };
    let signals: Vec<_> = [
        line(
            "tool_result",
            serde_json::json!({"tool": "bash", "decision": "deny", "reason": "'sh' is not on the curated allowlist"}),
        ),
        line(
            "tool_result",
            serde_json::json!({"tool": "bash", "decision": "allow", "ok": true}),
        ),
        line(
            "llm_call",
            serde_json::json!({"ok": false, "failureKind": "timeout", "iteration": 3}),
        ),
        line(
            "tool_call",
            serde_json::json!({"tool": "write_file", "arguments": "{\"path\": \"DONE.txt\"}"}),
        ),
        line(
            "tool_call",
            serde_json::json!({"tool": "list_files", "arguments": "{}"}),
        ),
    ]
    .iter()
    .filter_map(|line| DocketGrammar.signal(&run, line))
    .collect();
    assert_eq!(
        signals,
        [
            step(
                "agent.tool_refused",
                "bash was refused: 'sh' is not on the curated allowlist"
            ),
            step("agent.model_turn", "A model call failed: timeout"),
            step("agent.tool_call", "write_file {\"path\": \"DONE.txt\"}"),
            step("agent.tool_call", "list_files"),
        ]
    );
}
