//! Runtime start/stop for the orchestration reconciler: makes the enable flag a
//! runtime setting (`PUT /api/settings/orchestration`) instead of boot-time-only.
//! [`OrchRuntime::stop`] signals via a `watch`; a task finishes its current tick
//! before exiting. At most one generation runs, so `start()`/`stop()` never race.

use std::sync::Arc;

use tokio::sync::{Mutex, watch};

use tack_orch::reconciler::{self, ControlPlaneStore, ReconcilerConfig, SupervisedReconciler};

/// A live reconciler run plus the shutdown signal for it and its pollers.
struct Running {
    reconciler: SupervisedReconciler,
    stop_tx: watch::Sender<bool>,
}

/// Shared, toggleable handle to the orchestration reconciler (cheap `Clone`).
/// One instance on `AppState` shared by the boot path and the settings PUT.
#[derive(Clone)]
pub struct OrchRuntime {
    inner: Arc<Mutex<Option<Running>>>,
}

impl Default for OrchRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl OrchRuntime {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }

    /// Start a self-healing reconciler run: one poller per registered plane,
    /// kept in sync with `control_planes` for this generation's life (a plane
    /// registered after `start()` still gets polled). Idempotent: a no-op
    /// while a generation is already running.
    pub async fn start(&self, store: Arc<dyn ControlPlaneStore>, config: ReconcilerConfig) {
        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            return;
        }
        let (stop_tx, stop_rx) = watch::channel(false);
        let reconciler = reconciler::spawn_reconcilers_supervised(store, config, stop_rx).await;
        *guard = Some(Running {
            reconciler,
            stop_tx,
        });
    }

    /// Signal every running task to stop at its next safe point and drop this
    /// runtime's reference to them. No-op when nothing is running. Does not
    /// block for the tasks to exit, so a toggle-off request can't hang on an
    /// in-flight poll; stops the supervisor and every poller it was tracking.
    pub async fn stop(&self) {
        let mut guard = self.inner.lock().await;
        if let Some(running) = guard.take() {
            // Ignore a failed send: reconciler tasks don't exit on poll
            // failure today, but a receiver already gone just means stopped.
            let _ = running.stop_tx.send(true);
        }
    }

    /// Number of per-plane pollers currently alive. `0` both when disabled
    /// and when enabled with zero registered planes — reports whether a task
    /// is actually polling, not whether the feature is switched on.
    /// `GET /api/settings/orchestration`'s `reconciler_running` is `> 0` here.
    pub async fn live_task_count(&self) -> usize {
        let guard = self.inner.lock().await;
        match guard.as_ref() {
            Some(running) => running.reconciler.live_task_count().await,
            None => 0,
        }
    }
}

#[cfg(test)]
#[path = "orch_runtime/tests.rs"]
mod tests;
