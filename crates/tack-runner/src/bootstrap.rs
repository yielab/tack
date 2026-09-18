//! Composes the runner this crate ships: the real HTTP protocol client,
//! every in-tree harness adapter, git-worktree workspaces, and an
//! owner-only journal. The crate's single composition root — `tack-runner`'s
//! `main` and any other process hosting the runner role call the exact same
//! function, so there is one wiring to keep honest against
//! `docs/contracts/runner-v1/`, never a copy that can drift from it.

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
    harness::{AdapterRegistry, PROCESS_GROUP_CANCEL_CEILING, process::ProcessLimits},
};

/// The concrete runtime this crate ships: the real HTTP protocol, every
/// in-tree harness adapter, git-worktree workspaces, an owner-only journal.
pub type ProductionRunnerRuntime = RunnerRuntime<
    HttpRunnerClient<AdapterRegistry, GitWorktreeProvisioner, SystemClock>,
    SystemProcessSupervisor,
    LocalFilesystem,
    SystemClock,
>;

/// Operational bounds every composer of [`build_runtime`] must choose. Each
/// field governs how much of the host a spawned harness subprocess, or a
/// single protocol call, may consume. Neither implements [`Default`] — a
/// value silently inherited here would hide a real operational choice.
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

    // The real transport replaces `UnavailableProtocolClient`, the stub
    // that otherwise cannot reach a server at all.
    let staging_root = config.state_dir.join("staging");
    let secrets = SecretStore::open(&config.secret_store_path());
    let (adapters, _) = crate::harness::discover(
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
        // Every claimed attempt gets its own real git checkout, replacing
        // `UnavailableWorktreeProvisioner`, which refuses every provision.
        WorkspaceManager::new(
            config.state_dir.join("workspaces"),
            GitWorktreeProvisioner::default(),
        ),
    )
    // Without attaching this, `engine.rs`'s events/artifacts call sites
    // would never run in the production binary even though it compiles.
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
/// A binary's `main` reduces to argument parsing plus this call and its own
/// signal handling; any other host of the runner role injects its own
/// [`Shutdown`] instead of a process signal for identical wiring.
pub async fn run(
    config: RunnerConfig,
    limits: RunnerLimits,
    shutdown: Shutdown,
) -> Result<(), RunnerError> {
    build_runtime(config, limits).await?.run(shutdown).await
}

/// What probing this machine's harness installations found: a
/// [`RunnerCapabilities`] identical to what enrollment/refresh would send,
/// plus, per harness kind, why a harness that is not installed could not be
/// located — such a harness is never registered, so it leaves no trace in
/// `capabilities.harnesses`.
#[derive(Debug, Clone)]
pub struct DiscoveryReport {
    pub capabilities: RunnerCapabilities,
    pub missing_harnesses: BTreeMap<String, String>,
    /// Which backend `secrets` answered from — `tack runner doctor` prints
    /// this so a file is never mistaken for a keychain.
    pub secret_backend: crate::secrets::SecretBackendKind,
    /// What asking each provider for its model catalog produced, keyed by
    /// `config_key`. A reached fetch already lives in `capabilities`; this
    /// is the typed reason for every other outcome.
    pub provider_catalog: BTreeMap<String, crate::provider::CatalogStatus>,
}

/// Runs the exact discovery/capability-probing step [`build_runtime`]
/// performs, without building a full runtime or requiring a server or
/// enrollment credential. `tack runner doctor` is the only caller: it needs
/// to report what this machine can do without enrolling a runner, reusing
/// [`crate::harness::discover`]/[`report_capabilities`] so exactly one place
/// decides how a harness gets probed. `secrets` is never resolved against
/// during a probe, only asked which backend it is.
pub async fn probe(
    staging_root: &Path,
    process_limits: &ProcessLimits,
    secrets: &SecretStore,
    providers: &BTreeMap<String, crate::config::ProviderConfig>,
) -> DiscoveryReport {
    let (adapters, missing_harnesses) =
        crate::harness::discover(process_limits, staging_root, secrets, providers);
    let mut capabilities = report_capabilities(&adapters, &SystemClock).await;
    let provider_catalog =
        crate::provider::attach_catalog(&mut capabilities, providers, secrets, &SystemClock).await;
    DiscoveryReport {
        capabilities,
        missing_harnesses,
        secret_backend: secrets.backend(),
        provider_catalog,
    }
}

/// Builds the capability snapshot sent at enrollment and on every refresh.
/// Every feature statement here is deliberately conservative, because a
/// capability claim is load-bearing: the scheduler and the operator UI
/// both offer only what a runner says it can do.
///
/// - `cancel` reports [`PROCESS_GROUP_CANCEL_CEILING`] (advisory): a
///   process-group signal cannot reliably reach a descendant a harness
///   spawns into a new OS session.
/// - `artifacts` reports advisory: `RunnerEngine::submit_terminal_evidence`
///   uploads a staged artifact, but only when an adapter stages one and
///   only best-effort (a transport failure is logged, never retried).
/// - `decisions` reports unsupported: the protocol path is implemented and
///   reachable, but no harness adapter ever asks a question a decision
///   could answer — claiming support for a path nothing calls would be
///   exactly the lie this rule forbids.
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
