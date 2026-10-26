//! Keeps at most one embedded runner process alive: start is idempotent, stop does not wait,
//! and a process that exited on its own is noticed on the next status read. What a process *is*
//! (its configuration, its credential, how it is spawned) belongs to the [`RunnerHost`] the
//! binary supplies, so none of that is named here.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::net::SocketAddr;
use tokio::sync::Mutex;

use super::{LocalRunnerControl, LocalRunnerControlError, RuntimeState, RuntimeStatus};

/// One runner process that is running.
pub trait RunnerProcess: Send + Sync {
    /// Whether it has exited, on its own or after a stop.
    fn is_finished(&self) -> bool;
    /// The leases it holds now.
    fn active_attempts(&self) -> usize;
    /// Asks it to shut down. Does not wait for it to.
    fn request_stop(&self);
}

/// What only the binary that links the runner can answer.
#[async_trait]
pub trait RunnerHost: Send + Sync {
    /// How many attempts the runner works at once, as configured.
    async fn max_concurrent_attempts(&self) -> u32;
    /// The settings file the runner read; `None` is defaults only.
    fn config_file(&self) -> Option<String>;
    /// Resolves the runner's credential and spawns it against `api_base_url`. The error is the
    /// reason it could not start.
    async fn launch(&self, api_base_url: &str) -> Result<Box<dyn RunnerProcess>, String>;
    /// The id the API assigned at enrolment (`runr_...`), not the runner's configured name.
    async fn enrolled_runner_id(&self) -> Option<String>;
    async fn verify_program(&self) -> Option<String>;
    async fn resolve_secret(&self, reference: &str) -> Result<String, String>;
}

struct Running {
    process: Box<dyn RunnerProcess>,
    since: DateTime<Utc>,
}

struct State {
    /// `None` until the server has bound its listener and said so: before that `start` has
    /// nowhere to point the runner's HTTP client and fails rather than guess.
    bound_addr: Option<SocketAddr>,
    running: Option<Running>,
}

/// The [`LocalRunnerControl`] of one `tack serve` process. It is built before the listener
/// opens, which is why the bound address arrives later through [`Self::set_bound_addr`].
pub struct EmbeddedRunner<H: RunnerHost> {
    host: H,
    state: Mutex<State>,
}

impl<H: RunnerHost> EmbeddedRunner<H> {
    pub fn new(host: H) -> Self {
        Self {
            host,
            state: Mutex::new(State {
                bound_addr: None,
                running: None,
            }),
        }
    }

    pub fn host(&self) -> &H {
        &self.host
    }

    /// Told the server's own loopback address once it is known.
    pub async fn set_bound_addr(&self, addr: SocketAddr) {
        self.state.lock().await.bound_addr = Some(addr);
    }
}

#[async_trait]
impl<H: RunnerHost> LocalRunnerControl for EmbeddedRunner<H> {
    async fn status(&self) -> RuntimeStatus {
        let mut state = self.state.lock().await;
        // A runner that exited on its own (crashed, or done reacting to a stop) would otherwise
        // be reported running forever.
        if state
            .running
            .as_ref()
            .is_some_and(|running| running.process.is_finished())
        {
            state.running = None;
        }
        let max_concurrent_attempts = self.host.max_concurrent_attempts().await;
        match &state.running {
            Some(running) => RuntimeStatus {
                state: RuntimeState::Running,
                since: Some(running.since),
                max_concurrent_attempts,
                active_attempts: u32::try_from(running.process.active_attempts())
                    .unwrap_or(u32::MAX),
            },
            None => RuntimeStatus {
                state: RuntimeState::Stopped,
                since: None,
                max_concurrent_attempts,
                active_attempts: 0,
            },
        }
    }

    async fn start(&self) -> Result<(), LocalRunnerControlError> {
        let mut state = self.state.lock().await;
        if state.running.is_some() {
            // A second `enabled: true`, or the boot-time check racing a UI toggle, must never
            // spawn a duplicate runner.
            return Ok(());
        }
        let bound_addr = state.bound_addr.ok_or_else(|| {
            LocalRunnerControlError::StartFailed(
                "the server's own address is not yet known".to_owned(),
            )
        })?;
        let process = self
            .host
            .launch(&format!("http://{bound_addr}/api/runner/v1"))
            .await
            .map_err(LocalRunnerControlError::StartFailed)?;
        state.running = Some(Running {
            process,
            since: Utc::now(),
        });
        Ok(())
    }

    async fn stop(&self) {
        let mut state = self.state.lock().await;
        if let Some(running) = state.running.take() {
            // A toggle-off request must not hang on however long the runner's shutdown takes.
            running.process.request_stop();
        }
    }

    async fn runner_id(&self) -> Option<String> {
        self.state.lock().await.running.as_ref()?;
        self.host.enrolled_runner_id().await
    }

    async fn verify_program(&self) -> Option<String> {
        self.host.verify_program().await
    }

    fn config_file(&self) -> Option<String> {
        self.host.config_file()
    }

    async fn resolve_secret(&self, reference: &str) -> Result<String, LocalRunnerControlError> {
        self.host
            .resolve_secret(reference)
            .await
            .map_err(LocalRunnerControlError::SecretStore)
    }
}
