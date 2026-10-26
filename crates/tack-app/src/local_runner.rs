//! The embedded runner, as the rest of Tack sees it: the seam `tack-api` calls to turn it on and
//! off, the preference that says whether it should run, the folder checks and the test run that
//! the Settings screens ask of it, and the supervisor that keeps one runner process alive.
//!
//! This crate never depends on `tack-runner`. [`LocalRunnerControl`] is what the routes call, and
//! [`EmbeddedRunner`] implements it over a [`RunnerHost`], the few things only the binary that
//! links the runner can answer (`tack-cli`'s `local_runner` module). `AppState::local_runner` is
//! `None` for a caller that never wired one in; the routes then do not exist.

mod folder;
mod paths;
mod preference;
mod supervisor;
mod test_run;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;

pub use folder::{
    DetectedInstall, FolderError, FolderFacts, check_folder, folder_path, init_folder,
};
pub use paths::{
    NotLoopback, config_path_from, ensure_loopback, has_stored_session, host_name,
    migrate_legacy_state_dir, state_dir_under, with_runner_enabled,
};
pub use preference::{effective_enabled, set_enabled};
pub use supervisor::{EmbeddedRunner, RunnerHost, RunnerProcess};
pub use test_run::{TestRunError, start_test_run};

/// The embedded runner's actual runtime state, never the persisted preference: a preference can
/// read "on" for one instant after boot, before the auto-start task has run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeState {
    Stopped,
    Running,
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeStatus {
    pub state: RuntimeState,
    pub since: Option<DateTime<Utc>>,
    /// How many attempts the runner works at once, as configured.
    pub max_concurrent_attempts: u32,
    /// How many attempts it is working on right now; 0 while stopped.
    pub active_attempts: u32,
}

/// Every variant names an operation and, where the backend gave one, its own reason, never a
/// secret value. Safe to log.
#[derive(Debug, thiserror::Error)]
pub enum LocalRunnerControlError {
    #[error("embedded runner failed to start: {0}")]
    StartFailed(String),
    #[error("secret store operation failed: {0}")]
    SecretStore(String),
}

/// The seam `tack-api` calls to control an embedded runner without depending on `tack-runner`
/// or learning where its secret store lives.
#[async_trait]
pub trait LocalRunnerControl: Send + Sync {
    /// Whether the embedded runner is alive right now, and since when.
    async fn status(&self) -> RuntimeStatus;

    /// Starts the embedded runner if it is not already running. A no-op, not an error, if it is.
    async fn start(&self) -> Result<(), LocalRunnerControlError>;

    /// Stops the embedded runner if running. A no-op otherwise.
    async fn stop(&self);

    /// The enrolled runner's id once the runner has registered, `None` before.
    async fn runner_id(&self) -> Option<String>;

    /// Read-only resolution of a stored `secret_reference` (`store:<name>` or `env:<VARIABLE>`)
    /// for the server's own outbound calls, such as a project's GitHub token. The value never
    /// leaves the process in a response or a log.
    async fn resolve_secret(&self, reference: &str) -> Result<String, LocalRunnerControlError>;

    /// The program the runner verifies with when its `[verify]` is prill, so the board runs the
    /// same prill; `None` when it verifies with nothing or with another program.
    async fn verify_program(&self) -> Option<String> {
        None
    }

    /// The settings file the runner read at start; `None` when it runs on defaults, which means
    /// no `[docket]`, no `[verify]` and no `[git]`.
    fn config_file(&self) -> Option<String> {
        None
    }
}
