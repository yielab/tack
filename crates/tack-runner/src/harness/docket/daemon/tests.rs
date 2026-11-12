//! The daemon against a fake `docket`: a shell script that serves `/health`
//! after a delay. It never reaches a real docket or a model.

use std::{os::unix::fs::PermissionsExt, path::Path, time::Duration};

use super::{Options, StartFlags, ensure_running_with};
use crate::config::DocketConfig;

const PYTHON_HEALTH: &str = r#"import sys, http.server as h
class H(h.BaseHTTPRequestHandler):
    def do_GET(s):
        s.send_response(200); s.send_header("content-length", "2"); s.end_headers(); s.wfile.write(b"{}")
    def log_message(*a): pass
h.HTTPServer(("127.0.0.1", int(sys.argv[1])), H).serve_forever()"#;

/// A `docket` that records each start, answers `/health` 0.3 s later, exits
/// when `die.<n>` appears (n = which start it is) and records a `SIGTERM`.
/// Its `start --help` lists `--port` and `options`.
fn fake_docket_with(dir: &Path, options: &[&str]) -> std::path::PathBuf {
    let script = dir.join("docket");
    let help = ["Options:", "--port"]
        .into_iter()
        .chain(options.iter().copied())
        .map(|option| format!("  {option}  what it does"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(
        &script,
        format!(
            r#"#!/bin/sh
if [ "$2" = --help ]; then printf '%s\n' "{help}"; exit 0; fi
echo "$@" >> "{dir}/starts"
port=0
while [ $# -gt 0 ]; do
  if [ "$1" = --port ]; then port=$2; fi
  shift
done
n=$(wc -l < "{dir}/starts" | tr -d ' ')
sleep 0.3
python3 -c '{python}' "$port" &
pid=$!
trap 'echo term >> "{dir}/signals"; kill $pid; exit 0' TERM
while kill -0 $pid 2>/dev/null; do
  if [ -e "{dir}/die.$n" ]; then kill $pid; exit 3; fi
  sleep 0.1
done
"#,
            dir = dir.display(),
            python = PYTHON_HEALTH,
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

fn fake_docket(dir: &Path) -> std::path::PathBuf {
    fake_docket_with(dir, &["--dispatch"])
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn lines(path: impl AsRef<Path>) -> usize {
    std::fs::read_to_string(path).map_or(0, |text| text.lines().count())
}

fn quick() -> Options {
    Options {
        ready_timeout: Duration::from_secs(15),
        backoff: Duration::from_millis(100),
        grace: Duration::from_secs(10),
    }
}

async fn answers(port: u16) -> bool {
    super::healthy(&super::http(), &format!("http://127.0.0.1:{port}")).await
}

async fn until(what: &str, mut ready: impl AsyncFnMut() -> bool) {
    for _ in 0..200 {
        if ready().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("never happened: {what}");
}

#[tokio::test]
async fn spawned_once_restarted_once_and_sent_sigterm_on_shutdown() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let config = DocketConfig {
        binary: Some(fake_docket(dir.path())),
        port,
        home: Some(dir.path().join("home")),
        ..DocketConfig::default()
    };
    let handle = ensure_running_with(&config, &dir.path().join("logs"), quick()).await;
    assert!(handle.spawned());
    assert!(answers(port).await, "ready before ensure_running returns");
    assert_eq!(lines(dir.path().join("starts")), 1);
    let first_line = std::fs::read_to_string(dir.path().join("starts")).unwrap();
    assert!(
        first_line.starts_with(&format!("start --dispatch --bind 127.0.0.1 --port {port}")),
        "{first_line}"
    );

    std::fs::write(dir.path().join("die.1"), "").unwrap();
    until("the second start", async || {
        lines(dir.path().join("starts")) == 2 && answers(port).await
    })
    .await;

    handle.shutdown().await;
    assert_eq!(lines(dir.path().join("starts")), 2, "no third start");
    assert_eq!(lines(dir.path().join("signals")), 1, "one SIGTERM");
    assert!(!answers(port).await);
}

#[tokio::test]
async fn a_docket_that_runs_its_queues_by_itself_is_started_in_the_foreground_without_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let config = DocketConfig {
        binary: Some(fake_docket_with(dir.path(), &["--foreground"])),
        port,
        home: Some(dir.path().join("home")),
        ..DocketConfig::default()
    };
    let handle = ensure_running_with(&config, &dir.path().join("logs"), quick()).await;
    assert!(answers(port).await);
    let first_line = std::fs::read_to_string(dir.path().join("starts")).unwrap();
    assert!(
        first_line.starts_with(&format!(
            "start --foreground --bind 127.0.0.1 --port {port}"
        )),
        "{first_line}"
    );
    handle.shutdown().await;
    assert_eq!(lines(dir.path().join("signals")), 1, "one SIGTERM");
}

#[tokio::test]
async fn never_spawned_when_base_url_is_set_or_something_already_answers() {
    let dir = tempfile::tempdir().unwrap();
    let port = free_port();
    let by_url = DocketConfig {
        binary: Some(fake_docket(dir.path())),
        port,
        base_url: Some(format!("http://127.0.0.1:{port}")),
        ..DocketConfig::default()
    };
    let handle = ensure_running_with(&by_url, &dir.path().join("logs"), quick()).await;
    assert!(!handle.spawned());
    handle.shutdown().await;
    assert_eq!(lines(dir.path().join("starts")), 0);

    // One daemon is up (started by hand); a runner beside it adopts it.
    let by_hand = DocketConfig {
        base_url: None,
        ..by_url
    };
    let running = ensure_running_with(&by_hand, &dir.path().join("logs"), quick()).await;
    assert!(running.spawned());
    let adopter = ensure_running_with(&by_hand, &dir.path().join("logs"), quick()).await;
    assert!(!adopter.spawned());
    adopter.shutdown().await;
    assert_eq!(lines(dir.path().join("starts")), 1);
    assert!(answers(port).await, "adopting then leaving stops nothing");
    running.shutdown().await;
}

/// The options part of `docket start --help` as docket prints it: the prose
/// above names flags too, and only the list counts.
const PROSE: &str = "
 Usage: python -m docket start [OPTIONS]

 --dispatch also runs every pod's queue (spends budget), --telegram polls the
 Telegram bot, --foreground runs it here and blocks (no manager, no unit),

 Example: docket start --dispatch
";

fn help(options: &[&str]) -> String {
    let rows: String = options
        .iter()
        .map(|option| format!("│ {option:<22} <bool>  What it does.   │\n"))
        .collect();
    format!("{PROSE}╭─ Options ─╮\n│ --port        -p      <int>  Port to bind │\n{rows}╰───╯\n")
}

#[test]
fn the_start_flags_follow_the_options_docket_lists_not_its_prose() {
    let both = StartFlags {
        foreground: true,
        dispatch: true,
    };
    assert_eq!(
        StartFlags::from_help(&help(&["--dispatch"])),
        StartFlags::UNASKED
    );
    assert_eq!(
        StartFlags::from_help(&help(&["--dispatch", "--foreground"])),
        both
    );
    assert_eq!(
        StartFlags::from_help(&help(&["--foreground"])),
        StartFlags {
            foreground: true,
            dispatch: false,
        },
        "the prose still names --dispatch"
    );
    assert_eq!(
        StartFlags::from_help(
            "Options:\n  --port INTEGER\n  --dispatch / --no-dispatch\n  -f, --foreground\n"
        ),
        both,
        "click's plain list"
    );
    assert_eq!(StartFlags::from_help(""), StartFlags::UNASKED);
    assert_eq!(
        StartFlags::from_help(PROSE),
        StartFlags::UNASKED,
        "a help with no --port is not read"
    );
}
