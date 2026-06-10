use std::net::SocketAddr;

use tracing::{info, warn};
use uuid::Uuid;

use flexpm_api::config::AppConfig;
use flexpm_api::router::{AppState, build_router};
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

    let state = AppState {
        repo,
        config: config.clone(),
        workspace_id,
        broadcast_tx,
    };

    // Build router
    let app = build_router(state);

    // Start server
    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
    info!(%addr, "Server listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
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
/// Moves the current DB to `.bak` (overwriting any previous bak), then renames
/// `.restore` into place.
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
    let bak_str = format!("{}.bak", db_path.to_string_lossy());
    let _ = std::fs::rename(&db_path, &bak_str);
    if let Err(e) = std::fs::rename(restore_path, &db_path) {
        warn!("Failed to apply staged restore: {e}");
    } else {
        info!("Restore applied; previous DB backed up to {bak_str}");
    }
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C handler");
    warn!("Shutdown signal received");
}
