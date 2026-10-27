//! Shared fixture for this crate's real-subprocess tests.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tack_test_support::poll_until_sync;

/// A spawned `tack serve`: the process, the address it reported binding, and its stdout lines.
pub struct Served {
    pub child: Child,
    pub base_url: String,
    pub log_lines: Arc<Mutex<Vec<String>>>,
}

/// Spawns `command` (a `tack serve` the caller has given its environment) on port 0 and waits for
/// the "Server listening" line to name the port the kernel gave it, so no two servers can ever be
/// handed the same one. Stdout is drained into `log_lines` as it is produced, not after the
/// process exits, so a verbose boot can never fill the pipe buffer and block the child on a write.
/// `RUST_LOG` is pinned to the `tack` targets so an ambient value cannot hide the line.
pub fn spawn_serve(command: &mut Command) -> Served {
    let mut child = command
        .env("TACK_HOST", "127.0.0.1")
        .env("TACK_PORT", "0")
        .env("RUST_LOG", "tack=info")
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn tack serve");
    let stdout = child.stdout.take().expect("piped stdout");
    let log_lines = Arc::new(Mutex::new(Vec::new()));
    let writer = Arc::clone(&log_lines);
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            writer.lock().expect("log lines lock").push(line);
        }
    });

    let port = poll_until_sync(Duration::from_secs(30), || {
        if let Some(status) = child.try_wait().expect("poll child status") {
            panic!("tack serve exited before it reported its address: {status}");
        }
        let lines = log_lines.lock().expect("log lines lock");
        lines
            .iter()
            .find(|line| line.contains("Server listening"))
            .and_then(|line| bound_port(line))
    });
    let Some(port) = port else {
        let _ = child.kill();
        panic!("tack serve did not report its address within 30s");
    };
    Served {
        child,
        base_url: format!("http://127.0.0.1:{port}"),
        log_lines,
    }
}

/// The port in `addr=127.0.0.1:<port>`; the digits end where the log line's colour codes begin.
fn bound_port(line: &str) -> Option<u16> {
    let (_, rest) = line.split_once("127.0.0.1:")?;
    let digits = rest.split(|c: char| !c.is_ascii_digit()).next()?;
    digits.parse().ok()
}

/// The operator token every spawned server in these tests is started with.
pub const API_TOKEN: &str = "test-operator-token";

/// A blocking client that sends [`API_TOKEN`] as a Bearer credential.
pub fn authed_client() -> reqwest::blocking::Client {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {API_TOKEN}").parse().expect("header value"),
    );
    reqwest::blocking::Client::builder()
        .default_headers(headers)
        .build()
        .expect("client")
}
