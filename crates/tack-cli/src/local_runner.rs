//! Hosts the runner role inside the `tack` binary: `tack runner start` runs
//! it as the whole process, `tack serve --with-runner` runs it as a task
//! alongside the server in the same process.
//!
//! Both paths build a [`tack_runner::RunnerConfig`] through the exact
//! precedence rules `tack-runner`'s own binary uses
//! ([`load_runner_config`]) and then hand it to `tack_runner::bootstrap`,
//! the crate's single composition root — there is no second way to wire a
//! runner in this codebase, only two callers of the same one.
//!
//! The embedded case speaks to the server it is embedded in exactly like a
//! remote runner would: ordinary runner-v1 HTTP against the loopback
//! address the server actually bound. It does not reach into `tack-api`'s
//! router or state, and never will — a shortcut here would create a second
//! implementation of the runner protocol client that `docs/contracts/
//! runner-v1/` cannot hold accountable.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use tack_runner::{
    ConfigError, ConfigOverrides, EnrollmentCredential, RunnerConfig, RunnerConfigSources,
    RunnerError, Shutdown, ShutdownHandle,
    bootstrap::{self, RunnerLimits},
    harness::process::ProcessLimits,
};
use tokio::task::JoinHandle;

/// Bounds applied to every harness subprocess a runner hosted by this binary
/// spawns. Mirrors the standalone `tack-runner` binary's own bounds: both
/// binaries run the identical composition root and there is no reason for a
/// harness to behave differently depending on which process launched it.
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

/// Loads runner configuration from an optional TOML file, the environment,
/// then `command_line` overrides, in that precedence — identical to
/// `tack-runner`'s own binary, reusing its types rather than re-parsing.
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

/// Whether the embedded runner should start, combining `--with-runner` with
/// its environment equivalent. Off unless one of the two explicitly says on.
pub fn with_runner_enabled(flag: bool) -> bool {
    flag || std::env::var("TACK_LOCAL_RUNNER_ENABLE")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Runs the runner role as the whole process (`tack runner start`), owning
/// its own Tokio runtime and its own ctrl-c wiring — the same shutdown
/// pattern `tack-runner`'s binary uses, reimplemented here because that
/// binary's `main` is not library code this crate can call into.
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

/// Rejects a server configuration the embedded runner must never start
/// against. An embedded runner executes arbitrary agent processes on the
/// host serving the UI, so a non-loopback bind is refused outright rather
/// than downgraded to "serve without a runner" — a caller must fail before
/// starting anything else.
fn ensure_loopback(config: &tack_api::config::AppConfig) -> anyhow::Result<()> {
    if !config.binds_loopback() {
        anyhow::bail!(
            "refusing to start --with-runner: {} is not a loopback address. An embedded \
             runner executes arbitrary agent processes on this host, so it is restricted \
             to a server bound to loopback",
            config.host
        );
    }
    Ok(())
}

/// Runs the server and an embedded runner together in one process
/// (`tack serve --with-runner`).
///
/// Refuses to start (before opening a socket or a database) when the server
/// is not bound to loopback — a startup error, never a silent downgrade to a
/// runner-less server. Once the server signals the address it actually
/// bound, the runner config is given a credential (manual, a stored session,
/// or a freshly self-provisioned one — see [`ensure_runner_credential`]),
/// pointed at that exact address, and handed to [`supervise`], which makes
/// either side dying take the whole process down loudly.
pub async fn serve_with_embedded_runner() -> anyhow::Result<()> {
    let server_config = tack_api::config::AppConfig::load();
    ensure_loopback(&server_config)?;

    let mut runner_config = load_runner_config(ConfigOverrides::default(), None)?;

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let mut server_task = tokio::spawn(tack_api::serve_with_ready(ready_tx));

    let bound_addr = match ready_rx.await {
        Ok(addr) => addr,
        Err(_) => {
            // The sender was dropped without sending, which only happens when
            // `serve_with_ready` returned before opening its listener; that
            // task's own result is the error worth surfacing, not the closed
            // channel.
            return match server_task.await {
                Ok(Ok(())) => Err(anyhow::anyhow!("server exited before it became ready")),
                Ok(Err(error)) => Err(error),
                Err(join_error) => Err(anyhow::anyhow!("server task panicked: {join_error}")),
            };
        }
    };

    // Only the address the listener actually bound is authoritative — the
    // configured port may have been 0, letting the OS choose it, and any
    // `api_base_url` sourced from file or environment describes a different
    // (remote-runner) deployment shape that does not apply here.
    runner_config.api_base_url = format!("http://{bound_addr}/api/runner/v1");

    // The server is up and its database is open; a credential failure past
    // this point must take the server down with it rather than leave it
    // running with no runner attached to it.
    if let Err(error) = ensure_runner_credential(&mut runner_config, &server_config).await {
        server_task.abort();
        return Err(error);
    }

    let (shutdown, shutdown_handle) = Shutdown::channel();
    let mut runner_task = tokio::spawn(bootstrap::run(runner_config, runner_limits(), shutdown));

    supervise(&mut server_task, &mut runner_task, &shutdown_handle).await
}

/// Makes sure `runner_config` carries something the embedded runner can
/// redeem, in order of preference:
///
/// 1. a manually configured `enrollment_credential` — always wins, and is
///    left untouched;
/// 2. a durable session already on disk under `state_dir` — reused as is.
///    [`crate::local_enrollment::self_provision`] is not called, so a
///    restart against an already-enrolled `state_dir` never mints a second
///    one-time token or creates a second pending runner row. The config's
///    credential is still set, to a placeholder
///    (`local_enrollment::stored_session_placeholder`) rather than left
///    empty — `tack_runner::bootstrap::build_runtime` requires *some*
///    credential before it ever looks at `state_dir`, a precondition that
///    predates this card (see that placeholder's own doc comment for the
///    full explanation and why it is not transmitted on a normal restart);
/// 3. otherwise, a one-time token self-provisioned in-process against the
///    server's own database (legitimate here specifically because the
///    operator and the runner are the same person on the same machine — see
///    `docs/adr/0058-standalone-single-binary-runner.md`).
///
/// Only ever called after [`ensure_loopback`] has already passed, since it
/// runs after the server has started inside [`serve_with_embedded_runner`] —
/// self-provisioning inherits that guard rather than re-deriving it.
async fn ensure_runner_credential(
    runner_config: &mut RunnerConfig,
    server_config: &tack_api::config::AppConfig,
) -> anyhow::Result<()> {
    if runner_config.enrollment_credential.is_some() {
        return Ok(());
    }
    if crate::local_enrollment::has_stored_session(&runner_config.state_dir) {
        runner_config.enrollment_credential =
            Some(crate::local_enrollment::stored_session_placeholder());
        return Ok(());
    }
    let credential = crate::local_enrollment::self_provision(&server_config.database_url).await?;
    runner_config.enrollment_credential = Some(credential);
    Ok(())
}

/// Waits on whichever of the server or embedded-runner task finishes first
/// and reacts honestly to which one it was:
///
/// - the server finishing is the normal shutdown path (it owns its own
///   ctrl-c handling) — the embedded runner has no reason to keep running
///   once its host process is going down, so it is asked to stop and then
///   joined;
/// - the runner finishing first, while nothing asked it to, means it died
///   or never got started — the server is aborted immediately and the
///   error is returned, because a server left running with its runner
///   silently gone is indistinguishable from a scheduler bug to whoever is
///   watching `tack serve`.
async fn supervise(
    server_task: &mut JoinHandle<anyhow::Result<()>>,
    runner_task: &mut JoinHandle<Result<(), RunnerError>>,
    shutdown_handle: &ShutdownHandle,
) -> anyhow::Result<()> {
    tokio::select! {
        server_result = &mut *server_task => {
            shutdown_handle.request();
            let runner_result = (&mut *runner_task)
                .await
                .map_err(|error| anyhow::anyhow!("embedded runner task panicked: {error}"))?;
            server_result
                .map_err(|error| anyhow::anyhow!("server task panicked: {error}"))??;
            runner_result.map_err(anyhow::Error::from)
        }
        runner_result = &mut *runner_task => {
            server_task.abort();
            let runner_result = runner_result
                .map_err(|error| anyhow::anyhow!("embedded runner task panicked: {error}"))?;
            match runner_result {
                Ok(()) => Err(anyhow::anyhow!(
                    "embedded runner exited unexpectedly while the server was still running"
                )),
                Err(error) => Err(anyhow::anyhow!(
                    "embedded runner failed while the server was still running: {error}"
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_runner_refuses_non_loopback_bind() {
        let config = tack_api::config::AppConfig {
            host: "0.0.0.0".to_owned(),
            ..tack_api::config::AppConfig::default()
        };

        let error = ensure_loopback(&config).expect_err("non-loopback bind must be refused");
        assert!(error.to_string().contains("loopback"));
    }

    #[test]
    fn embedded_runner_accepts_the_default_loopback_bind() {
        assert!(ensure_loopback(&tack_api::config::AppConfig::default()).is_ok());
    }

    #[test]
    fn with_runner_enabled_reads_the_environment_gate() {
        // SAFETY: this test owns the variable for its own duration and restores
        // it, but `std::env::set_var` is unsound to run concurrently with other
        // threads reading the same variable — `cargo test` runs this crate's
        // tests in one process, so serialize via the variable's own uniqueness
        // (nothing else in this crate reads `TACK_LOCAL_RUNNER_ENABLE`).
        let previous = std::env::var("TACK_LOCAL_RUNNER_ENABLE").ok();
        assert!(!with_runner_enabled(false));

        unsafe {
            std::env::set_var("TACK_LOCAL_RUNNER_ENABLE", "1");
        }
        assert!(with_runner_enabled(false));
        assert!(with_runner_enabled(true));

        unsafe {
            match &previous {
                Some(value) => std::env::set_var("TACK_LOCAL_RUNNER_ENABLE", value),
                None => std::env::remove_var("TACK_LOCAL_RUNNER_ENABLE"),
            }
        }
    }

    /// Proves the "loud, not silent" half of ADR 0058's failure mode: if the
    /// embedded runner stops before anything asked it to, the server task is
    /// not left running.
    #[tokio::test]
    async fn supervise_aborts_the_server_when_the_runner_dies_first() {
        let mut server_task: JoinHandle<anyhow::Result<()>> = tokio::spawn(std::future::pending());
        let mut runner_task: JoinHandle<Result<(), RunnerError>> =
            tokio::spawn(async { Err(RunnerError::ClientStopped) });
        let (_shutdown, shutdown_handle) = Shutdown::channel();

        let result = supervise(&mut server_task, &mut runner_task, &shutdown_handle).await;

        let error = result.expect_err("a runner dying early must surface as a process error");
        assert!(error.to_string().contains("embedded runner"));
        let join_error = server_task
            .await
            .expect_err("the server task must have been aborted, not left running");
        assert!(join_error.is_cancelled());
    }

    /// Proves the clean-shutdown half: the server finishing (its own ctrl-c
    /// path) tells the embedded runner to stop too, rather than leaving it
    /// running past its host.
    #[tokio::test]
    async fn supervise_stops_the_runner_once_the_server_stops() {
        let mut server_task: JoinHandle<anyhow::Result<()>> = tokio::spawn(async { Ok(()) });
        let (mut shutdown, shutdown_handle) = Shutdown::channel();
        let mut runner_task: JoinHandle<Result<(), RunnerError>> = tokio::spawn(async move {
            shutdown.requested().await;
            Ok(())
        });

        let result = supervise(&mut server_task, &mut runner_task, &shutdown_handle).await;

        assert!(
            result.is_ok(),
            "a clean shutdown must not surface an error: {result:?}"
        );
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tack-local-runner-test-{label}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create test temp dir");
        dir
    }

    /// A `database_url` that fails at parse time, before any filesystem or
    /// network I/O — so a test built against it proves whether
    /// `tack_db::init_pool` (and therefore self-provisioning) was ever
    /// reached, without needing a real database.
    fn unreachable_database_url() -> tack_api::config::AppConfig {
        tack_api::config::AppConfig {
            database_url: "not-a-real-database-url".to_owned(),
            ..tack_api::config::AppConfig::default()
        }
    }

    #[tokio::test]
    async fn ensure_runner_credential_leaves_a_manual_credential_untouched() {
        let state_dir = unique_temp_dir("manual");
        let mut runner_config = RunnerConfig {
            enrollment_credential: Some(EnrollmentCredential::new("manually-configured-token")),
            state_dir: state_dir.clone(),
            ..RunnerConfig::defaults()
        };
        let server_config = unreachable_database_url();

        let result = ensure_runner_credential(&mut runner_config, &server_config).await;

        assert!(
            result.is_ok(),
            "a manual credential must never require touching the database: {result:?}"
        );
        assert_eq!(
            runner_config
                .enrollment_credential
                .expect("credential must remain set")
                .expose(),
            "manually-configured-token",
            "a manual credential must win over both the stored-session placeholder and self-provisioning"
        );
        std::fs::remove_dir_all(&state_dir).ok();
    }

    #[tokio::test]
    async fn ensure_runner_credential_reuses_a_stored_session_without_self_provisioning() {
        let state_dir = unique_temp_dir("stored-session");
        std::fs::write(state_dir.join("session.json"), "{}").expect("write session.json");
        let mut runner_config = RunnerConfig {
            enrollment_credential: None,
            state_dir: state_dir.clone(),
            ..RunnerConfig::defaults()
        };
        // Deliberately unreachable: if this path wrongly called
        // `local_enrollment::self_provision`, opening the pool would fail
        // and this assertion below would see an `Err`, not an `Ok`.
        let server_config = unreachable_database_url();

        let result = ensure_runner_credential(&mut runner_config, &server_config).await;

        assert!(
            result.is_ok(),
            "a stored session must satisfy the credential requirement without opening the database: {result:?}"
        );
        assert!(
            runner_config.enrollment_credential.is_some(),
            "build_runtime's precondition still needs a credential to be present"
        );
        std::fs::remove_dir_all(&state_dir).ok();
    }

    #[tokio::test]
    async fn ensure_runner_credential_attempts_self_provisioning_when_nothing_else_is_available() {
        let state_dir = unique_temp_dir("no-session");
        let mut runner_config = RunnerConfig {
            enrollment_credential: None,
            state_dir: state_dir.clone(),
            ..RunnerConfig::defaults()
        };
        let server_config = unreachable_database_url();

        let result = ensure_runner_credential(&mut runner_config, &server_config).await;

        assert!(
            result.is_err(),
            "with no manual credential and no stored session, self-provisioning must actually be \
             attempted — proved here by its failure against a deliberately unreachable database, \
             the same database_url the previous test proves does NOT get touched when a session exists"
        );
        std::fs::remove_dir_all(&state_dir).ok();
    }
}
