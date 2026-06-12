use std::net::SocketAddr;
use std::time::Duration;

use chrono::Utc;
use tracing::{info, warn};
use uuid::Uuid;

use flexpm_api::config::AppConfig;
use flexpm_api::remote_backup;
use flexpm_api::router::{AppState, build_router};
use flexpm_api::webhook::WebhookClient;
use flexpm_db::{Repository, init_pool, migrations, repo};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration
    let config = AppConfig::load();

    // Initialize logging/tracing
    init_tracing(&config);

    info!(
        version = env!("CARGO_PKG_VERSION"),
        host = %config.host,
        port = config.port,
        database = %config.database_url,
        "Starting FlexPM server"
    );

    // Apply any staged restore before opening the pool.
    apply_staged_restore(&config);

    // Initialize database
    let pool = init_pool(&config.database_url).await?;
    migrations::run_all(&pool).await?;
    repo::templates::seed_builtin_templates(&pool).await?;

    // Ensure a default workspace exists
    let workspace_id = ensure_default_workspace(&pool).await?;

    // Build application state
    let repo = Repository::new(pool);

    // Create broadcast channel for WebSocket updates (capacity: 100 messages)
    let (broadcast_tx, _) = tokio::sync::broadcast::channel(100);

    let webhook = config.webhook_url.clone().map(|url| {
        info!(url=%url, "Webhook delivery enabled");
        WebhookClient::new(url, config.webhook_secret.clone())
    });

    let state = AppState {
        repo,
        config: config.clone(),
        workspace_id,
        broadcast_tx,
        webhook,
    };

    // Spawn background task: automatic remote backup on configured interval
    if let Some(interval_secs) = config.backup_interval_secs {
        if config.remote_backup_enabled() {
            let bg_state = state.clone();
            tokio::spawn(async move {
                let mut interval =
                    tokio::time::interval(Duration::from_secs(interval_secs));
                interval.tick().await; // skip the first immediate tick
                loop {
                    interval.tick().await;
                    run_scheduled_backup(&bg_state).await;
                }
            });
            info!(interval_secs, "Remote backup scheduler enabled");
        }
    }

    // Spawn background task: fire "item.due_soon" webhook for items due within the next hour
    if state.webhook.is_some() {
        let bg_state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3600));
            interval.tick().await; // skip the first immediate tick
            loop {
                interval.tick().await;
                check_due_soon(&bg_state).await;
            }
        });
    }

    // Build router
    let app = build_router(state);

    // Start server
    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    info!(%addr, "Server listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Plain-stdout banner for end users running the distributed binary —
    // visible regardless of log level/format.
    let display_host = if config.host == "0.0.0.0" {
        "localhost"
    } else {
        &config.host
    };
    println!(
        "\n  FlexPM v{} is running.\n  Open http://{}:{} in your browser.\n  Press Ctrl+C to stop.\n",
        env!("CARGO_PKG_VERSION"),
        display_host,
        config.port
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Server shut down gracefully");
    Ok(())
}

fn init_tracing(config: &AppConfig) {
    use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!(
            "flexpm_api={level},flexpm_db={level},flexpm_core={level},tower_http=debug",
            level = config.log_level
        ))
    });

    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    if config.log_json {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer.json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
    }
}

async fn ensure_default_workspace(pool: &sqlx::SqlitePool) -> anyhow::Result<Uuid> {
    let existing: Option<String> = sqlx::query_scalar("SELECT id FROM workspaces LIMIT 1")
        .fetch_optional(pool)
        .await?;

    if let Some(id_str) = existing {
        Ok(Uuid::parse_str(&id_str)?)
    } else {
        let id = Uuid::new_v4();
        let vocab = serde_json::to_string(&flexpm_core::vocabulary::default_vocabulary())?;
        sqlx::query(
            "INSERT INTO workspaces (id, name, description, default_vocabulary) VALUES (?, 'Default Workspace', 'Auto-created workspace', ?)"
        )
        .bind(id.to_string())
        .bind(&vocab)
        .execute(pool)
        .await?;
        info!(%id, "Created default workspace");
        Ok(id)
    }
}

/// If a `.restore` file exists next to the live DB, apply it before startup.
///
/// When the restore originated from a remote bundle, an attachments staging dir
/// (`<storage_dir>.restore/`) may also exist. Both are swapped atomically:
/// - current DB   → `<db>.bak`
/// - `.restore`   → live DB path
/// - current attachments dir → `<storage_dir>.bak/`
/// - `.restore/`  → live storage dir
fn apply_staged_restore(config: &AppConfig) {
    let Some(db_path) = config.db_file_path() else {
        return;
    };
    let restore_str = format!("{}.restore", db_path.to_string_lossy());
    let restore_path = std::path::Path::new(&restore_str);
    if !restore_path.exists() {
        return;
    }
    warn!(path = %restore_str, "Applying staged restore");

    // Swap database
    let bak_str = format!("{}.bak", db_path.to_string_lossy());
    let _ = std::fs::rename(&db_path, &bak_str);
    if let Err(e) = std::fs::rename(restore_path, &db_path) {
        warn!("Failed to apply staged DB restore: {e}");
        return;
    }
    info!("DB restore applied; previous DB backed up to {bak_str}");

    // Swap attachments dir if a staging dir exists
    let storage_restore = format!("{}.restore", config.storage_dir);
    let storage_restore_path = std::path::Path::new(&storage_restore);
    if storage_restore_path.exists() {
        let storage_bak = format!("{}.bak", config.storage_dir);
        let _ = std::fs::rename(&config.storage_dir, &storage_bak);
        if let Err(e) = std::fs::rename(storage_restore_path, &config.storage_dir) {
            warn!("Failed to apply staged attachments restore: {e}");
        } else {
            info!(
                "Attachments restore applied; previous dir backed up to {}",
                storage_bak
            );
        }
    }
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C handler");
    warn!("Shutdown signal received");
}

/// Run a scheduled remote backup: create bundle, upload, prune old backups.
async fn run_scheduled_backup(state: &AppState) {
    match remote_backup::store_from_config(&state.config) {
        Ok(store) => {
            match remote_backup::create_bundle(state.pool(), &state.config).await {
                Ok((bundle, manifest)) => {
                    if let Err(e) =
                        remote_backup::upload(store.as_ref(), &manifest, bundle).await
                    {
                        warn!(error = %e, "Scheduled backup upload failed");
                        return;
                    }
                    if let Err(e) = remote_backup::prune(
                        store.as_ref(),
                        &state.config.backup_prefix,
                        state.config.backup_retention,
                    )
                    .await
                    {
                        warn!(error = %e, "Scheduled backup prune failed");
                    } else {
                        info!(key = %manifest.object_key, "Scheduled backup complete");
                    }
                }
                Err(e) => warn!(error = %e, "Scheduled backup bundle creation failed"),
            }
        }
        Err(e) => warn!(error = %e, "Scheduled backup store init failed"),
    }
}

/// Query for items due within the next hour and fire `item.due_soon` webhooks.
async fn check_due_soon(state: &AppState) {
    let Some(wh) = &state.webhook else { return };

    let now = Utc::now();
    let window_end = now + chrono::Duration::hours(1);

    match state.repo.list_items_due_soon(now, window_end).await {
        Ok(items) => {
            for item in items {
                let payload = serde_json::json!({
                    "event": "item.due_soon",
                    "timestamp": now.to_rfc3339(),
                    "project_id": item.project_id,
                    "item": item,
                });
                wh.fire("item.due_soon", payload);
            }
        }
        Err(e) => {
            tracing::warn!(error=%e, "Failed to query due-soon items for webhook");
        }
    }
}
