use super::*;

#[test]
fn embedded_runner_refuses_non_loopback_bind() {
    let config = tack_api::config::AppConfig {
        host: "0.0.0.0".to_owned(),
        ..tack_api::config::AppConfig::default()
    };

    let error = ensure_loopback(&config).expect_err("non-loopback bind must be refused");
    assert!(error.to_string().contains("loopback"));
}

#[test]
fn embedded_runner_accepts_the_default_loopback_bind() {
    assert!(ensure_loopback(&tack_api::config::AppConfig::default()).is_ok());
}

#[test]
fn with_runner_enabled_reads_the_environment_gate() {
    // SAFETY: this test owns the variable for its own duration and restores
    // it, but `std::env::set_var` is unsound to run concurrently with other
    // threads reading the same variable — `cargo test` runs this crate's
    // tests in one process, so serialize via the variable's own uniqueness
    // (nothing else in this crate reads `TACK_LOCAL_RUNNER_ENABLE`).
    let previous = std::env::var("TACK_LOCAL_RUNNER_ENABLE").ok();
    assert!(!with_runner_enabled(false));

    unsafe {
        std::env::set_var("TACK_LOCAL_RUNNER_ENABLE", "1");
    }
    assert!(with_runner_enabled(false));
    assert!(with_runner_enabled(true));

    unsafe {
        match &previous {
            Some(value) => std::env::set_var("TACK_LOCAL_RUNNER_ENABLE", value),
            None => std::env::remove_var("TACK_LOCAL_RUNNER_ENABLE"),
        }
    }
}

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
        "a manual credential must win over both the stored-session placeholder and self-provisioning"
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
        runner_config.enrollment_credential.is_some(),
        "build_runtime's precondition still needs a credential to be present"
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

fn control_with_state_dir(state_dir: &Path) -> EmbeddedRunnerControl {
    let mut runner_config = RunnerConfig::defaults();
    runner_config.state_dir = state_dir.to_path_buf();
    EmbeddedRunnerControl {
        server_config: unreachable_database_url(),
        config_path: None,
        state: Mutex::new(State {
            bound_addr: None,
            runner_config,
            running: None,
        }),
    }
}

#[tokio::test]
async fn a_fresh_control_reports_stopped_with_no_since() {
    let dir = unique_temp_dir("status-fresh");
    let control = control_with_state_dir(dir.path());

    let status = control.status().await;

    assert_eq!(status.state, RuntimeState::Stopped);
    assert!(status.since.is_none());
}

#[tokio::test]
async fn start_fails_typed_before_the_bound_address_is_known() {
    let dir = unique_temp_dir("start-no-addr");
    let control = control_with_state_dir(dir.path());

    let result = control.start().await;

    assert!(matches!(
        result,
        Err(LocalRunnerControlError::StartFailed(_))
    ));
    assert_eq!(control.status().await.state, RuntimeState::Stopped);
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

#[test]
fn embedded_default_state_dir_nests_under_storage_dir() {
    assert_eq!(
        embedded_default_state_dir("./storage"),
        PathBuf::from("./storage/runner")
    );
    assert_eq!(
        embedded_default_state_dir("/srv/tack-b/storage"),
        PathBuf::from("/srv/tack-b/storage/runner")
    );
}

/// Serializes every `with_cwd` call in this process: `cargo llvm-cov`'s
/// instrumented run (and plain `cargo test`) puts every test in this file
/// on threads of one shared process, unlike `nextest`'s one-process-per-test
/// — without this, two `migrate_legacy_state_dir_*` tests racing each
/// other's `set_current_dir` intermittently fail under either of those,
/// even though each test's own directories never collide.
static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Changes the process's current directory for the duration of a
/// closure, restoring it afterward even if the closure panics — needed
/// by every test below that exercises [`migrate_legacy_state_dir`],
/// since its legacy side is the crate's bare, cwd-relative default.
fn with_cwd<T>(dir: &Path, body: impl FnOnce() -> T) -> T {
    let _guard = CWD_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(dir).expect("set cwd");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
    std::env::set_current_dir(original).expect("restore cwd");
    match result {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[test]
fn migrate_legacy_state_dir_moves_legacy_directory_once() {
    let cwd_guard = unique_temp_dir("migrate-cwd");
    let target_root = unique_temp_dir("migrate-target");
    let new_dir = target_root.path().join("storage").join("runner");

    with_cwd(cwd_guard.path(), || {
        let legacy = Path::new(tack_runner::config::DEFAULT_STATE_DIR);
        std::fs::create_dir_all(legacy).expect("create legacy dir");
        std::fs::write(legacy.join("session.json"), "legacy-session")
            .expect("write legacy session");

        migrate_legacy_state_dir(&new_dir);

        assert!(
            new_dir.join("session.json").is_file(),
            "legacy state must be moved to the new, scoped directory"
        );
        assert_eq!(
            std::fs::read_to_string(new_dir.join("session.json")).unwrap(),
            "legacy-session"
        );
        assert!(
            !legacy.exists(),
            "the legacy directory must not be left behind after a successful migration"
        );
    });
}

#[test]
fn migrate_legacy_state_dir_never_touches_provisioned_dir() {
    let cwd_guard = unique_temp_dir("migrate-cwd-noop");
    let target_root = unique_temp_dir("migrate-target-noop");
    let new_dir = target_root.path().join("runner");
    std::fs::create_dir_all(&new_dir).expect("pre-create the new directory");
    std::fs::write(new_dir.join("session.json"), "already-provisioned").expect("write new session");

    with_cwd(cwd_guard.path(), || {
        let legacy = Path::new(tack_runner::config::DEFAULT_STATE_DIR);
        std::fs::create_dir_all(legacy).expect("create legacy dir");
        std::fs::write(legacy.join("session.json"), "stale-legacy-session")
            .expect("write legacy session");

        migrate_legacy_state_dir(&new_dir);

        assert_eq!(
            std::fs::read_to_string(new_dir.join("session.json")).unwrap(),
            "already-provisioned",
            "an existing new-style directory must never be overwritten by a stale legacy one"
        );
        assert!(
            legacy.join("session.json").is_file(),
            "the legacy directory is left untouched once the new one already exists"
        );
    });
}

#[test]
fn migrate_legacy_state_dir_is_noop_when_neither_dir_exists() {
    let cwd_guard = unique_temp_dir("migrate-cwd-fresh");
    let target_root = unique_temp_dir("migrate-target-fresh");
    let new_dir = target_root.path().join("storage").join("runner");

    with_cwd(cwd_guard.path(), || {
        migrate_legacy_state_dir(&new_dir);

        assert!(
            !new_dir.exists(),
            "a fresh install has nothing to migrate; the new directory is created \
             later, on first write, not by the migration step itself"
        );
    });
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

#[test]
fn with_no_variable_the_runner_reads_the_default_settings_file_only_when_it_exists() {
    let root = unique_temp_dir("default-config");
    let default = root.path().join("tack").join("runner.toml");
    let named = root.path().join("named.toml");

    assert_eq!(
        config_path_from(None, Some(default.clone())),
        None,
        "no file at the default, no file"
    );
    std::fs::create_dir_all(default.parent().unwrap()).expect("config dir");
    std::fs::write(&default, "[verify]\nenabled = true\n").expect("default file");
    assert_eq!(
        config_path_from(None, Some(default.clone())),
        Some(default.clone())
    );
    assert_eq!(
        config_path_from(Some(named.clone().into_os_string()), Some(default.clone())),
        Some(named),
        "the variable wins over the default"
    );
    assert_eq!(
        config_path_from(Some(std::ffi::OsString::new()), Some(default)),
        None,
        "set but empty asks for defaults only"
    );
}

#[tokio::test]
async fn the_embedded_runner_loads_the_file_tack_local_runner_config_names() {
    // SAFETY: same posture as `with_runner_enabled_reads_the_environment_gate`:
    // nothing else in this crate's tests reads `TACK_LOCAL_RUNNER_CONFIG`, and
    // the variable is restored before the test returns.
    let previous = std::env::var_os(RUNNER_CONFIG_ENV);
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

    unsafe {
        std::env::remove_var(RUNNER_CONFIG_ENV);
    }
    assert!(runner_config_path().is_none());
    let without = EmbeddedRunnerControl::new(server_config.clone()).expect("defaults");
    assert!(!without.state.lock().await.runner_config.verify.enabled);

    unsafe {
        std::env::set_var(RUNNER_CONFIG_ENV, &config_path);
    }
    assert_eq!(runner_config_path().as_deref(), Some(config_path.as_path()));
    let with = EmbeddedRunnerControl::new(server_config).expect("named file");
    let state = with.state.lock().await;
    assert!(state.runner_config.verify.enabled);
    assert_eq!(state.runner_config.verify.program, "prill");
    assert!(state.runner_config.git.push_branches);
    assert_eq!(
        state.runner_config.state_dir,
        root.path().join("storage").join("runner"),
        "the file names no state_dir, so the storage-scoped default still applies"
    );
    drop(state);

    unsafe {
        match previous {
            Some(value) => std::env::set_var(RUNNER_CONFIG_ENV, value),
            None => std::env::remove_var(RUNNER_CONFIG_ENV),
        }
    }
}
