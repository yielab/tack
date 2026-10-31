//! The composition root of `tack serve`: the one place that opens the database, builds the
//! `Repository` and the `AppState`, and hands them to the HTTP server. The background loops
//! are started here too, by `tack_app::loop_config::LoopConfig::start` into one
//! `tack_app::background::Loops` that is stopped when the server returns; the server itself
//! starts only the artifact sweep, which needs its HTTP types.

use std::net::SocketAddr;
use std::sync::Arc;

use tack_api::LocalRunnerControl;
use tack_api::config::AppConfig;
use tack_api::router::AppState;
use tack_api::server::prepare;
use tack_api::webhook::WebhookClient;
use tack_app::background::{Loops, STOP_GRACE};
use tack_app::due_soon::DueSoonSink;
use tack_db::{Repository, init_pool, migrations, repo};
use tack_protocol::{Clock, SystemClock};
use tokio::sync::oneshot;
use tokio_util::task::TaskTracker;
use tracing::info;

/// Boot the server and block until it is shut down. `ready_tx` receives the real bound
/// address once the listener is open; `local_runner` is the embedded runner the UI can turn on
/// (`None` leaves its routes absent).
pub async fn run(
    mut config: AppConfig,
    ready_tx: Option<oneshot::Sender<SocketAddr>>,
    local_runner: Option<Arc<dyn LocalRunnerControl>>,
) -> anyhow::Result<()> {
    prepare(&mut config)?;

    let pool = init_pool(&config.database_url).await?;
    migrations::run_all(&pool).await?;
    let repo = Repository::new(pool);
    repo::templates::seed_builtin_templates(&repo).await?;
    repo::execution::seed_builtin_profiles(&repo, SystemClock.now()).await?;

    let workspace_id = repo.ensure_default_workspace().await?;

    // A board from before a decision ended with its run left questions nobody can answer.
    let closed = repo
        .expire_decisions_of_ended_attempts(SystemClock.now())
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

    let mut loops = Loops::new();

    let state = AppState {
        repo,
        config,
        workspace_id,
        broadcast_tx,
        webhook,
        local_runner,
        tasks: TaskTracker::new(),
    };

    // The loops the configuration turns on; the announcement of items due within the hour goes
    // to the webhook, when one is there to hear it.
    let due_soon = state
        .webhook
        .clone()
        .map(|webhook| Arc::new(webhook) as Arc<dyn DueSoonSink>);
    state.config.loops.start(
        &mut loops,
        &state.repo,
        Arc::new(SystemClock),
        state.ports(),
        state.config.backup_config(),
        due_soon,
    );

    let served = tack_api::serve(state, ready_tx).await;
    loops.stop(STOP_GRACE).await;
    served
}

#[cfg(test)]
mod tests;
