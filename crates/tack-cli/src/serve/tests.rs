use super::*;

// Serializes the tests below that must mutate process env vars to steer
// `AppConfig::load()`, since env vars are process-global and `cargo
// test` runs this binary's tests concurrently. An async-aware mutex
// because the guard needs to stay held across this test's `.await`s.
static SERVE_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Puts back whatever `TACK_PORT`/`TACK_DATABASE_URL` held before the
/// test ran, including on panic, so a failure here can't leak a stray
/// `TACK_PORT=0` into a test that runs after it in the same process.
struct EnvRestore {
    port: Option<String>,
    database_url: Option<String>,
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        // SAFETY: the caller holds `SERVE_ENV_LOCK` for the guard's
        // whole lifetime, so no other thread in this test binary reads
        // or writes these two vars concurrently with this restore.
        unsafe {
            match &self.port {
                Some(v) => std::env::set_var("TACK_PORT", v),
                None => std::env::remove_var("TACK_PORT"),
            }
            match &self.database_url {
                Some(v) => std::env::set_var("TACK_DATABASE_URL", v),
                None => std::env::remove_var("TACK_DATABASE_URL"),
            }
        }
    }
}

/// Sets each env var, returning its previous value for `restore_env_vars`.
///
/// # Safety
/// The caller must hold `SERVE_ENV_LOCK` for the whole interval between this
/// call and the matching `restore_env_vars`.
unsafe fn set_env_vars(pairs: &[(&'static str, &str)]) -> Vec<(&'static str, Option<String>)> {
    pairs
        .iter()
        .map(|(k, v)| {
            let previous = std::env::var(k).ok();
            unsafe { std::env::set_var(k, v) };
            (*k, previous)
        })
        .collect()
}

/// Restores env vars captured by `set_env_vars`.
///
/// # Safety
/// The caller must hold `SERVE_ENV_LOCK` for the whole guarded interval.
unsafe fn restore_env_vars(saved: Vec<(&'static str, Option<String>)>) {
    for (k, v) in saved {
        match v {
            Some(v) => unsafe { std::env::set_var(k, v) },
            None => unsafe { std::env::remove_var(k) },
        }
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
    let _env_guard = SERVE_ENV_LOCK.lock().await;
    let _restore = EnvRestore {
        port: std::env::var("TACK_PORT").ok(),
        database_url: std::env::var("TACK_DATABASE_URL").ok(),
    };
    // SAFETY: serialized by `SERVE_ENV_LOCK` above.
    unsafe {
        std::env::set_var("TACK_PORT", "0");
        std::env::set_var("TACK_DATABASE_URL", "sqlite::memory:");
    }

    let (ready_tx, ready_rx) = oneshot::channel();
    let server = tokio::spawn(run(Some(ready_tx), None));

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
    let _env_guard = SERVE_ENV_LOCK.lock().await;
    let _restore = EnvRestore {
        port: std::env::var("TACK_PORT").ok(),
        database_url: std::env::var("TACK_DATABASE_URL").ok(),
    };
    // SAFETY: serialized by `SERVE_ENV_LOCK` above.
    let saved = unsafe {
        set_env_vars(&[
            ("TACK_PORT", "0"),
            ("TACK_DATABASE_URL", "sqlite::memory:"),
            ("TACK_HOST", "0.0.0.0"),
            ("TACK_LOCAL_RUNNER_ENABLE", "1"),
            ("TACK_API_ALLOW_UNAUTHENTICATED_NONLOOPBACK", "1"),
        ])
    };
    let control = Arc::new(RecordingControl {
        started: std::sync::atomic::AtomicBool::new(false),
    });
    let local_runner: Arc<dyn LocalRunnerControl> = control.clone();
    let (ready_tx, ready_rx) = oneshot::channel();
    let server = tokio::spawn(run(Some(ready_tx), Some(local_runner)));
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
    // SAFETY: still serialized by `SERVE_ENV_LOCK`.
    unsafe { restore_env_vars(saved) };
}
