use std::{path::PathBuf, process::ExitCode, time::Duration};

use clap::Parser;
use tack_runner::{
    ConfigError, ConfigOverrides, EnrollmentCredential, RunnerConfig, RunnerConfigSources,
    Shutdown,
    bootstrap::{self, RunnerLimits},
    harness::process::ProcessLimits,
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

    let limits = RunnerLimits {
        harness_process: HARNESS_PROCESS_LIMITS,
        protocol_request_timeout: PROTOCOL_REQUEST_TIMEOUT,
    };
    let (shutdown, shutdown_handle) = Shutdown::channel();
    let mut runtime_task = tokio::spawn(bootstrap::run(config, limits, shutdown));

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
