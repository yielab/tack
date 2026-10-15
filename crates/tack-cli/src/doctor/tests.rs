use super::*;
use tack_orch::execution::{HarnessKind, ModelListSource, ModelsReport, SignInReport};

fn fixed_timestamp() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339("2026-08-06T12:00:00Z")
        .expect("fixture timestamp")
        .into()
}

fn capability(kind: &str, installed_version: &str, probe_error: Option<&str>) -> HarnessCapability {
    HarnessCapability {
        harness_kind: HarnessKind::new(kind),
        installed_version: installed_version.to_owned(),
        probe_error: probe_error.map(str::to_owned),
        probed_at: fixed_timestamp(),
        decisions: None,
        requires_model: false,
        requires_model_reason: None,
        models: None,
        default_model: None,
        sign_in: None,
        docs: None,
        autonomy_ceiling: None,
        teams: None,
        additional: Default::default(),
    }
}

/// `(name, capability entry, why it is missing, expected status)`. Absent
/// means "not installed": a located binary whose probe then fails is a
/// probe error against a present harness, with whatever version it confirmed.
#[test]
fn classify_separates_present_probe_error_and_absent() {
    let absent = "no executable named `claude` was found on PATH";
    let missing = BTreeMap::from([("claude-code".to_owned(), absent.to_owned())]);
    let codex = |version, error| Some(capability("codex", version, error));
    let rows: [(&str, Option<HarnessCapability>, &str); 5] = [
        ("codex", codex("0.149.1", None), "present 0.149.1"),
        ("codex", codex("", Some("unparseable")), "probe_error -"),
        (
            "codex",
            codex("1.18.0", Some("later")),
            "probe_error 1.18.0",
        ),
        ("claude-code", None, &format!("absent {absent}")),
        ("codex", None, "absent harness not registered"),
    ];
    for (kind, entry, expected) in rows {
        let harnesses: Vec<HarnessCapability> = entry.into_iter().collect();
        let described = match classify(kind, &harnesses, &missing) {
            HarnessStatus::Present { version } => format!("present {version}"),
            HarnessStatus::ProbeError { version, .. } => {
                format!("probe_error {}", version.unwrap_or("-"))
            }
            HarnessStatus::Absent { reason } => format!("absent {reason}"),
        };
        assert_eq!(described, expected, "{kind}");
    }
}

/// A fully-populated `DiscoveryReport` — every feature flag and the given
/// harnesses — for tests that only prove `render` runs to completion over a
/// realistic shape without panicking.
fn sample_discovery_report(harnesses: Vec<HarnessCapability>) -> bootstrap::DiscoveryReport {
    bootstrap::DiscoveryReport {
        capabilities: tack_orch::execution::RunnerCapabilities {
            protocol_version: None,
            runner_version: "0.0.0-test".to_owned(),
            reported_at: fixed_timestamp(),
            labels: Default::default(),
            concurrency: tack_orch::execution::Concurrency {
                total: 1,
                available: 1,
                additional: Default::default(),
            },
            harnesses,
            features: tack_orch::execution::FeatureCapabilities {
                cancel: CapabilityValue {
                    support: CapabilitySupport::Advisory,
                    reason: None,
                    additional: Default::default(),
                },
                resume: CapabilityValue {
                    support: CapabilitySupport::Unsupported,
                    reason: None,
                    additional: Default::default(),
                },
                decisions: CapabilityValue {
                    support: CapabilitySupport::Unsupported,
                    reason: None,
                    additional: Default::default(),
                },
                artifacts: CapabilityValue {
                    support: CapabilitySupport::Advisory,
                    reason: None,
                    additional: Default::default(),
                },
                usage: CapabilityValue {
                    support: CapabilitySupport::Advisory,
                    reason: None,
                    additional: Default::default(),
                },
                additional: Default::default(),
            },
            limits: tack_orch::execution::CapabilityLimits {
                event_payload_bytes_max: 1,
                artifact_content_bytes_max: 1,
                additional: Default::default(),
            },
            additional: Default::default(),
        },
        missing_harnesses: BTreeMap::from([(
            "claude-code".to_owned(),
            "no executable named `claude` was found on PATH".to_owned(),
        )]),
        secret_backend: tack_runner::secrets::SecretBackendKind::File,
        declared: BTreeMap::new(),
    }
}

#[test]
fn render_does_not_panic_on_a_populated_report() {
    let mut with_models = capability("codex", "1.18.0", None);
    with_models.requires_model = true;

    render(&sample_discovery_report(vec![with_models]));
}

/// A harness with artifacts and a policy prints both lines; one without
/// prints `not supported` with its reason.
#[test]
fn declared_lines_state_artifacts_and_policy_per_harness() {
    let value = |support, reason: &str| CapabilityValue {
        support,
        reason: Some(reason.to_owned()),
        additional: Default::default(),
    };
    let features = |artifacts, policy: Option<CapabilityValue>| {
        let mut features = sample_discovery_report(Vec::new()).capabilities.features;
        features.artifacts = artifacts;
        features.additional = policy
            .map(|p| BTreeMap::from([("permission_policy".to_owned(), serde_json::json!(p))]))
            .unwrap_or_default();
        features
    };
    let rows = [
        (
            features(
                value(CapabilitySupport::Supported, "lists written files"),
                Some(value(CapabilitySupport::Supported, "passed as --policy")),
            ),
            vec![
                "  artifacts:         supported — lists written files",
                "  permission policy: supported — passed as --policy",
            ],
        ),
        (
            features(value(CapabilitySupport::Unsupported, "only the log"), None),
            vec![
                "  artifacts:         not supported — only the log",
                "  permission policy: not supported — this harness declares no policy",
            ],
        ),
    ];
    for (declared, expected) in rows {
        assert_eq!(declared_lines(&declared), expected);
    }
}

/// A harness prints one `signed in:` and one `models:` line, whatever it
/// could and could not read.
#[test]
fn readiness_lines_say_who_is_signed_in_and_which_models_exist() {
    let ids = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
    let sign_in = |state, detail: &str, command: Option<&str>| {
        Some(SignInReport {
            state,
            detail: detail.to_owned(),
            command: command.map(str::to_owned),
        })
    };
    let models = |source, names: &[&str], reason: Option<&str>| {
        Some(ModelsReport {
            source,
            ids: ids(names),
            reason: reason.map(str::to_owned),
        })
    };
    type Row = (
        &'static str,
        Option<SignInReport>,
        Option<ModelsReport>,
        Option<&'static str>,
        bool,
        [&'static str; 2],
    );
    let rows: [Row; 5] = [
        (
            "signed in with a long list and a default",
            sign_in(SignInState::SignedIn, "claude.ai", Some("claude")),
            models(
                ModelListSource::Listed,
                &["a", "b", "c", "d", "e", "f", "g"],
                None,
            ),
            Some("a"),
            true,
            [
                "  signed in:   yes (claude.ai)",
                "  models:      listed: a, b, c, d, e, and 2 more; default a; a request must name one",
            ],
        ),
        (
            "signed out names the command",
            sign_in(SignInState::SignedOut, "Not logged in", Some("codex login")),
            models(ModelListSource::Suggested, &["sonnet"], None),
            None,
            false,
            [
                "  signed in:   no (Not logged in) — run `codex login`",
                "  models:      suggested: sonnet; the agent chooses when none is named",
            ],
        ),
        (
            "unknown sign-in, no models",
            sign_in(SignInState::Unknown, "read elsewhere", None),
            models(ModelListSource::None, &[], Some("docket lists none")),
            None,
            true,
            [
                "  signed in:   unknown (read elsewhere)",
                "  models:      none listed (docket lists none); a request must name one",
            ],
        ),
        (
            "a list that failed",
            sign_in(SignInState::SignedIn, "ok", None),
            models(ModelListSource::None, &[], None),
            None,
            false,
            [
                "  signed in:   yes (ok)",
                "  models:      none listed (no reason given); the agent chooses when none is named",
            ],
        ),
        (
            "a runner that said nothing",
            None,
            None,
            None,
            false,
            [
                "  signed in:   (not reported)",
                "  models:      (not reported); the agent chooses when none is named",
            ],
        ),
    ];
    for (name, sign_in, models, default, requires, expected) in rows {
        let mut cap = capability("codex", "1", None);
        cap.sign_in = sign_in;
        cap.models = models;
        cap.default_model = default.map(str::to_owned);
        cap.requires_model = requires;
        assert_eq!(readiness_lines(&cap), expected, "{name}");
    }
}
