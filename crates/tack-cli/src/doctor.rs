//! `tack runner doctor`: reports which harness binaries this machine has,
//! what each declares it can do, and where its credentials come from —
//! without enrolling a runner or requiring a running server.
//!
//! Everything printed here comes from [`tack_runner::bootstrap::probe`], the
//! same discovery/capability-probing step a real enrollment or refresh
//! performs. There is no second, independently derived guess at what a
//! runner would report: this command and a live runner share one source of
//! truth.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use tack_orch::execution::{
    CapabilitySupport, CapabilityValue, FeatureCapabilities, HarnessCapability, ModelListSource,
    SignInState,
};
use tack_runner::{
    ConfigOverrides, RunnerConfig, SecretStore, bootstrap, harness::process::ProcessLimits,
};

/// Irrelevant to a probe (the `--version` call is bounded by its own,
/// shorter, internal timeout), but `harness::discover`
/// requires a value and this command never constructs a real `RunnerConfig`
/// to source one from. Mirrors `local_runner.rs`'s own
/// `HARNESS_PROCESS_LIMITS` so the two callers of the same composition root
/// stay visibly consistent.
const PROCESS_LIMITS: ProcessLimits =
    ProcessLimits::new(4 * 1024 * 1024, 1024 * 1024, Duration::from_secs(3_600));

/// Runs the probe and prints the report; `as_json` switches to the raw
/// [`tack_orch::execution::RunnerCapabilities`] snapshot instead of the
/// human-readable rendering, so it can be diffed byte-for-byte against
/// whatever a real enrollment or refresh sent a server.
pub fn run(as_json: bool, config_path: Option<PathBuf>) -> anyhow::Result<()> {
    let config = read_config(config_path)?;
    let runtime = tokio::runtime::Runtime::new()?;
    let (report, verify, daemon) = runtime.block_on(probe(config));

    if as_json {
        println!("{}", serde_json::to_string_pretty(&report.capabilities)?);
        return Ok(());
    }

    render(&report, daemon.as_ref());
    render_verify(&verify);
    Ok(())
}

/// The runner's own file (`--config`, else `TACK_LOCAL_RUNNER_CONFIG`), read the way a live
/// runner reads it: a broken file is an error, never a silent default.
fn read_config(config_path: Option<PathBuf>) -> anyhow::Result<RunnerConfig> {
    Ok(crate::local_runner::load_runner_config(
        ConfigOverrides::default(),
        config_path
            .or_else(crate::local_runner::runner_config_path)
            .as_deref(),
    )?)
}

/// The report, the `[verify]` table, and how the docket daemon answers when
/// docket is configured at all, all from the runner's own configuration.
async fn probe(
    config: RunnerConfig,
) -> (
    bootstrap::DiscoveryReport,
    tack_runner::VerifyConfig,
    Option<Result<String, String>>,
) {
    // A doctor run never claims or executes an attempt, so the only thing
    // this path feeds — `wait()`'s artifact-staging directory — is never
    // reached; nothing is created or written under it.
    // Named but never created or written — see the comment above — so this is
    // not a test artifact and takes no `tempfile` guard.
    #[allow(clippy::disallowed_methods)]
    let staging_root = std::env::temp_dir().join("tack-runner-doctor-unused-staging");
    // The secret store is the one the same file's state directory names, so
    // the backend this reports is the one a live runner would actually pick.
    let secrets = SecretStore::open(&config.secret_store_path());
    let report = bootstrap::probe(&staging_root, &PROCESS_LIMITS, &secrets, &config.docket).await;
    let daemon = if config.docket.configured() {
        Some(tack_runner::harness::docket::daemon::describe(&config.docket).await)
    } else {
        None
    };
    (report, config.verify, daemon)
}

/// The `[verify]` table as this runner would read it. The board never runs
/// the verifier; this machine's runner does, after a succeeded attempt.
fn render_verify(verify: &tack_runner::VerifyConfig) {
    if !verify.enabled {
        println!();
        println!("verify: disabled");
        return;
    }
    let found = if tack_runner::verify::program_found(&verify.program) {
        "found"
    } else {
        "NOT FOUND on PATH; every succeeded attempt will report verify_failed"
    };
    println!();
    println!("verify: enabled");
    println!("  kind:    {}", verify.kind);
    println!("  program: {} ({found})", verify.program);
    println!("  args:    {}", verify.args.join(" "));
    println!("  timeout: {}s", verify.timeout_seconds);
}

/// What this machine's probe found for one harness kind.
enum HarnessStatus<'a> {
    /// `version` is `None` when the harness publishes none (docket reports its API instead).
    Present {
        version: Option<&'a str>,
    },
    /// The binary was found but a probe step failed — an unparseable version string, a nonzero exit,
    /// or a timed-out process. Distinct from `Absent`:
    /// this machine can find the harness, something about probing it went
    /// wrong.
    ProbeError {
        version: Option<&'a str>,
        reason: &'a str,
    },
    Absent {
        reason: &'a str,
    },
}

/// Classifies one harness kind from the raw probe output. A harness whose
/// binary could not be located is never registered, so it has no capability
/// entry; `missing` holds why.
fn classify<'a>(
    kind: &str,
    harnesses: &'a [HarnessCapability],
    missing: &'a BTreeMap<String, String>,
) -> HarnessStatus<'a> {
    let Some(capability) = harnesses.iter().find(|h| h.harness_kind.as_str() == kind) else {
        let reason = missing
            .get(kind)
            .map_or("harness not registered", String::as_str);
        return HarnessStatus::Absent { reason };
    };
    let version =
        (!capability.installed_version.is_empty()).then_some(capability.installed_version.as_str());
    match &capability.probe_error {
        None => HarnessStatus::Present { version },
        Some(reason) => HarnessStatus::ProbeError { version, reason },
    }
}

fn render(report: &bootstrap::DiscoveryReport, daemon: Option<&Result<String, String>>) {
    let capabilities = &report.capabilities;
    println!("Tack runner doctor — harness discovery for this machine");
    println!();

    for descriptor in tack_runner::harness::DESCRIPTORS {
        let kind = descriptor.kind;
        let status = classify(kind, &capabilities.harnesses, &report.missing_harnesses);

        println!("{kind}");
        match status {
            HarnessStatus::Present { version } => {
                println!("  status:      present");
                if let Some(version) = version {
                    println!("  version:     {version}");
                }
            }
            HarnessStatus::ProbeError { version, reason } => {
                println!("  status:      present, probe error");
                if let Some(version) = version {
                    println!("  version:     {version} (partially confirmed)");
                }
                println!("  probe_error: {reason}");
            }
            HarnessStatus::Absent { reason } => {
                println!("  status:      absent");
                println!("  reason:      {reason}");
            }
        }
        println!("  credentials: {}", descriptor.credential_note);

        if let Some(capability) = capabilities
            .harnesses
            .iter()
            .find(|h| h.harness_kind.as_str() == kind)
        {
            render_sandbox(capability);
            if let (true, Some(daemon)) = (kind == "docket", daemon) {
                for line in docket_lines(capability, daemon) {
                    println!("{line}");
                }
            }
            for line in readiness_lines(capability) {
                println!("{line}");
            }
        }
        if let Some(declared) = report.declared.get(kind) {
            for line in declared_lines(declared) {
                println!("{line}");
            }
        }
        println!();
    }

    println!(
        "Runner-wide summary (what the runner as a whole can do; artifacts and permission policy differ per harness, above):"
    );
    render_feature("cancel", &capabilities.features.cancel);
    render_feature("resume", &capabilities.features.resume);
    render_feature("decisions", &capabilities.features.decisions);
    render_feature("artifacts", &capabilities.features.artifacts);
    render_feature("usage", &capabilities.features.usage);
    println!();

    println!("Secret store (resolves `secret_reference` environment entries):");
    println!("  backend: {}", report.secret_backend);
    println!(
        "  note: the platform credential store gets {:?} to answer before this falls back to \
         an owner-only file; a store that is slow to start up (a Secret Service activating \
         over D-Bus, for example) can miss that window on one boot and clear it on the next, \
         so which backend answers for the same secret name is not guaranteed to stay the same \
         across restarts.",
        tack_runner::secrets::PLATFORM_STORE_TIMEOUT
    );
    println!();
}

fn render_feature(name: &str, value: &CapabilityValue) {
    let support = support_label(value.support);
    match &value.reason {
        Some(reason) => println!("  {name:<10} {support:<11} — {reason}"),
        None => println!("  {name:<10} {support}"),
    }
}

/// The two features that differ per harness, as printed under its block:
/// what it keeps and whether its tool list and network flag are enforced.
fn declared_lines(declared: &FeatureCapabilities) -> Vec<String> {
    let line = |name: &str, support: CapabilitySupport, reason: &str| {
        let label = match support {
            CapabilitySupport::Unsupported => "not supported",
            other => support_label(other),
        };
        format!("  {name:<18} {label} — {reason}")
    };
    let artifacts = &declared.artifacts;
    let policy = declared
        .additional
        .get("permission_policy")
        .and_then(|v| serde_json::from_value::<CapabilityValue>(v.clone()).ok());
    vec![
        line(
            "artifacts:",
            artifacts.support,
            artifacts.reason.as_deref().unwrap_or("no reason given"),
        ),
        match policy {
            Some(p) => line(
                "permission policy:",
                p.support,
                p.reason.as_deref().unwrap_or("no reason given"),
            ),
            None => line(
                "permission policy:",
                CapabilitySupport::Unsupported,
                "this harness declares no policy",
            ),
        },
    ]
}

/// One line: `sandbox: bwrap`, or `sandbox: none (<why>)`.
fn render_sandbox(capability: &HarnessCapability) {
    let field = |name: &str| capability.additional.get(name).and_then(|v| v.as_str());
    match (field("sandbox"), field("sandbox_reason")) {
        (Some("bwrap"), _) => println!("  sandbox:     bwrap"),
        (Some(kind), Some(reason)) => println!("  sandbox:     {kind} ({reason})"),
        (Some(kind), None) => println!("  sandbox:     {kind}"),
        (None, _) => {}
    }
}

/// The `signed in:` and `models:` lines of one harness.
fn readiness_lines(capability: &HarnessCapability) -> Vec<String> {
    const SHOWN: usize = 5;
    let signed_in = match &capability.sign_in {
        Some(sign_in) => {
            let command = sign_in
                .command
                .as_deref()
                .map(|command| format!(" — run `{command}`"))
                .unwrap_or_default();
            match sign_in.state {
                SignInState::SignedIn => format!("yes ({})", sign_in.detail),
                SignInState::SignedOut => format!("no ({}){command}", sign_in.detail),
                SignInState::Unknown => format!("unknown ({})", sign_in.detail),
            }
        }
        None => "(not reported)".to_owned(),
    };
    let needs = if capability.requires_model {
        "a request must name one"
    } else {
        "the agent chooses when none is named"
    };
    let models = match &capability.models {
        Some(models) if models.source == ModelListSource::None => format!(
            "none listed ({}); {needs}",
            models.reason.as_deref().unwrap_or("no reason given")
        ),
        Some(models) => {
            let source = match models.source {
                ModelListSource::Listed => "listed",
                _ => "suggested",
            };
            let more = models.ids.len().saturating_sub(SHOWN);
            let tail = if more > 0 {
                format!(", and {more} more")
            } else {
                String::new()
            };
            let default = capability
                .default_model
                .as_deref()
                .map(|model| format!("; default {model}"))
                .unwrap_or_default();
            format!(
                "{source}: {}{tail}{default}; {needs}",
                models
                    .ids
                    .iter()
                    .take(SHOWN)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        None => format!("(not reported); {needs}"),
    };
    vec![
        format!("  signed in:   {signed_in}"),
        format!("  models:      {models}"),
    ]
}

fn support_label(support: CapabilitySupport) -> &'static str {
    match support {
        CapabilitySupport::Supported => "supported",
        CapabilitySupport::Unsupported => "unsupported",
        CapabilitySupport::Advisory => "advisory",
    }
}

#[cfg(test)]
#[path = "doctor/tests.rs"]
mod tests;

/// The lines under the `docket` block: the API it speaks, whether its daemon
/// answers, and one line per team. `daemon` is what
/// [`tack_runner::harness::docket::daemon::describe`] found.
fn docket_lines(capability: &HarnessCapability, daemon: &Result<String, String>) -> Vec<String> {
    let version = capability.api_version().unwrap_or("unknown");
    let api = match &capability.probe_error {
        Some(reason) => format!("{version} ({reason})"),
        None => version.to_owned(),
    };
    let mut lines = vec![
        format!("  api:         {api}"),
        format!(
            "  daemon:      {}",
            match daemon {
                Ok(running) | Err(running) => running,
            }
        ),
    ];
    for team in capability.teams.iter().flatten() {
        let fixes: Vec<&str> = team.violations.iter().map(|v| v.fix.as_str()).collect();
        let state = if team.profile == "lights-out" && fixes.is_empty() {
            "lights-out".to_owned()
        } else if fixes.is_empty() {
            "attended".to_owned()
        } else {
            format!("attended (fix: {})", fixes.join("; "))
        };
        lines.push(format!(
            "  team {}: {state} · {}",
            team.name,
            reads_line(team.read_only.as_deref())
        ));
    }
    lines
}

/// What a team's sandbox reads besides its own work: the folders docket granted, nothing, or
/// that this docket does not say.
fn reads_line(read_only: Option<&[String]>) -> String {
    match read_only {
        None => "reads: not reported by this docket".to_owned(),
        Some([]) => "reads nothing outside its work".to_owned(),
        Some(paths) => format!("reads {}", paths.join(", ")),
    }
}
