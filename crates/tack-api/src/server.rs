//! Server entry point, exposed as a library function so the single `tack`
//! binary (in the `tack-cli` crate) can start the HTTP server in-process.

use std::net::SocketAddr;
use std::time::Duration;

use chrono::Utc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::handlers::settings::effective_orch_enabled;
use crate::orch_store::build_control_plane_store;
use crate::remote_backup;
use crate::router::{AppState, build_router};
use crate::webhook::WebhookClient;
use tack_db::{Repository, init_pool, migrations, repo};
use tack_orch::reconciler;

/// Boot the Tack HTTP server: load config, run migrations, start background
/// tasks, and serve until a shutdown signal is received.
pub async fn serve() -> anyhow::Result<()> {
    // Load configuration
    let config = AppConfig::load();

    // Initialize logging/tracing
    init_tracing(&config);

    // Shout about insecure configurations before doing anything else.
    security_preflight(&config);

    info!(
        version = env!("CARGO_PKG_VERSION"),
        host = %config.host,
        port = config.port,
        database = %config.database_url,
        "Starting Tack server"
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
        orch_runtime: crate::orch_runtime::OrchRuntime::new(),
    };

    // Spawn background task: automatic remote backup on configured interval.
    // Guard the interval: `tokio::time::interval` panics on a zero duration, and
    // anything under a minute would hammer the object store, so clamp low values.
    //
    // The interval itself is env-only (`TACK_BACKUP_INTERVAL_SECS`), but whether
    // the destination is *configured* can also come from UI-saved settings, so
    // spawn whenever an interval is set and re-check the effective config each
    // tick — a UI-only cloud config still schedules (28.1).
    if let Some(interval_secs) = config.backup_interval_secs {
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
        let bg_state = state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            interval.tick().await; // skip the first immediate tick
            loop {
                interval.tick().await;
                run_scheduled_backup(&bg_state).await;
            }
        });
        info!(interval_secs, "Remote backup scheduler enabled");
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

    // Start the orchestration reconciler, one task per registered control
    // plane, polling `/health` + `/status.json` (TODO.md §Wave 1, card A2 /
    // task 33.6) — if the *effective* setting says to. Off by default
    // (TODO.md §0 rule 8, rewritten 2026-08-05 — card E1): the effective
    // value is the `app_meta`-stored flag if the UI has ever set one, else
    // `TACK_ORCH_ENABLE`'s startup value — same precedence Cloud Backup
    // already uses for its own settings. Unlike Cloud Backup, this one also
    // has a runtime toggle: `PUT /api/settings/orchestration`
    // (`handlers/settings.rs`) calls `state.orch_runtime.start`/`.stop`
    // directly, so an operator can turn this on or off without a restart —
    // this boot-time call is just what makes the *initial* state agree with
    // whatever was last saved (or the env default, on a first-ever boot).
    if effective_orch_enabled(&state).await {
        // Card B4 (Wave 2, realtime broadcast): the store gets a clone of the
        // same broadcast sender every WebSocket subscriber shares, so it can
        // emit `BoardEvent::AgentRunUpdated`/`ApprovalPending` straight from
        // its `upsert_runs`/`upsert_approvals` — see orch_store.rs's module
        // doc for why the emit lives there and not in the reconciler.
        //
        // Card C5 (Wave 3, second half of task 35.6): `with_app_context`
        // hands the store the rest of what `AppState` carries, so
        // `upsert_runs` can run `dispatcher::apply_mapped_status` — the
        // workflow engine — when a run reaches a terminal state, exactly
        // like a human-driven PATCH. See orch_store.rs's own doc comments
        // for the "human wins" design and why it's optional everywhere else.
        let store = build_control_plane_store(&state);
        state
            .orch_runtime
            .start(
                store,
                reconciler::ReconcilerConfig {
                    poll_secs: config.orch_poll_secs,
                    // Card B2 (trace ingestion): must be the same cutoff
                    // `spawn_retention_sweep` uses (config.orch_event_retention_days
                    // both places) — persist_events' retention-composition guard
                    // depends on the two agreeing. See reconciler.rs's
                    // `ReconcilerConfig` doc comment.
                    event_retention_days: config.orch_event_retention_days,
                    ..Default::default()
                },
            )
            .await;
        info!(
            control_planes = state.orch_runtime.live_task_count().await,
            poll_secs = config.orch_poll_secs,
            "Orchestration reconciler enabled"
        );
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
        "\n  Tack v{} is running.\n  Open http://{}:{} in your browser.\n  Press Ctrl+C to stop.\n",
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

/// Loudly flag insecure-by-default configurations at startup. Does not refuse to
/// boot (a non-loopback bind without a token can be intentional behind a trusted
/// reverse proxy), but makes the exposure impossible to miss in the logs.
fn security_preflight(config: &AppConfig) {
    let host = config.host.as_str();
    let loopback = matches!(host, "127.0.0.1" | "::1" | "localhost")
        || host.starts_with("127.")
        || host.eq_ignore_ascii_case("::ffff:127.0.0.1");

    if !loopback && config.api_token.is_none() {
        let opted_in = std::env::var("TACK_INSECURE_NO_AUTH")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        warn!("═══════════════════════════════════════════════════════════════════════");
        warn!(
            host = %host,
            "SECURITY: Tack is bound to a NON-LOOPBACK address with NO API token."
        );
        warn!("Every project, item and attachment is reachable by anyone who can reach");
        warn!("this host. Set TACK_API_TOKEN to require a Bearer token on all requests,");
        warn!("or bind to 127.0.0.1 for local-only use.");
        if opted_in {
            warn!("Proceeding anyway because TACK_INSECURE_NO_AUTH is set.");
        } else {
            warn!(
                "Proceeding anyway. Set TACK_INSECURE_NO_AUTH=1 to acknowledge and silence-flag this."
            );
        }
        warn!("═══════════════════════════════════════════════════════════════════════");
    }

    // The Alexa endpoint is only skill-ID-authenticated (forgeable) unless a
    // shared secret is configured — warn when it is exposed without one.
    if config.alexa_skill_id.is_some() && config.alexa_shared_secret.is_none() {
        warn!(
            "SECURITY: /api/alexa is enabled without TACK_ALEXA_SHARED_SECRET. The skill \
             ID is not a secret, so requests are forgeable. Set TACK_ALEXA_SHARED_SECRET \
             and append ?token=<secret> to the skill's endpoint URL (see docs/ALEXA.md)."
        );
    }
}

fn init_tracing(config: &AppConfig) {
    use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(format!(
            "tack_api={level},tack_db={level},tack_core={level},tower_http=debug",
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
        let vocab = serde_json::to_string(&tack_core::vocabulary::default_vocabulary())?;
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
/// (`<storage_dir>.restore/`) may also exist. The swap is fail-safe: on any
/// failure it rolls back to the original files rather than booting an empty DB
/// (28.3). Stale `-wal`/`-shm` sidecars of the old DB are deleted first so
/// SQLite cannot replay them onto the freshly restored database.
fn apply_staged_restore(config: &AppConfig) {
    use std::path::Path;

    let Some(db_path) = config.db_file_path() else {
        return;
    };
    let restore_str = format!("{}.restore", db_path.to_string_lossy());
    let restore_path = Path::new(&restore_str);
    if !restore_path.exists() {
        return;
    }
    warn!(path = %restore_str, "Applying staged restore");

    let ts = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let storage_restore = format!("{}.restore", config.storage_dir);
    let storage_restore_path = Path::new(&storage_restore);

    match apply_restore_swap(
        &db_path,
        restore_path,
        Path::new(&config.storage_dir),
        storage_restore_path,
        &ts,
    ) {
        Ok(baks) => {
            info!(
                db_bak = %baks.db_bak,
                "Restore applied; previous DB preserved as a timestamped .bak"
            );
            // Keep only the newest .bak generation for each target.
            prune_old_baks(&db_path, &baks.db_bak);
            if let Some(storage_bak) = &baks.storage_bak {
                prune_old_baks(Path::new(&config.storage_dir), storage_bak);
            }
        }
        Err(e) => {
            warn!("Staged restore failed and was rolled back; the original database is intact: {e}")
        }
    }
}

/// Paths of the timestamped backups produced by a successful swap.
struct RestoreBaks {
    db_bak: String,
    storage_bak: Option<String>,
}

/// Fail-safe swap of the staged DB (and optional attachments dir) into place.
///
/// Sequence, each step rolled back on failure of a later step:
/// 1. delete stale `<db>-wal`/`<db>-shm`
/// 2. move current DB → `<db>.bak-<ts>`
/// 3. promote `<db>.restore` → live DB
/// 4. (if a storage staging dir exists) move current storage → `<dir>.bak-<ts>`
/// 5. promote `<dir>.restore` → live storage dir
///
/// On success returns the `.bak` paths. On any failure the original DB and
/// storage dir are put back so the server boots the pre-restore state.
fn apply_restore_swap(
    db_path: &std::path::Path,
    restore_db: &std::path::Path,
    storage_dir: &std::path::Path,
    storage_restore: &std::path::Path,
    ts: &str,
) -> std::io::Result<RestoreBaks> {
    // 1. Delete stale WAL/SHM of the DB about to be replaced.
    let wal = format!("{}-wal", db_path.to_string_lossy());
    let shm = format!("{}-shm", db_path.to_string_lossy());
    let _ = std::fs::remove_file(&wal);
    let _ = std::fs::remove_file(&shm);

    // 2. Move the current DB aside (may not exist on a fresh install).
    let db_bak = format!("{}.bak-{}", db_path.to_string_lossy(), ts);
    let db_existed = db_path.exists();
    if db_existed {
        std::fs::rename(db_path, &db_bak)?;
    }

    // 3. Promote the restored DB.
    if let Err(e) = std::fs::rename(restore_db, db_path) {
        // Roll back: put the original DB back.
        if db_existed {
            let _ = std::fs::rename(&db_bak, db_path);
        }
        return Err(e);
    }

    // 4/5. Storage swap (only when a staging dir is present).
    let mut storage_bak = None;
    if storage_restore.exists() {
        let bak = format!("{}.bak-{}", storage_dir.to_string_lossy(), ts);
        let storage_existed = storage_dir.exists();
        if storage_existed && let Err(e) = std::fs::rename(storage_dir, &bak) {
            // Roll back the DB swap.
            let _ = std::fs::rename(db_path, restore_db);
            if db_existed {
                let _ = std::fs::rename(&db_bak, db_path);
            }
            return Err(e);
        }
        if let Err(e) = std::fs::rename(storage_restore, storage_dir) {
            // Roll back storage, then the DB swap.
            if storage_existed {
                let _ = std::fs::rename(&bak, storage_dir);
            }
            let _ = std::fs::rename(db_path, restore_db);
            if db_existed {
                let _ = std::fs::rename(&db_bak, db_path);
            }
            return Err(e);
        }
        storage_bak = Some(bak);
    }

    Ok(RestoreBaks {
        db_bak,
        storage_bak,
    })
}

/// Delete older `<base>.bak-*` generations, keeping only `keep`. Best-effort.
fn prune_old_baks(base: &std::path::Path, keep: &str) {
    let Some(parent) = base.parent() else { return };
    let Some(name) = base.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    let prefix = format!("{name}.bak-");
    let keep_name = std::path::Path::new(keep)
        .file_name()
        .and_then(|n| n.to_str());
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let fname = entry.file_name();
        let Some(fname) = fname.to_str() else {
            continue;
        };
        if fname.starts_with(&prefix) && Some(fname) != keep_name {
            let path = entry.path();
            if path.is_dir() {
                let _ = std::fs::remove_dir_all(&path);
            } else {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install CTRL+C handler");
    warn!("Shutdown signal received");
}

/// Run a scheduled remote backup. Reads the *effective* config each tick so
/// UI-saved settings (bucket/prefix/creds/retention) are honored (28.1), and
/// runs the conflict-safe upload path (28.2) — on a cross-device conflict it
/// warns and skips rather than clobbering another device's newer work.
async fn run_scheduled_backup(state: &AppState) {
    let cfg = crate::handlers::settings::effective_backup_config(state).await;
    if !cfg.remote_backup_enabled() {
        // A UI-only config may not be set yet; nothing to do this tick.
        tracing::debug!("Scheduled backup skipped: remote backup not configured");
        return;
    }

    let store = match remote_backup::store_from_config(&cfg) {
        Ok(store) => store,
        Err(e) => {
            warn!(error = %e, "Scheduled backup store init failed");
            return;
        }
    };

    // Scheduled backups never force: a conflict means another device is ahead,
    // so we skip and let the user restore/resolve.
    match remote_backup::perform_backup(state.pool(), &cfg, store.as_ref(), false).await {
        Ok(manifest) => info!(key = %manifest.object_key, "Scheduled backup complete"),
        Err(remote_backup::BackupError::GenerationConflict {
            remote_generation, ..
        }) => {
            warn!(
                remote_generation,
                "Scheduled backup skipped: another device has newer work (restore to resolve)"
            );
        }
        Err(e) => warn!(error = %e, "Scheduled backup failed"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn workdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("tack-swap-{tag}-{}", Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// 28.3: the DB swap succeeds but the storage swap fails — the whole
    /// operation must roll back so the original DB is intact and bootable.
    #[test]
    fn restore_swap_rolls_back_when_storage_swap_fails() {
        let dir = workdir("storage-fail");
        let db_path = dir.join("tack.db");
        let restore_db = dir.join("tack.db.restore");
        fs::write(&db_path, b"ORIGINAL").unwrap();
        fs::write(&restore_db, b"RESTORED").unwrap();

        // A staging dir exists (so the storage step runs)…
        let storage_restore = dir.join("storage.restore");
        fs::create_dir_all(&storage_restore).unwrap();
        fs::write(storage_restore.join("a.bin"), b"attach").unwrap();

        // …but the live storage path sits under a MISSING parent, so promoting
        // the staging dir into place fails with ENOENT — after the DB swap.
        let storage_dir = dir.join("missing_parent").join("storage");

        let result =
            apply_restore_swap(&db_path, &restore_db, &storage_dir, &storage_restore, "TS");
        assert!(result.is_err(), "storage promotion should fail");

        // Rolled back: the ORIGINAL DB is back in place and bootable.
        assert_eq!(fs::read(&db_path).unwrap(), b"ORIGINAL");
        // The staged restore file is put back for a future attempt.
        assert_eq!(fs::read(&restore_db).unwrap(), b"RESTORED");
        // No stray .bak left behind for the DB.
        assert!(!Path::new(&format!("{}.bak-TS", db_path.to_string_lossy())).exists());

        fs::remove_dir_all(&dir).ok();
    }

    /// Happy path: DB + storage both swap, and a timestamped .bak is kept.
    #[test]
    fn restore_swap_promotes_db_and_storage() {
        let dir = workdir("ok");
        let db_path = dir.join("tack.db");
        let restore_db = dir.join("tack.db.restore");
        fs::write(&db_path, b"ORIGINAL").unwrap();
        fs::write(&restore_db, b"RESTORED").unwrap();

        let storage_dir = dir.join("storage");
        fs::create_dir_all(&storage_dir).unwrap();
        fs::write(storage_dir.join("old.bin"), b"old").unwrap();
        let storage_restore = dir.join("storage.restore");
        fs::create_dir_all(&storage_restore).unwrap();
        fs::write(storage_restore.join("new.bin"), b"new").unwrap();

        let baks = apply_restore_swap(&db_path, &restore_db, &storage_dir, &storage_restore, "TS")
            .expect("swap should succeed");

        assert_eq!(fs::read(&db_path).unwrap(), b"RESTORED");
        assert!(storage_dir.join("new.bin").exists());
        assert!(!storage_dir.join("old.bin").exists());
        assert!(Path::new(&baks.db_bak).exists());
        assert!(baks.storage_bak.is_some());
        // The staging inputs are consumed.
        assert!(!restore_db.exists());
        assert!(!storage_restore.exists());

        fs::remove_dir_all(&dir).ok();
    }

    /// Stale `-wal`/`-shm` sidecars of the replaced DB are deleted so SQLite
    /// cannot replay them onto the freshly restored database.
    #[test]
    fn restore_swap_deletes_stale_wal_shm() {
        let dir = workdir("wal");
        let db_path = dir.join("tack.db");
        let restore_db = dir.join("tack.db.restore");
        fs::write(&db_path, b"ORIGINAL").unwrap();
        fs::write(&restore_db, b"RESTORED").unwrap();
        fs::write(dir.join("tack.db-wal"), b"waljunk").unwrap();
        fs::write(dir.join("tack.db-shm"), b"shmjunk").unwrap();

        // No storage staging dir → storage step is skipped.
        let storage_dir = dir.join("storage");
        let storage_restore = dir.join("storage.restore");

        apply_restore_swap(&db_path, &restore_db, &storage_dir, &storage_restore, "TS").unwrap();

        assert!(
            !dir.join("tack.db-wal").exists(),
            "stale -wal must be deleted"
        );
        assert!(
            !dir.join("tack.db-shm").exists(),
            "stale -shm must be deleted"
        );
        assert_eq!(fs::read(&db_path).unwrap(), b"RESTORED");

        fs::remove_dir_all(&dir).ok();
    }
}
