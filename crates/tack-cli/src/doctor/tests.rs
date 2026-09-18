use super::*;
use tack_orch::execution::{HarnessKind, ModelCombination, ModelId, ModelProvider};

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
        model_combinations: Vec::new(),
        model_passthrough: None,
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

/// `render`/`render_model_info` are exercised for their side effects
/// (stdout), not a return value; this only proves they run to
/// completion for a shape with real model combinations, so a future
/// field addition to `ModelCombination` fails loudly here instead of
/// only in a manual `--json` read.
/// A fully-populated `DiscoveryReport` — every feature flag, one harness
/// with model combinations and passthrough, one configured provider — for
/// tests that only prove `render`/`render_provider` run to completion
/// over a realistic shape without panicking.
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
        provider_catalog: BTreeMap::from([(
            tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
            tack_runner::provider::CatalogStatus::Configured {
                model_count: 373,
                priced_model_count: 352,
                context_window_model_count: 355,
                checked_at: fixed_timestamp(),
            },
        )]),
    }
}

#[test]
fn render_does_not_panic_on_a_populated_report() {
    let mut with_models = capability("codex", "1.18.0", None);
    with_models.model_combinations = vec![ModelCombination {
        model_provider: ModelProvider::new("openai"),
        model_ids: vec![ModelId::new("grok-code")],
        discovery: "codex models".to_owned(),
        model_metadata: Default::default(),
        additional: Default::default(),
    }];
    with_models.model_passthrough = Some(CapabilityValue {
        support: CapabilitySupport::Unsupported,
        reason: Some("declaration-based only".to_owned()),
        additional: Default::default(),
    });

    render(&sample_discovery_report(vec![with_models]));
}

/// `render_provider` is exercised for its side effects (stdout), not a
/// return value; this only proves every `CatalogStatus` variant renders
/// to completion, so a future variant addition fails loudly here
/// instead of only in a manual `--json` read. Also proves a provider
/// this build knows about but the map has no entry for (an empty map)
/// renders as "not configured" instead of panicking on a missing key.
#[test]
fn render_provider_does_not_panic_for_any_catalog_status() {
    render_provider(&BTreeMap::new());
    for status in [
        tack_runner::provider::CatalogStatus::NotConfigured,
        tack_runner::provider::CatalogStatus::SecretUnresolved,
        tack_runner::provider::CatalogStatus::Unreachable { status: Some(401) },
        tack_runner::provider::CatalogStatus::Unreachable { status: None },
        tack_runner::provider::CatalogStatus::Configured {
            model_count: 373,
            priced_model_count: 352,
            context_window_model_count: 355,
            checked_at: fixed_timestamp(),
        },
    ] {
        let mut catalog = BTreeMap::new();
        for provider in tack_runner::provider::registry() {
            catalog.insert(provider.config_key().to_owned(), status.clone());
        }
        render_provider(&catalog);
    }
}
