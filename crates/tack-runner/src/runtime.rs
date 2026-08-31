use std::sync::Arc;

use tokio::sync::watch;

use crate::{
    Clock, ProcessSupervisor, RunnerConfig, RunnerError, RunnerFilesystem, RunnerProtocolClient,
};

/// A clonable shutdown observation channel for the runner and every child task.
#[derive(Clone)]
pub struct Shutdown {
    receiver: watch::Receiver<bool>,
}

/// The sole authority that requests shutdown for a [`Shutdown`] channel.
pub struct ShutdownHandle {
    sender: watch::Sender<bool>,
}

impl Shutdown {
    pub fn channel() -> (Self, ShutdownHandle) {
        let (sender, receiver) = watch::channel(false);
        (Self { receiver }, ShutdownHandle { sender })
    }

    pub async fn requested(&mut self) {
        while !*self.receiver.borrow() {
            if self.receiver.changed().await.is_err() {
                return;
            }
        }
    }

    pub fn is_requested(&self) -> bool {
        *self.receiver.borrow()
    }
}

impl ShutdownHandle {
    pub fn request(&self) {
        // A dropped receiver means all runner work has already completed.
        let _ = self.sender.send(true);
    }
}

/// Runtime composition root with all side-effect boundaries injected.
pub struct RunnerRuntime<C, P, F, Cl> {
    client: Arc<C>,
    processes: Arc<P>,
    filesystem: Arc<F>,
    clock: Arc<Cl>,
    config: RunnerConfig,
}

impl<C, P, F, Cl> RunnerRuntime<C, P, F, Cl>
where
    C: RunnerProtocolClient + 'static,
    P: ProcessSupervisor + 'static,
    F: RunnerFilesystem + 'static,
    Cl: Clock + 'static,
{
    pub fn new(client: C, processes: P, filesystem: F, clock: Cl, config: RunnerConfig) -> Self {
        Self {
            client: Arc::new(client),
            processes: Arc::new(processes),
            filesystem: Arc::new(filesystem),
            clock: Arc::new(clock),
            config,
        }
    }

    pub async fn run(self, shutdown: Shutdown) -> Result<(), RunnerError> {
        self.config.require_enrollment_credential()?;
        self.filesystem.prepare_state_dir(&self.config.state_dir)?;
        let started_at = self.clock.now();
        tracing::info!(runner_id = %self.config.runner_id, ?started_at, "runner runtime started");

        let client = Arc::clone(&self.client);
        let client_shutdown = shutdown.clone();
        let mut client_task = tokio::spawn(async move { client.serve(client_shutdown).await });
        let mut shutdown_wait = shutdown;

        let client_result = tokio::select! {
            result = &mut client_task => Some(result),
            () = shutdown_wait.requested() => None,
        };

        // Cleanup starts before joining the protocol task so a real supervisor
        // can ask child processes to stop while the client observes shutdown.
        self.processes.terminate_all().await?;

        let client_result = match client_result {
            Some(result) => result.map_err(|_| RunnerError::ClientTaskJoin)?,
            None => client_task.await.map_err(|_| RunnerError::ClientTaskJoin)?,
        };

        match client_result {
            Ok(()) if shutdown_wait.is_requested() => Ok(()),
            Ok(()) => Err(RunnerError::ClientStopped),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::Path,
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
        },
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use async_trait::async_trait;
    use tokio::sync::Notify;

    use super::*;

    #[derive(Default)]
    struct FakeClient {
        started: Arc<Notify>,
        completed: Arc<AtomicBool>,
    }

    #[async_trait]
    impl RunnerProtocolClient for FakeClient {
        async fn serve(&self, mut shutdown: Shutdown) -> Result<(), RunnerError> {
            self.started.notify_one();
            shutdown.requested().await;
            self.completed.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeProcesses(Arc<AtomicUsize>);

    #[async_trait]
    impl ProcessSupervisor for FakeProcesses {
        async fn terminate_all(&self) -> Result<(), RunnerError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeFilesystem(Arc<AtomicUsize>);

    impl RunnerFilesystem for FakeFilesystem {
        fn prepare_state_dir(&self, _path: &Path) -> Result<(), RunnerError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct FakeClock;

    impl Clock for FakeClock {
        fn now(&self) -> SystemTime {
            UNIX_EPOCH + Duration::from_secs(52)
        }
    }

    fn config() -> RunnerConfig {
        RunnerConfig::from_sources(crate::RunnerConfigSources {
            command_line: crate::ConfigOverrides {
                enrollment_credential: Some(crate::EnrollmentCredential::new("test-only")),
                ..crate::ConfigOverrides::default()
            },
            ..crate::RunnerConfigSources::default()
        })
        .expect("test config")
    }

    #[tokio::test]
    async fn shutdown_terminates_processes_and_joins_client_task() {
        let client = FakeClient::default();
        let started = Arc::clone(&client.started);
        let completed = Arc::clone(&client.completed);
        let processes = FakeProcesses::default();
        let process_calls = Arc::clone(&processes.0);
        let filesystem = FakeFilesystem::default();
        let filesystem_calls = Arc::clone(&filesystem.0);
        let runtime = RunnerRuntime::new(client, processes, filesystem, FakeClock, config());
        let (shutdown, shutdown_handle) = Shutdown::channel();

        let run = tokio::spawn(runtime.run(shutdown));
        started.notified().await;
        shutdown_handle.request();

        assert!(run.await.expect("runtime task joins").is_ok());
        assert!(completed.load(Ordering::SeqCst), "client task was joined");
        assert_eq!(process_calls.load(Ordering::SeqCst), 1);
        assert_eq!(filesystem_calls.load(Ordering::SeqCst), 1);
    }

    /// This test no longer pins a release blocker: `UnavailableProtocolClient`
    /// used to be the *only* production `RunnerProtocolClient` in the tree,
    /// so the packaged binary could not reach a server at all.
    /// `main.rs` now wires `transport::HttpRunnerClient` instead, closing
    /// that gap.
    ///
    /// The test stays rather than being retired: `UnavailableProtocolClient`
    /// still exists and is still reachable — any future composition that
    /// omits a transport gets it — and its whole value is that such a
    /// composition fails as a typed `ProtocolUnavailable` rather than
    /// reporting success and idling forever. Deleting the test would remove
    /// the only guard on that fallback's honesty.
    #[tokio::test]
    async fn unavailable_protocol_is_a_typed_failure_not_success() {
        let runtime = RunnerRuntime::new(
            crate::UnavailableProtocolClient,
            FakeProcesses::default(),
            FakeFilesystem::default(),
            FakeClock,
            config(),
        );
        let (shutdown, _) = Shutdown::channel();

        assert!(matches!(
            runtime.run(shutdown).await,
            Err(RunnerError::ProtocolUnavailable)
        ));
    }
}
