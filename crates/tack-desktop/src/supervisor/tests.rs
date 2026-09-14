use super::*;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, Command, Stdio};

/// A `std::process::Child`-backed [`SidecarHandle`] for tests.
#[derive(Debug)]
struct ChildHandle(Child);

impl SidecarHandle for ChildHandle {
    fn pid(&self) -> u32 {
        self.0.id()
    }

    fn kill(mut self) -> std::io::Result<()> {
        // `Child::kill` only sends the signal; without `wait` the process
        // stays a zombie (a liveness check would still see it as
        // "alive") until reaped. The real sidecar reaps internally, so
        // this compensates only for the plain `std::process::Child` used
        // here.
        self.0.kill()?;
        self.0.wait()?;
        Ok(())
    }

    fn exited(&mut self) -> Option<ExitReport> {
        match self.0.try_wait() {
            Ok(Some(status)) => Some(ExitReport {
                code: status.code(),
                #[cfg(unix)]
                signal: status.signal(),
                #[cfg(not(unix))]
                signal: None,
            }),
            _ => None,
        }
    }
}

/// Spawns a fixed script (a fake sidecar) that ignores argv/env content
/// except the port it must bind, so tests need no webview and no real
/// `tack` binary.
struct ScriptLauncher {
    script: PathBuf,
    port: u16,
    /// Poisons the next spawn to simulate a launch failure.
    fail_next: bool,
}

impl SidecarLauncher for ScriptLauncher {
    type Process = ChildHandle;

    fn spawn(&self, env: &[(String, String)]) -> Result<Self::Process, SupervisorError> {
        if self.fail_next {
            return Err(SupervisorError::SpawnFailed("poisoned for test".into()));
        }
        let mut cmd = Command::new("python3");
        cmd.arg(&self.script)
            .arg(self.port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.spawn()
            .map(ChildHandle)
            .map_err(|e| SupervisorError::SpawnFailed(e.to_string()))
    }
}

/// Writes a tiny Python HTTP server that answers `/api/health` on the port
/// given as argv[1], reporting `version`. Python ships on every CI image
/// this repo already targets; nothing here depends on the real `tack`
/// binary.
fn write_fake_sidecar(dir: &std::path::Path, version: &str) -> PathBuf {
    let path = dir.join("fake-tack.py");
    let mut f = std::fs::File::create(&path).unwrap();
    write!(
        f,
        r#"import http.server, json, sys

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/api/health":
            body = json.dumps({{"status": "ok", "version": "{version}"}}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        else:
            self.send_response(404)
            self.end_headers()

    def log_message(self, *args):
        pass

port = int(sys.argv[1])
http.server.HTTPServer(("127.0.0.1", port), Handler).serve_forever()
"#
    )
    .unwrap();
    path
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Spawns `script` by hand, as an operator would before launching the app,
/// and waits until it actually answers `/api/health` — the fixture every
/// "attach to something already running" test needs before it can prove
/// the supervisor never spawns a second one.
async fn spawn_and_wait_healthy(
    script: &std::path::Path,
    port: u16,
    client: &reqwest::Client,
    base_url: &str,
) -> Child {
    let child = Command::new("python3")
        .arg(script)
        .arg(port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut interval = tokio::time::interval(Duration::from_millis(50));
    for _ in 0..50 {
        interval.tick().await;
        if probe_health(client, base_url).await.is_some() {
            break;
        }
    }
    child
}

fn folders(root: &std::path::Path) -> ServerFolders {
    ServerFolders {
        database_url: format!("sqlite:{}/tack.db?mode=rwc", root.display()),
        storage_dir: root.join("storage"),
        runner_state_dir: root.join("runner"),
        log_file: root.join("logs/tack.log"),
    }
}

#[tokio::test]
async fn spawns_and_becomes_healthy_when_nothing_is_listening() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_fake_sidecar(tmp.path(), "0.0.0-fake");
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();
    let launcher = ScriptLauncher {
        script,
        port,
        fail_next: false,
    };

    let outcome = attach_or_start(
        &client,
        &base_url,
        port,
        &launcher,
        &folders(tmp.path()),
        "0.0.0-fake",
    )
    .await
    .expect("supervisor should spawn and observe health");

    let (health, process) = match outcome {
        Outcome::Started { health, process } => (health, process),
        Outcome::Attached { .. } => panic!("nothing was listening; must not attach"),
    };
    assert_eq!(health.status, "ok");
    assert_eq!(health.version, "0.0.0-fake");

    let pid = process.pid();
    shutdown(process).expect("shutdown should succeed");

    // SAFETY: liveness probe only, same as the production shutdown path.
    #[cfg(unix)]
    {
        let alive = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
        assert!(!alive, "child pid {pid} must not survive shutdown");
    }
}

#[tokio::test]
async fn attaches_without_spawning_when_something_already_answers() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_fake_sidecar(tmp.path(), "0.0.0-fake");
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();

    // The supervisor must attach to this hand-started server, not spawn a
    // second one.
    let mut hand_started = spawn_and_wait_healthy(&script, port, &client, &base_url).await;

    let launcher = ScriptLauncher {
        script,
        port,
        fail_next: true, // spawning must never be attempted
    };
    let outcome = attach_or_start(
        &client,
        &base_url,
        port,
        &launcher,
        &folders(tmp.path()),
        "0.0.0-fake",
    )
    .await
    .expect("supervisor should attach");

    match outcome {
        Outcome::Attached { health } => assert_eq!(health.status, "ok"),
        Outcome::Started { .. } => panic!("something already answered; must not spawn"),
    }

    // The hand-started server must still be running — attach never signals it.
    assert!(
        hand_started.try_wait().unwrap().is_none(),
        "attach must not touch a server it did not start"
    );
    hand_started.kill().unwrap();
    let _ = hand_started.wait();
}

#[tokio::test]
async fn reports_spawn_failure_instead_of_hanging() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_fake_sidecar(tmp.path(), "0.0.0-fake");
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();
    let launcher = ScriptLauncher {
        script,
        port,
        fail_next: true,
    };

    let err = attach_or_start(
        &client,
        &base_url,
        port,
        &launcher,
        &folders(tmp.path()),
        "0.0.0-fake",
    )
    .await
    .expect_err("a poisoned launcher must surface an error, not hang");
    assert!(matches!(err, SupervisorError::SpawnFailed(_)));
}

#[tokio::test]
async fn refuses_to_spawn_when_the_port_is_held_by_something_else() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_fake_sidecar(tmp.path(), "0.0.0-fake");
    let client = reqwest::Client::new();

    // Something that is not Tack: a bare TCP listener that never answers
    // HTTP at all, let alone `/api/health`. Bound directly to port 0 and
    // held for the rest of the test, never `accept()`-ed — the kernel
    // completes the handshake for any number of connections up to the
    // backlog on its own, which is exactly the "port is open but nothing
    // Tack-shaped is behind it" case this guards. Calling `accept()` here
    // would service one connection and then, on thread exit, close the
    // listening socket out from under the second probe. Binding directly
    // here (rather than `free_port()` followed by a second bind to the
    // same number) avoids a race on a busy host where something else
    // takes the port in the gap between the two binds.
    let raw_listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = raw_listener.local_addr().unwrap().port();
    let base_url = format!("http://127.0.0.1:{port}");

    let launcher = ScriptLauncher {
        script,
        port,
        fail_next: true, // spawning must never be attempted
    };
    let err = attach_or_start(
        &client,
        &base_url,
        port,
        &launcher,
        &folders(tmp.path()),
        "0.0.0-fake",
    )
    .await
    .expect_err("a foreign listener on the port must be refused, not spawned into");
    assert!(matches!(err, SupervisorError::PortOccupiedByOther(p) if p == port));
}

#[tokio::test]
async fn refuses_to_attach_to_a_server_older_than_bundled() {
    let tmp = tempfile::tempdir().unwrap();
    let script = write_fake_sidecar(tmp.path(), "0.1.0-beta.1");
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();

    // A server started by hand, running an older release than this app bundles.
    let mut hand_started = spawn_and_wait_healthy(&script, port, &client, &base_url).await;

    let launcher = ScriptLauncher {
        script,
        port,
        fail_next: true, // spawning must never be attempted
    };
    let err = attach_or_start(
        &client,
        &base_url,
        port,
        &launcher,
        &folders(tmp.path()),
        "0.1.0-beta.7",
    )
    .await
    .expect_err("an attached server older than the bundled version must be refused");
    assert!(matches!(
        err,
        SupervisorError::OutdatedServer { ref server_version, ref bundled_version }
            if server_version == "0.1.0-beta.1" && bundled_version == "0.1.0-beta.7"
    ));

    // Refusing to use it must not touch it — same rule as any other attach.
    assert!(
        hand_started.try_wait().unwrap().is_none(),
        "refusing an outdated server must not touch it"
    );
    hand_started.kill().unwrap();
    let _ = hand_started.wait();
}

#[test]
fn check_server_version_flags_a_strictly_older_semver() {
    assert_eq!(
        check_server_version("0.1.0-beta.1", "0.1.0-beta.7"),
        VersionCheck::Outdated
    );
}

#[test]
fn check_server_version_orders_prerelease_numbers_numerically() {
    // A naive string compare would put "beta.10" before "beta.9" —
    // semver orders numeric prerelease identifiers as numbers, not text.
    assert_eq!(
        check_server_version("0.1.0-beta.9", "0.1.0-beta.10"),
        VersionCheck::Outdated
    );
    assert_eq!(
        check_server_version("0.1.0-beta.10", "0.1.0-beta.9"),
        VersionCheck::Compatible
    );
}

#[test]
fn check_server_version_treats_equal_and_newer_as_compatible() {
    assert_eq!(
        check_server_version("0.1.0-beta.7", "0.1.0-beta.7"),
        VersionCheck::Compatible
    );
    assert_eq!(
        check_server_version("0.2.0", "0.1.0-beta.7"),
        VersionCheck::Compatible
    );
}

#[test]
fn check_server_version_is_unknown_when_unparseable() {
    assert_eq!(
        check_server_version("not-a-version", "0.1.0-beta.7"),
        VersionCheck::Unknown
    );
    assert_eq!(
        check_server_version("0.1.0-beta.7", "also-not-a-version"),
        VersionCheck::Unknown
    );
}

#[test]
fn started_server_exit_is_reported_exactly_once() {
    let report = ExitReport {
        code: Some(1),
        signal: None,
    };
    let (state, event) = watch_tick(
        WatchState::default(),
        ServerKind::Started,
        true,
        Some(report),
    );
    assert_eq!(event, Some(WatchEvent::StartedExited(report)));

    // The child keeps reporting the same exit (or the caller keeps
    // asking) — already notified, so nothing fires a second time.
    let (_, event) = watch_tick(state, ServerKind::Started, true, Some(report));
    assert_eq!(event, None);
}

#[test]
fn started_server_that_keeps_answering_changes_nothing() {
    let mut state = WatchState::default();
    for _ in 0..10 {
        let (next, event) = watch_tick(state, ServerKind::Started, true, None);
        assert_eq!(event, None);
        state = next;
    }
    assert_eq!(state, WatchState::default());
}

#[test]
fn four_missed_attached_polls_do_not_trigger_unresponsive() {
    let mut state = WatchState::default();
    for _ in 0..4 {
        let (next, event) = watch_tick(state, ServerKind::Attached, false, None);
        assert_eq!(event, None);
        state = next;
    }
}

#[test]
fn five_missed_attached_polls_trigger_unresponsive() {
    let mut state = WatchState::default();
    let mut fired = None;
    for _ in 0..5 {
        let (next, event) = watch_tick(state, ServerKind::Attached, false, None);
        state = next;
        if event.is_some() {
            fired = event;
        }
    }
    assert_eq!(fired, Some(WatchEvent::AttachedUnresponsive));
}

#[test]
fn attached_server_recovers_after_failure_no_second_dialog() {
    let mut state = WatchState::default();
    for _ in 0..5 {
        let (next, _) = watch_tick(state, ServerKind::Attached, false, None);
        state = next;
    }
    let (recovered_state, event) = watch_tick(state, ServerKind::Attached, true, None);
    assert_eq!(event, Some(WatchEvent::AttachedRecovered));
    assert_eq!(recovered_state, WatchState::default());

    // A later, fresh episode of missing polls still only needs five
    // ticks to fire again -- recovery must not have left the streak
    // counter or the notified flag stuck.
    let mut state = recovered_state;
    let mut fired = None;
    for _ in 0..5 {
        let (next, event) = watch_tick(state, ServerKind::Attached, false, None);
        state = next;
        if event.is_some() {
            fired = event;
        }
    }
    assert_eq!(fired, Some(WatchEvent::AttachedUnresponsive));
}

#[test]
fn unknown_kind_resets_any_carried_state() {
    let dirty = WatchState {
        missing_streak: 3,
        notified: true,
    };
    let (state, event) = watch_tick(dirty, ServerKind::Unknown, false, None);
    assert_eq!(state, WatchState::default());
    assert_eq!(event, None);
}
