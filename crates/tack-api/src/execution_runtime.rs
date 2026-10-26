//! Runtime start/stop control for the execution-domain artifact/event and
//! decision-expiry sweep.
//!
//! A `tokio::sync::watch` stop signal with one "generation" tracked at a
//! time, deliberately blocking on stop: [`ExecutionRuntime::stop`] *joins*
//! the background task — `server.rs` calls it once, after
//! `axum::serve(...)` returns from graceful shutdown, so a blocking join
//! here costs nothing (every HTTP request has already stopped). The
//! replay/idempotency purge and the fleet health watch are not here: they
//! live in `tack_app::retention` and `tack_app::observability`, and
//! `tack-cli` spawns them.

use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::Duration;
use tack_db::Repository;
use tokio::sync::{Mutex, watch};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::config::AppConfig;
use crate::handlers::runner_protocol::artifact_storage::ArtifactStorage;
use crate::handlers::runner_protocol::retention::{
    self as artifact_event_retention, RetentionPolicy,
};

/// What `server.rs` needs from `AppConfig` to start this runtime — a
/// deliberately narrow view (not `&AppConfig` itself) so this module never
/// grows a temptation to reach for an unrelated config field.
///
/// `storage_dir` is `String`, not `Copy` — this type dropped its
/// `Copy` derive accordingly; every existing call site already passes it by
/// value once per `start()` call, so this is a non-breaking widening, not a
/// behavior change.
#[derive(Debug, Clone)]
pub struct ExecutionRuntimeConfig {
    pub retention_enable: bool,
    pub retention_days: u32,
    pub retention_interval_secs: u64,
    /// `AppConfig::storage_dir` (`TACK_STORAGE_DIR`) — the same value
    /// `router.rs::operator_execution_routes` derives its own artifact-
    /// download storage root from
    /// (`format!("{}/execution-artifacts", state.config.storage_dir)`).
    /// [`spawn_artifact_and_decision_sweep`] below builds its
    /// [`ArtifactStorage`] from this field using the **identical** `format!`
    /// expression so the two agree on exactly which directory holds the
    /// blobs — see that function's own doc comment.
    pub storage_dir: String,
}

impl From<&AppConfig> for ExecutionRuntimeConfig {
    fn from(config: &AppConfig) -> Self {
        Self {
            retention_enable: config.execution_retention_enable,
            retention_days: config.execution_retention_days,
            retention_interval_secs: config.execution_retention_interval_secs,
            storage_dir: config.storage_dir.clone(),
        }
    }
}

struct Running {
    /// The artifact/event sweep + decision-expiry loop — see
    /// [`spawn_artifact_and_decision_sweep`].
    artifact_decision_handle: Option<JoinHandle<()>>,
    stop_tx: watch::Sender<bool>,
}

/// Handle to the execution-domain artifact/event and decision-expiry sweep.
/// One instance is constructed and started in `server.rs::serve()`; `stop()`
/// is called after `axum::serve(...)` returns so the background task is
/// guaranteed joined before the process exits.
///
/// Not stored on `AppState`: nothing in the current HTTP surface needs to
/// toggle this at runtime. A local variable in `server.rs::serve()` is
/// sufficient; a future runtime-toggle route would need to add the field to
/// `AppState` in `router.rs`.
pub struct ExecutionRuntime {
    inner: Mutex<Option<Running>>,
}

impl Default for ExecutionRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionRuntime {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
        }
    }

    /// Start the artifact/event sweep + decision-expiry sweep, gated by
    /// `retention_enable` (the same flag as `tack_app::retention`'s purge —
    /// see [`spawn_artifact_and_decision_sweep`]'s doc comment for why they
    /// must share it). Idempotent: a `start()` while a generation is already
    /// running is a no-op.
    pub async fn start(&self, repo: Repository, config: ExecutionRuntimeConfig) {
        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            return;
        }

        let (stop_tx, stop_rx) = watch::channel(false);

        // Same storage-root expression `router.rs`'s
        // `operator_execution_routes` uses for its own artifact-download
        // `ArtifactStorage` — see `ExecutionRuntimeConfig::storage_dir`'s doc
        // comment for why these two independently-constructed instances must
        // agree.
        let artifact_storage = Arc::new(ArtifactStorage::new(format!(
            "{}/execution-artifacts",
            config.storage_dir
        )));
        let artifact_decision_handle = spawn_artifact_and_decision_sweep(
            config.retention_enable,
            repo.clone(),
            artifact_storage,
            config.retention_days,
            config.retention_interval_secs,
            tack_app::retention::DEFAULT_BATCH_SIZE,
            stop_rx,
        );

        *guard = Some(Running {
            artifact_decision_handle,
            stop_tx,
        });
    }

    /// Signal the task to stop, then **join** it if it was actually spawned
    /// (a disabled task never had a handle to join in the first place —
    /// that's not a failure to join, it's nothing to join). A no-op when
    /// nothing is running, mirroring `start()`'s idempotency.
    ///
    /// This really does block until the live task's loop has observed the
    /// stop signal and returned, at whatever safe point it defines (never
    /// mid-transaction — see [`spawn_artifact_and_decision_sweep`]'s doc).
    pub async fn stop(&self) {
        let mut guard = self.inner.lock().await;
        if let Some(running) = guard.take() {
            let _ = running.stop_tx.send(true);
            if let Some(handle) = running.artifact_decision_handle {
                let _ = handle.await;
            }
        }
    }
}

/// Resolves once `stop_rx` carries `true`, or when the sender is dropped.
async fn wait_until_stopped(rx: &mut watch::Receiver<bool>) {
    loop {
        if *rx.borrow() {
            return;
        }
        if rx.changed().await.is_err() {
            return;
        }
    }
}

/// Spawns the recurring caller `sweep_events`/`sweep_artifacts`
/// (`handlers/runner_protocol/retention.rs`) and
/// `tack_app::runs::expire_decisions` otherwise lack.
/// Returns `None` (spawns nothing) when `enabled` is `false`.
///
/// All three share `config.retention_enable`
/// (`TACK_EXECUTION_RETENTION_ENABLE`) even though only artifact/event
/// purging deletes data — decision expiry rides the same gate rather than
/// getting a looser one of its own, so it can never run more permissively
/// than the deletion it's bundled with. A future need to run decision
/// expiry under different conditions must be a new, explicit flag, never a
/// loosening of this one.
#[allow(clippy::too_many_arguments)]
fn spawn_artifact_and_decision_sweep(
    enabled: bool,
    repo: Repository,
    artifact_storage: Arc<ArtifactStorage>,
    retention_days: u32,
    interval_secs: u64,
    batch_limit: i64,
    mut stop_rx: watch::Receiver<bool>,
) -> Option<JoinHandle<()>> {
    if !enabled {
        return None;
    }

    Some(tokio::spawn(async move {
        let interval_secs = interval_secs.max(1);
        let policy = RetentionPolicy {
            event_retention: Duration::days(retention_days as i64),
            artifact_retention: Duration::days(retention_days as i64),
        };
        loop {
            if *stop_rx.borrow() {
                info!("execution artifact/decision sweep stopping");
                return;
            }

            let now = chrono::Utc::now();

            match artifact_event_retention::sweep_events(&repo, now, &policy, batch_limit).await {
                Ok(n) if n > 0 => {
                    info!(events_deleted = n, "execution retention: events swept")
                }
                Ok(_) => debug!("execution retention: no stale events to sweep"),
                Err(e) => warn!(
                    error = %e,
                    "execution retention: event sweep failed; will retry next cycle"
                ),
            }

            match artifact_event_retention::sweep_artifacts(
                &repo,
                &artifact_storage,
                now,
                &policy,
                batch_limit,
            )
            .await
            {
                Ok(outcome)
                    if outcome.artifacts_deleted > 0 || outcome.artifacts_without_a_blob > 0 =>
                {
                    info!(
                        artifacts_deleted = outcome.artifacts_deleted,
                        artifacts_without_a_blob = outcome.artifacts_without_a_blob,
                        "execution retention: artifacts swept"
                    )
                }
                Ok(_) => debug!("execution retention: no stale artifacts to sweep"),
                Err(e) => warn!(
                    error = %e,
                    "execution retention: artifact sweep failed; will retry next cycle"
                ),
            }

            match tack_app::runs::expire_decisions::expire_decisions(
                &repo,
                &tack_protocol::SystemClock,
            )
            .await
            {
                Ok(n) if n > 0 => info!(
                    decisions_expired = n,
                    "execution retention: overdue decisions expired"
                ),
                Ok(_) => debug!("execution retention: no overdue decisions to expire"),
                Err(e) => warn!(
                    error = %e,
                    "execution retention: decision expiry failed; will retry next cycle"
                ),
            }

            tokio::select! {
                _ = tokio::time::sleep(StdDuration::from_secs(interval_secs)) => {}
                _ = wait_until_stopped(&mut stop_rx) => {
                    info!("execution artifact/decision sweep stopping");
                    return;
                }
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tack_db::{init_pool, migrations};

    async fn test_repo() -> Repository {
        let pool = init_pool("sqlite::memory:").await.unwrap();
        migrations::run_all(&pool).await.unwrap();
        Repository::new(pool)
    }

    /// An arbitrary, never-created path is safe here: every test below
    /// starts from an empty in-memory DB, so `sweep_artifacts`'s
    /// `list_execution_artifacts_older_than` read returns nothing and the
    /// function returns before ever touching `ArtifactStorage`/the
    /// filesystem — see `spawn_artifact_and_decision_sweep`'s own doc
    /// comment. Real on-disk behavior is proved separately in
    /// `crates/tack-api/tests/wiring/execution_sweep.rs`.
    fn unused_storage_dir() -> String {
        "/tmp/tack-api-execution-runtime-test-unused-storage-dir".to_string()
    }

    #[tokio::test]
    async fn start_then_stop_leaves_nothing_running_and_stop_joins() {
        let runtime = ExecutionRuntime::new();
        let repo = test_repo().await;
        runtime
            .start(
                repo,
                ExecutionRuntimeConfig {
                    retention_enable: true,
                    retention_days: 90,
                    retention_interval_secs: 3600,
                    storage_dir: unused_storage_dir(),
                },
            )
            .await;

        // `stop()` returning at all (rather than hanging) is itself part of
        // the proof: every spawned task's `tokio::select!` loop must have
        // observed the stop signal and returned for their `JoinHandle`s to
        // resolve.
        runtime.stop().await;

        // A second stop() is a harmless no-op (nothing left to signal or join).
        runtime.stop().await;
    }

    #[tokio::test]
    async fn disabled_config_starts_nothing_and_stop_is_a_harmless_no_op() {
        let runtime = ExecutionRuntime::new();
        let repo = test_repo().await;
        runtime
            .start(
                repo,
                ExecutionRuntimeConfig {
                    retention_enable: false,
                    retention_days: 90,
                    retention_interval_secs: 3600,
                    storage_dir: unused_storage_dir(),
                },
            )
            .await;
        runtime.stop().await;
    }

    #[tokio::test]
    async fn second_start_call_on_execution_runtime_is_a_noop() {
        let runtime = ExecutionRuntime::new();
        let repo = test_repo().await;
        let config = ExecutionRuntimeConfig {
            retention_enable: true,
            retention_days: 90,
            retention_interval_secs: 3600,
            storage_dir: unused_storage_dir(),
        };
        runtime.start(repo.clone(), config.clone()).await;
        // A second start() while the first generation is live must not
        // spawn a second set of tasks (would leak, and a subsequent single
        // stop() would only join one generation's handles) — this only
        // proves it doesn't hang/panic; the no-duplicate-generation
        // invariant is enforced by the same `guard.is_some()` early-return
        // shape as `start()` above.
        runtime.start(repo, config).await;
        runtime.stop().await;
    }
}
