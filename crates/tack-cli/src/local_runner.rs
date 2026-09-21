//! Hosts the runner role inside the `tack` binary: `tack runner start` runs
//! it as the whole process, `tack serve` (with or without `--with-runner`)
//! runs it as a controllable task alongside the server in the same process.
//!
//! Both paths build a [`tack_runner::RunnerConfig`] through the same
//! precedence rules `tack-runner`'s own binary uses ([`load_runner_config`])
//! and hand it to `tack_runner::bootstrap`, the crate's single composition
//! root — there is no second way to wire a runner in this codebase.
//!
//! The embedded case speaks to its own server exactly like a remote runner
//! would: ordinary runner-v1 HTTP against the loopback address the server
//! bound. It never reaches into `tack-api`'s router or state — a shortcut
//! here would create a second implementation of the protocol client that
//! `docs/contracts/runner-v1/` cannot hold accountable.
//!
//! **The seam this module adds (ADR 0061 decisions 2 and 6):** [`serve`]
//! always wires an [`EmbeddedRunnerControl`] into `AppState`, whether or not
//! `--with-runner`/`TACK_LOCAL_RUNNER_ENABLE` says to start it immediately —
//! that flag only decides `AppConfig::local_runner_enable`'s startup value.
//! `serve_inner`'s own auto-start check and `PUT /api/local-runner` both
//! call the same [`EmbeddedRunnerControl::start`]: one code path in, never a
//! boot-time one and a UI-triggered one.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tack_api::{
    CatalogSnapshot, LocalRunnerControl, LocalRunnerControlError, RuntimeState, RuntimeStatus,
    SecretMeta,
};
use tack_runner::{
    ConfigError, ConfigOverrides, EnrollmentCredential, RunnerConfig, RunnerConfigSources,
    RunnerError, Shutdown, ShutdownHandle,
    bootstrap::{self, RunnerLimits},
    harness::process::ProcessLimits,
    secrets::SecretStore,
};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

/// Mirrors the standalone `tack-runner` binary's own harness-subprocess
/// bounds — both run the identical composition root.
const HARNESS_PROCESS_LIMITS: ProcessLimits =
    ProcessLimits::new(4 * 1024 * 1024, 1024 * 1024, Duration::from_secs(3_600));

/// How long any single non-claim protocol call may take.
const PROTOCOL_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

fn runner_limits() -> RunnerLimits {
    RunnerLimits {
        harness_process: HARNESS_PROCESS_LIMITS,
        protocol_request_timeout: PROTOCOL_REQUEST_TIMEOUT,
    }
}

/// TOML file, then environment, then `command_line` overrides — identical
/// precedence to `tack-runner`'s own binary, reusing its types.
fn load_runner_config(
    command_line: ConfigOverrides,
    config_path: Option<&Path>,
) -> Result<RunnerConfig, ConfigError> {
    let file_toml = config_path
        .map(|path| std::fs::read_to_string(path).map_err(|_| ConfigError::Unreadable))
        .transpose()?;
    RunnerConfig::from_sources(RunnerConfigSources {
        file_toml: file_toml.as_deref(),
        environment: RunnerConfig::environment_overrides(),
        command_line,
    })
}

/// Where the embedded runner's on-disk state (credential, attempt journal,
/// secret store) lives by default: one level under this server's own
/// `storage_dir`, mirroring `execution-artifacts`. Ties runner state to
/// `TACK_STORAGE_DIR` so a server on a different database never resolves to
/// another server's runner state.
fn embedded_default_state_dir(storage_dir: &str) -> PathBuf {
    Path::new(storage_dir).join("runner")
}

/// Best-effort recovery for an install whose runner state still sits at the
/// crate's bare, cwd-relative default (`DEFAULT_STATE_DIR`) instead of the
/// new database-scoped directory: renames it there so an already-enrolled
/// credential isn't stranded. No-op if `new_dir` already exists (never
/// clobber existing state). A failed rename leaves the legacy directory in
/// place and reported; the caller provisions a fresh identity instead.
fn migrate_legacy_state_dir(new_dir: &Path) {
    if new_dir.exists() {
        return;
    }
    let legacy_dir = Path::new(tack_runner::config::DEFAULT_STATE_DIR);
    if !legacy_dir.is_dir() {
        return;
    }
    if let Some(parent) = new_dir.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        tracing::warn!(
            %error,
            "could not prepare the embedded runner's new state directory; its legacy state is left in place"
        );
        return;
    }
    match std::fs::rename(legacy_dir, new_dir) {
        Ok(()) => tracing::info!(
            "migrated the embedded runner's on-disk state to the directory scoped to this server's own storage configuration"
        ),
        Err(error) => tracing::warn!(
            %error,
            "could not migrate the embedded runner's legacy state directory; it is left in place and a fresh identity will be provisioned instead"
        ),
    }
}

/// Whether the embedded runner should start at boot, combining
/// `--with-runner` with its environment equivalent. Off unless one explicitly
/// says on. Only feeds `AppConfig::local_runner_enable`'s startup value — a
/// later UI toggle overrides it via `effective_local_runner_enabled`.
pub fn with_runner_enabled(flag: bool) -> bool {
    flag || std::env::var("TACK_LOCAL_RUNNER_ENABLE")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Runs the runner role as the whole process, owning its own Tokio runtime
/// and ctrl-c wiring — reimplements `tack-runner`'s shutdown since that
/// binary's `main` isn't callable library code.
pub fn run_standalone(
    config_path: Option<PathBuf>,
    api_url: Option<String>,
    runner_id: Option<String>,
    state_dir: Option<PathBuf>,
    enrollment_token: Option<String>,
) -> anyhow::Result<()> {
    let config = load_runner_config(
        ConfigOverrides {
            api_base_url: api_url,
            runner_id,
            state_dir,
            enrollment_credential: enrollment_token.map(EnrollmentCredential::new),
            ..ConfigOverrides::default()
        },
        config_path.as_deref(),
    )?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run_to_shutdown(config))
}

async fn run_to_shutdown(config: RunnerConfig) -> anyhow::Result<()> {
    let (shutdown, shutdown_handle) = Shutdown::channel();
    let mut runner_task = tokio::spawn(bootstrap::run(config, runner_limits(), shutdown));

    tokio::select! {
        result = &mut runner_task => {
            let result = result.map_err(|error| anyhow::anyhow!("runner task panicked: {error}"))?;
            result.map_err(anyhow::Error::from)
        }
        signal = tokio::signal::ctrl_c() => {
            signal?;
            tracing::info!("shutdown signal received");
            shutdown_handle.request();
            let result = runner_task
                .await
                .map_err(|error| anyhow::anyhow!("runner task panicked: {error}"))?;
            result.map_err(anyhow::Error::from)
        }
    }
}

/// Rejects a server configuration that explicitly asked the embedded runner
/// to auto-start on a non-loopback bind — refused outright, before any
/// socket or database opens, since an embedded runner executes arbitrary
/// agent processes on the host serving the UI. A plain `tack serve` with no
/// such request never reaches this; see [`serve`] for why the same rule is
/// checked again later against the *persisted* preference.
fn ensure_loopback(config: &tack_api::config::AppConfig) -> anyhow::Result<()> {
    if !config.binds_loopback() {
        anyhow::bail!(
            "refusing to start with an embedded runner: {} is not a loopback address. An \
             embedded runner executes arbitrary agent processes on this host, so it is \
             restricted to a server bound to loopback",
            config.host
        );
    }
    Ok(())
}

/// Makes sure `runner_config` carries something the embedded runner can
/// redeem: an explicit credential wins outright; otherwise a durable session
/// on disk, reused as-is if its runner id still resolves in this database
/// (never re-provisioned, so no second token is minted) — a placeholder
/// credential only because `build_runtime` requires *some* value up front.
/// Otherwise a fresh one-time token, self-provisioned in-process (ADR 0058:
/// operator and runner are the same person on the same machine). An orphaned
/// session file is left untouched: `establish_session` tries `refresh`
/// first, is refused, and falls through to the fresh token. Called only
/// after [`ensure_loopback`] has passed.
async fn ensure_runner_credential(
    runner_config: &mut RunnerConfig,
    server_config: &tack_api::config::AppConfig,
) -> anyhow::Result<()> {
    if runner_config.enrollment_credential.is_some() {
        return Ok(());
    }
    if crate::local_enrollment::has_stored_session(&runner_config.state_dir) {
        if crate::local_enrollment::stored_session_orphaned(
            &runner_config.state_dir,
            &server_config.database_url,
        )
        .await?
        {
            tracing::info!(
                "the embedded runner's stored credential belongs to a database this server is \
                 no longer using; provisioning a fresh identity instead of reusing it"
            );
        } else {
            runner_config.enrollment_credential =
                Some(crate::local_enrollment::stored_session_placeholder());
            return Ok(());
        }
    }
    let credential = crate::local_enrollment::self_provision(&server_config.database_url).await?;
    runner_config.enrollment_credential = Some(credential);
    Ok(())
}

/// Where [`EmbeddedRunnerControl`] remembers which secret names it has set
/// and when — never a value, and deliberately *not* inside
/// `SecretStore`'s own file/keychain entries: `tack-runner`'s `SecretStore`
/// tracks no timestamp at all, and it stores one entry per credential name
/// with no room for metadata alongside it. A name present in the real store
/// but absent here (set by `tack runner secret set` before this UI ever
/// ran) reports `set_at: None` rather than a fabricated time.
fn secret_meta_path(state_dir: &Path) -> PathBuf {
    state_dir.join("secret_meta.json")
}

fn load_secret_meta(state_dir: &Path) -> HashMap<String, DateTime<Utc>> {
    std::fs::read(secret_meta_path(state_dir))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Best-effort: a failure to persist this sidecar loses only "since when"
/// display metadata, never the secret itself (already durably written to
/// the real store by the caller before this runs).
fn save_secret_meta(state_dir: &Path, meta: &HashMap<String, DateTime<Utc>>) {
    let Ok(json) = serde_json::to_vec(meta) else {
        return;
    };
    if let Err(error) = std::fs::write(secret_meta_path(state_dir), json) {
        tracing::warn!(%error, "failed to persist local-runner secret metadata (non-fatal)");
    }
}

/// Narrows the per-provider catalog map to the single status this response
/// carries — the panel it feeds is one provider's key field, so no entry for
/// it reads the same as unconfigured. Per-model price/context-window counts
/// stop here on purpose: they'd go on the wire, and the capability type has
/// nowhere to carry per-model metadata yet.
fn map_catalog_status(status: Option<&tack_runner::provider::CatalogStatus>) -> CatalogSnapshot {
    use tack_runner::provider::CatalogStatus;
    match status {
        None | Some(CatalogStatus::NotConfigured) => CatalogSnapshot::NotConfigured,
        Some(CatalogStatus::SecretUnresolved) => CatalogSnapshot::SecretUnresolved,
        Some(CatalogStatus::Unreachable { status }) => CatalogSnapshot::Unreachable {
            http_status: *status,
        },
        Some(CatalogStatus::Configured {
            model_count,
            checked_at,
            ..
        }) => CatalogSnapshot::Configured {
            model_count: *model_count,
            checked_at: *checked_at,
        },
    }
}

struct Running {
    shutdown_handle: ShutdownHandle,
    runner_task: JoinHandle<Result<(), RunnerError>>,
    since: DateTime<Utc>,
}

struct State {
    /// `None` until the server has bound its listener and told
    /// [`EmbeddedRunnerControl::set_bound_addr`] — before that, `start` has
    /// nowhere to point the runner's HTTP client and fails rather than guess.
    bound_addr: Option<SocketAddr>,
    runner_config: RunnerConfig,
    running: Option<Running>,
    secret_meta: HashMap<String, DateTime<Utc>>,
    /// Whether `providers[vercel_ai_gateway].enabled` reads `true` only
    /// because `set_secret` flipped it as a convenience, not because the
    /// operator's own config already said so. Set only at the instant
    /// `set_secret` flips `false` → `true`; cleared only by `remove_secret`
    /// undoing that exact flip, so an operator's own `true` is never turned
    /// off by a later secret removal. Never persisted.
    vercel_ai_gateway_secret_auto_enabled: bool,
}

/// The seam `tack-api`'s routes call (`handlers::local_runner`'s
/// `LocalRunnerControl` trait) without ever depending on `tack-runner`
/// itself. One instance lives for the life of a `tack serve` process,
/// constructed once in [`serve`] and shared (via `Arc`) between
/// `AppState::local_runner` and this module's own boot-time wiring.
pub struct EmbeddedRunnerControl {
    server_config: tack_api::config::AppConfig,
    state: Mutex<State>,
}

impl EmbeddedRunnerControl {
    /// Builds the runner configuration this control starts from. The state
    /// directory defaults to [`embedded_default_state_dir`] — scoped to
    /// `server_config.storage_dir`, so a server opened against a different
    /// database never resolves to another server's runner state — unless
    /// `TACK_RUNNER_STATE_DIR` is set, which still wins here exactly as it
    /// would for the standalone binary (`docs/CONFIG.md`'s documented escape
    /// hatch, never overridden by a default this module computes for
    /// itself).
    fn new(server_config: tack_api::config::AppConfig) -> Result<Self, ConfigError> {
        let command_line = if std::env::var_os("TACK_RUNNER_STATE_DIR").is_some() {
            ConfigOverrides::default()
        } else {
            let default_state_dir = embedded_default_state_dir(&server_config.storage_dir);
            migrate_legacy_state_dir(&default_state_dir);
            ConfigOverrides {
                state_dir: Some(default_state_dir),
                ..ConfigOverrides::default()
            }
        };
        let runner_config = load_runner_config(command_line, None)?;
        let secret_meta = load_secret_meta(&runner_config.state_dir);
        Ok(Self {
            server_config,
            state: Mutex::new(State {
                bound_addr: None,
                runner_config,
                running: None,
                secret_meta,
                vercel_ai_gateway_secret_auto_enabled: false,
            }),
        })
    }

    /// Told the server's own bound loopback address once it is known — see
    /// [`serve`]'s doc comment for why this can't be passed to `new`
    /// instead: the listener the address comes from hasn't opened yet at
    /// construction time.
    async fn set_bound_addr(&self, addr: SocketAddr) {
        self.state.lock().await.bound_addr = Some(addr);
    }
}

#[async_trait]
impl LocalRunnerControl for EmbeddedRunnerControl {
    async fn status(&self) -> RuntimeStatus {
        let mut state = self.state.lock().await;
        // Self-heals a runner that exited on its own (crashed, or finished
        // reacting to a prior `stop()`'s shutdown signal) — without this, a
        // task that died without anyone calling `stop()` would leave
        // `running` stale and this method would keep reporting "running"
        // forever.
        if state
            .running
            .as_ref()
            .is_some_and(|running| running.runner_task.is_finished())
        {
            state.running = None;
        }
        match &state.running {
            Some(running) => RuntimeStatus {
                state: RuntimeState::Running,
                since: Some(running.since),
            },
            None => RuntimeStatus {
                state: RuntimeState::Stopped,
                since: None,
            },
        }
    }

    async fn start(&self) -> Result<(), LocalRunnerControlError> {
        let mut state = self.state.lock().await;
        self.start_locked(&mut state).await
    }

    async fn stop(&self) {
        let mut state = self.state.lock().await;
        if let Some(running) = state.running.take() {
            // Doesn't block waiting for the task to actually exit — a
            // toggle-off HTTP request must not hang on however long the
            // runner's own shutdown takes.
            running.shutdown_handle.request();
        }
    }

    async fn list_secrets(&self) -> Vec<SecretMeta> {
        let state = self.state.lock().await;
        let store = SecretStore::open(&state.runner_config.secret_store_path());
        let names = store.list().unwrap_or_default();
        names
            .into_iter()
            .map(|name| {
                let set_at = state.secret_meta.get(&name).copied();
                SecretMeta { name, set_at }
            })
            .collect()
    }

    async fn set_secret(&self, name: &str, value: &str) -> Result<(), LocalRunnerControlError> {
        let mut state = self.state.lock().await;
        let store = SecretStore::open(&state.runner_config.secret_store_path());
        store
            .set(name, value)
            .map_err(|error| LocalRunnerControlError::SecretStore(error.to_string()))?;
        state.secret_meta.insert(name.to_owned(), Utc::now());
        save_secret_meta(&state.runner_config.state_dir, &state.secret_meta);

        // A UI-only user must never also have to hand-edit a TOML
        // `enabled` flag once they've pasted a key — flip the one provider
        // this build knows on the moment its default secret name is set.
        // Narrow on purpose: a deployment that configured a *different*
        // secret-store entry name via `TACK_RUNNER_PROVIDER_
        // VERCEL_AI_GATEWAY_SECRET` keeps using its own console-only
        // toggle, unchanged by this route. Only actually flipping the flag
        // (not merely finding it already `true`) marks the auto-enable —
        // [`EmbeddedRunnerControl::remove_secret`] undoes exactly this flip
        // and nothing else, so an operator's own `enabled = true` is never
        // clobbered by a later key removal.
        if name == tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET {
            let was_enabled = state
                .runner_config
                .providers
                .get(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
                .is_some_and(|provider| provider.enabled);
            if let Some(provider) = state
                .runner_config
                .providers
                .get_mut(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
            {
                provider.enabled = true;
            }
            if !was_enabled {
                state.vercel_ai_gateway_secret_auto_enabled = true;
            }
        }
        if provider_resolves_secret(&state.runner_config.providers, name) {
            self.restart_for_provider_change_locked(&mut state).await?;
        }
        Ok(())
    }

    async fn remove_secret(&self, name: &str) -> Result<(), LocalRunnerControlError> {
        let mut state = self.state.lock().await;
        let store = SecretStore::open(&state.runner_config.secret_store_path());
        store
            .remove(name)
            .map_err(|error| LocalRunnerControlError::SecretStore(error.to_string()))?;
        state.secret_meta.remove(name);
        save_secret_meta(&state.runner_config.state_dir, &state.secret_meta);

        // Decided before the flag below may change: a provider whose only
        // credential was just removed is one the running runner was
        // configured with, whether or not it stays enabled afterwards.
        let affects_provider = provider_resolves_secret(&state.runner_config.providers, name);

        // The mirror of `set_secret`'s own flip, narrowed the identical
        // way: only the default secret name is ever considered, so a
        // provider configured under a different name via
        // `TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_SECRET` is never touched
        // here either. And only undoes `set_secret`'s own convenience flip
        // — an `enabled = true` the operator set directly (TOML, the
        // `_ENABLED` environment variable, or already `true` at boot) is
        // left exactly as it was, because `vercel_ai_gateway_secret_auto_
        // enabled` is only ever `true` when this process's own `set_secret`
        // is what turned it on.
        if name == tack_runner::config::DEFAULT_VERCEL_AI_GATEWAY_SECRET
            && state.vercel_ai_gateway_secret_auto_enabled
            && let Some(provider) = state
                .runner_config
                .providers
                .get_mut(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY)
        {
            provider.enabled = false;
            state.vercel_ai_gateway_secret_auto_enabled = false;
        }
        if affects_provider {
            self.restart_for_provider_change_locked(&mut state).await?;
        }
        Ok(())
    }

    async fn catalog(&self) -> CatalogSnapshot {
        let state = self.state.lock().await;
        let staging_root = state.runner_config.state_dir.join("staging");
        let secrets = SecretStore::open(&state.runner_config.secret_store_path());
        let limits = runner_limits();
        let report = bootstrap::probe(
            &staging_root,
            &limits.harness_process,
            &secrets,
            &state.runner_config.providers,
        )
        .await;
        map_catalog_status(
            report
                .provider_catalog
                .get(tack_runner::config::VERCEL_AI_GATEWAY_CONFIG_KEY),
        )
    }

    async fn resolve_secret(&self, reference: &str) -> Result<String, LocalRunnerControlError> {
        let state = self.state.lock().await;
        let store = SecretStore::open(&state.runner_config.secret_store_path());
        store
            .resolve(reference)
            .map(|value| value.expose().to_owned())
            .map_err(|error| LocalRunnerControlError::SecretStore(error.to_string()))
    }
}

/// How long a stopped runner task is given to actually exit before a
/// configuration change stops waiting for it and aborts it outright. Shutdown
/// is observed between protocol calls and terminates every harness child
/// first, so this is a backstop against a wedged task, not the expected path.
const RUNNER_STOP_TIMEOUT: Duration = Duration::from_secs(10);

/// Whether any configured provider resolves its credential from the
/// secret-store entry `name`. `RunnerConfig`'s `secret` references may carry
/// the explicit `store:` prefix `SecretStore::resolve` also accepts, so both
/// spellings name the same entry.
fn provider_resolves_secret(
    providers: &std::collections::BTreeMap<String, tack_runner::config::ProviderConfig>,
    name: &str,
) -> bool {
    providers.values().any(|provider| {
        provider
            .secret
            .strip_prefix("store:")
            .unwrap_or(&provider.secret)
            == name
    })
}

impl EmbeddedRunnerControl {
    /// The runner receives its configuration by value when it is spawned —
    /// its adapters hold their own copy of the provider table, and the
    /// capability snapshot it enrolls with is computed once at boot. That
    /// is the runner's model: configuration is fixed for the life of one
    /// runner task. So a change to a configured provider's credential is not
    /// something a running task can be told about; it is a new task. Without
    /// this, the key an operator just pasted reaches this control's own copy
    /// of the configuration (which is what `catalog` reads, so the page
    /// reports a live catalog) while every dispatch still resolves the
    /// provider through the spawned task's stale copy and refuses it.
    ///
    /// A stopped runner needs nothing: the next `start` reads the current
    /// configuration. The old task is joined, not merely signalled, before
    /// the new one is spawned — two runner tasks sharing one state directory
    /// would share one journal with nothing arbitrating between them.
    async fn restart_for_provider_change_locked(
        &self,
        state: &mut State,
    ) -> Result<(), LocalRunnerControlError> {
        if state.running.is_none() {
            return Ok(());
        }
        self.stop_and_join_locked(state).await;
        self.start_locked(state).await
    }

    async fn stop_and_join_locked(&self, state: &mut State) {
        let Some(running) = state.running.take() else {
            return;
        };
        running.shutdown_handle.request();
        // The `JoinHandle` moves into the timeout future, which drops it on
        // expiry — and dropping a `JoinHandle` detaches the task rather than
        // cancelling it. The abort handle taken first is what actually ends
        // a task that ignored its shutdown signal.
        let abort = running.runner_task.abort_handle();
        match tokio::time::timeout(RUNNER_STOP_TIMEOUT, running.runner_task).await {
            Ok(_) => {
                tracing::info!("embedded runner stopped for a provider configuration change");
            }
            Err(_) => {
                abort.abort();
                tracing::warn!(
                    "embedded runner did not stop within the shutdown budget; its task was aborted"
                );
            }
        }
    }

    async fn start_locked(&self, state: &mut State) -> Result<(), LocalRunnerControlError> {
        if state.running.is_some() {
            // Idempotent — a second `PUT {"enabled": true}` (or the
            // boot-time check racing a UI toggle) must never spawn a
            // duplicate runner task.
            return Ok(());
        }
        let bound_addr = state.bound_addr.ok_or_else(|| {
            LocalRunnerControlError::StartFailed(
                "the server's own address is not yet known".to_owned(),
            )
        })?;

        let mut runner_config = state.runner_config.clone();
        runner_config.api_base_url = format!("http://{bound_addr}/api/runner/v1");
        ensure_runner_credential(&mut runner_config, &self.server_config)
            .await
            .map_err(|error| LocalRunnerControlError::StartFailed(error.to_string()))?;

        let (shutdown, shutdown_handle) = Shutdown::channel();
        let runner_task = tokio::spawn(bootstrap::run(
            runner_config.clone(),
            runner_limits(),
            shutdown,
        ));
        // Remembered so a later stop-then-start reuses whatever credential
        // resolution just produced (a stored session, or the fresh
        // self-provisioned one) instead of redoing it from scratch.
        state.runner_config = runner_config;
        state.running = Some(Running {
            shutdown_handle,
            runner_task,
            since: Utc::now(),
        });
        Ok(())
    }
}

/// Runs the server with an embedded runner always wired in — even a plain
/// `tack serve` with no flag, on any bind, so `PUT /api/local-runner` can
/// turn it on later with no restart. A non-loopback bind never starts the
/// runner and never exposes its routes (ADR 0061 decision 6 is a safety
/// invariant, not a startup nicety): this function refuses to boot only when
/// *this boot's own* flag or environment variable explicitly asked for
/// `--with-runner` on a non-loopback bind ([`ensure_loopback`]);
/// `tack_api::server::serve_inner` separately re-checks loopback against the
/// *persisted* preference once the database is open, before calling
/// [`EmbeddedRunnerControl::start`] — so a stale "enabled" row saved from an
/// earlier loopback session can never auto-start a runner on a
/// differently-configured deployment, it is just silently not honored.
/// `PUT /api/local-runner` reaches the identical `start()`, gated the
/// identical way by `router::build_router`'s own loopback check.
pub async fn serve() -> anyhow::Result<()> {
    let server_config = tack_api::config::AppConfig::load();
    if server_config.local_runner_enable {
        ensure_loopback(&server_config)?;
    }

    let control = Arc::new(EmbeddedRunnerControl::new(server_config)?);
    let local_runner: Arc<dyn tack_api::LocalRunnerControl> = control.clone();

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let server_task = tokio::spawn(tack_api::serve_with_ready_and_local_runner(
        ready_tx,
        local_runner,
    ));

    let bound_addr = match ready_rx.await {
        Ok(addr) => addr,
        Err(_) => {
            // The sender was dropped without sending, which only happens when
            // `serve_with_ready_and_local_runner` returned before opening its
            // listener; that task's own result is the error worth
            // surfacing, not the closed channel.
            return match server_task.await {
                Ok(Ok(())) => Err(anyhow::anyhow!("server exited before it became ready")),
                Ok(Err(error)) => Err(error),
                Err(join_error) => Err(anyhow::anyhow!("server task panicked: {join_error}")),
            };
        }
    };
    control.set_bound_addr(bound_addr).await;

    // Everything past this point — the auto-start decision, ctrl-c
    // handling, graceful shutdown — is `tack_api::server::serve_inner`'s
    // own job; this function's only remaining job is to react honestly if
    // that task ends.
    match server_task.await {
        Ok(result) => result,
        Err(join_error) => Err(anyhow::anyhow!("server task panicked: {join_error}")),
    }
}

#[cfg(test)]
#[path = "local_runner/tests.rs"]
mod tests;
