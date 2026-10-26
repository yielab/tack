//! The rules behind the embedded-runner routes: the saved preference, where the runner keeps its
//! state, the supervisor that keeps one process alive, and the test run Settings starts.

use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_trait::async_trait;
use serde_json::json;
use tack_app::local_runner::{
    EmbeddedRunner, LocalRunnerControl, LocalRunnerControlError, RunnerHost, RunnerProcess,
    RuntimeState, RuntimeStatus, TestRunError, config_path_from, effective_enabled,
    ensure_loopback, has_stored_session, migrate_legacy_state_dir, set_enabled, start_test_run,
    state_dir_under, with_runner_enabled,
};
use tack_db::queries;
use tack_test_support::{create_test_workspace, seed, setup_test_db};

// The preference.

#[tokio::test]
async fn the_default_applies_until_the_ui_saves_a_preference() {
    let repo = setup_test_db().await;

    assert!(effective_enabled(&repo, true).await);
    assert!(!effective_enabled(&repo, false).await);

    set_enabled(&repo, true).await.unwrap();
    assert!(effective_enabled(&repo, false).await);
    set_enabled(&repo, false).await.unwrap();
    assert!(
        !effective_enabled(&repo, true).await,
        "a saved off beats an on default"
    );
}

#[tokio::test]
async fn an_unreadable_saved_value_falls_back_to_the_default() {
    let repo = setup_test_db().await;
    repo.set_app_meta("local_runner_config", "not json")
        .await
        .unwrap();

    assert!(effective_enabled(&repo, true).await);
    assert!(!effective_enabled(&repo, false).await);
}

// Whether and where the runner may start.

#[test]
fn embedded_runner_refuses_non_loopback_bind() {
    let error = ensure_loopback("0.0.0.0", false).expect_err("non-loopback bind must be refused");
    assert!(error.to_string().contains("loopback"));
    assert!(error.to_string().contains("0.0.0.0"));
}

#[test]
fn embedded_runner_accepts_a_loopback_bind() {
    assert!(ensure_loopback("127.0.0.1", true).is_ok());
}

#[test]
fn with_runner_enabled_reads_the_environment_gate() {
    // SAFETY: nothing else in this test binary reads `TACK_LOCAL_RUNNER_ENABLE`, and each test
    // runs in its own process under nextest.
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

#[test]
fn state_dir_nests_under_storage_dir() {
    assert_eq!(
        state_dir_under("./storage"),
        PathBuf::from("./storage/runner")
    );
    assert_eq!(
        state_dir_under("/srv/tack-b/storage"),
        PathBuf::from("/srv/tack-b/storage/runner")
    );
}

fn temp_dir(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

#[test]
fn migrate_legacy_state_dir_moves_legacy_directory_once() {
    let root = temp_dir("migrate");
    let legacy = root.path().join("legacy");
    let new_dir = root.path().join("storage").join("runner");
    std::fs::create_dir_all(&legacy).expect("create legacy dir");
    std::fs::write(legacy.join("session.json"), "legacy-session").expect("write legacy session");

    migrate_legacy_state_dir(&legacy, &new_dir);

    assert_eq!(
        std::fs::read_to_string(new_dir.join("session.json")).unwrap(),
        "legacy-session",
        "legacy state must be moved to the new, scoped directory"
    );
    assert!(
        !legacy.exists(),
        "the legacy directory must not be left behind after a successful migration"
    );
}

#[test]
fn migrate_legacy_state_dir_never_touches_provisioned_dir() {
    let root = temp_dir("migrate-noop");
    let legacy = root.path().join("legacy");
    let new_dir = root.path().join("runner");
    std::fs::create_dir_all(&legacy).expect("create legacy dir");
    std::fs::write(legacy.join("session.json"), "stale-legacy-session").expect("write legacy");
    std::fs::create_dir_all(&new_dir).expect("pre-create the new directory");
    std::fs::write(new_dir.join("session.json"), "already-provisioned").expect("write new");

    migrate_legacy_state_dir(&legacy, &new_dir);

    assert_eq!(
        std::fs::read_to_string(new_dir.join("session.json")).unwrap(),
        "already-provisioned",
        "an existing new-style directory must never be overwritten by a stale legacy one"
    );
    assert!(
        legacy.join("session.json").is_file(),
        "the legacy directory is left untouched once the new one already exists"
    );
}

#[test]
fn migrate_legacy_state_dir_is_noop_when_neither_dir_exists() {
    let root = temp_dir("migrate-fresh");
    let new_dir = root.path().join("storage").join("runner");

    migrate_legacy_state_dir(&root.path().join("legacy"), &new_dir);

    assert!(
        !new_dir.exists(),
        "a fresh install has nothing to migrate; the new directory is created later, on first \
         write, not by the migration step itself"
    );
}

#[test]
fn with_no_variable_the_runner_reads_the_default_settings_file_only_when_it_exists() {
    let root = temp_dir("default-config");
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
        config_path_from(Some(OsString::new()), Some(default)),
        None,
        "set but empty asks for defaults only"
    );
}

#[test]
fn has_stored_session_is_false_for_an_empty_directory() {
    let guard = temp_dir("no-session");

    assert!(!has_stored_session(guard.path()));
}

#[test]
fn has_stored_session_is_true_once_session_json_exists() {
    let guard = temp_dir("session");
    std::fs::write(guard.path().join("session.json"), "{}").unwrap();

    assert!(has_stored_session(guard.path()));
}

#[test]
fn has_stored_session_is_false_when_state_dir_is_absent() {
    let guard = temp_dir("absent");

    assert!(!has_stored_session(&guard.path().join("absent")));
}

// The supervisor.

#[derive(Default)]
struct Counters {
    launched: AtomicUsize,
    stopped: AtomicBool,
    finished: AtomicBool,
    fail_launch: AtomicBool,
}

struct FakeProcess(Arc<Counters>);

impl RunnerProcess for FakeProcess {
    fn is_finished(&self) -> bool {
        self.0.finished.load(Ordering::SeqCst)
    }

    fn active_attempts(&self) -> usize {
        2
    }

    fn request_stop(&self) {
        self.0.stopped.store(true, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct FakeHost(Arc<Counters>);

#[async_trait]
impl RunnerHost for FakeHost {
    async fn max_concurrent_attempts(&self) -> u32 {
        4
    }

    fn config_file(&self) -> Option<String> {
        Some("runner.toml".to_owned())
    }

    async fn launch(&self, api_base_url: &str) -> Result<Box<dyn RunnerProcess>, String> {
        assert_eq!(api_base_url, "http://127.0.0.1:3210/api/runner/v1");
        if self.0.fail_launch.load(Ordering::SeqCst) {
            return Err("no credential".to_owned());
        }
        self.0.launched.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(FakeProcess(self.0.clone())))
    }

    async fn enrolled_runner_id(&self) -> Option<String> {
        Some("runr_1".to_owned())
    }

    async fn verify_program(&self) -> Option<String> {
        Some("prill".to_owned())
    }

    async fn resolve_secret(&self, reference: &str) -> Result<String, String> {
        Err(format!("no secret {reference}"))
    }
}

fn supervisor() -> (EmbeddedRunner<FakeHost>, Arc<Counters>) {
    let host = FakeHost::default();
    let counters = host.0.clone();
    (EmbeddedRunner::new(host), counters)
}

fn bound() -> SocketAddr {
    "127.0.0.1:3210".parse().unwrap()
}

#[tokio::test]
async fn a_fresh_supervisor_is_stopped_and_has_no_runner_id() {
    let (control, _) = supervisor();

    let status = control.status().await;

    assert_eq!(status.state, RuntimeState::Stopped);
    assert!(status.since.is_none());
    assert_eq!(status.max_concurrent_attempts, 4);
    assert_eq!(status.active_attempts, 0);
    assert_eq!(control.runner_id().await, None);
}

#[tokio::test]
async fn start_fails_typed_before_the_bound_address_is_known() {
    let (control, counters) = supervisor();

    let result = control.start().await;

    assert!(matches!(
        result,
        Err(LocalRunnerControlError::StartFailed(_))
    ));
    assert_eq!(counters.launched.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_second_start_does_not_launch_a_second_runner() {
    let (control, counters) = supervisor();
    control.set_bound_addr(bound()).await;

    control.start().await.unwrap();
    control.start().await.unwrap();

    assert_eq!(counters.launched.load(Ordering::SeqCst), 1);
    let status = control.status().await;
    assert_eq!(status.state, RuntimeState::Running);
    assert_eq!(status.active_attempts, 2);
    assert_eq!(control.runner_id().await.as_deref(), Some("runr_1"));
}

#[tokio::test]
async fn a_failed_launch_is_reported_and_leaves_the_runner_stopped() {
    let (control, counters) = supervisor();
    counters.fail_launch.store(true, Ordering::SeqCst);
    control.set_bound_addr(bound()).await;

    let result = control.start().await;

    assert!(
        matches!(&result, Err(LocalRunnerControlError::StartFailed(reason)) if reason == "no credential"),
        "{result:?}"
    );
    assert_eq!(control.status().await.state, RuntimeState::Stopped);
}

#[tokio::test]
async fn stop_asks_the_process_to_stop_and_reports_stopped_at_once() {
    let (control, counters) = supervisor();
    control.set_bound_addr(bound()).await;
    control.start().await.unwrap();

    control.stop().await;

    assert!(counters.stopped.load(Ordering::SeqCst));
    assert_eq!(control.status().await.state, RuntimeState::Stopped);
    assert_eq!(control.runner_id().await, None);
}

#[tokio::test]
async fn a_runner_that_exited_on_its_own_is_reported_stopped() {
    let (control, counters) = supervisor();
    control.set_bound_addr(bound()).await;
    control.start().await.unwrap();

    counters.finished.store(true, Ordering::SeqCst);

    assert_eq!(control.status().await.state, RuntimeState::Stopped);
    control.start().await.unwrap();
    assert_eq!(
        counters.launched.load(Ordering::SeqCst),
        2,
        "a runner that died can be started again"
    );
}

#[tokio::test]
async fn the_host_answers_the_questions_only_it_can() {
    let (control, _) = supervisor();

    assert_eq!(control.verify_program().await.as_deref(), Some("prill"));
    assert_eq!(control.config_file().as_deref(), Some("runner.toml"));
    assert!(matches!(
        control.resolve_secret("store:x").await,
        Err(LocalRunnerControlError::SecretStore(reason)) if reason == "no secret store:x"
    ));
}

// The test run.

struct RunningRunner(&'static str);

#[async_trait]
impl LocalRunnerControl for RunningRunner {
    async fn status(&self) -> RuntimeStatus {
        RuntimeStatus {
            state: RuntimeState::Running,
            since: None,
            max_concurrent_attempts: 1,
            active_attempts: 0,
        }
    }

    async fn start(&self) -> Result<(), LocalRunnerControlError> {
        Ok(())
    }

    async fn stop(&self) {}

    async fn runner_id(&self) -> Option<String> {
        Some(self.0.to_owned())
    }

    async fn resolve_secret(&self, _reference: &str) -> Result<String, LocalRunnerControlError> {
        Err(LocalRunnerControlError::SecretStore("none".to_owned()))
    }
}

#[tokio::test]
async fn a_test_run_is_filed_under_one_hidden_project_and_pinned_to_the_runner() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    tack_db::repo::execution::seed_builtin_profiles(repo.pool())
        .await
        .unwrap();
    seed::registered_runner(&repo, "runner-test").await;
    let control = RunningRunner("runner-test");
    let run = |harness: &str| {
        start_test_run(
            &repo,
            &tack_protocol::SystemClock,
            Some(&control as &dyn LocalRunnerControl),
            workspace_id,
            harness.to_owned(),
        )
    };

    let first = run("claude-code").await.expect("first test run");

    let row = queries::execution_requests::request_inputs(&repo, &first)
        .await
        .unwrap()
        .expect("the request was stored");
    assert_eq!(row.selector_id, "runner-test");
    let repository: serde_json::Value = serde_json::from_str(&row.repository_snapshot).unwrap();
    assert_eq!(repository["kind"], "scratch");
    assert_eq!(repository["remote"], "");
    assert_eq!(row.requested_model, None);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&row.permission_policy).unwrap(),
        // The Implementer's own tools for this harness: none would leave the agent unable to act.
        json!({
            "tools": ["Read", "Edit", "Write", "Bash", "Agent", "Grep", "Glob"],
            "network": false
        })
    );
    assert_eq!(
        Some(row.agent_profile_id),
        queries::agent_profiles::implementer_profile_id(&repo)
            .await
            .unwrap()
    );
    assert_eq!(row.timeout_seconds, 300);

    let second = run("claude-code").await.expect("second test run");

    assert_ne!(second, first);
    let project_id = queries::projects::project_id_named(&repo, workspace_id, "Agent tests")
        .await
        .unwrap()
        .expect("the test project exists");
    let project = repo
        .get_project(project_id.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(project.archived, "the test project is hidden");
    assert_eq!(
        repo.get_item_tree(project.id).await.unwrap().len(),
        2,
        "the second call reuses the project and files a second item in it"
    );
    assert_eq!(repo.list_projects(workspace_id).await.unwrap().len(), 0);
}

#[tokio::test]
async fn a_test_run_without_the_implementer_profile_is_not_found() {
    let repo = setup_test_db().await;
    let workspace_id = create_test_workspace(&repo).await;
    seed::agent_profiles_cleared(&repo).await;
    let control = RunningRunner("runner-test");

    let result = start_test_run(
        &repo,
        &tack_protocol::SystemClock,
        Some(&control as &dyn LocalRunnerControl),
        workspace_id,
        "claude-code".to_owned(),
    )
    .await;

    assert!(
        matches!(result, Err(TestRunError::NoImplementerProfile)),
        "{result:?}"
    );
}
