use std::{collections::BTreeMap, path::PathBuf, process::ExitCode, sync::Arc, time::Duration};

use clap::Parser;
use tack_orch::execution::{
    CapabilityLimits, CapabilityValue, Concurrency, FeatureCapabilities, ProtocolVersion,
    RunnerCapabilities,
};
use tack_runner::{
    Clock, ConfigError, ConfigOverrides, EnrollmentCredential, LocalFilesystem, RunnerConfig,
    RunnerConfigSources, RunnerRuntime, Shutdown, SystemClock, SystemProcessSupervisor,
    client::{
        HttpPullProtocol, HttpRunnerClient, OwnerOnlyJournal, RetryPolicy, RunnerEngine,
        WorkspaceManager, workspace::git::GitWorktreeProvisioner,
    },
    harness::{
        AdapterRegistry, HarnessProbe, PROCESS_GROUP_CANCEL_CEILING,
        claude_code::ClaudeCodeAdapter, codex::CodexAdapter, opencode::OpenCodeAdapter,
        process::ProcessLimits,
    },
};

/// Bounds applied to every harness subprocess this runner spawns. Explicit
/// rather than defaulted: each value is a real operational choice, and
/// `ProcessLimits` deliberately has no `Default` for that reason.
const HARNESS_PROCESS_LIMITS: ProcessLimits =
    ProcessLimits::new(4 * 1024 * 1024, 1024 * 1024, Duration::from_secs(3_600));

/// How long any single non-claim protocol call may take. Claims set their own,
/// longer, budget from the long-poll window they request.
const PROTOCOL_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Parser)]
#[command(name = "tack-runner", about = "Local pull-based Tack runner")]
struct Cli {
    /// Optional TOML configuration file.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Runner protocol endpoint. Overrides file and environment configuration.
    #[arg(long)]
    api_url: Option<String>,
    /// Stable identifier sent to the control plane.
    #[arg(long)]
    runner_id: Option<String>,
    /// Local directory for runner state. Overrides file and environment configuration.
    #[arg(long)]
    state_dir: Option<PathBuf>,
    /// Enrollment credential. Prefer TACK_RUNNER_ENROLLMENT_TOKEN so it is not visible in shell history.
    #[arg(long, hide_env_values = true)]
    enrollment_token: Option<String>,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Error types are deliberately credential-free; never print config or CLI debug output here.
            eprintln!("tack-runner: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), tack_runner::RunnerError> {
    let file_toml = read_config(cli.config.as_deref())?;
    let config = RunnerConfig::from_sources(RunnerConfigSources {
        file_toml: file_toml.as_deref(),
        environment: RunnerConfig::environment_overrides(),
        command_line: ConfigOverrides {
            api_base_url: cli.api_url,
            runner_id: cli.runner_id,
            state_dir: cli.state_dir,
            enrollment_credential: cli.enrollment_token.map(EnrollmentCredential::new),
        },
    })?;

    init_tracing();
    // Check before any filesystem side effect or protocol work. The value itself
    // is intentionally never included in diagnostics or tracing fields.
    config.require_enrollment_credential()?;

    // The real transport replaces `UnavailableProtocolClient`, the stub that
    // is otherwise the only production `RunnerProtocolClient` in the tree
    // and cannot reach a server at all.
    let staging_root = config.state_dir.join("staging");
    let adapters = build_adapter_registry(&staging_root);
    let capabilities = report_capabilities(&adapters, &SystemClock).await;
    let protocol = Arc::new(HttpPullProtocol::new(
        &config.api_base_url,
        PROTOCOL_REQUEST_TIMEOUT,
        RetryPolicy::default(),
    )?);
    let engine = RunnerEngine::new(
        Arc::clone(&protocol),
        adapters,
        OwnerOnlyJournal::new(config.state_dir.join("journal")),
        // Every claimed attempt gets its own real git checkout under the
        // runner's state directory. This replaces
        // `UnavailableWorktreeProvisioner`, which refuses every provision
        // with a typed `WorktreeUnavailable`.
        WorkspaceManager::new(
            config.state_dir.join("workspaces"),
            GitWorktreeProvisioner::default(),
        ),
    )
    // `HttpPullProtocol` implements `AttemptDataProtocol`; without attaching
    // it here, `engine.rs`'s real call sites for events/artifacts would
    // never run in the production binary even though the code compiles.
    .with_data_protocol(Arc::clone(&protocol) as Arc<dyn tack_runner::client::AttemptDataProtocol>);
    let client = HttpRunnerClient::new(protocol, engine, config.clone(), SystemClock, capabilities);

    let runtime = RunnerRuntime::new(
        client,
        SystemProcessSupervisor,
        LocalFilesystem,
        SystemClock,
        config,
    );
    let (shutdown, shutdown_handle) = Shutdown::channel();
    let mut runtime_task = tokio::spawn(runtime.run(shutdown));

    tokio::select! {
        result = &mut runtime_task => result.map_err(|_| tack_runner::RunnerError::ClientTaskJoin)?,
        signal = tokio::signal::ctrl_c() => {
            signal.map_err(|_| tack_runner::RunnerError::ClientTaskJoin)?;
            tracing::info!("shutdown signal received");
            shutdown_handle.request();
            runtime_task.await.map_err(|_| tack_runner::RunnerError::ClientTaskJoin)??;
            Ok(())
        }
    }
}

fn read_config(path: Option<&std::path::Path>) -> Result<Option<String>, ConfigError> {
    path.map(|path| std::fs::read_to_string(path).map_err(|_| ConfigError::Unreadable))
        .transpose()
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
}

/// Registers every harness whose binary this machine actually has.
///
/// A harness that cannot be discovered is **not registered** rather than
/// registered with a placeholder: `AdapterRegistry::resolve` then reports a
/// typed "no adapter is registered for harness kind" instead of accepting an
/// attempt it could never run. Each harness needs two instances because
/// `register_adapter` and `register_probe` each take an owned box; the probe
/// copy's version cache is therefore separate from the adapter copy's, so the
/// adapter falls back to its own one-off version detection at `wait()` time —
/// documented behaviour, never a fabricated version.
fn build_adapter_registry(staging_root: &std::path::Path) -> AdapterRegistry {
    let mut registry = AdapterRegistry::new();

    let codex = CodexAdapter::discover(HARNESS_PROCESS_LIMITS, staging_root.to_path_buf());
    let kind = HarnessProbe::harness_kind(&codex);
    registry.register_adapter(kind, Box::new(codex));
    if registry
        .register_probe(Box::new(CodexAdapter::discover(
            HARNESS_PROCESS_LIMITS,
            staging_root.to_path_buf(),
        )))
        .is_err()
    {
        tracing::warn!(harness = "codex", "probe rejected at registration");
    }

    match ClaudeCodeAdapter::discover() {
        Ok(adapter) => {
            let kind = HarnessProbe::harness_kind(&adapter);
            registry.register_adapter(kind, Box::new(adapter));
            match ClaudeCodeAdapter::discover() {
                Ok(probe) => {
                    if registry.register_probe(Box::new(probe)).is_err() {
                        tracing::warn!(harness = "claude_code", "probe rejected at registration");
                    }
                }
                Err(_) => tracing::warn!(harness = "claude_code", "probe could not be built"),
            }
        }
        // The reason string can name a filesystem path; only the fact is logged.
        Err(_) => tracing::info!(harness = "claude_code", "binary not found; not registered"),
    }

    let opencode = OpenCodeAdapter::discover(HARNESS_PROCESS_LIMITS, staging_root.to_path_buf());
    let kind = HarnessProbe::harness_kind(&opencode);
    registry.register_adapter(kind, Box::new(opencode));
    if registry
        .register_probe(Box::new(OpenCodeAdapter::discover(
            HARNESS_PROCESS_LIMITS,
            staging_root.to_path_buf(),
        )))
        .is_err()
    {
        tracing::warn!(harness = "opencode", "probe rejected at registration");
    }

    registry
}

/// Builds the capability snapshot sent at enrollment and on every refresh.
///
/// Every feature statement here is deliberately conservative, because a
/// capability claim is load-bearing: the scheduler and the operator UI both
/// offer only what a runner says it can do.
///
/// - `cancel` reports [`PROCESS_GROUP_CANCEL_CEILING`] (advisory), the honest
///   ceiling of a process-group signal, which cannot reliably reach a
///   descendant a harness spawns into a new OS session.
/// - `artifacts` reports **advisory**: `engine.rs` has a real call site
///   (`RunnerEngine::submit_terminal_evidence`), so a completed attempt
///   with a staged artifact genuinely uploads it — but only when an
///   adapter happens to stage one (`terminal_reason.artifact`), and only
///   best-effort (a transport failure is logged, never retried or replayed
///   on restart, and never blocks the attempt's own completion). That
///   conditionality is exactly what `advisory` is for; `supported` would
///   overclaim an upload this runner cannot yet guarantee.
/// - `decisions` still reports **unsupported**. `AttemptDataProtocol::
///   create_decision`/`poll_decisions` are implemented and reachable from
///   `engine.rs`, but no harness adapter in this tree ever asks a question a
///   decision could answer — there is still no call site that would ever
///   open one. Claiming support for a path nothing calls is exactly the kind
///   of lie the "capability claims are load-bearing" rule forbids.
async fn report_capabilities<C: Clock>(
    adapters: &AdapterRegistry,
    clock: &C,
) -> RunnerCapabilities {
    let harnesses = adapters.capabilities().await;
    let feature = |support, reason: Option<&str>| CapabilityValue {
        support,
        reason: reason.map(str::to_owned),
        additional: BTreeMap::new(),
    };
    RunnerCapabilities {
        protocol_version: Some(ProtocolVersion::v1()),
        runner_version: env!("CARGO_PKG_VERSION").to_owned(),
        reported_at: chrono::DateTime::<chrono::Utc>::from(clock.now()),
        labels: BTreeMap::from([
            ("os".to_owned(), std::env::consts::OS.to_owned()),
            ("arch".to_owned(), std::env::consts::ARCH.to_owned()),
        ]),
        concurrency: Concurrency {
            total: 1,
            available: 1,
            additional: BTreeMap::new(),
        },
        harnesses,
        features: FeatureCapabilities {
            cancel: feature(
                PROCESS_GROUP_CANCEL_CEILING,
                Some("process-group signal cannot reach a detached descendant"),
            ),
            resume: feature(
                tack_orch::execution::CapabilitySupport::Unsupported,
                Some("no resumable session contract"),
            ),
            decisions: feature(
                tack_orch::execution::CapabilitySupport::Unsupported,
                Some("no harness adapter in this tree ever opens a decision"),
            ),
            artifacts: feature(
                tack_orch::execution::CapabilitySupport::Advisory,
                Some("uploaded when an adapter stages one; best-effort, not replayed on restart"),
            ),
            usage: feature(
                tack_orch::execution::CapabilitySupport::Advisory,
                Some("usage is reported only when a harness emits it"),
            ),
            additional: BTreeMap::new(),
        },
        limits: CapabilityLimits {
            event_payload_bytes_max: 65_536,
            artifact_content_bytes_max: 52_428_800,
            additional: BTreeMap::new(),
        },
        additional: BTreeMap::new(),
    }
}
