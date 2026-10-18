//! Acceptance proof that `EmbeddedRunnerControl::runner_id()` hands the API
//! the `runr_` id it assigned at enrolment, not the runner's configured name.
//! The only reader of that id is `POST /api/local-runner/test-run`, which files
//! an `exact_runner` request against it; the API refuses a selector it does not
//! know, so a 200 is the proof.
//!
//! One real `tack serve --with-runner` subprocess against a scratch HOME and a
//! `PATH` with no harness binary on it, so nothing can run.

use std::process::{Child, Command, Stdio};
use std::time::Duration;

use serde_json::{Value, json};
use tack_test_support::poll_until_sync as poll_until;

mod common;
use common::free_port;

struct ServerGuard {
    child: Child,
    base_url: String,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_server_with_runner(root: &std::path::Path) -> ServerGuard {
    let port = free_port();
    let home = root.join("home");
    std::fs::create_dir_all(&home).expect("scratch home");
    let child = Command::new(env!("CARGO_BIN_EXE_tack"))
        .arg("serve")
        .arg("--with-runner")
        .current_dir(root)
        .env("HOME", &home)
        .env("PATH", root.join("no-binaries"))
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/nonexistent/tack-no-keychain",
        )
        .env("TACK_HOST", "127.0.0.1")
        .env("TACK_PORT", port.to_string())
        .env("TACK_API_TOKEN", common::API_TOKEN)
        .env(
            "TACK_DATABASE_URL",
            format!("sqlite:{}/tack.db?mode=rwc", root.display()),
        )
        .env("TACK_STORAGE_DIR", root.join("storage"))
        .env_remove("TACK_RUNNER_STATE_DIR")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn tack serve --with-runner");
    ServerGuard {
        child,
        base_url: format!("http://127.0.0.1:{port}"),
    }
}

#[test]
fn the_embedded_runner_reports_the_id_the_api_assigned() {
    let root = tempfile::Builder::new()
        .prefix("embedded-runner-id")
        .tempdir()
        .expect("root");
    let server = start_server_with_runner(root.path());
    let client = common::authed_client();

    let enrolled = poll_until(Duration::from_secs(30), || {
        let body = client
            .get(format!("{}/api/runners", server.base_url))
            .timeout(Duration::from_millis(500))
            .send()
            .ok()?
            .json::<Value>()
            .ok()?;
        body.get("data")?
            .as_array()?
            .iter()
            .find(|row| row.get("state").and_then(Value::as_str) == Some("active"))
            .and_then(|row| row.get("runner_id").and_then(Value::as_str))
            .map(str::to_owned)
    })
    .expect("the embedded runner must reach `active`");
    assert!(
        enrolled.starts_with("runr_"),
        "the API assigns runr_ ids: {enrolled}"
    );

    // The API marks the runner `active` when it answers the enrolment; the
    // runner persists the session it reads `runner_id()` from a moment later,
    // so the first test-run can still find it "not enrolled". Retry only that.
    let (status, body) = poll_until(Duration::from_secs(30), || {
        let response = client
            .post(format!("{}/api/local-runner/test-run", server.base_url))
            .json(&json!({ "harness_kind": "claude-code" }))
            .timeout(Duration::from_secs(10))
            .send()
            .expect("POST test-run");
        let status = response.status();
        let body = response.json::<Value>().unwrap_or(Value::Null);
        let not_yet_enrolled = body["error"]["message"] == "The embedded runner is not enrolled";
        (!not_yet_enrolled).then_some((status, body))
    })
    .expect("the embedded runner must persist its enrolment");
    assert!(
        status.is_success(),
        "test-run is filed against runner_id(), which the API only accepts as an enrolled runr_ id: {status} {body}"
    );
    assert!(body["request_id"].is_string(), "{body}");
}

/// The embedded runner reports `local-runner` as its name on every refresh. A row the operator
/// named `local-runner` must not stop it from resuming its own session: the board's row owns the
/// name, so the runner is `running` after each restart on the same database.
#[test]
fn the_embedded_runner_resumes_while_another_row_holds_local_runner() {
    let root = tempfile::Builder::new()
        .prefix("embedded-runner-restart")
        .tempdir()
        .expect("root");
    let client = common::authed_client();
    let running = |server: &ServerGuard| {
        poll_until(Duration::from_secs(30), || {
            let body = client
                .get(format!("{}/api/local-runner", server.base_url))
                .timeout(Duration::from_millis(500))
                .send()
                .ok()?
                .json::<Value>()
                .ok()?;
            (body["state"] == "running").then_some(())
        })
        .expect("the embedded runner must be running");
        // A runner that cannot resume stops a moment after it started.
        std::thread::sleep(Duration::from_secs(2));
        let body = client
            .get(format!("{}/api/local-runner", server.base_url))
            .send()
            .expect("GET local-runner")
            .json::<Value>()
            .expect("json");
        assert_eq!(body["state"], "running", "{body}");
    };

    let first = start_server_with_runner(root.path());
    running(&first);
    let created = client
        .post(format!("{}/api/runners/enrollment", first.base_url))
        .json(&json!({"name": "local-runner", "total_capacity": 1, "available_capacity": 1}))
        .send()
        .expect("POST enrollment");
    assert!(created.status().is_success(), "{}", created.status());
    drop(first);

    for _ in 0..2 {
        let restarted = start_server_with_runner(root.path());
        running(&restarted);
    }
}
