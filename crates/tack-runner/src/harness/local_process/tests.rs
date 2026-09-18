//! The lifecycle every harness shares, proved once against the fake harness
//! script through a grammar that adds nothing of its own.

use super::*;
use crate::harness::test_support::{
    FixedClock, clock, fake_harness, gateway, scratch, script, secret_store, set_env,
    set_secret_reference, spec,
};

const KIND: &str = "test-harness";
const GATEWAY: &str = crate::config::VERCEL_AI_GATEWAY_PROVIDER;

const BASE: HarnessDescriptor = HarnessDescriptor {
    kind: KIND,
    program: "tack-test-fixture-nonexistent-harness",
    wire: Wire::OpenAiResponses,
    model_selection: ModelSelection::Explicit("a model is required"),
    native_provider: "native",
    inherited_env: &[],
    min_capture_bytes: (0, 0),
    model_passthrough: "forwarded verbatim",
    probe_notes: &[("note", "attached to every probe")],
    credential_note: "",
};

static PLAIN: HarnessDescriptor = BASE;

static INHERITING: HarnessDescriptor = HarnessDescriptor {
    model_selection: ModelSelection::Optional,
    inherited_env: &["HOME"],
    ..BASE
};

struct TestGrammar {
    descriptor: &'static HarnessDescriptor,
    observed_model: Option<&'static str>,
}

impl HarnessGrammar for TestGrammar {
    fn descriptor(&self) -> &'static HarnessDescriptor {
        self.descriptor
    }

    fn capabilities(&self) -> FeatureCapabilities {
        let advisory = || capability(CapabilitySupport::Advisory, "test");
        FeatureCapabilities {
            cancel: advisory(),
            resume: advisory(),
            decisions: advisory(),
            artifacts: advisory(),
            usage: advisory(),
            additional: BTreeMap::new(),
        }
    }

    fn invocation(&self, run: &RunContext<'_>) -> Result<Invocation, HarnessError> {
        let tools = &run.spec.work.request.permission_policy.tools;
        if tools.iter().any(|tool| tool == "forbidden") {
            return Err(HarnessError::Rejected {
                reason: "grammar refused the policy".to_owned(),
            });
        }
        Ok(Invocation {
            args: vec!["run".to_owned()],
            env: BTreeMap::new(),
        })
    }

    fn report(&self, _run: &RunContext<'_>, result: &ProcessResult) -> RunReport {
        RunReport {
            observed_model: self.observed_model.map(str::to_owned),
            ..RunReport::verdict(
                result.exit == ProcessExit::Exited(0),
                serde_json::json!({"stdout": result.stdout.text, "stderr": result.stderr.text}),
            )
        }
    }
}

type Harness = LocalProcessHarness<TestGrammar, FixedClock>;

fn limits() -> ProcessLimits {
    ProcessLimits {
        termination_grace: Duration::from_millis(150),
        ..ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(10))
    }
}

fn harness_with(grammar: TestGrammar, locator: BinaryLocator, state: &Path) -> Harness {
    LocalProcessHarness::new(
        grammar,
        locator,
        clock(),
        limits(),
        state.join("staging"),
        secret_store(state),
    )
}

fn harness(state: &Path) -> Harness {
    let grammar = TestGrammar {
        descriptor: &PLAIN,
        observed_model: None,
    };
    harness_with(grammar, fake_harness(), state)
}

async fn run(harness: &Harness, spec: &ExecutionSpec) -> HarnessOutcome {
    let handle = harness.start(spec).await.expect("start");
    harness.wait(&handle).await.expect("wait")
}

fn absent_binary() -> BinaryLocator {
    BinaryLocator::Search {
        program: PLAIN.program.to_owned(),
        path: Some(std::ffi::OsString::new()),
        home: None,
    }
}

// ---- before spawn ---------------------------------------------------------

type Breakage = fn(&mut ExecutionSpec);

/// `(name, what is wrong with the request, a fragment of the reason)`.
fn broken_requests() -> [(&'static str, Breakage, &'static str); 5] {
    [
        (
            "other kind",
            |s| s.work.request.requested_harness_kind = DomainHarnessKind::new("x"),
            "does not match",
        ),
        (
            "no model",
            |s| s.work.request.requested_model_id = None,
            "a model is required",
        ),
        (
            "grammar policy",
            |s| s.work.request.permission_policy.tools = vec!["forbidden".into()],
            "grammar refused",
        ),
        (
            "missing secret",
            |s| set_secret_reference(s, "SECRET_VAR", "absent"),
            "secret_reference_unresolved: absent",
        ),
        (
            "disabled provider",
            |s| {
                s.work.request.requested_model_provider =
                    Some(tack_orch::execution::RequestedModelProvider::new(GATEWAY))
            },
            GATEWAY,
        ),
    ]
}

#[tokio::test]
async fn validate_rejects_what_start_would_refuse() {
    for (name, breakage, fragment) in broken_requests() {
        let state = scratch("validate");
        let mut request = spec(KIND, state.path());
        breakage(&mut request);
        let error = harness(state.path())
            .validate(&request)
            .await
            .expect_err(name);
        assert!(
            matches!(&error, HarnessError::Rejected { reason } if reason.contains(fragment)),
            "{name}: {error:?}"
        );
    }
}

#[tokio::test]
async fn an_absent_binary_is_rejected_and_probed_as_an_error() {
    let state = scratch("absent");
    let grammar = TestGrammar {
        descriptor: &PLAIN,
        observed_model: None,
    };
    let harness = harness_with(grammar, absent_binary(), state.path());
    let rejected = harness.validate(&spec(KIND, state.path())).await;
    assert!(matches!(rejected, Err(HarnessError::Rejected { .. })));
    let probe = harness.probe().await;
    assert_eq!(probe.installed_version, "");
    assert!(probe.probe_error.expect("error").contains("not found"));
}

/// The request would hang for an hour if it were ever spawned, so a broken
/// pre-spawn check turns into a timeout here instead of a stuck run.
#[tokio::test]
async fn a_rejected_start_spawns_nothing_and_tracks_nothing() {
    let state = scratch("rejected-start");
    let workspace = scratch("rejected-start-workspace");
    std::fs::write(workspace.path().join("sentinel.txt"), b"before").expect("seed");
    let harness = harness(state.path());
    let mut request = spec(KIND, workspace.path());
    request.work.request.requested_model_id = None;
    set_env(&mut request, &[("TACK_FAKE_HARNESS_MODE", "hang")]);

    let started = tokio::time::timeout(Duration::from_secs(5), harness.start(&request)).await;
    assert!(matches!(started, Ok(Err(HarnessError::Rejected { .. }))));
    assert!(harness.running.lock().await.is_empty());
    assert_eq!(
        std::fs::read_dir(workspace.path()).expect("read").count(),
        1
    );
    assert!(!state.path().join("secrets.json").exists());
}

// ---- a run ----------------------------------------------------------------

#[tokio::test]
async fn a_run_reports_the_request_model_as_unconfirmed() {
    let state = scratch("run");
    let outcome = run(&harness(state.path()), &spec(KIND, state.path())).await;

    assert_eq!(outcome.terminal_state, AttemptState::Succeeded);
    let actual = &outcome.actual_execution;
    assert_eq!(actual.harness_kind.as_str(), KIND);
    assert_eq!(actual.model_provider.as_str(), "openai");
    assert_eq!(actual.model_id.as_str(), "opaque/model-alpha");
    assert_eq!(actual.model_observation_source, "requested_not_confirmed");
    assert_eq!(
        outcome.usage.duration_ms.source,
        MeasurementSource::Measured
    );
    assert_eq!(
        outcome.usage.tokens_in.source,
        MeasurementSource::NotMeasured
    );
    assert_eq!(outcome.usage.cost_usd.value, None);
}

/// `(provider, observed by the harness, requested) -> (model, source)`. A
/// configured endpoint answers after the CLI printed its model, so what the
/// CLI reports through one is a request, not an observation.
#[tokio::test]
async fn the_model_source_says_who_vouches_for_the_model() {
    let rows = [
        (Some("anthropic"), Some("seen"), "seen", "harness_reported"),
        (
            Some(GATEWAY),
            Some("seen"),
            "seen",
            "requested_not_confirmed",
        ),
        (None, None, "unknown", "not_observed"),
    ];
    for (provider, observed_model, model, source) in rows {
        let state = scratch("model-source");
        state_secret(&state, "key");
        let grammar = TestGrammar {
            descriptor: &INHERITING,
            observed_model,
        };
        let harness =
            harness_with(grammar, fake_harness(), state.path()).with_providers(gateway("key"));
        let mut request = spec(KIND, state.path());
        request.work.request.requested_model_id = None;
        request.work.request.requested_model_provider =
            provider.map(tack_orch::execution::RequestedModelProvider::new);

        let actual = run(&harness, &request).await.actual_execution;
        assert_eq!(actual.model_id.as_str(), model, "{provider:?}");
        assert_eq!(actual.model_observation_source, source, "{provider:?}");
        assert_eq!(actual.model_provider.as_str(), provider.unwrap_or("native"));
    }
}

const SECRET: &str = "stored-secret-canary-41c7";
const CREDENTIAL: &str = "gateway-credential-canary-9b02";

/// The gateway's key under `name`, and a different value under `requested`
/// for a request's own `secret_reference`: were they the same value, the
/// request would register it and hide a credential that is not.
fn state_secret(state: &tempfile::TempDir, name: &str) {
    let store = secret_store(state.path());
    store.set(name, CREDENTIAL).expect("seed store");
    store.set("requested", SECRET).expect("seed store");
}

#[tokio::test]
async fn a_failed_exit_is_a_failed_attempt() {
    let state = scratch("failure");
    let mut request = spec(KIND, state.path());
    set_env(&mut request, &[("TACK_FAKE_HARNESS_MODE", "failure")]);
    let outcome = run(&harness(state.path()), &request).await;
    assert_eq!(outcome.terminal_state, AttemptState::Failed);
}

#[tokio::test]
async fn the_run_log_is_staged_outside_the_workspace() {
    let state = scratch("artifact");
    let workspace = scratch("artifact-workspace");
    let outcome = run(&harness(state.path()), &spec(KIND, workspace.path())).await;

    let artifact = &outcome.terminal_reason["artifact"];
    assert_eq!(artifact["kind"], "log");
    let staged = PathBuf::from(artifact["staged_path"].as_str().expect("staged_path"));
    assert!(staged.starts_with(state.path().join("staging")));
    let bytes = std::fs::read(&staged).expect("read staged artifact");
    assert!(String::from_utf8_lossy(&bytes).contains("fake-harness-ok"));
    assert_eq!(
        artifact["sha256"].as_str().expect("sha256"),
        crate::harness::sha256::sha256_hex(&bytes)
    );
}

/// The fake harness echoes its environment and stdin back on both streams:
/// a worst-case leaky CLI.
#[tokio::test]
async fn secrets_never_survive_into_the_reason_or_the_staged_log() {
    const ENV_CANARY: &str = "tack-test-canary-env-58d1";
    const PROMPT_CANARY: &str = "tack-test-canary-prompt-a341";
    let state = scratch("redaction");
    let mut request = spec(KIND, state.path());
    request.work.request.resolved_agent_profile.instructions = format!("do {PROMPT_CANARY}");
    let echo = [
        ("TACK_FAKE_HARNESS_MODE", "echo_canary"),
        ("TACK_TEST_SECRET", ENV_CANARY),
        ("TACK_FAKE_HARNESS_ECHO_ENV_KEYS", "TACK_TEST_SECRET"),
    ];
    set_env(&mut request, &echo);

    let outcome = run(&harness(state.path()), &request).await;
    let reason = outcome.terminal_reason.to_string();
    assert!(
        reason.contains("[REDACTED]"),
        "nothing was echoed: {reason}"
    );
    let staged = outcome.terminal_reason["artifact"]["staged_path"].as_str();
    let log = std::fs::read_to_string(staged.expect("staged")).expect("read staged log");
    for canary in [ENV_CANARY, PROMPT_CANARY] {
        assert!(
            !reason.contains(canary) && !log.contains(canary),
            "{canary}"
        );
    }
}

// ---- what reaches the child ----------------------------------------------

/// Runs a shim that records the names of its environment variables, and
/// the value of `$ECHOED` on stdout. Returns the names and the outcome.
async fn run_recording_env(
    descriptor: &'static HarnessDescriptor,
    provider: &str,
    echoed: &str,
) -> (Vec<String>, HarnessOutcome) {
    let state = scratch("child-env");
    state_secret(&state, "key");
    let marker = state.path().join("env.marker");
    let body = format!(
        "[ \"$1\" = run ] && env > {} && printf '%s' \"${echoed}\"\nexit 0",
        marker.display()
    );
    let grammar = TestGrammar {
        descriptor,
        observed_model: None,
    };
    let harness = harness_with(grammar, script(state.path(), &body), state.path())
        .with_providers(gateway("key"));
    let mut request = spec(KIND, state.path());
    request.work.request.requested_model_provider =
        Some(tack_orch::execution::RequestedModelProvider::new(provider));
    set_secret_reference(&mut request, "SECRET_VAR", "requested");
    let outcome = run(&harness, &request).await;
    let recorded = std::fs::read_to_string(&marker).expect("shim recorded its environment");
    let names = recorded
        .lines()
        .filter_map(|line| line.split('=').next().map(str::to_owned))
        .collect();
    (names, outcome)
}

#[tokio::test]
async fn the_child_gets_only_what_was_asked_for() {
    let (names, _) = run_recording_env(&PLAIN, "openai", "").await;
    let has = |name: &str| names.iter().any(|recorded| recorded == name);
    assert!(has("SECRET_VAR") && has("RUST_LOG"), "{names:?}");
    assert!(!has("HOME") && !has("AI_GATEWAY_API_KEY"), "{names:?}");

    let (names, _) = run_recording_env(&INHERITING, GATEWAY, "").await;
    let has = |name: &str| names.iter().any(|recorded| recorded == name);
    assert!(has("HOME") && has("AI_GATEWAY_API_KEY"), "{names:?}");
    assert!(!has("USER"), "{names:?}");
}

/// The shim prints the variable's value; seeing `[REDACTED]` in its place
/// proves both that the resolved value reached the child and that it was
/// registered for redaction.
#[tokio::test]
async fn resolved_secrets_reach_the_child_and_are_redacted() {
    for variable in ["SECRET_VAR", "AI_GATEWAY_API_KEY"] {
        let (_, outcome) = run_recording_env(&PLAIN, GATEWAY, variable).await;
        let reason = outcome.terminal_reason.to_string();
        assert!(reason.contains("[REDACTED]"), "{variable}: {reason}");
        assert!(
            !reason.contains(SECRET) && !reason.contains(CREDENTIAL),
            "{variable}"
        );
    }
}

#[tokio::test]
async fn secret_resolution_is_logged_by_name_never_by_value() {
    crate::test_log_capture::install();
    run_recording_env(&PLAIN, GATEWAY, "").await;
    let log = crate::test_log_capture::captured();
    assert!(
        log.contains("SECRET_VAR"),
        "resolution was not logged: {log}"
    );
    assert!(
        !log.contains(SECRET) && !log.contains(CREDENTIAL),
        "a secret reached a log line"
    );
}

// ---- probe ------------------------------------------------------------------

#[test]
fn a_version_is_a_leading_token_or_a_later_plain_one() {
    let rows = [
        ("2.1.223 (Claude Code)\n", Some("2.1.223")),
        ("3.0.0-beta.1 (Claude Code)\n", Some("3.0.0-beta.1")),
        ("codex-cli 0.149.1", Some("0.149.1")),
        (
            "harness-cli version 999.999.999-nightly-exotic-format\n",
            None,
        ),
        ("", None),
    ];
    for (output, version) in rows {
        assert_eq!(parse_version(output), version, "{output:?}");
    }
}

/// Probes the fake harness in `mode`, steered by one extra variable.
async fn probe_in(mode: &str, extra: (&str, &str)) -> HarnessCapability {
    let state = scratch("probe");
    let env = [("TACK_FAKE_HARNESS_MODE", mode), extra]
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .into();
    let harness = harness(state.path()).with_probe(Duration::from_millis(300), env);
    tokio::time::timeout(Duration::from_secs(5), harness.probe())
        .await
        .expect("a probe respects its own timeout")
}

/// `(mode, extra variable, version, error fragment, raw output kept)`.
#[tokio::test]
async fn a_probe_reports_a_version_or_says_why_it_cannot() {
    let version = ("TACK_FAKE_HARNESS_VERSION", "9.9.9");
    let exit = ("TACK_FAKE_HARNESS_EXIT_CODE", "3");
    let rows = [
        ("version", version, "9.9.9", None, false),
        (
            "unknown_version",
            version,
            "",
            Some("not a recognizable"),
            true,
        ),
        ("failure", exit, "", Some("status 3"), false),
        ("hang", version, "", Some("timed out"), false),
    ];
    for (mode, extra, version, error, keeps_raw) in rows {
        let probe = probe_in(mode, extra).await;
        assert_eq!(probe.installed_version, version, "{mode}");
        assert_eq!(probe.harness_kind.as_str(), KIND);
        let kept = probe.additional.contains_key("raw_version_output");
        assert_eq!(kept, keeps_raw, "{mode}");
        assert!(probe.additional.contains_key("note"));
        let reported = probe.probe_error.unwrap_or_default();
        assert!(reported.contains(error.unwrap_or("")), "{mode}: {reported}");
        assert_eq!(reported.is_empty(), error.is_none(), "{mode}");
        let passthrough = probe.model_passthrough.expect("passthrough attested");
        assert_eq!(passthrough.support, CapabilitySupport::Supported);
        assert!(probe.model_combinations.is_empty());
    }
}

// ---- cancel and reconcile ---------------------------------------------------

/// `spawn_child` keeps the shell itself alive, waiting on a child; `hang`
/// would replace it with `sleep`, which is no longer the harness program.
async fn start_hanging(harness: &Harness, workspace: &Path) -> (LocalRunHandle, u32) {
    let mut request = spec(KIND, workspace);
    set_env(&mut request, &[("TACK_FAKE_HARNESS_MODE", "spawn_child")]);
    let handle = harness.start(&request).await.expect("start");
    let pid = handle_pid(&handle.process_id).expect("the handle carries the pid");
    (handle, pid)
}

#[cfg(unix)]
#[tokio::test]
async fn cancel_stops_the_process_and_forgets_it() {
    let state = scratch("cancel");
    let harness = harness(state.path());
    let (handle, pid) = start_hanging(&harness, state.path()).await;
    assert!(crate::harness::process::process_alive(pid));

    let evidence = harness.cancel(&handle).await.expect("cancel");
    assert_eq!(evidence.observation, CancelObservation::ProcessStopped);
    assert_eq!(evidence.details["pid"], pid);
    assert!(!crate::harness::process::process_alive(pid));
    assert!(harness.running.lock().await.is_empty());
    assert!(matches!(
        harness.wait(&handle).await,
        Err(HarnessError::Process)
    ));
}

fn journal(process_id: Option<&str>) -> AttemptJournal {
    use crate::client::journal::{JournalState, WorkspaceJournal};
    AttemptJournal {
        attempt_id: crate::client::AttemptId::new("attempt"),
        runner_id: crate::client::RunnerId::new("runner"),
        fencing_token: crate::client::FencingToken(1),
        workspace: WorkspaceJournal {
            workspace_id: crate::client::WorkspaceId::new("ws_test"),
            path: PathBuf::from("/does-not-matter"),
            base_revision: "revision".into(),
        },
        state: JournalState::ProcessObservedRunning,
        process_id: process_id.map(str::to_owned),
        last_event_checkpoint: None,
        pending_terminal_report: None,
    }
}

/// A live pid is only `ProcessRunning` when it is still this harness's
/// program: the OS may have given the pid to something else since.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn reconcile_tells_this_harness_from_a_reused_pid() {
    let state = scratch("reconcile");
    let harness = harness(state.path());
    let (handle, _) = start_hanging(&harness, state.path()).await;
    let mut unrelated = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn");
    let mut exited = std::process::Command::new("true").spawn().expect("spawn");
    exited.wait().expect("reap");

    let rows = [
        (
            Some(handle.process_id.clone()),
            Ok(RecoveryObservation::ProcessRunning),
        ),
        (
            Some(unrelated.id().to_string()),
            Ok(RecoveryObservation::ProcessStopped),
        ),
        (
            Some(format!("codex:{}:0", exited.id())),
            Ok(RecoveryObservation::ProcessStopped),
        ),
        (None, Ok(RecoveryObservation::ProcessStopped)),
        (Some("not-a-pid".to_owned()), Err(())),
    ];
    for (process_id, expected) in rows {
        let observed = harness.reconcile(&journal(process_id.as_deref())).await;
        assert_eq!(observed.map_err(|_| ()), expected, "{process_id:?}");
    }
    harness.cancel(&handle).await.expect("cancel");
    let _ = unrelated.kill();
    let _ = unrelated.wait();
}
