//! The composition root of `tack serve`: the one place that opens the database, builds the
//! `Repository` and the `AppState`, and hands them to the HTTP server. The background loops
//! are spawned here too, one `tack_app::<module>::run(..)` per loop, as each moves out of
//! `tack-api`; the server itself starts none of them.

use std::net::SocketAddr;
use std::sync::Arc;

use tack_api::LocalRunnerControl;
use tack_api::config::AppConfig;
use tack_api::router::AppState;
use tack_api::server::{ensure_default_workspace, prepare};
use tack_api::webhook::WebhookClient;
use tack_db::{Repository, init_pool, migrations, repo};
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

    tack_api::serve(state, ready_tx).await
}

#[cfg(test)]
mod tests;
