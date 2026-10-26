//! The composition root of `tack serve`: the one place that opens the database, builds the
//! `Repository` and the `AppState`, and hands them to the HTTP server. The background loops
//! are spawned here too, one `tack_app::<module>::run(..)` per loop, as each moves out of
//! `tack-api`; the server itself starts none of them.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tack_api::LocalRunnerControl;
use tack_api::config::AppConfig;
use tack_api::router::AppState;
use tack_api::server::{ensure_default_workspace, prepare};
use tack_api::webhook::WebhookClient;
use tack_db::{Repository, init_pool, migrations, repo};
use tack_protocol::SystemClock;
use tokio::sync::oneshot;
use tracing::info;

/// Boot the server and block until it is shut down. `ready_tx` receives the real bound
/// address once the listener is open; `local_runner` is the embedded runner the UI can turn on
/// (`None` leaves its routes absent).
pub async fn run(
    ready_tx: Option<oneshot::Sender<SocketAddr>>,
    local_runner: Option<Arc<dyn LocalRunnerControl>>,
) -> anyhow::Result<()> {
    let mut config = AppConfig::load();
    prepare(&mut config)?;

    let pool = init_pool(&config.database_url).await?;
    migrations::run_all(&pool).await?;
    repo::templates::seed_builtin_templates(&pool).await?;
    repo::execution::seed_builtin_profiles(&pool).await?;

    let workspace_id = ensure_default_workspace(&pool).await?;
    let repo = Repository::new(pool);

    // A board from before a decision ended with its run left questions nobody can answer.
    let closed = repo
        .expire_decisions_of_ended_attempts(&tack_protocol::SystemClock)
        .await?;
    if closed > 0 {
        info!(
            closed,
            "closed the questions of runs that had already ended"
        );
    }

    // WebSocket update fan-out; capacity 100 messages.
    let (broadcast_tx, _) = tokio::sync::broadcast::channel(100);

    let webhook = config.webhook_url.clone().map(|url| {
        info!(url=%url, "Webhook delivery enabled");
        WebhookClient::new(url, config.webhook_secret.clone())
    });

    let state = AppState {
        repo,
        config,
        workspace_id,
        broadcast_tx,
        webhook,
        local_runner,
    };

    // Tasks a project starts by itself; off when its interval is 0.
    if state.config.auto_start_seconds > 0 {
        tokio::spawn(tack_app::auto_start::run(
            state.repo.clone(),
            Arc::new(SystemClock),
            Arc::new(state.host()),
            Duration::from_secs(state.config.auto_start_seconds),
        ));
    }

    // Purges stale replay bookkeeping and terminal events; off unless the operator opts in, since
    // it deletes rows.
    if state.config.execution_retention_enable {
        tokio::spawn(tack_app::retention::run(
            state.repo.clone(),
            Arc::new(SystemClock),
            tack_app::retention::Settings {
                retention_days: state.config.execution_retention_days,
                batch_size: tack_app::retention::DEFAULT_BATCH_SIZE,
            },
            Duration::from_secs(state.config.execution_retention_interval_secs),
        ));
    }

    // Logs a stale lease or a request stuck in needs_operator; on by default, it only reads.
    if state.config.execution_health_enable {
        tokio::spawn(tack_app::observability::run(
            state.repo.clone(),
            Arc::new(SystemClock),
            tack_app::observability::Settings::default(),
            Duration::from_secs(state.config.execution_health_interval_secs),
        ));
    }

    tack_api::serve(state, ready_tx).await
}

#[cfg(test)]
mod tests;
