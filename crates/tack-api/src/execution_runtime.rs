//! Runtime start/stop control for the execution-domain retention sweep and
//! health watch (card III-F5, Wave 5).
//!
//! Mirrors `orch_runtime.rs`'s own start/stop shape (a `tokio::sync::watch`
//! stop signal, one "generation" tracked at a time) with one deliberate
//! difference: [`ExecutionRuntime::stop`] *joins* both background tasks
//! before returning, where `OrchRuntime::stop` explicitly does not (its own
//! doc comment: "does not block waiting for the tasks to actually exit").
//! This card's acceptance bar is "shutdown joins task" — a real `.await` on
//! the `JoinHandle`, not merely a signal-and-return — so this type cannot
//! reuse that precedent's semantics even though the surrounding shape is
//! the same. `server.rs` calls this once, after `axum::serve(...)` returns
//! from graceful shutdown, so a slightly-blocking join here costs nothing:
//! by that point every HTTP request has already stopped.
//!
//! # Why this file is thin
//!
//! All retention/observability *logic* — the cancellable spawn loops, the
//! `ExecutionRetentionStore`/`ExecutionObservabilityStore` traits, and their
//! real `tack_db::Repository`-backed implementations — lives in
//! `tack_orch::execution_retention`/`execution_observability` (that crate
//! already depends on `tack-db` directly; see those modules' own doc
//! comments for why the concrete adapters live there rather than being
//! re-implemented in this crate). This module only wires configuration +
//! the repository into those spawn functions and gives `server.rs` one
//! `start()`/`stop()` pair to call.

use std::sync::Arc;

use tack_db::Repository;
use tack_orch::execution_observability::{
    self, ExecutionObservabilityConfig, RepoExecutionObservabilityStore,
};
use tack_orch::execution_retention::{self, ExecutionRetentionConfig, RepoExecutionRetentionStore};
use tokio::sync::{Mutex, watch};
use tokio::task::JoinHandle;

use crate::config::AppConfig;

/// What `server.rs` needs from `AppConfig` to start this runtime — a
/// deliberately narrow view (not `&AppConfig` itself) so this module never
/// grows a temptation to reach for an unrelated config field.
#[derive(Debug, Clone, Copy)]
pub struct ExecutionRuntimeConfig {
    pub retention_enable: bool,
    pub retention_days: u32,
    pub retention_interval_secs: u64,
    pub health_enable: bool,
    pub health_interval_secs: u64,
}

impl From<&AppConfig> for ExecutionRuntimeConfig {
    fn from(config: &AppConfig) -> Self {
        Self {
            retention_enable: config.execution_retention_enable,
            retention_days: config.execution_retention_days,
            retention_interval_secs: config.execution_retention_interval_secs,
            health_enable: config.execution_health_enable,
            health_interval_secs: config.execution_health_interval_secs,
        }
    }
}

struct Running {
    retention_handle: Option<JoinHandle<()>>,
    health_handle: Option<JoinHandle<()>>,
    stop_tx: watch::Sender<bool>,
}

/// Handle to the execution-domain retention sweep + health watch. One
/// instance is constructed and started in `server.rs::serve()`; `stop()` is
/// called after `axum::serve(...)` returns so both background tasks are
/// guaranteed joined before the process exits (CLAUDE.md/this card's
/// "shutdown joins task" acceptance bar).
///
/// Not stored on `AppState` (unlike [`crate::orch_runtime::OrchRuntime`]):
/// nothing in the current HTTP surface needs to toggle this at runtime, and
/// `AppState` lives in `router.rs`, which this card must not edit. A local
/// variable in `server.rs::serve()` is sufficient — see this card's handoff
/// for the exact wiring and why a future runtime-toggle route would need
/// `router.rs`'s owner to add the field.
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

    /// Start the retention sweep and/or health watch, each independently
    /// gated by its own `*_enable` flag — either, both, or neither may
    /// spawn a task. Idempotent: a `start()` while a generation is already
    /// running is a no-op (mirrors `OrchRuntime::start`).
    pub async fn start(&self, repo: Repository, config: ExecutionRuntimeConfig) {
        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            return;
        }

        let (stop_tx, stop_rx) = watch::channel(false);

        let retention_store = Arc::new(RepoExecutionRetentionStore(repo.clone()));
        let retention_clock = Arc::new(execution_retention::SystemClock);
        let retention_handle = execution_retention::spawn_execution_retention_sweep(
            config.retention_enable,
            retention_store,
            retention_clock,
            ExecutionRetentionConfig {
                retention_days: config.retention_days,
                batch_size: execution_retention::DEFAULT_EXECUTION_RETENTION_BATCH_SIZE,
                sweep_interval_secs: config.retention_interval_secs,
            },
            stop_rx.clone(),
        );

        let health_store = Arc::new(RepoExecutionObservabilityStore(repo));
        let health_clock = Arc::new(execution_observability::SystemClock);
        let health_handle = execution_observability::spawn_execution_health_watch(
            config.health_enable,
            health_store,
            health_clock,
            ExecutionObservabilityConfig {
                check_interval_secs: config.health_interval_secs,
                event_window_secs: execution_observability::DEFAULT_EVENT_WINDOW_SECS,
            },
            stop_rx,
        );

        *guard = Some(Running {
            retention_handle,
            health_handle,
            stop_tx,
        });
    }

    /// Signal both tasks to stop, then **join** whichever of them were
    /// actually spawned (a disabled task never had a handle to join in the
    /// first place — that's not a failure to join, it's nothing to join). A
    /// no-op when nothing is running, mirroring `start()`'s idempotency.
    ///
    /// This is the load-bearing difference from `OrchRuntime::stop`: it
    /// really does block until each live task's loop has observed the stop
    /// signal and returned, at whatever safe point that task defines (never
    /// mid-transaction — see `execution_retention`/`execution_observability`'s
    /// own doc comments).
    pub async fn stop(&self) {
        let mut guard = self.inner.lock().await;
        if let Some(running) = guard.take() {
            let _ = running.stop_tx.send(true);
            if let Some(handle) = running.retention_handle {
                let _ = handle.await;
            }
            if let Some(handle) = running.health_handle {
                let _ = handle.await;
            }
        }
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

    #[tokio::test]
    async fn start_then_stop_leaves_nothing_running_and_stop_actually_joins() {
        let runtime = ExecutionRuntime::new();
        let repo = test_repo().await;
        runtime
            .start(
                repo,
                ExecutionRuntimeConfig {
                    retention_enable: true,
                    retention_days: 90,
                    retention_interval_secs: 3600,
                    health_enable: true,
                    health_interval_secs: 3600,
                },
            )
            .await;

        // `stop()` returning at all (rather than hanging) is itself part of
        // the proof: both spawned tasks' `tokio::select!` loops must have
        // observed the stop signal and returned for their `JoinHandle`s to
        // resolve.
        runtime.stop().await;

        // A second stop() is a harmless no-op (nothing left to signal or
        // join) — mirrors OrchRuntime's own idempotency guarantee.
        runtime.stop().await;
    }

    #[tokio::test]
    async fn disabled_config_starts_no_tasks_and_stop_is_still_a_harmless_no_op() {
        let runtime = ExecutionRuntime::new();
        let repo = test_repo().await;
        runtime
            .start(
                repo,
                ExecutionRuntimeConfig {
                    retention_enable: false,
                    retention_days: 90,
                    retention_interval_secs: 3600,
                    health_enable: false,
                    health_interval_secs: 3600,
                },
            )
            .await;
        runtime.stop().await;
    }

    #[tokio::test]
    async fn start_is_idempotent_while_already_running() {
        let runtime = ExecutionRuntime::new();
        let repo = test_repo().await;
        let config = ExecutionRuntimeConfig {
            retention_enable: true,
            retention_days: 90,
            retention_interval_secs: 3600,
            health_enable: true,
            health_interval_secs: 3600,
        };
        runtime.start(repo.clone(), config).await;
        // A second start() while the first generation is live must not
        // spawn a second set of tasks (would leak, and a subsequent single
        // stop() would only join one generation's handles) — this only
        // proves it doesn't hang/panic; the no-duplicate-generation
        // invariant itself mirrors `OrchRuntime::start`'s own tested
        // behavior (`repeated_global_start_stop_cycles_leave_no_task_running`
        // in `tack-orch`), same `guard.is_some()` early-return shape.
        runtime.start(repo, config).await;
        runtime.stop().await;
    }
}
