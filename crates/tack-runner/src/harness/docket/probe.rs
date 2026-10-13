//! What the located `docket` binary can do, measured once. The probe is an
//! `exec` invocation that docket refuses before any network call, because
//! `DOCKET_HOME` is deliberately left unset: zero spend, no model endpoint,
//! no state written. Why this probe and not another is in
//! `fixtures/docket/README.md`.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long one probe may take; a refusal is immediate, so this only bounds
/// a binary that is not docket at all.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// The harness contract this grammar writes, and the version of docket that
/// first speaks it.
pub const CONTRACT: &str = "1.2";
pub const FIRST_VERSION: &str = "0.2.0-beta.5";

/// The default is what an unprobed or unlocatable docket is treated as: one
/// that does not speak the contract.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocketFeatures {
    /// docket answered the contract 1.2 probe with a 1.2 refusal.
    pub speaks_contract: bool,
    /// What `docket --version` printed, or empty when it printed nothing
    /// recognizable. Reported, never compared.
    pub version: String,
}

impl DocketFeatures {
    /// A docket that speaks contract 1.2, for tests and for a probe that
    /// found one.
    pub fn speaking(version: &str) -> Self {
        Self {
            speaks_contract: true,
            version: version.to_owned(),
        }
    }

    /// Runs `program --version` and the contract probe. A docket that
    /// implements contract 1.2 refuses `--contract 1.2` with no `DOCKET_HOME`
    /// on a `v: "1.2.0"` line; an older one refuses it on a `1.0.0` line
    /// whose error begins `--contract must be one of`.
    pub fn probe(program: &Path) -> Self {
        let version = run(program, &["--version"])
            .and_then(|out| {
                super::super::local_process::parse_version(out.trim()).map(str::to_owned)
            })
            .unwrap_or_default();
        Self {
            speaks_contract: speaks(run(
                program,
                &[
                    "exec",
                    "--workspace",
                    ".",
                    "--task",
                    "probe",
                    "--contract",
                    CONTRACT,
                ],
            )),
            version,
        }
    }

    /// Why this docket cannot run here, or `None`.
    pub fn unsupported(&self) -> Option<String> {
        (!self.speaks_contract).then(|| {
            let found = if self.version.is_empty() {
                String::new()
            } else {
                format!("; this one is {}", self.version)
            };
            format!(
                "needs docket {FIRST_VERSION} or later, which reads its own setup \
                 (harness contract {CONTRACT}){found}"
            )
        })
    }
}

/// Whether `stdout` ends with docket's refused result line for contract 1.2.
pub(super) fn speaks(stdout: Option<String>) -> bool {
    let Some(stdout) = stdout else {
        return false;
    };
    let Some(line) = stdout.lines().rev().find(|line| line.starts_with('{')) else {
        return false;
    };
    serde_json::from_str::<serde_json::Value>(line).is_ok_and(|result| {
        result["status"] == "refused" && result["v"] == format!("{CONTRACT}.0").as_str()
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
