//! The one place a background loop is spawned: a named tick on an interval, stopped by a shared
//! signal and watched for a panic.
//!
//! `tack serve` owns one [`Loops`], every loop's `spawn` adds its tick to it, and shutdown calls
//! [`Loops::stop`], so no loop outlives the process unnoticed and none dies without a log line.

use std::any::Any;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::time::Duration;

use futures::FutureExt;
use tokio::task::JoinSet;
use tokio::time::{MissedTickBehavior, interval, timeout};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

/// How long shutdown waits for the loops and the spawned use cases before it drops them.
pub const STOP_GRACE: Duration = Duration::from_secs(10);

/// When a loop makes its first pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// At once: what the pass looks at is worth seeing the moment the server is up.
    Immediately,
    /// One interval after the start, for a pass that talks to the network or writes a backup.
    AfterInterval,
}

/// The loops of one server: the tasks, and the signal that stops them all.
#[derive(Debug, Default)]
pub struct Loops {
    stop: CancellationToken,
    tasks: JoinSet<()>,
}

impl Loops {
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs `tick` every `every` as the loop `name`, until [`Loops::stop`]. A pass that is running
    /// when the signal comes finishes first; a pass is one use case with its own transactions, and
    /// cutting it off would only leave the next start to redo it. A pass that is late is not
    /// repeated to catch up. A panic ends the loop and is logged with its name. `every` must not
    /// be zero.
    pub fn spawn_tick<F, Fut>(
        &mut self,
        name: &'static str,
        start: Start,
        every: Duration,
        mut tick: F,
    ) where
        F: FnMut() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let stop = self.stop.clone();
        self.tasks.spawn(async move {
            let passes = async {
                let mut timer = interval(every);
                timer.set_missed_tick_behavior(MissedTickBehavior::Delay);
                if start == Start::AfterInterval {
                    timer.tick().await;
                }
                loop {
                    tokio::select! {
                        () = stop.cancelled() => break,
                        _ = timer.tick() => {}
                    }
                    tick().await;
                }
            };
            match AssertUnwindSafe(passes).catch_unwind().await {
                Ok(()) => debug!(name, "background loop stopped"),
                Err(panic) => {
                    error!(name, panic = %message(&*panic), "background loop panicked and is gone");
                }
            }
        });
    }

    /// Signals every loop to stop and waits up to `grace` for them to return; the ones still
    /// running after that are dropped where they stand.
    pub async fn stop(mut self, grace: Duration) {
        self.stop.cancel();
        let joined = timeout(grace, async {
            while self.tasks.join_next().await.is_some() {}
        })
        .await;
        if joined.is_err() {
            warn!(
                still_running = self.tasks.len(),
                "background loops did not stop in time; dropping them"
            );
        }
    }
}

fn message(panic: &(dyn Any + Send)) -> &str {
    panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("not a text")
}
