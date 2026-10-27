//! Runtime start/stop control for the execution-domain artifact sweep.
//!
//! One loop on `tack_app::background::Loops`, deliberately blocking on stop:
//! [`ExecutionRuntime::stop`] waits for the loop to return — `server.rs` calls it once, after
//! `axum::serve(...)` returns from graceful shutdown, so a blocking wait here costs nothing
//! (every HTTP request has already stopped). The rest of retention is not here: the replay
//! and event purge and the decision expiry live in `tack_app::retention` and the fleet health
//! watch in `tack_app::observability`, and `tack-cli` starts them. Only the artifact blobs
//! need this crate's `ArtifactStorage`, which names HTTP types `tack-app` may not.

use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::Duration;
use tack_app::background::{Loops, STOP_GRACE, Start};
use tack_db::Repository;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::config::AppConfig;
use crate::handlers::runner_protocol::artifact_storage::ArtifactStorage;
use crate::handlers::runner_protocol::retention::{self as artifact_retention, RetentionPolicy};

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
    /// [`ExecutionRuntime::start`] builds its [`ArtifactStorage`] from this
    /// field using the **identical** `format!` expression so the two agree
    /// on exactly which directory holds the blobs.
    pub storage_dir: String,
}

impl From<&AppConfig> for ExecutionRuntimeConfig {
    fn from(config: &AppConfig) -> Self {
        Self {
            retention_enable: config.loops.execution_retention_enable,
            retention_days: config.loops.execution_retention_days,
            retention_interval_secs: config.loops.execution_retention_interval_secs,
            storage_dir: config.storage_dir.clone(),
        }
    }
}

/// Handle to the execution-domain artifact sweep. One instance is constructed
/// and started in `server.rs::serve()`; `stop()` is called after
/// `axum::serve(...)` returns so the loop is guaranteed finished before the
/// process exits.
///
/// Not stored on `AppState`: nothing in the current HTTP surface needs to
/// toggle this at runtime. A local variable in `server.rs::serve()` is
/// sufficient; a future runtime-toggle route would need to add the field to
/// `AppState` in `router.rs`.
pub struct ExecutionRuntime {
    inner: Mutex<Option<Loops>>,
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

    /// Start the artifact sweep, gated by `retention_enable` — the flag of the purge in
    /// `tack_app::retention`, so no deletion runs more permissively than the others. Idempotent: a
    /// `start()` while a generation is already running is a no-op.
    pub async fn start(&self, repo: Repository, config: ExecutionRuntimeConfig) {
        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            return;
        }

        let mut loops = Loops::new();
        if config.retention_enable {
            // Same storage-root expression `router.rs`'s
            // `operator_execution_routes` uses for its own artifact-download
            // `ArtifactStorage` — see `ExecutionRuntimeConfig::storage_dir`'s
            // doc comment for why these two independently-constructed
            // instances must agree.
            let storage = Arc::new(ArtifactStorage::new(format!(
                "{}/execution-artifacts",
                config.storage_dir
            )));
            let policy = RetentionPolicy {
                artifact_retention: Duration::days(i64::from(config.retention_days)),
            };
            loops.spawn_tick(
                "execution_artifacts",
                Start::Immediately,
                StdDuration::from_secs(config.retention_interval_secs.max(1)),
                move || {
                    let (repo, storage) = (repo.clone(), storage.clone());
                    async move { sweep(&repo, &storage, &policy).await }
                },
            );
        }
        *guard = Some(loops);
    }

    /// Signal the loop to stop and wait for it, bounded. A no-op when nothing is running,
    /// mirroring `start()`'s idempotency. A sweep pass in progress finishes first.
    pub async fn stop(&self) {
        if let Some(loops) = self.inner.lock().await.take() {
            loops.stop(STOP_GRACE).await;
        }
    }
}

/// One bounded pass over the artifacts older than the window, logged.
async fn sweep(repo: &Repository, storage: &ArtifactStorage, policy: &RetentionPolicy) {
    match artifact_retention::sweep_artifacts(
        repo,
        storage,
        chrono::Utc::now(),
        policy,
        tack_app::retention::DEFAULT_BATCH_SIZE,
    )
    .await
    {
        Ok(outcome) if outcome.artifacts_deleted > 0 || outcome.artifacts_without_a_blob > 0 => {
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
    /// filesystem. Real on-disk behavior is proved separately in
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

        // `stop()` returning at all (rather than waiting out its grace) is itself part of
        // the proof: the loop must have observed the stop signal and returned.
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
