//! `tack runner secret set|list|remove`: manages the runner-local secret
//! store a harness's `secret_reference` environment entries resolve
//! against, without enrolling a runner or requiring a running server.
//!
//! Talks to no server: this reads and writes the same on-disk/keychain
//! state a real `tack-runner` process would, using the same
//! [`RunnerConfig`] resolution (so `--state-dir`/`TACK_RUNNER_STATE_DIR`
//! behave identically here and there).

use std::io::Read;
use std::path::PathBuf;

use tack_runner::{RunnerConfig, RunnerConfigSources, SecretStore};

use crate::SecretAction;

fn resolve_state_dir(state_dir: Option<PathBuf>) -> PathBuf {
    RunnerConfig::from_sources(RunnerConfigSources {
        environment: RunnerConfig::environment_overrides(),
        command_line: tack_runner::ConfigOverrides {
            state_dir,
            ..Default::default()
        },
        ..Default::default()
    })
    .map(|config| config.secret_store_path())
    .unwrap_or_else(|_| RunnerConfig::defaults().secret_store_path())
}

/// Reads the secret value from `TACK_RUNNER_SECRET_VALUE` if set, otherwise
/// from stdin. Never accepts it as a positional argument — an argv value
/// would be visible in `ps`/`/proc/<pid>/cmdline` and in shell history.
fn read_secret_value() -> anyhow::Result<String> {
    if let Ok(value) = std::env::var("TACK_RUNNER_SECRET_VALUE") {
        return Ok(value);
    }
    let mut buffer = String::new();
    std::io::stdin().read_to_string(&mut buffer)?;
    let trimmed = buffer.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        anyhow::bail!(
            "no secret value given: set TACK_RUNNER_SECRET_VALUE or pipe the value to stdin"
        );
    }
    Ok(trimmed.to_owned())
}

pub fn run(action: SecretAction) -> anyhow::Result<()> {
    match action {
        SecretAction::Set { name, state_dir } => cmd_set(name, state_dir),
        SecretAction::List { state_dir, json } => cmd_list(state_dir, json),
        SecretAction::Remove { name, state_dir } => cmd_remove(name, state_dir),
    }
}

fn cmd_set(name: String, state_dir: Option<PathBuf>) -> anyhow::Result<()> {
    let store = SecretStore::open(&resolve_state_dir(state_dir));
    let value = read_secret_value()?;
    store.set(&name, &value)?;
    println!("stored {name:?} in the {} backend", store.backend());
    Ok(())
}

fn cmd_list(state_dir: Option<PathBuf>, json: bool) -> anyhow::Result<()> {
    let store = SecretStore::open(&resolve_state_dir(state_dir));
    let names = store.list()?;

    if json {
        println!("{}", serde_json::to_string_pretty(&names)?);
        return Ok(());
    }

    println!("backend: {}", store.backend());
    if names.is_empty() {
        println!("(no secrets stored)");
    } else {
        for name in names {
            println!("  {name}");
        }
    }
    Ok(())
}

fn cmd_remove(name: String, state_dir: Option<PathBuf>) -> anyhow::Result<()> {
    let store = SecretStore::open(&resolve_state_dir(state_dir));
    store.remove(&name)?;
    println!("removed {name:?} from the {} backend", store.backend());
    Ok(())
}
