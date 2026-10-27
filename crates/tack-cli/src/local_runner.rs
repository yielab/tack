//! Hosts the runner role inside the `tack` binary: `tack runner start` runs it as the whole
//! process, `tack serve` runs it as a controllable task beside the server.
//!
//! Both build a [`tack_runner::RunnerConfig`] through the precedence rules `tack-runner`'s own
//! binary uses ([`load_runner_config`]) and hand it to `tack_runner::bootstrap`, the crate's
//! single composition root. The embedded case speaks to its own server like a remote runner
//! would: ordinary runner-v1 HTTP against the loopback address the server bound, never through
//! `tack-api`'s router or state.
//!
//! [`serve`] always wires a [`EmbeddedRunner`] into `AppState`, whether or not
//! `--with-runner` says to start it now; `serve::run`'s auto-start and `PUT /api/local-runner`
//! both reach the same `start`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tack_app::local_runner::{
    EmbeddedRunner, RunnerHost, RunnerProcess, config_path_from, ensure_loopback,
    has_stored_session, migrate_legacy_state_dir, state_dir_under,
};
use tack_runner::{
    ConfigError, ConfigOverrides, EnrollmentCredential, RunnerConfig, RunnerConfigSources,
    Shutdown, ShutdownHandle, VerifyKind,
    active::ActiveAttempts,
    bootstrap::{self, RunnerLimits},
    harness::process::ProcessLimits,
    secrets::SecretStore,
};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

pub use tack_app::local_runner::with_runner_enabled;

/// The standalone `tack-runner` binary's own harness-subprocess bounds: both run the same
/// composition root.
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

/// TOML file, then environment, then `command_line` overrides: the precedence of `tack-runner`'s
/// own binary, reusing its types.
pub(crate) fn load_runner_config(
    command_line: ConfigOverrides,
    config_path: Option<&Path>,
) -> Result<RunnerConfig, ConfigError> {
    let file_toml = config_path
        .map(|path| {
            std::fs::read_to_string(path).map_err(|failure| {
                ConfigError::Unreadable(tack_runner::error::IoKind(failure.kind()))
            })
        })
        .transpose()?;
    RunnerConfig::from_sources(RunnerConfigSources {
        file_toml: file_toml.as_deref(),
        environment: RunnerConfig::environment_overrides(),
        command_line,
    })
}

/// The variable `tack serve --runner-config` sets, naming the TOML file the embedded runner
/// loads, the one `tack runner start --config` reads. Without it the runner reads
/// [`default_runner_config_path`] when that file exists; with neither it has defaults only, which
/// leaves its `[docket]`, `[verify]`, `[git]`, `[secrets]` and `[environment]` tables unset.
pub const RUNNER_CONFIG_ENV: &str = "TACK_LOCAL_RUNNER_CONFIG";

/// The embedded runner's configuration file: the one [`RUNNER_CONFIG_ENV`] names, else the
/// per-user default when that file exists.
pub fn runner_config_path() -> Option<PathBuf> {
    // A unit test never reads the settings file of the machine it runs on.
    let default = if cfg!(test) {
        None
    } else {
        default_runner_config_path()
    };
    config_path_from(std::env::var_os(RUNNER_CONFIG_ENV), default)
}

/// `<per-user config dir>/tack/runner.toml`, whether or not it exists.
pub fn default_runner_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("tack").join("runner.toml"))
}

/// Runs the runner role as the whole process, owning its own Tokio runtime and ctrl-c wiring.
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

    tack_runner::logging::init();
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run_to_shutdown(config))
}

async fn run_to_shutdown(config: RunnerConfig) -> anyhow::Result<()> {
    let (shutdown, shutdown_handle) = Shutdown::channel();
    let mut runner_task = tokio::spawn(bootstrap::run(config, runner_limits(), shutdown));
    let panicked = |error| anyhow::anyhow!("runner task panicked: {error}");

    tokio::select! {
        result = &mut runner_task => result.map_err(panicked)?.map_err(anyhow::Error::from),
        signal = tokio::signal::ctrl_c() => {
            signal?;
            tracing::info!("shutdown signal received");
            shutdown_handle.request();
            runner_task.await.map_err(panicked)?.map_err(anyhow::Error::from)
        }
    }
}

/// Makes sure `runner_config` carries something the embedded runner can redeem: an explicit
/// credential wins; otherwise a durable session on disk, reused as-is if its runner id still
/// resolves in this database (no credential is set: the runner starts from the session);
/// otherwise a fresh one-time token, self-provisioned in-process. An orphaned
/// session file is left untouched: `establish_session` is refused on `refresh` and falls through
/// to the fresh token.
async fn ensure_runner_credential(
    runner_config: &mut RunnerConfig,
    server_config: &tack_api::config::AppConfig,
) -> anyhow::Result<()> {
    if runner_config.enrollment_credential.is_some() {
        return Ok(());
    }
    let state_dir = &runner_config.state_dir;
    let database_url = &server_config.database_url;
    if has_stored_session(state_dir) {
        if crate::local_enrollment::stored_session_orphaned(state_dir, database_url).await? {
            tracing::info!(
                "the embedded runner's stored credential belongs to a database this server is \
                 no longer using; provisioning a fresh identity instead of reusing it"
            );
        } else {
            crate::local_enrollment::name_stored_runner_after_this_computer(
                state_dir,
                database_url,
            )
            .await;
            return Ok(());
        }
    }
    let credential = crate::local_enrollment::self_provision(database_url).await?;
    runner_config.enrollment_credential = Some(credential);
    Ok(())
}

/// The runner task `bootstrap::run_with_active` spawned, as the supervisor sees it.
struct Process {
    shutdown_handle: ShutdownHandle,
    runner_task: JoinHandle<Result<(), tack_runner::RunnerError>>,
    /// The leases the runner holds now, shared with its engine.
    active: ActiveAttempts,
}

impl RunnerProcess for Process {
    fn is_finished(&self) -> bool {
        self.runner_task.is_finished()
    }

    fn active_attempts(&self) -> usize {
        self.active.len()
    }

    fn request_stop(&self) {
        self.shutdown_handle.request();
    }
}

/// What the supervisor in `tack-app` cannot know: how this binary configures, enrols and
/// spawns the runner.
pub struct CliRunnerHost {
    server_config: tack_api::config::AppConfig,
    /// The file the runner configuration was read from; `None` is defaults only.
    config_path: Option<PathBuf>,
    /// Remembered across stop and start, so a restart reuses whatever credential resolution
    /// produced (a stored session, or the fresh self-provisioned one).
    runner_config: Mutex<RunnerConfig>,
}

impl CliRunnerHost {
    /// The state directory defaults to one scoped to `server_config.storage_dir`, so a server
    /// opened against a different database never resolves to another server's runner state,
    /// unless `TACK_RUNNER_STATE_DIR` is set, which wins here as for the standalone binary.
    fn new(server_config: tack_api::config::AppConfig) -> Result<Self, ConfigError> {
        let command_line = if std::env::var_os("TACK_RUNNER_STATE_DIR").is_some() {
            ConfigOverrides::default()
        } else {
            let state_dir = state_dir_under(&server_config.storage_dir);
            migrate_legacy_state_dir(
                Path::new(tack_runner::config::DEFAULT_STATE_DIR),
                &state_dir,
            );
            ConfigOverrides {
                state_dir: Some(state_dir),
                ..ConfigOverrides::default()
            }
        };
        let config_path = runner_config_path();
        let runner_config = load_runner_config(command_line, config_path.as_deref())?;
        Ok(Self {
            server_config,
            config_path,
            runner_config: Mutex::new(runner_config),
        })
    }
}

#[async_trait]
impl RunnerHost for CliRunnerHost {
    async fn max_concurrent_attempts(&self) -> u32 {
        self.runner_config.lock().await.max_concurrent_attempts
    }

    fn config_file(&self) -> Option<String> {
        self.config_path
            .as_ref()
            .map(|path| path.display().to_string())
    }

    async fn launch(&self, api_base_url: &str) -> Result<Box<dyn RunnerProcess>, String> {
        let mut remembered = self.runner_config.lock().await;
        let mut runner_config = remembered.clone();
        runner_config.api_base_url = api_base_url.to_owned();
        ensure_runner_credential(&mut runner_config, &self.server_config)
            .await
            .map_err(|error| error.to_string())?;

        let (shutdown, shutdown_handle) = Shutdown::channel();
        let active = ActiveAttempts::new(runner_config.max_concurrent_attempts);
        let runner_task = tokio::spawn(bootstrap::run_with_active(
            runner_config.clone(),
            runner_limits(),
            shutdown,
            active.clone(),
        ));
        *remembered = runner_config;
        Ok(Box::new(Process {
            shutdown_handle,
            runner_task,
            active,
        }))
    }

    async fn enrolled_runner_id(&self) -> Option<String> {
        // The id the API assigned at enrolment (`runr_...`), persisted with the session, not
        // `runner_config.runner_id`, which is the runner's own configured name.
        let config = self.runner_config.lock().await;
        tack_runner::client::persisted_session_runner_id(&config.state_dir)
    }

    async fn verify_program(&self) -> Option<String> {
        let config = self.runner_config.lock().await;
        let verify = &config.verify;
        (verify.enabled && verify.kind == VerifyKind::Prill).then(|| verify.program.clone())
    }

    async fn resolve_secret(&self, reference: &str) -> Result<String, String> {
        let config = self.runner_config.lock().await;
        SecretStore::open(&config.secret_store_path())
            .resolve(reference)
            .map(|value| value.expose().to_owned())
            .map_err(|error| error.to_string())
    }
}

/// Runs the server with an embedded runner always wired in, even for a plain `tack serve` on any
/// bind, so `PUT /api/local-runner` can turn it on later with no restart. A non-loopback bind
/// never starts the runner and never exposes its routes; this function refuses to boot only when
/// this boot's own flag or environment variable asked for `--with-runner` on one.
/// `serve::run` re-checks loopback against the persisted preference before it starts the runner,
/// so a stale "enabled" row from an earlier loopback session is silently not honored.
pub async fn serve() -> anyhow::Result<()> {
    let server_config = tack_api::config::AppConfig::load();
    if server_config.local_runner_enable {
        ensure_loopback(&server_config.host, server_config.binds_loopback())?;
    }
    warn_if_data_folder_database_is_unused(&server_config);

    let control = Arc::new(EmbeddedRunner::new(CliRunnerHost::new(server_config)?));
    let local_runner: Arc<dyn tack_api::LocalRunnerControl> = control.clone();

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let server_task = tokio::spawn(crate::serve::run(Some(ready_tx), Some(local_runner)));

    let Ok(bound_addr) = ready_rx.await else {
        // The sender was dropped before sending, which only happens when `serve::run` returned
        // before opening its listener: that task's own result is the error worth surfacing.
        return match server_task.await {
            Ok(Ok(())) => Err(anyhow::anyhow!("server exited before it became ready")),
            Ok(Err(error)) => Err(error),
            Err(join_error) => Err(anyhow::anyhow!("server task panicked: {join_error}")),
        };
    };
    control.set_bound_addr(bound_addr).await;

    // The auto-start decision, ctrl-c handling and graceful shutdown are `serve::run`'s job.
    server_task
        .await
        .map_err(|join_error| anyhow::anyhow!("server task panicked: {join_error}"))?
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
