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

/// Where the embedded runner's on-disk state (credential, attempt journal, secret store) lives
/// by default: one level under this server's own `storage_dir`, so a server on a different
/// database never resolves to another server's runner state.
pub fn state_dir_under(storage_dir: &str) -> PathBuf {
    Path::new(storage_dir).join("runner")
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
