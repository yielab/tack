//! Composes the runner this crate ships: the real HTTP protocol client,
//! every in-tree harness adapter, git-worktree workspaces, and an
//! owner-only journal.
//!
//! This is the crate's single composition root. `tack-runner`'s own `main`
//! calls it after parsing arguments and reading configuration; any other
//! process that wants to host the runner role calls the exact same
//! function, so there is one wiring of adapters, capabilities, protocol,
//! engine, journal and workspace to keep honest against
//! `docs/contracts/runner-v1/` — not a copy that can drift from it.

use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};

use tack_orch::execution::{
    CapabilityLimits, CapabilityValue, Concurrency, FeatureCapabilities, ProtocolVersion,
    RunnerCapabilities,
};

use crate::{
    Clock, LocalFilesystem, RunnerConfig, RunnerError, RunnerRuntime, SecretStore, Shutdown,
    SystemClock, SystemProcessSupervisor,
    client::{
        AttemptDataProtocol, HttpPullProtocol, HttpRunnerClient, OwnerOnlyJournal, RetryPolicy,
        RunnerEngine, WorkspaceManager, workspace::git::GitWorktreeProvisioner,
    },
    harness::{
        AdapterRegistry, HarnessProbe, PROCESS_GROUP_CANCEL_CEILING,
        claude_code::ClaudeCodeAdapter, codex::CodexAdapter, opencode::OpenCodeAdapter,
        process::ProcessLimits,
    },
};

/// The concrete runtime this crate ships: the real HTTP protocol, every
/// in-tree harness adapter, git-worktree workspaces, an owner-only journal.
pub type ProductionRunnerRuntime = RunnerRuntime<
    HttpRunnerClient<AdapterRegistry, GitWorktreeProvisioner, SystemClock>,
    SystemProcessSupervisor,
    LocalFilesystem,
    SystemClock,
>;

/// Operational bounds every composer of [`build_runtime`] must choose.
///
/// Each field governs how much of the host a spawned harness subprocess, or
/// a single protocol call, may consume. Neither has a sane process-wide
/// default: [`ProcessLimits`] deliberately implements no [`Default`] for the
/// same reason this struct does not either — a value silently inherited
/// here would hide a real operational choice from whoever is composing the
/// runner.
#[derive(Debug, Clone)]
pub struct RunnerLimits {
    pub harness_process: ProcessLimits,
    pub protocol_request_timeout: Duration,
}

/// Builds a runtime identical to the one `tack-runner`'s binary has always
/// assembled, without starting it.
///
/// Fails fast on a missing enrollment credential before any filesystem
/// side effect or protocol work — the credential itself is never put into
/// the returned error, a log line, or any diagnostic.
pub async fn build_runtime(
    config: RunnerConfig,
    limits: RunnerLimits,
) -> Result<ProductionRunnerRuntime, RunnerError> {
    config.require_enrollment_credential()?;

    // The real transport replaces `UnavailableProtocolClient`, the stub that
    // is otherwise the only production `RunnerProtocolClient` in the tree
    // and cannot reach a server at all.
    let staging_root = config.state_dir.join("staging");
    let secrets = SecretStore::open(&config.secret_store_path());
    let adapters = build_adapter_registry(
        &limits.harness_process,
        &staging_root,
        &secrets,
        &config.providers,
    );
    let mut capabilities = report_capabilities(&adapters, &SystemClock).await;
    crate::provider::attach_catalog(&mut capabilities, &config.providers, &secrets, &SystemClock)
        .await;
    let protocol = Arc::new(HttpPullProtocol::new(
        &config.api_base_url,
        limits.protocol_request_timeout,
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
    .with_data_protocol(Arc::clone(&protocol) as Arc<dyn AttemptDataProtocol>);
    let client = HttpRunnerClient::new(protocol, engine, config.clone(), SystemClock, capabilities);

    Ok(RunnerRuntime::new(
        client,
        SystemProcessSupervisor,
        LocalFilesystem,
        SystemClock,
        config,
    ))
}

/// Builds the production runtime and runs it to completion under `shutdown`.
///
/// This is the whole composition root as a single call: a binary's `main`
/// reduces to argument parsing plus this call and its own signal handling,
/// and any other process hosting the runner role gets the identical wiring
/// by injecting its own [`Shutdown`] instead of a process signal.
pub async fn run(
    config: RunnerConfig,
    limits: RunnerLimits,
    shutdown: Shutdown,
) -> Result<(), RunnerError> {
    build_runtime(config, limits).await?.run(shutdown).await
}

/// What probing this machine's harness installations found: a
/// [`RunnerCapabilities`] identical to what enrollment/refresh would send,
/// plus [`ClaudeCodeAdapter::discover`]'s own error when Claude Code could
/// not be registered at all.
///
/// [`build_adapter_registry`] never registers an adapter or probe for a
/// harness whose `discover` fails, so a missing `claude` binary leaves no
/// trace in `capabilities.harnesses` whatsoever — Codex and OpenCode, by
/// contrast, are always registered, and their absence surfaces as a
/// `probe_error` on their own entry in that same vec instead. A caller that
/// wants to report Claude Code's absence honestly, rather than silently omit
/// it, needs this second field; nothing else in this crate captures it.
#[derive(Debug, Clone)]
pub struct DiscoveryReport {
    pub capabilities: RunnerCapabilities,
    pub claude_code_discovery_error: Option<String>,
    /// Which backend `secrets` answered from — `tack runner doctor` prints
    /// this so a file is never mistaken for a keychain.
    pub secret_backend: crate::secrets::SecretBackendKind,
    /// What asking the configured provider for its model catalog produced.
    /// The catalog itself, when one was fetched, already lives inside
    /// `capabilities` (merged into each eligible harness's
    /// `model_combinations`) — this field is the typed reason for every
    /// other outcome, for `tack runner doctor`'s own rendering.
    pub provider_catalog: crate::provider::CatalogStatus,
}

/// Runs the exact discovery/capability-probing step [`build_runtime`]
/// performs, without building a full runtime or requiring a server or
/// enrollment credential.
///
/// `tack runner doctor` is the only caller today: it needs to report what
/// this machine can do without enrolling a runner. It calls this instead of
/// re-deriving [`build_adapter_registry`]/[`report_capabilities`] itself, so
/// there remains exactly one place that decides how a harness gets probed,
/// never two that could quietly diverge. `secrets` is never resolved against
/// during a probe (no attempt exists to resolve for); it is only asked which
/// backend it is, which the adapters need at construction regardless.
pub async fn probe(
    staging_root: &Path,
    process_limits: &ProcessLimits,
    secrets: &SecretStore,
    providers: &BTreeMap<String, crate::config::ProviderConfig>,
) -> DiscoveryReport {
    let adapters = build_adapter_registry(process_limits, staging_root, secrets, providers);
    let mut capabilities = report_capabilities(&adapters, &SystemClock).await;
    let provider_catalog =
        crate::provider::attach_catalog(&mut capabilities, providers, secrets, &SystemClock).await;
    DiscoveryReport {
        capabilities,
        claude_code_discovery_error: ClaudeCodeAdapter::discover(secrets.clone()).err(),
        secret_backend: secrets.backend(),
        provider_catalog,
    }
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
fn build_adapter_registry(
    process_limits: &ProcessLimits,
    staging_root: &Path,
    secrets: &SecretStore,
    providers: &BTreeMap<String, crate::config::ProviderConfig>,
) -> AdapterRegistry {
    let mut registry = AdapterRegistry::new();

    let codex = CodexAdapter::discover(
        process_limits.clone(),
        staging_root.to_path_buf(),
        secrets.clone(),
    )
    .with_providers(providers.clone());
    let kind = HarnessProbe::harness_kind(&codex);
    registry.register_adapter(kind, Box::new(codex));
    if registry
        .register_probe(Box::new(
            CodexAdapter::discover(
                process_limits.clone(),
                staging_root.to_path_buf(),
                secrets.clone(),
            )
            .with_providers(providers.clone()),
        ))
        .is_err()
    {
        tracing::warn!(harness = "codex", "probe rejected at registration");
    }

    match ClaudeCodeAdapter::discover(secrets.clone()) {
        Ok(adapter) => {
            let adapter = adapter.with_providers(providers.clone());
            let kind = HarnessProbe::harness_kind(&adapter);
            registry.register_adapter(kind, Box::new(adapter));
            match ClaudeCodeAdapter::discover(secrets.clone()) {
                Ok(probe) => {
                    let probe = probe.with_providers(providers.clone());
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

    let opencode = OpenCodeAdapter::discover(
        process_limits.clone(),
        staging_root.to_path_buf(),
        secrets.clone(),
    );
    let kind = HarnessProbe::harness_kind(&opencode);
    registry.register_adapter(kind, Box::new(opencode));
    if registry
        .register_probe(Box::new(OpenCodeAdapter::discover(
            process_limits.clone(),
            staging_root.to_path_buf(),
            secrets.clone(),
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
