use super::*;

/// A server on a free port and a database in memory, the configuration a test hands `run`.
fn test_config() -> AppConfig {
    AppConfig {
        port: 0,
        database_url: "sqlite::memory:".into(),
        ..AppConfig::default()
    }
}

/// Acceptance case for the readiness seam: start the server through
/// `run`, wait on the oneshot (no sleep, no retry loop),
/// then make one real HTTP request against the address it reports.
/// Requesting port 0 forces an OS-assigned port, so a passing request
/// also proves the signaled address is the real one, not the guess a
/// caller would otherwise have to poll.
#[tokio::test]
async fn run_signals_the_real_bound_address() {
    let (ready_tx, ready_rx) = oneshot::channel();
    let server = tokio::spawn(run(test_config(), Some(ready_tx), None));

    let addr = ready_rx.await.expect("readiness signal never arrived");
    assert_ne!(
        addr.port(),
        0,
        "signaled address must be the real OS-assigned port, not the configured 0"
    );

    let response = reqwest::get(format!("http://{addr}/api/health"))
        .await
        .expect("request against the signaled address must succeed");
    assert!(response.status().is_success());

    server.abort();
}

/// A control whose `start()` records whether it was ever called — the
/// auto-start check under test in
/// `persisted_enable_pref_never_auto_starts_non_loopback_bind`
/// below is the only thing calling it in that test.
struct RecordingControl {
    started: std::sync::atomic::AtomicBool,
}

#[async_trait::async_trait]
impl LocalRunnerControl for RecordingControl {
    async fn status(&self) -> tack_api::RuntimeStatus {
        tack_api::RuntimeStatus {
            state: tack_api::RuntimeState::Stopped,
            since: None,
            max_concurrent_attempts: 1,
            active_attempts: 0,
        }
    }
    async fn start(&self) -> Result<(), tack_api::LocalRunnerControlError> {
        self.started
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    async fn stop(&self) {}
    async fn runner_id(&self) -> Option<String> {
        Some("runner-test".to_string())
    }
    async fn resolve_secret(
        &self,
        _reference: &str,
    ) -> Result<String, tack_api::LocalRunnerControlError> {
        Err(tack_api::LocalRunnerControlError::SecretStore(
            "RecordingControl never resolves a secret".into(),
        ))
    }
}

/// The regression this seam exists to prevent: a preference saved from
/// an earlier loopback session (here, simulated with
/// `TACK_LOCAL_RUNNER_ENABLE=1` — the env-default half of the same
/// precedence `app_meta` would otherwise win) must never auto-start an
/// embedded runner on a server now bound to `0.0.0.0`. Proved directly
/// against `RecordingControl::start`, not just against the route being
/// absent — a route being unreachable would not by itself prove the
/// runner never actually started.
#[tokio::test]
async fn persisted_enable_pref_never_auto_starts_non_loopback_bind() {
    let config = AppConfig {
        host: "0.0.0.0".into(),
        local_runner_enable: true,
        allow_unauthenticated_nonloopback: true,
        ..test_config()
    };
    let control = Arc::new(RecordingControl {
        started: std::sync::atomic::AtomicBool::new(false),
    });
    let local_runner: Arc<dyn LocalRunnerControl> = control.clone();
    let (ready_tx, ready_rx) = oneshot::channel();
    let server = tokio::spawn(run(config, Some(ready_tx), Some(local_runner)));
    let addr = ready_rx.await.expect("readiness signal never arrived");
    // The server itself must still come up normally — refusing to
    // *start the runner* is not refusing to *serve*.
    let response = reqwest::get(format!("http://{addr}/api/health"))
        .await
        .expect("server must still boot and serve on a non-loopback bind");
    assert!(response.status().is_success());

    assert!(
        !control.started.load(std::sync::atomic::Ordering::SeqCst),
        "a non-loopback bind must never auto-start the embedded runner, \
         even with TACK_LOCAL_RUNNER_ENABLE=1"
    );

    server.abort();
}
