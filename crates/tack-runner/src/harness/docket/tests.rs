//! docket's own claims: its command line, how a result line becomes a
//! report, what `docket setup harness` says, and the real binary against a
//! fake model endpoint named in its own setup. Fixtures are captured
//! transcripts under `fixtures/docket/`. The lifecycle is proved in
//! `local_process/tests.rs`.

use std::time::Duration;

use super::*;
use crate::harness::HarnessAdapter;
use crate::harness::local_process::{CommandOutcome, LocalProcessHarness};
use crate::harness::process::ProcessLimits;
use crate::harness::test_support::{finished, scratch, secret_store, spec};
use tack_orch::execution::{Approvals, CapabilityValue, ModelListSource, SignInState};

/// The contract's claim fixture, with docket's own tools (its fixture names
/// tools of another harness, which a docket policy document cannot), the
/// network allowed and no budget: nothing the command line must carry.
fn request(workspace: &std::path::Path) -> crate::harness::ExecutionSpec {
    let mut request = spec(DESCRIPTOR.kind, workspace);
    request.work.request.permission_policy.tools = vec!["*".to_owned()];
    request.work.request.permission_policy.network = true;
    request.work.request.budgets = serde_json::json!({});
    request
}

fn run_context<'a>(
    request: &'a crate::harness::ExecutionSpec,
    scratch: &'a std::path::Path,
) -> RunContext<'a> {
    RunContext {
        spec: request,
        scratch,
        program: None,
    }
}

#[test]
fn a_request_becomes_a_contract_1_2_run() {
    let state = scratch("docket-invocation");
    let scratch_dir = scratch("docket-invocation-scratch");
    let request = request(state.path());
    let invocation = invocation(&run_context(&request, scratch_dir.path()), None).expect("args");
    let at = |name: &str| scratch_dir.path().join(name).display().to_string();
    assert_eq!(
        invocation.args,
        [
            "exec",
            "--contract",
            "1.2",
            "--workspace",
            &state.path().display().to_string(),
            "--task-file",
            &at("task.md"),
            "--agent-id",
            "attempt",
            "--timeout",
            "30",
            "--token-file",
            &at("token.json"),
            "--model",
            "opaque/model-alpha",
        ]
    );
    assert_eq!(invocation.env["DOCKET_HOME"], at("docket-home"));
    assert_eq!(invocation.env["DOCKET_SETUP_HOME"], at("docket-setup"));
    assert!(!invocation.stdin_stays_open);
    assert_eq!(
        std::fs::read_to_string(scratch_dir.path().join("task.md")).expect("task file"),
        request.work.request.resolved_agent_profile.instructions
    );
    // The task is the file; stdin carries nothing.
    assert!(
        DocketGrammar
            .prompt("the task".to_owned(), false)
            .is_empty()
    );
}

#[test]
fn a_request_naming_no_model_passes_no_model_flag() {
    let state = scratch("docket-no-model");
    let scratch_dir = scratch("docket-no-model-scratch");
    let mut request = request(state.path());
    request.work.request.requested_model = None;
    let args = invocation(&run_context(&request, scratch_dir.path()), None)
        .expect("docket's setup default answers")
        .args;
    assert!(!args.iter().any(|arg| arg == "--model"), "{args:?}");
    assert_eq!(DESCRIPTOR.model_selection, ModelSelection::Optional);
}

/// docket's setup home holds pods, policies, traces and an audit log beside
/// the three files a run reads; only those three reach the run, each
/// read-only, and only the ones that exist. Every value in its credential
/// store is stripped from what the run prints.
#[test]
fn the_setup_home_reaches_the_run_as_three_read_only_files() {
    let state = scratch("docket-setup-home");
    let scratch_dir = scratch("docket-setup-home-scratch");
    let home = state.path().join(".docket");
    std::fs::create_dir_all(home.join("pods")).expect("setup home");
    for name in ["docket-providers.json", "secrets.json", "audit.log"] {
        std::fs::write(home.join(name), "{}").expect("setup file");
    }
    std::fs::write(
        home.join("secrets.json"),
        r#"{"OPENROUTER_API_KEY": "sk-or-canary", "KEPT_IN_KEYRING": ""}"#,
    )
    .expect("secrets");
    let workspace = scratch("docket-setup-home-workspace");
    let request = request(workspace.path());
    let invocation =
        invocation(&run_context(&request, scratch_dir.path()), Some(&home)).expect("args");
    let setup = scratch_dir.path().join("docket-setup");
    assert_eq!(
        invocation.read_only_files,
        [
            (
                home.join("docket-providers.json"),
                setup.join("docket-providers.json")
            ),
            (home.join("secrets.json"), setup.join("secrets.json")),
        ]
    );
    assert!(invocation.shared_files.is_empty());
    assert_eq!(invocation.secrets, ["sk-or-canary"]);

    let none = super::invocation(&run_context(&request, scratch_dir.path()), None).expect("args");
    assert!(none.read_only_files.is_empty() && none.secrets.is_empty());
}

#[test]
fn a_recipe_is_refused_naming_it() {
    let state = scratch("docket-recipe");
    let mut request = request(state.path());
    request.work.request.resolved_agent_profile.tool_policy =
        serde_json::json!({"docket": {"recipe": "my-recipe"}});
    let error = invocation(&run_context(&request, state.path()), None).expect_err("refused");
    assert!(
        matches!(&error, HarnessError::Rejected { reason } if reason.contains("my-recipe")),
        "{error:?}"
    );
    for empty in [
        serde_json::json!({"docket": {"recipe": ""}}),
        serde_json::json!({"docket": {}}),
    ] {
        request.work.request.resolved_agent_profile.tool_policy = empty;
        invocation(&run_context(&request, state.path()), None).expect("no recipe named");
    }
}

/// The probe's answer, over what each docket prints: only a 1.2 refusal
/// is a docket that runs here, and one that is not says what it needs.
#[test]
fn only_a_docket_that_refuses_on_contract_1_2_runs_here() {
    let rows = [
        (
            Some(include_str!("../fixtures/docket/0.2.0b5/probe-1.2.ndjson")),
            true,
        ),
        (
            Some(include_str!(
                "../fixtures/docket/0.2.0b5/probe-1.2-on-0.2.0b4.ndjson"
            )),
            false,
        ),
        (Some("docket 9.9.4\n"), false),
        (Some(""), false),
        (None, false),
    ];
    for (stdout, speaks) in rows {
        assert_eq!(
            probe::speaks(stdout.map(str::to_owned)),
            speaks,
            "{stdout:?}"
        );
    }
    assert_eq!(DocketFeatures::speaking("0.2.0b5").unsupported(), None);
    let old = DocketFeatures {
        version: "0.2.0b4".to_owned(),
        ..DocketFeatures::default()
    };
    assert_eq!(
        old.unsupported().as_deref(),
        Some(
            "needs docket 0.2.0-beta.5 or later, which reads its own setup (harness contract \
             1.2); this one is 0.2.0b4"
        )
    );
    assert!(
        DocketFeatures::default()
            .unsupported()
            .is_some_and(|why| why.ends_with("(harness contract 1.2)"))
    );
}

fn ran(code: i32, stdout: &str) -> CommandOutcome {
    CommandOutcome::Ran {
        code,
        stdout: stdout.to_owned(),
        stderr: String::new(),
    }
}

/// `(document, exit, signed in, listed models, default model, requires a model)`
/// over the three captured setup homes.
#[test]
fn readiness_is_what_docket_setup_harness_says() {
    let ready = include_str!("../fixtures/docket/0.2.0b5/setup-harness-ready.json");
    let no_default = include_str!("../fixtures/docket/0.2.0b5/setup-harness-nodefault.json");
    let empty = include_str!("../fixtures/docket/0.2.0b5/setup-harness-empty.json");
    let local = vec!["local/local-model".to_owned()];
    let rows = [
        (
            ready,
            0,
            SignInState::SignedIn,
            local.clone(),
            Some("local/local-model"),
            false,
        ),
        (no_default, 1, SignInState::SignedIn, local, None, true),
        (empty, 1, SignInState::SignedOut, Vec::new(), None, true),
    ];
    for (document, code, state, models, default_model, requires) in rows {
        let found = readiness::readiness(&ran(code, document));
        assert_eq!(found.sign_in.state, state, "{document}");
        assert_eq!(found.sign_in.command.as_deref(), Some("docket setup"));
        assert_eq!(found.models.ids, models, "{document}");
        assert_eq!(found.default_model.as_deref(), default_model, "{document}");
        assert_eq!(found.requires_model, Some(requires), "{document}");
    }
    let ready = readiness::readiness(&ran(0, ready));
    assert_eq!(ready.sign_in.detail, "provider local");
    assert_eq!(ready.models.source, ModelListSource::Listed);
    // A home that lists a model but sets none up says why a run naming no
    // model would be refused, in docket's own words.
    let no_default = readiness::readiness(&ran(1, no_default));
    assert!(
        no_default
            .sign_in
            .detail
            .starts_with("provider local; no model is set up for role 'implementer'"),
        "{}",
        no_default.sign_in.detail
    );
    let empty = readiness::readiness(&ran(1, empty));
    assert!(empty.sign_in.detail.contains("docket setup model set"));
    assert_eq!(empty.models.source, ModelListSource::None);

    for (outcome, why) in [
        (ran(2, ""), "exited with status 2"),
        (ran(0, "not json"), "printed nothing readable"),
        (CommandOutcome::TimedOut, "timed out"),
    ] {
        let found = readiness::readiness(&outcome);
        assert_eq!(found.sign_in.state, SignInState::Unknown);
        assert!(
            found.sign_in.detail.ends_with(why),
            "{}",
            found.sign_in.detail
        );
        assert_eq!(found.requires_model, None);
    }
}

#[test]
fn capability_lines_say_what_contract_1_2_offers() {
    let reason = |line: &CapabilityValue| line.reason.clone().unwrap_or_default();
    let found = capabilities(&DocketFeatures::speaking("0.2.0b5"));
    for line in [&found.cancel, &found.decisions, &found.artifacts] {
        assert_eq!(line.support, CapabilitySupport::Supported);
        assert!(
            reason(line).ends_with("; contract 1.2 against docket 0.2.0b5"),
            "{line:?}"
        );
    }
    assert_eq!(found.resume.support, CapabilitySupport::Unsupported);
    assert_eq!(found.usage.support, CapabilitySupport::Advisory);
    assert!(reason(&found.usage).contains("--max-tokens"));
    assert_eq!(
        found.additional["permission_policy"]["support"],
        "supported"
    );
    // A docket that was never probed claims no version.
    let unprobed = capabilities(&DocketFeatures::default());
    assert!(!reason(&unprobed.decisions).contains("against docket"));
}

/// The captured cancelled run makes one model call and one bash call, names
/// that call's group, started then gone, and ends on its result line; no
/// other line of it means anything to the core.
#[test]
fn process_lines_become_group_signals() {
    let state = scratch("docket-signal");
    let request = request(state.path());
    let run = RunContext {
        spec: &request,
        scratch: state.path(),
        program: None,
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

/// `ask` keeps stdin open for the answer lines; any other posture keeps
/// docket's refusal.
#[test]
fn only_ask_opens_the_answer_channel() {
    let state = scratch("docket-invocation-ask");
    let mut request = request(state.path());
    for (approvals, asks) in [(Some(Approvals::Ask), true), (None, false)] {
        request.work.request.permission_policy.approvals = approvals;
        let invocation = invocation(&run_context(&request, state.path()), None).expect("args");
        assert_eq!(invocation.stdin_stays_open, asks, "{approvals:?}");
        assert_eq!(
            invocation
                .args
                .windows(2)
                .any(|pair| pair == ["--answers", "stdin"]),
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
    let request = request(state.path());
    let run = RunContext {
        spec: &request,
        scratch: state.path(),
        program: None,
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
            "{{\"answer\":{{\"action\":\"{action}\",\"approvalToken\":\"apr-957ee731-4440-4419-b4e0-da5b18fabf67\",\"content\":null}},\"token\":\"run-721a5ebd-30b1-4634-b1b2-197989d99934\",\"v\":\"1.2.0\"}}"
        )
    };
    assert_eq!(answer_for(Some("accept")), line("accept"));
    assert_eq!(answer_for(Some("decline")), line("decline"));
    assert_eq!(answer_for(None), line("decline"));
}

#[test]
fn a_run_on_the_setup_default_reports_the_model_that_answered() {
    let report = read(include_str!(
        "../fixtures/docket/0.2.0b5/ok-setup-default.ndjson"
    ));
    assert!(report.succeeded, "{:?}", report.terminal_reason);
    assert_eq!(
        report.observed_model.as_deref(),
        Some("served/observed-model-x")
    );
    assert_eq!((report.tokens_in, report.tokens_out), (Some(22), Some(10)));
    assert_eq!(
        report.terminal_reason["files"],
        serde_json::json!([{"path": "greeting.txt", "op": "write"}])
    );
}

#[test]
fn a_result_line_lists_its_files_and_limits() {
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
}

/// One request, `network: false` and two tools: the token budget and the
/// policy document are passed, the document is exact, and a request the
/// document cannot express never reaches docket.
#[test]
fn limits_and_policy_are_passed() {
    let state = scratch("docket-invocation-limits");
    let scratch_dir = scratch("docket-invocation-limits-scratch");
    let mut request = request(state.path());
    request.work.request.requested_model = None;
    request.work.request.permission_policy.tools = vec!["read".to_owned(), "BASH".to_owned()];
    request.work.request.permission_policy.network = false;
    request.work.request.budgets = serde_json::json!({"tokens": 200_000});
    let policy = scratch_dir.path().join("policy.yaml").display().to_string();
    let args = |request: &crate::harness::ExecutionSpec| {
        invocation(&run_context(request, scratch_dir.path()), None).map(|i| i.args[13..].to_vec())
    };
    assert_eq!(
        args(&request).expect("args"),
        ["--max-tokens", "200000", "--policy", &policy]
    );
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
        assert!(
            !args(&request)
                .expect("args")
                .contains(&"--max-tokens".to_owned())
        );
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
        let error = args(&request).expect_err("refused");
        assert!(
            matches!(&error, HarnessError::Rejected { reason } if reason.contains(field)),
            "{error:?}"
        );
    }
    request.work.request.permission_policy.tools = Vec::new();
    request.work.request.permission_policy.additional =
        [("sandbox".to_owned(), serde_json::json!(true))].into();
    let error = args(&request).expect_err("refused");
    assert!(
        matches!(&error, HarnessError::Rejected { reason } if reason.contains("permission_policy.sandbox"))
    );
}

fn read(stdout: &str) -> RunReport {
    let state = scratch("docket-report");
    let request = request(state.path());
    DocketGrammar.report(
        &run_context(&request, state.path()),
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

/// The located docket, when it speaks contract 1.2: the policy document
/// this grammar writes is one docket accepts. docket validates every
/// `--policy` file once its setup resolves a model, and refuses an invalid
/// one by name; a valid one ends the run at the endpoint, a closed loopback
/// port, with no spend.
#[test]
fn the_real_docket_accepts_the_policy_document() {
    let Ok(program) = crate::harness::locate::locate_installed(DESCRIPTOR.program) else {
        eprintln!("skipping: `docket` not found on PATH");
        return;
    };
    let found = DocketFeatures::probe(&program);
    assert!(!found.version.is_empty(), "docket --version was not read");
    if let Some(why) = found.unsupported() {
        eprintln!("skipping: the located docket {why}");
        return;
    }
    let state = scratch("docket-negotiated");
    let scratch_dir = scratch("docket-negotiated-scratch");
    let mut request = request(state.path());
    request.work.request.permission_policy.tools = vec!["read".to_owned()];
    let invocation = invocation(&run_context(&request, scratch_dir.path()), None).expect("args");
    let policy_path = scratch_dir.path().join("policy.yaml");
    assert!(policy_path.exists(), "{:?}", invocation.args);
    let invalid = scratch_dir.path().join("invalid.yaml");
    std::fs::write(&invalid, "kind: policy\nthen: block\n").expect("write");
    let setup = scratch("docket-negotiated-setup");
    seed_setup(setup.path(), "http://127.0.0.1:9");
    let refusal = |policy: &std::path::Path| {
        let output = std::process::Command::new(&program)
            .args([
                "exec",
                "--contract",
                "1.2",
                "--workspace",
                ".",
                "--task",
                "x",
                "--policy",
            ])
            .arg(policy)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("DOCKET_HOME", scratch_dir.path().join("docket-home"))
            .env("DOCKET_SETUP_HOME", setup.path().join(".docket"))
            .output()
            .expect("run docket");
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    assert!(refusal(&invalid).contains("invalid --policy"));
    let valid = refusal(&policy_path);
    assert!(!valid.contains("invalid --policy"), "{valid}");
}

const FAKE_KEY: &str = "sk-fake-canary-71c2";
const SERVED_MODEL: &str = "served/observed-model-x";

/// Sets `HOME` for the lifetime of the guard, so the grammar finds the
/// test's own docket setup and a panic still restores the real one.
struct HomeOverride(Option<std::ffi::OsString>);

impl HomeOverride {
    fn set(home: &std::path::Path) -> Self {
        let previous = std::env::var_os("HOME");
        // SAFETY-ish: process-wide, but safe under `cargo nextest` (one
        // process per test), never under a bare `cargo test`.
        unsafe { std::env::set_var("HOME", home) };
        Self(previous)
    }
}

impl Drop for HomeOverride {
    fn drop(&mut self) {
        match self.0.take() {
            Some(home) => unsafe { std::env::set_var("HOME", home) },
            None => unsafe { std::env::remove_var("HOME") },
        }
    }
}

/// A docket setup home whose one provider is `server`, keyed with
/// [`FAKE_KEY`] in its credential store, and whose default model is that
/// provider's one model.
fn seed_setup(home: &std::path::Path, server: &str) {
    let docket = home.join(".docket");
    std::fs::create_dir_all(&docket).expect("setup home");
    let providers = serde_json::json!({"providers": {"fake": {
        "name": "fake",
        "dialect": "openai-chat",
        "baseUrl": format!("{server}/v1"),
        "auth": {"type": "bearer", "credentials": ["FAKE_MODEL_KEY"]},
        "models": [{"id": "model-x", "contextWindow": 32768, "maxTokens": 4096}],
    }}});
    std::fs::write(docket.join("docket-providers.json"), providers.to_string()).expect("write");
    std::fs::write(
        docket.join("docket-models.json"),
        r#"{"default": "fake/model-x"}"#,
    )
    .expect("write");
    std::fs::write(
        docket.join("secrets.json"),
        serde_json::json!({"FAKE_MODEL_KEY": FAKE_KEY}).to_string(),
    )
    .expect("write");
}

/// One response per call: a `write` tool call whose content echoes the
/// credential (a worst case: a model repeating a value it was never given),
/// then a final message. The served model differs from the one set up, so
/// finding it in the outcome proves it came from the response.
async fn mount_tool_call(server: &wiremock::MockServer, call: serde_json::Value) {
    let calls = std::sync::atomic::AtomicUsize::new(0);
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/v1/chat/completions"))
        .respond_with(move |_: &wiremock::Request| {
            let first = calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0;
            let message = if first {
                serde_json::json!({
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{"id": "call_1", "type": "function", "function": call}],
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
        .mount(server)
        .await;
}

/// The located docket, `None` (with why) when there is none or it predates
/// contract 1.2.
fn live_harness(state: &std::path::Path) -> Option<DocketAdapter> {
    let Ok(program) = crate::harness::locate::locate_installed(DESCRIPTOR.program) else {
        eprintln!("skipping live docket test: `docket` not found on PATH");
        return None;
    };
    if let Some(why) = DocketFeatures::probe(&program).unsupported() {
        eprintln!("skipping live docket test: the located docket {why}");
        return None;
    }
    let harness = LocalProcessHarness::discover(
        DocketGrammar,
        ProcessLimits::new(1_048_576, 1_048_576, Duration::from_secs(30)),
        state.join("staging"),
        secret_store(state),
    )
    .expect("docket located above");
    Some(harness)
}

fn live_request(workspace: &std::path::Path) -> crate::harness::ExecutionSpec {
    let mut request = request(workspace);
    request.work.request.requested_model = None;
    request.work.request.resolved_agent_profile.instructions =
        "Write a short greeting to greeting.txt".to_owned();
    // docket's own tool names: the policy document can only name those.
    request.work.request.permission_policy.tools =
        ["read", "write", "edit", "bash"].map(str::to_owned).into();
    request
}

/// The real docket runs on its own setup: the request names no model and
/// no endpoint, docket takes its default model and its key from the setup
/// home, the key reaches the endpoint, and the staged log never shows it.
#[tokio::test]
async fn the_real_docket_edits_a_file_on_its_own_setup() {
    let state = scratch("docket-live");
    let Some(harness) = live_harness(state.path()) else {
        return;
    };
    let workspace = scratch("docket-live-workspace");
    let home = scratch("docket-live-home");
    let server = wiremock::MockServer::start().await;
    mount_tool_call(
        &server,
        serde_json::json!({
            "name": "write",
            "arguments": serde_json::json!({
                "path": "greeting.txt",
                "content": format!("hello from the fake model server; canary={FAKE_KEY}\n"),
            })
            .to_string(),
        }),
    )
    .await;
    seed_setup(home.path(), &server.uri());
    let _home = HomeOverride::set(home.path());

    let request = live_request(workspace.path());
    let handle = harness.start(&request).await.expect("start");
    let outcome = harness.wait(&handle).await.expect("wait");

    assert_eq!(
        outcome.terminal_state,
        crate::client::AttemptState::Succeeded,
        "{:?}",
        outcome.terminal_reason
    );
    let written =
        std::fs::read_to_string(workspace.path().join("greeting.txt")).expect("docket wrote it");
    assert!(written.contains("hello from the fake model server"));
    assert_eq!(
        outcome.actual_execution.model.as_deref(),
        Some(SERVED_MODEL)
    );
    assert!(outcome.usage.tokens_in.value.unwrap_or(0) > 0);

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
    assert!(!log.contains(FAKE_KEY), "the stored key reached the log");
    // docket wrote nothing into the setup home it read.
    let mut names: Vec<_> = std::fs::read_dir(home.path().join(".docket"))
        .expect("setup home")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "docket-models.json",
            "docket-providers.json",
            "secrets.json"
        ]
    );
}

/// Gates one `bash` call (`git push origin production` is a `prod-deploy`
/// action class for docket) and accepts it: the answer line this grammar
/// writes is one docket's contract 1.2 takes.
#[tokio::test]
async fn the_real_docket_asks_and_a_gated_bash_call_is_accepted() {
    let state = scratch("docket-live-ask");
    let Some(harness) = live_harness(state.path()) else {
        return;
    };
    let workspace = scratch("docket-live-ask-workspace");
    let home = scratch("docket-live-ask-home");
    let server = wiremock::MockServer::start().await;
    mount_tool_call(
        &server,
        serde_json::json!({
            "name": "bash",
            "arguments": serde_json::json!({"command": "git push origin production"}).to_string(),
        }),
    )
    .await;
    seed_setup(home.path(), &server.uri());
    let _home = HomeOverride::set(home.path());

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
    let request = request(state.path());
    let run = RunContext {
        spec: &request,
        scratch: state.path(),
        program: None,
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
