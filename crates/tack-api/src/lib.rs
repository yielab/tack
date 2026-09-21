pub mod config;
pub mod debug;
pub mod error;
// Execution-domain retention sweep + health watch runtime wiring — see the
// module's own doc comment.
pub mod execution_runtime;
pub mod github_sync;
pub mod handlers;
pub mod middleware;
pub mod openapi;
pub mod remote_backup;
pub mod router;
pub mod server;
pub mod webhook;

// Re-export commonly used items
pub use handlers::local_runner::{
    CatalogSnapshot, LocalRunnerControl, LocalRunnerControlError, RuntimeState, RuntimeStatus,
    SecretMeta,
};
pub use router::AppState;
pub use server::{serve, serve_with_ready, serve_with_ready_and_local_runner};

/// Background task: poll linked GitHub repos for inbound issue-state changes
/// on `config.github_poll_seconds`, the same interval/skip-first-tick shape
/// as the remote-backup scheduler in `server.rs`. A no-op loop (returns
/// immediately) when polling is off (`github_poll_seconds == 0`) or no
/// `github_token` is configured — [`github_sync::poll_once`] re-checks the
/// token every tick too, since a build without this gate could still be
/// reconfigured without a restart in the future.
///
/// A caller starts this with `tokio::spawn(run_github_poll_task(state.clone()))`
/// beside the backup scheduler's own spawn, once the listener is up.
pub async fn run_github_poll_task(state: AppState) {
    if state.config.github_poll_seconds == 0 || state.config.github_token.is_none() {
        return;
    }
    let mut etags = std::collections::HashMap::new();
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(
        state.config.github_poll_seconds,
    ));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    interval.tick().await; // skip the first immediate tick
    loop {
        interval.tick().await;
        match github_sync::poll_once(&state, &mut etags).await {
            Ok(summary) => {
                tracing::debug!(
                    repos_polled = summary.repos_polled,
                    issues_updated = summary.issues_updated,
                    "GitHub inbound poll complete"
                );
            }
            Err(error) => tracing::warn!(%error, "GitHub inbound poll failed"),
        }
    }
}
