//! Where the embedded runner keeps its state, which settings file it reads, and the rules for
//! whether it may start: decisions over paths, flags and environment variables, with no
//! runner type in them.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The server was asked to start an embedded runner on a bind that is not loopback.
#[derive(Debug, thiserror::Error)]
#[error(
    "refusing to start with an embedded runner: {host} is not a loopback address. An \
     embedded runner executes arbitrary agent processes on this host, so it is \
     restricted to a server bound to loopback"
)]
pub struct NotLoopback {
    host: String,
}

/// Refuses an embedded runner on a bind that is not loopback: it executes arbitrary agent
/// processes on the host serving the UI.
pub fn ensure_loopback(host: &str, binds_loopback: bool) -> Result<(), NotLoopback> {
    if binds_loopback {
        Ok(())
    } else {
        Err(NotLoopback {
            host: host.to_owned(),
        })
    }
}

/// Whether the embedded runner should start at boot: `--with-runner`, or its environment
/// equivalent. Off unless one explicitly says on.
pub fn with_runner_enabled(flag: bool) -> bool {
    flag || std::env::var("TACK_LOCAL_RUNNER_ENABLE")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Where the embedded runner's on-disk state (credential, attempt journal, secret store) lives
/// by default: one level under this server's own `storage_dir`, so a server on a different
/// database never resolves to another server's runner state.
pub fn state_dir_under(storage_dir: &str) -> PathBuf {
    Path::new(storage_dir).join("runner")
}

/// Best-effort recovery for an install whose runner state still sits at the bare, cwd-relative
/// `legacy_dir` instead of the database-scoped directory: renames it to `new_dir` so an
/// already-enrolled credential is not stranded. No-op if `new_dir` already exists (existing
/// state is never clobbered). A failed rename leaves the legacy directory in place and logged;
/// the caller provisions a fresh identity instead.
pub fn migrate_legacy_state_dir(legacy_dir: &Path, new_dir: &Path) {
    if new_dir.exists() || !legacy_dir.is_dir() {
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

/// The settings file the variable names. Set but empty, it names none and the default is not
/// read either: that is how a script asks for defaults only. Unset, the default when that file
/// exists.
pub fn config_path_from(variable: Option<OsString>, default: Option<PathBuf>) -> Option<PathBuf> {
    match variable {
        Some(value) if value.is_empty() => None,
        Some(value) => Some(PathBuf::from(value)),
        None => default.filter(|path| path.is_file()),
    }
}

/// Whether a durable session from a prior enrollment already exists in `state_dir`.
pub fn has_stored_session(state_dir: &Path) -> bool {
    state_dir.join("session.json").is_file()
}

/// This computer's host name, empty when the system will not say.
pub fn host_name() -> String {
    std::process::Command::new("hostname")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .unwrap_or_default()
}
