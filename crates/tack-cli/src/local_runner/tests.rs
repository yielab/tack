use super::*;
use tack_app::local_runner::{LocalRunnerControl, LocalRunnerControlError, RuntimeState};

fn unique_temp_dir(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

/// A `database_url` that fails at parse time, before any filesystem or
/// network I/O — so a test built against it proves whether
/// `tack_db::init_pool` (and therefore self-provisioning) was ever
/// reached, without needing a real database.
fn unreachable_database_url() -> tack_api::config::AppConfig {
    tack_api::config::AppConfig {
        database_url: "not-a-real-database-url".to_owned(),
        ..tack_api::config::AppConfig::default()
    }
}

#[tokio::test]
async fn ensure_runner_credential_leaves_manual_one_untouched() {
    let state_dir_guard = unique_temp_dir("manual");
    let state_dir = state_dir_guard.path();
    let mut runner_config = RunnerConfig {
        enrollment_credential: Some(EnrollmentCredential::new("manually-configured-token")),
        state_dir: state_dir.to_path_buf(),
        ..RunnerConfig::defaults()
    };
    let server_config = unreachable_database_url();

    let result = ensure_runner_credential(&mut runner_config, &server_config).await;

    assert!(
        result.is_ok(),
        "a manual credential must never require touching the database: {result:?}"
    );
    assert_eq!(
        runner_config
            .enrollment_credential
            .expect("credential must remain set")
            .expose(),
        "manually-configured-token",
        "a manual credential must win over self-provisioning"
    );
    std::fs::remove_dir_all(state_dir).ok();
}

#[tokio::test]
async fn ensure_runner_credential_reuses_session_no_self_provisioning() {
    let state_dir_guard = unique_temp_dir("stored-session");
    let state_dir = state_dir_guard.path();
    std::fs::write(state_dir.join("session.json"), "{}").expect("write session.json");
    let mut runner_config = RunnerConfig {
        enrollment_credential: None,
        state_dir: state_dir.to_path_buf(),
        ..RunnerConfig::defaults()
    };
    // Deliberately unreachable: if this path wrongly called
    // `local_enrollment::self_provision`, opening the pool would fail
    // and this assertion below would see an `Err`, not an `Ok`.
    let server_config = unreachable_database_url();

    let result = ensure_runner_credential(&mut runner_config, &server_config).await;

    assert!(
        result.is_ok(),
        "a stored session must satisfy the credential requirement without opening the database: {result:?}"
    );
    assert!(
        runner_config.enrollment_credential.is_none(),
        "the runner starts from the stored session and is given no credential"
    );
    std::fs::remove_dir_all(state_dir).ok();
}

#[tokio::test]
async fn ensure_runner_credential_self_provisions_with_nothing_else() {
    let state_dir_guard = unique_temp_dir("no-session");
    let state_dir = state_dir_guard.path();
    let mut runner_config = RunnerConfig {
        enrollment_credential: None,
        state_dir: state_dir.to_path_buf(),
        ..RunnerConfig::defaults()
    };
    let server_config = unreachable_database_url();

    let result = ensure_runner_credential(&mut runner_config, &server_config).await;

    assert!(
        result.is_err(),
        "with no manual credential and no stored session, self-provisioning must actually be \
         attempted — proved here by its failure against a deliberately unreachable database, \
         the same database_url the previous test proves does NOT get touched when a session exists"
    );
    std::fs::remove_dir_all(state_dir).ok();
}

fn control_with_state_dir(state_dir: &Path) -> EmbeddedRunner<CliRunnerHost> {
    let mut runner_config = RunnerConfig::defaults();
    runner_config.state_dir = state_dir.to_path_buf();
    EmbeddedRunner::new(CliRunnerHost {
        server_config: unreachable_database_url(),
        config_path: None,
        runner_config: Mutex::new(runner_config),
    })
}

/// Points D-Bus at nothing so `SecretStore::open` takes its file fallback:
/// a test never writes into the real keychain, and never waits on it — a
/// keychain call that fails under load returns before the code under test runs.
fn force_file_secret_backend() {
    // SAFETY: every caller writes the same value, so a concurrent reader
    // sees either the old environment or this one.
    unsafe {
        std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "/dev/null");
    }
}

#[tokio::test]
async fn start_then_stop_round_trips_the_runtime_state() {
    let dir = unique_temp_dir("start-stop");
    force_file_secret_backend();
    let control = control_with_state_dir(dir.path());
    // `unreachable_database_url` makes self-provisioning fail deliberately
    // (no session on disk, no manual credential) — this test only needs
    // to prove `start()` reaches the point of attempting it, and that a
    // failure there is reported rather than silently leaving `running`
    // set. A live start-then-claim proof belongs to an integration test
    // with a real server, not this unit.
    control.set_bound_addr("127.0.0.1:1".parse().unwrap()).await;

    let result = control.start().await;

    assert!(
        matches!(result, Err(LocalRunnerControlError::StartFailed(_))),
        "self-provisioning against an unreachable database must fail typed: {result:?}"
    );
    assert_eq!(
        control.status().await.state,
        RuntimeState::Stopped,
        "a failed start must not leave the control reporting running"
    );
}

// The acceptance proof for two servers each seeing only their own
// runner's enrollment lives in `tests/embedded_runner_state_scoping.rs`
// as a real two-subprocess test, not here: `tack_api::server::serve_inner`
// installs a process-global `tracing` subscriber via `try_init`, so only
// the first call in a process actually configures it — a second call is
// a silent no-op rather than a crash, but a second embedded server in
// that same process would still run under the first one's logging
// config instead of its own. So this crate's own unit tests — which all
// share one test binary process per test function, not per server —
// still test at most one real embedded server each. Two genuinely
// separate `tack` processes have no such conflict.

#[tokio::test]
async fn the_embedded_runner_loads_the_file_tack_local_runner_config_names() {
    let root = unique_temp_dir("runner-config");
    let config_path = root.path().join("runner.toml");
    std::fs::write(
        &config_path,
        "[verify]\nenabled = true\nprogram = \"prill\"\n\n[git]\npush_branches = true\n",
    )
    .expect("config file");
    let server_config = tack_api::config::AppConfig {
        storage_dir: root.path().join("storage").display().to_string(),
        ..tack_api::config::AppConfig::default()
    };

    // Nothing names a file: defaults only (a unit test never reads the machine's own file).
    assert!(runner_config_path(None, None).is_none());
    let without = CliRunnerHost::new(server_config.clone(), None, false).expect("defaults");
    assert!(!without.runner_config.lock().await.verify.enabled);

    // The variable names it, as does the flag, which wins.
    let variable = Some(config_path.clone().into_os_string());
    assert_eq!(
        runner_config_path(None, variable).as_deref(),
        Some(config_path.as_path())
    );
    assert_eq!(
        runner_config_path(Some(config_path.clone()), Some("".into())).as_deref(),
        Some(config_path.as_path())
    );
    let with = CliRunnerHost::new(server_config, Some(config_path), false).expect("named file");
    let loaded = with.runner_config.lock().await;
    assert!(loaded.verify.enabled);
    assert_eq!(loaded.verify.program, "prill");
    assert!(loaded.git.push_branches);
    assert_eq!(
        loaded.state_dir,
        root.path().join("storage").join("runner"),
        "the file names no state_dir, so the storage-scoped default still applies"
    );
}
