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

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tack_api::{LocalRunnerControl, LocalRunnerControlError, RuntimeState, RuntimeStatus};
use tack_runner::{
    ConfigError, ConfigOverrides, EnrollmentCredential, RunnerConfig, RunnerConfigSources,
    RunnerError, Shutdown, ShutdownHandle, VerifyKind,
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
pub(crate) fn load_runner_config(
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

/// The variable `tack serve --runner-config` sets, naming the TOML file the
/// embedded runner loads — the same file `tack runner start --config` reads.
/// Without it the runner reads [`default_runner_config_path`] when that file
/// exists; with neither it has defaults only, which leaves its `[docket]`,
/// `[verify]`, `[git]`, `[secrets]` and `[environment]` tables unset: those
/// have no environment equivalent.
pub const RUNNER_CONFIG_ENV: &str = "TACK_LOCAL_RUNNER_CONFIG";

/// The embedded runner's configuration file: the one `TACK_LOCAL_RUNNER_CONFIG`
/// names, else [`default_runner_config_path`] when that file exists. The
/// default lives in the per-user config directory, not beside the database,
/// so a board whose data directory is wiped, or one started by
/// `tack service install`'s unit, still finds it.
pub fn runner_config_path() -> Option<PathBuf> {
    // A unit test never reads the settings file of the machine it runs on.
    let default = if cfg!(test) {
        None
    } else {
        default_runner_config_path()
    };
    config_path_from(std::env::var_os(RUNNER_CONFIG_ENV), default)
}

/// The variable's file when it names one. Set but empty, it names none and the
/// default is not read either: that is how a script asks for defaults only.
/// Unset, the default when that file exists.
fn config_path_from(
    variable: Option<std::ffi::OsString>,
    default: Option<PathBuf>,
) -> Option<PathBuf> {
    match variable {
        Some(value) if value.is_empty() => None,
        Some(value) => Some(PathBuf::from(value)),
        None => default.filter(|path| path.is_file()),
    }
}

/// `<per-user config dir>/tack/runner.toml` (`~/.config/tack/runner.toml` on
/// Linux), whether or not it exists.
pub fn default_runner_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("tack").join("runner.toml"))
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

struct Running {
    shutdown_handle: ShutdownHandle,
    runner_task: JoinHandle<Result<(), RunnerError>>,
    since: DateTime<Utc>,
    /// The leases the runner holds now, shared with its engine.
    active: tack_runner::active::ActiveAttempts,
}

struct State {
    /// `None` until the server has bound its listener and told
    /// [`EmbeddedRunnerControl::set_bound_addr`] — before that, `start` has
    /// nowhere to point the runner's HTTP client and fails rather than guess.
    bound_addr: Option<SocketAddr>,
    runner_config: RunnerConfig,
    running: Option<Running>,
}

/// The seam `tack-api`'s routes call (`handlers::local_runner`'s
/// `LocalRunnerControl` trait) without ever depending on `tack-runner`
/// itself. One instance lives for the life of a `tack serve` process,
/// constructed once in [`serve`] and shared (via `Arc`) between
/// `AppState::local_runner` and this module's own boot-time wiring.
pub struct EmbeddedRunnerControl {
    server_config: tack_api::config::AppConfig,
    /// The file the runner configuration was read from; `None` is defaults only.
    config_path: Option<PathBuf>,
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
        let config_path = runner_config_path();
        let runner_config = load_runner_config(command_line, config_path.as_deref())?;
        Ok(Self {
            server_config,
            config_path,
            state: Mutex::new(State {
                bound_addr: None,
                runner_config,
                running: None,
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
        let max_concurrent_attempts = state.runner_config.max_concurrent_attempts;
        match &state.running {
            Some(running) => RuntimeStatus {
                state: RuntimeState::Running,
                since: Some(running.since),
                max_concurrent_attempts,
                active_attempts: u32::try_from(running.active.len()).unwrap_or(u32::MAX),
            },
            None => RuntimeStatus {
                state: RuntimeState::Stopped,
                since: None,
                max_concurrent_attempts,
                active_attempts: 0,
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

    async fn runner_id(&self) -> Option<String> {
        let state = self.state.lock().await;
        state.running.as_ref()?;
        // The id the API assigned at enrolment (`runr_…`), persisted with the
        // session — not `runner_config.runner_id`, which is the runner's own
        // configured name and is never a selector the API would accept.
        tack_runner::client::persisted_session_runner_id(&state.runner_config.state_dir)
    }

    async fn verify_program(&self) -> Option<String> {
        let state = self.state.lock().await;
        let verify = &state.runner_config.verify;
        (verify.enabled && verify.kind == VerifyKind::Prill).then(|| verify.program.clone())
    }

    fn config_file(&self) -> Option<String> {
        self.config_path
            .as_ref()
            .map(|path| path.display().to_string())
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

impl EmbeddedRunnerControl {
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
        let active =
            tack_runner::active::ActiveAttempts::new(runner_config.max_concurrent_attempts);
        let runner_task = tokio::spawn(bootstrap::run_with_active(
            runner_config.clone(),
            runner_limits(),
            shutdown,
            active.clone(),
        ));
        // Remembered so a later stop-then-start reuses whatever credential
        // resolution just produced (a stored session, or the fresh
        // self-provisioned one) instead of redoing it from scratch.
        state.runner_config = runner_config;
        state.running = Some(Running {
            shutdown_handle,
            runner_task,
            since: Utc::now(),
            active,
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
    warn_if_data_folder_database_is_unused(&server_config);

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

/// The default database is `tack.db` in the folder the server was started in. When nothing names
/// a database and a different `tack.db` already sits in the data folder, say which one this
/// start uses, so starting from the wrong folder does not look like losing everything.
fn warn_if_data_folder_database_is_unused(config: &tack_api::config::AppConfig) {
    if std::env::var_os("TACK_DATABASE_URL").is_some()
        || config.database_url != tack_api::config::AppConfig::default().database_url
    {
        return;
    }
    let (Ok(root), Some(used)) = (crate::service::data_root(), config.db_file_path()) else {
        return;
    };
    let kept = root.join("tack.db");
    let used = std::path::absolute(&used).unwrap_or(used);
    if kept.exists() && used != kept {
        eprintln!(
            "tack: this start uses the database in this folder, {}, not the one in your data folder, {}. \
             To use that one, start tack from its folder or set TACK_DATABASE_URL.",
            used.display(),
            kept.display()
        );
    }
}

#[cfg(test)]
#[path = "local_runner/tests.rs"]
mod tests;
