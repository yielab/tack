//! What the located `docket` binary can do, measured once. The probes are
//! all `exec` invocations that docket refuses before any network
//! call, because `DOCKET_HOME` is deliberately left unset: zero spend, no
//! model endpoint, no state written. Why these probes and not others is in
//! `fixtures/docket/README.md`.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long one probe may take; a refusal is immediate, so this only bounds
/// a binary that is not docket at all.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// The harness contract this docket speaks when asked for the newest one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Contract {
    #[default]
    V1_0,
    V1_1,
}

impl Contract {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V1_0 => "1.0",
            Self::V1_1 => "1.1",
        }
    }
}

/// The default is "nothing negotiated": contract 1.0 and no optional flag,
/// which is exactly what an unprobed or unlocatable docket is treated as.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocketFeatures {
    pub contract: Contract,
    pub answers: bool,
    pub token_file: bool,
    pub max_tokens: bool,
    pub policy: bool,
    pub recipe: bool,
    /// What `docket --version` printed, or empty when it printed nothing
    /// recognizable. Reported, never compared.
    pub version: String,
}

impl DocketFeatures {
    /// Runs `program` a handful of times (in parallel) and reads what it
    /// refuses. The flags are only probed on a docket that speaks 1.1,
    /// since every one of them is a 1.1 flag.
    pub fn probe(program: &Path) -> Self {
        let version = run(program, &["--version"])
            .and_then(|out| {
                super::super::local_process::parse_version(out.trim()).map(str::to_owned)
            })
            .unwrap_or_default();
        let speaks_1_1 = refusal(program, &["--contract", "1.1"]).is_some_and(|r| r.v == "1.1.0");
        if !speaks_1_1 {
            return Self {
                version,
                ..Self::default()
            };
        }
        let flag = |name: &'static str, value: &'static str| {
            refusal(program, &["--contract", "1.0", name, value])
                .is_some_and(|r| r.error.contains(name))
        };
        let (answers, token_file, max_tokens, policy, recipe) = std::thread::scope(|scope| {
            let answers = scope.spawn(|| flag("--answers", "stdin"));
            let token_file = scope.spawn(|| flag("--token-file", "/x"));
            let max_tokens = scope.spawn(|| flag("--max-tokens", "1"));
            let policy = scope.spawn(|| flag("--policy", "/x"));
            let recipe = scope.spawn(|| flag("--recipe", "x"));
            let join =
                |handle: std::thread::ScopedJoinHandle<'_, bool>| handle.join().unwrap_or(false);
            (
                join(answers),
                join(token_file),
                join(max_tokens),
                join(policy),
                join(recipe),
            )
        });
        Self {
            contract: Contract::V1_1,
            answers,
            token_file,
            max_tokens,
            policy,
            recipe,
            version,
        }
    }
}

struct Refusal {
    v: String,
    error: String,
}

/// `exec` with no `DOCKET_HOME`, plus `extra`: docket answers with
/// one `refused` result line (exit 2) after reading its flags and before
/// doing anything else. `None` when the output was not such a line.
fn refusal(program: &Path, extra: &[&str]) -> Option<Refusal> {
    let mut args = vec![
        "exec",
        "--workspace",
        ".",
        "--task",
        "probe",
        "--model",
        "probe/probe",
    ];
    args.extend_from_slice(extra);
    let stdout = run(program, &args)?;
    let line = stdout.lines().rev().find(|line| line.starts_with('{'))?;
    let result: serde_json::Value = serde_json::from_str(line).ok()?;
    (result["status"] == "refused").then(|| Refusal {
        v: result["v"].as_str().unwrap_or_default().to_owned(),
        error: result["error"].as_str().unwrap_or_default().to_owned(),
    })
}

/// Stdout of `program args`, run with only `PATH` in its environment. Any
/// exit status counts: a refusal is a non-zero exit.
fn run(program: &Path, args: &[&str]) -> Option<String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut stdout, &mut text)
            .ok()
            .map(|_| text)
    });
    let started = Instant::now();
    while child.try_wait().ok()?.is_none() {
        if started.elapsed() > PROBE_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    reader.join().ok()?
}
