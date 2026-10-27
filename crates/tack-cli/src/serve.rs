//! The composition root of `tack serve`: the one place that opens the database, builds the
//! `Repository` and the `AppState`, and hands them to the HTTP server. The background loops
//! are started here too, one `tack_app::<module>::spawn(..)` per loop into one
//! `tack_app::background::Loops` that is stopped when the server returns; the server itself
//! starts only the artifact sweep, which needs its HTTP types.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tack_api::LocalRunnerControl;
use tack_api::config::AppConfig;
use tack_api::router::AppState;
use tack_api::server::prepare;
use tack_api::webhook::WebhookClient;
use tack_app::background::{Loops, STOP_GRACE};
use tack_db::{Repository, init_pool, migrations, repo};
use tack_protocol::{Clock, SystemClock};
use tokio::sync::oneshot;
use tokio_util::task::TaskTracker;
use tracing::{info, warn};

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
    spawn_backup_scheduler(&mut loops, &config, &repo);

    let state = AppState {
        repo,
        config,
        workspace_id,
        broadcast_tx,
        webhook,
        local_runner,
        tasks: TaskTracker::new(),
    };

    // Tasks a project starts by itself; off when its interval is 0.
    if state.config.auto_start_seconds > 0 {
        tack_app::auto_start::spawn(
            &mut loops,
            state.repo.clone(),
            Arc::new(SystemClock),
            Arc::new(state.host()),
            Duration::from_secs(state.config.auto_start_seconds),
        );
    }

    // Inbound GitHub poll: returns at once when polling is off or no token is set.
    tack_app::github_sync::spawn(
        &mut loops,
        state.repo.clone(),
        Arc::new(tack_protocol::SystemClock),
        tack_app::github_sync::Settings {
            github_token: state.config.github_token.clone(),
            api_base: state.config.github_api_base.clone(),
        },
        Arc::new(state.host()),
        Duration::from_secs(state.config.github_poll_seconds),
    );

    // Announce the items due within the hour, when a webhook is there to hear it.
    if let Some(webhook) = state.webhook.clone() {
        tack_app::due_soon::spawn(
            &mut loops,
            state.repo.clone(),
            Arc::new(tack_protocol::SystemClock),
            Arc::new(webhook),
            tack_app::due_soon::INTERVAL,
        );
    }

    // Purges stale replay bookkeeping and terminal events and expires overdue decisions; off unless
    // the operator opts in, since it deletes rows.
    if state.config.execution_retention_enable {
        tack_app::retention::spawn(
            &mut loops,
            state.repo.clone(),
            Arc::new(SystemClock),
            tack_app::retention::Settings {
                retention_days: state.config.execution_retention_days,
                batch_size: tack_app::retention::DEFAULT_BATCH_SIZE,
            },
            Duration::from_secs(state.config.execution_retention_interval_secs),
        );
    }

    // Logs a stale lease or a request stuck in needs_operator; on by default, it only reads.
    if state.config.execution_health_enable {
        tack_app::observability::spawn(
            &mut loops,
            state.repo.clone(),
            Arc::new(SystemClock),
            tack_app::observability::Settings::default(),
            Duration::from_secs(state.config.execution_health_interval_secs),
        );
    }

    let served = tack_api::serve(state, ready_tx).await;
    loops.stop(STOP_GRACE).await;
    served
}

/// Automatic remote backup on a configured interval, clamped to a 60s floor
/// (`tokio::time::interval` panics on zero, and anything faster would hammer
/// the object store). The interval is env-only (`TACK_BACKUP_INTERVAL_SECS`),
/// but the destination can come from UI-saved settings too, so this spawns
/// whenever an interval is set and the loop re-checks the effective config each tick.
fn spawn_backup_scheduler(loops: &mut Loops, config: &AppConfig, repo: &Repository) {
    let Some(interval_secs) = config.backup_interval_secs else {
        return;
    };
    const MIN_BACKUP_INTERVAL_SECS: u64 = 60;
    let interval_secs = if interval_secs < MIN_BACKUP_INTERVAL_SECS {
        warn!(
            requested = interval_secs,
            clamped_to = MIN_BACKUP_INTERVAL_SECS,
            "TACK_BACKUP_INTERVAL_SECS is below the 60s minimum; clamping"
        );
        MIN_BACKUP_INTERVAL_SECS
    } else {
        interval_secs
    };
    tack_app::remote_backup::spawn(
        loops,
        repo.clone(),
        Arc::new(SystemClock),
        config.backup_config(),
        Duration::from_secs(interval_secs),
    );
    info!(interval_secs, "Remote backup scheduler enabled");
}

#[cfg(test)]
mod tests;
