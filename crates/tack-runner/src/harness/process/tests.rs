use super::*;
use crate::harness::fixtures::fake_harness_command;
use std::path::Path;

/// A scratch directory that removes itself, and everything written under
/// it, when the returned guard drops — including when an assertion panics
/// first.
fn temp_workspace(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}

fn spec(workspace: &Path, env: BTreeMap<String, String>) -> ProcessSpec {
    let (program, args) = fake_harness_command();
    ProcessSpec {
        program,
        args,
        env,
        stdin: None,
        working_directory: workspace.to_path_buf(),
        workspace_root: workspace.to_path_buf(),
        keep_stdin_open: false,
    }
}

fn env_with_mode(mode: &str) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    env.insert("TACK_FAKE_HARNESS_MODE".to_owned(), mode.to_owned());
    env
}

fn generous_limits() -> ProcessLimits {
    ProcessLimits::new(1_000_000, 1_000_000, Duration::from_secs(10))
}

/// Spawns `env` in `workspace` and waits for it to finish under `limits`,
/// redacting `secrets`. Shared by every test that only cares about the
/// outcome, not the spawn/wait mechanics.
async fn run(
    workspace: &Path,
    env: BTreeMap<String, String>,
    limits: &ProcessLimits,
    secrets: &SecretMaterial,
) -> ProcessResult {
    spec(workspace, env)
        .spawn()
        .await
        .expect("spawn")
        .wait_with_capture(limits, secrets)
        .await
        .expect("wait")
}

async fn run_default(
    workspace: &Path,
    env: BTreeMap<String, String>,
    limits: &ProcessLimits,
) -> ProcessResult {
    run(workspace, env, limits, &SecretMaterial::new()).await
}

#[tokio::test]
async fn success_mode_exits_cleanly_and_captures_stdout() {
    let workspace_dir = temp_workspace("success");
    let result = run_default(
        workspace_dir.path(),
        env_with_mode("success"),
        &generous_limits(),
    )
    .await;

    assert_eq!(result.exit, ProcessExit::Exited(0));
    assert!(result.stdout.text.contains("fake-harness-ok"));
    assert!(!result.stdout.truncated);
}

#[tokio::test]
async fn failure_mode_reports_configured_exit_code() {
    let workspace_dir = temp_workspace("failure");
    let mut env = env_with_mode("failure");
    env.insert("TACK_FAKE_HARNESS_EXIT_CODE".to_owned(), "17".to_owned());
    let result = run_default(workspace_dir.path(), env, &generous_limits()).await;

    assert_eq!(result.exit, ProcessExit::Exited(17));
}

/// Acceptance: "adapters cannot cross-read each other's workspaces" —
/// structural half. A spec whose working directory is not inside its
/// declared workspace root is refused before anything is spawned.
#[tokio::test]
async fn spawn_refuses_a_working_directory_outside_its_workspace_root() {
    let workspace_dir = temp_workspace("escape-root");
    let workspace = workspace_dir.path();
    let sibling_dir = temp_workspace("escape-sibling");
    let sibling = sibling_dir.path();
    let mut escaping = spec(workspace, env_with_mode("success"));
    escaping.working_directory = sibling.to_path_buf();

    assert!(matches!(
        escaping.spawn().await,
        Err(ProcessError::WorkspaceEscape)
    ));
}

/// Runs the `read_relative` fixture mode against `workspace`, telling it to
/// read `canary.txt` and report the result.
async fn read_canary(workspace: &Path) -> ProcessResult {
    let mut env = env_with_mode("read_relative");
    env.insert(
        "TACK_FAKE_HARNESS_READ_PATH".to_owned(),
        "canary.txt".to_owned(),
    );
    run_default(workspace, env, &generous_limits()).await
}

/// Acceptance: "adapters cannot cross-read each other's workspaces" —
/// empirical half, for the non-adversarial case this layer actually
/// defends against: two attempts get two real, distinct workspace
/// directories, each with its own file at the *same relative name*. A
/// process spawned into workspace A and told to read `./canary.txt`
/// must see A's own content and never B's, proving workspace assignment
/// is never accidentally shared or aliased across attempts. (A harness
/// that deliberately path-traverses via `../` once running is a
/// different, OS-sandboxing problem — chroot/namespaces/landlock — not
/// attempted here.
/// The structural half above, refusing a working directory outside its
/// declared root before spawn, is what actually stops a *misconfigured*
/// adapter from pointing at the wrong workspace in the first place.)
#[tokio::test]
async fn each_confined_process_only_ever_sees_its_own_canary_file() {
    let workspace_a_dir = temp_workspace("isolation-a");
    let workspace_a = workspace_a_dir.path();
    let workspace_b_dir = temp_workspace("isolation-b");
    let workspace_b = workspace_b_dir.path();
    std::fs::write(workspace_a.join("canary.txt"), "workspace-a-secret")
        .expect("write workspace a canary");
    std::fs::write(workspace_b.join("canary.txt"), "workspace-b-secret")
        .expect("write workspace b canary");

    let read_a = read_canary(workspace_a).await;
    assert_eq!(read_a.exit, ProcessExit::Exited(0));
    assert!(read_a.stdout.text.contains("workspace-a-secret"));
    assert!(!read_a.stdout.text.contains("workspace-b-secret"));

    let read_b = read_canary(workspace_b).await;
    assert_eq!(read_b.exit, ProcessExit::Exited(0));
    assert!(read_b.stdout.text.contains("workspace-b-secret"));
    assert!(!read_b.stdout.text.contains("workspace-a-secret"));
}

/// Drives 8 MiB of real stdout through a 64 KiB cap: the child exits
/// cleanly, so draining past the cap never blocks it on a full pipe, and
/// exactly the cap's worth of bytes is kept.
#[tokio::test]
async fn high_volume_output_never_blocks_the_child() {
    let workspace_dir = temp_workspace("high-volume");
    const VOLUME_BYTES: usize = 8 * 1024 * 1024;
    const CAP: usize = 64 * 1024;
    let mut env = env_with_mode("high_volume");
    env.insert(
        "TACK_FAKE_HARNESS_VOLUME_BYTES".to_owned(),
        VOLUME_BYTES.to_string(),
    );
    let limits = ProcessLimits::new(CAP, CAP, Duration::from_secs(30));
    let result = run_default(workspace_dir.path(), env, &limits).await;

    assert_eq!(
        result.exit,
        ProcessExit::Exited(0),
        "child was not deadlocked"
    );
    assert!(result.stdout.truncated);
    assert_eq!(result.stdout.total_bytes_seen, VOLUME_BYTES as u64);
    assert_eq!(result.stdout.bytes_dropped, (VOLUME_BYTES - CAP) as u64);
}

struct CaptureRow {
    label: &'static str,
    line_count: usize,
    truncated: bool,
}

const CAPTURE_ROWS: [CaptureRow; 3] = [
    CaptureRow {
        label: "under the cap",
        line_count: 5,
        truncated: false,
    },
    CaptureRow {
        label: "exactly at the cap",
        line_count: 10,
        truncated: false,
    },
    CaptureRow {
        label: "over the cap",
        line_count: 20,
        truncated: true,
    },
];

/// Acceptance: capture keeps the head and the tail of the stream, over a
/// table of sizes relative to the cap. Under and at the cap, the capture is
/// the whole stream, byte-identical to a plain bounded read. Over the cap,
/// the text still starts with the stream's first bytes and still ends with
/// its actual last line — the one a harness's terminal `result` line lives
/// on — rather than losing it to a straight head-only cut.
#[tokio::test]
async fn capture_bounded_keeps_the_head_and_the_tail_of_the_stream() {
    const CAP: usize = 100;
    const HEAD_CAP: usize = CAP / 2;

    for row in &CAPTURE_ROWS {
        let lines: Vec<String> = (0..row.line_count)
            .map(|index| format!("line-{index:04}\n"))
            .collect();
        let stream = lines.concat();
        let total = stream.len() as u64;
        let mut cursor = std::io::Cursor::new(stream.clone().into_bytes());
        let raw = capture_bounded(&mut cursor, CAP).await;
        let captured = finalize_capture(raw, &SecretMaterial::new());

        assert_eq!(captured.total_bytes_seen, total, "{}", row.label);
        assert_eq!(captured.truncated, row.truncated, "{}", row.label);
        if row.truncated {
            let last_line = lines.last().expect("at least one line");
            assert!(
                captured.text.starts_with(&stream[..HEAD_CAP]),
                "{}",
                row.label
            );
            assert!(captured.text.ends_with(last_line.as_str()), "{}", row.label);
            assert_eq!(captured.bytes_dropped, total - CAP as u64, "{}", row.label);
        } else {
            assert_eq!(captured.text, stream, "{}", row.label);
            assert_eq!(captured.bytes_dropped, 0, "{}", row.label);
        }
    }
}

/// Spawns the `spawn_child` fixture mode in a fresh workspace and waits
/// for its grandchild's pidfile, returning the workspace guard (kept alive
/// for the caller), the parent process, and both pids.
async fn spawn_with_live_grandchild(
    label: &str,
) -> (tempfile::TempDir, SupervisedProcess, u32, u32) {
    let workspace_dir = temp_workspace(label);
    let workspace = workspace_dir.path();
    let pidfile = workspace.join("grandchild.pid");
    let mut env = env_with_mode("spawn_child");
    env.insert(
        "TACK_FAKE_HARNESS_PIDFILE".to_owned(),
        pidfile.to_str().expect("utf8 pidfile path").to_owned(),
    );
    env.insert(
        "TACK_FAKE_HARNESS_SLEEP_SECONDS".to_owned(),
        "3600".to_owned(),
    );
    let process = spec(workspace, env).spawn().await.expect("spawn");
    let direct_child_pid = process.pid();
    let grandchild_pid = wait_for_pidfile(&pidfile).await;
    (workspace_dir, process, grandchild_pid, direct_child_pid)
}

/// Acceptance: cancel kills descendants. The fake binary spawns a
/// grandchild `sleep` in the background (inheriting the same process
/// group, since it does not call `setpgid` itself) and writes its pid to
/// a file before waiting on it. Cancelling the direct child must also
/// reap that grandchild, not merely the shell that spawned it.
#[tokio::test]
async fn cancel_kills_the_whole_descendant_tree_not_just_the_child() {
    let (_workspace_dir, process, grandchild_pid, direct_child_pid) =
        spawn_with_live_grandchild("cancel-tree").await;
    assert!(
        process_alive(grandchild_pid),
        "grandchild must be observed running before cancellation"
    );

    let outcome = process
        .cancel(Duration::from_secs(2))
        .await
        .expect("cancel");
    assert!(matches!(
        outcome,
        CancelOutcome::Stopped | CancelOutcome::Killed
    ));

    // Poll instead of a single fixed sleep so reaping is not a hidden
    // pacing dependency.
    assert!(
        wait_until_dead(grandchild_pid, Duration::from_secs(5)).await,
        "grandchild must be gone after cancelling its parent"
    );
    assert!(
        !process_alive(direct_child_pid)
            || wait_until_dead(direct_child_pid, Duration::from_secs(5)).await,
        "direct child must also be gone"
    );
}

/// Acceptance: timeouts. A process that runs longer than the configured
/// timeout is killed and reported as `TimedOut` rather than hanging the
/// caller or being reported as any other terminal shape.
#[tokio::test]
async fn a_process_exceeding_timeout_is_killed_and_reported_timed_out() {
    let workspace_dir = temp_workspace("timeout");
    let mut env = env_with_mode("hang");
    env.insert(
        "TACK_FAKE_HARNESS_SLEEP_SECONDS".to_owned(),
        "3600".to_owned(),
    );
    let limits = ProcessLimits::new(4096, 4096, Duration::from_millis(50));
    let result = run_default(workspace_dir.path(), env, &limits).await;

    assert_eq!(result.exit, ProcessExit::TimedOut);
}

/// Acceptance: secret canaries are absent from logs and events. A canary
/// value is placed in the child's environment and stdin, and the fake
/// binary is asked to actively echo it back on both stdout and stderr —
/// simulating a worst-case leaky harness. The captured output must still
/// not contain it once `SecretMaterial` has been told about it, and the
/// `ProcessSpec`'s own `Debug` output must never contain it either.
#[tokio::test]
async fn secret_canaries_never_survive_into_output_or_spec_debug() {
    let workspace_dir = temp_workspace("canary");
    let workspace = workspace_dir.path();
    const CANARY_ENV: &str = "tack-test-canary-env-73f1";
    const CANARY_STDIN: &str = "tack-test-canary-stdin-91ab";
    let mut env = env_with_mode("echo_canary");
    env.insert("TACK_TEST_SECRET".to_owned(), CANARY_ENV.to_owned());
    env.insert(
        "TACK_FAKE_HARNESS_ECHO_ENV_KEYS".to_owned(),
        "TACK_TEST_SECRET".to_owned(),
    );

    let mut process_spec = spec(workspace, env);
    process_spec.stdin = Some(CANARY_STDIN.as_bytes().to_vec());

    let debug_output = format!("{process_spec:?}");
    assert!(!debug_output.contains(CANARY_ENV));
    assert!(!debug_output.contains(CANARY_STDIN));

    let mut secrets = SecretMaterial::new();
    secrets.register(CANARY_ENV).register(CANARY_STDIN);

    let result = process_spec
        .spawn()
        .await
        .expect("spawn")
        .wait_with_capture(&generous_limits(), &secrets)
        .await
        .expect("wait");

    assert!(
        result.stdout.text.contains("[REDACTED]"),
        "the fake harness must actually have echoed something for this test to be meaningful"
    );
    assert!(!result.stdout.text.contains(CANARY_ENV));
    assert!(!result.stdout.text.contains(CANARY_STDIN));
    assert!(!result.stderr.text.contains(CANARY_ENV));
    assert!(!result.stderr.text.contains(CANARY_STDIN));
}

/// Not itself one of the five acceptance bullets, but every mode
/// documented at the top of `fake_harness.sh` (`version`,
/// `unknown_version`, `malformed`) is exercised here at least once, so a
/// regression in the shared fixture is caught here rather than
/// discovered later by whichever adapter reaches for it first.
#[tokio::test]
async fn every_documented_fixture_mode_behaves_as_documented() {
    let limits = generous_limits();

    let workspace_dir = temp_workspace("mode-version");
    let mut env = env_with_mode("version");
    env.insert("TACK_FAKE_HARNESS_VERSION".to_owned(), "9.9.9".to_owned());
    let result = run_default(workspace_dir.path(), env, &limits).await;
    assert_eq!(result.exit, ProcessExit::Exited(0));
    assert!(result.stdout.text.contains("9.9.9"));

    let workspace_dir = temp_workspace("mode-unknown-version");
    let result = run_default(
        workspace_dir.path(),
        env_with_mode("unknown_version"),
        &limits,
    )
    .await;
    assert_eq!(result.exit, ProcessExit::Exited(0));
    assert!(result.stdout.text.contains("999.999.999"));

    let workspace_dir = temp_workspace("mode-malformed");
    let result = run_default(workspace_dir.path(), env_with_mode("malformed"), &limits).await;
    assert_eq!(result.exit, ProcessExit::Exited(0));
    assert!(
        serde_json::from_str::<serde_json::Value>(&result.stdout.text).is_err(),
        "malformed mode must actually produce unparseable output"
    );
}

/// `pub(crate)`: `harness::tests`'s own cross-adapter descendant-tree
/// cancellation acceptance test polls for the same grandchild pidfile
/// this module's own primitive-level test does, and shares this exact
/// polling logic rather than a second copy.
pub(crate) async fn wait_for_pidfile(path: &Path) -> u32 {
    tack_test_support::poll_until(Duration::from_secs(5), async || {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|contents| contents.trim().parse::<u32>().ok())
    })
    .await
    .unwrap_or_else(|| panic!("grandchild pidfile was never written: {}", path.display()))
}

/// `pub(crate)`: shared with `harness::tests`'s cross-adapter descendant-tree
/// cancellation test, for the same reason as [`wait_for_pidfile`] above.
pub(crate) async fn wait_until_dead(pid: u32, budget: Duration) -> bool {
    tack_test_support::poll_until(budget, async || (!process_alive(pid)).then_some(()))
        .await
        .is_some()
        || !process_alive(pid)
}
