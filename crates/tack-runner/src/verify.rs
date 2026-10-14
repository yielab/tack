//! Runs the operator's verifier over one succeeded attempt's evidence.
//!
//! The verifier is a program the operator names in the runner's `[verify]`
//! table; the board never runs it. [`run`] copies the staged evidence into
//! `<scratch>/evidence`, runs the program under the runner's process limits
//! and the configured timeout with an environment of `PATH` only, and stages
//! the `mrp-v1` pack that results as `<scratch>/evidence/mrp.json` (kind
//! `mrp`) for the engine's ordinary upload. Every failure is a
//! [`VerifyFailure`]; none of them changes the attempt's outcome.
//!
//! Two kinds of program ([`VerifyKind`]):
//!
//! - `command`: `<program> <args…> --evidence <scratch>/evidence --workspace
//!   <workspace> --output <scratch>/evidence/mrp.json`, and the pack it
//!   writes is read through [`tack_core::mrp`] as it is.
//! - `prill`: the program is prill, whose command line, input documents and
//!   pack are all its own; [`prill`] is the seam that snapshots the
//!   workspace, writes prill's documents, runs `<program> verify …` and
//!   translates the pack it prints.

mod prill;

/// The brief adapter, public so `tests/briefing_schema.rs` can check it against prill's schema.
pub use prill::briefing;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use tack_core::mrp::MergeReadinessPack;

use crate::{
    client::workspace::Workspace,
    config::{VerifyConfig, VerifyKind},
    harness::{
        artifact::ArtifactStager,
        process::{ProcessExit, ProcessLimits, ProcessResult, ProcessSpec},
        redact::SecretMaterial,
    },
};

/// The media type of the staged pack.
pub const MRP_MEDIA_TYPE: &str = "application/vnd.tack.mrp+json";

/// How many characters of the verifier's stderr a failure keeps.
const STDERR_PREFIX_CHARS: usize = 512;

/// Why a verification produced no pack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyFailure {
    /// The verifier's exit code, when it exited on its own.
    pub exit_code: Option<i32>,
    pub reason: String,
}

fn failure(exit_code: Option<i32>, reason: impl Into<String>) -> VerifyFailure {
    VerifyFailure {
        exit_code,
        reason: reason.into(),
    }
}

/// Whether `program` resolves: a path with a separator is looked at directly,
/// a bare name is looked for in every `PATH` directory.
pub fn program_found(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    #[allow(clippy::disallowed_methods)]
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).any(|directory| directory.join(program).is_file())
}

/// Runs the verifier for one attempt. `scratch` is the evidence capture's
/// scratch directory; its `src` holds the files already staged. Returns the
/// staged `mrp.json`, in the shape `terminal_reason.artifacts` entries have.
pub async fn run(
    config: &VerifyConfig,
    limits: &ProcessLimits,
    workspace: &Workspace,
    scratch: &Path,
) -> Result<serde_json::Value, VerifyFailure> {
    let evidence = scratch.join("evidence");
    fs::create_dir_all(&evidence)
        .map_err(|_| failure(None, "the evidence directory could not be created"))?;
    if let Ok(entries) = fs::read_dir(scratch.join("src")) {
        for entry in entries.flatten() {
            let _ = fs::copy(entry.path(), evidence.join(entry.file_name()));
        }
    }
    let output = evidence.join("mrp.json");

    match config.kind {
        VerifyKind::Command => run_command(config, limits, workspace, &evidence, &output).await?,
        VerifyKind::Prill => prill::run(config, limits, workspace, &evidence, &output).await?,
    }

    let attempt_id = workspace.attempt_id.as_str();
    let stager = ArtifactStager::new(scratch.join("staged"));
    let artifact = stager
        .stage_file(
            attempt_id,
            &evidence,
            Path::new("mrp.json"),
            "mrp",
            MRP_MEDIA_TYPE,
        )
        .map_err(|_| failure(Some(0), "the pack could not be staged"))?;
    Ok(serde_json::json!({
        "kind": artifact.kind,
        "name": artifact.name,
        "media_type": artifact.media_type,
        "size_bytes": artifact.size_bytes,
        "sha256": artifact.sha256,
        "staged_path": artifact.staged_path.display().to_string(),
    }))
}

/// `kind = "command"`: the program reads the evidence directory and writes
/// the pack to `output` itself.
async fn run_command(
    config: &VerifyConfig,
    limits: &ProcessLimits,
    workspace: &Workspace,
    evidence: &Path,
    output: &Path,
) -> Result<(), VerifyFailure> {
    let mut args = config.args.clone();
    args.extend([
        "--evidence".to_owned(),
        evidence.display().to_string(),
        "--workspace".to_owned(),
        workspace.path.display().to_string(),
        "--output".to_owned(),
        output.display().to_string(),
    ]);
    let result = spawn_verifier(config, limits, workspace, args).await?;
    let stderr = stderr_prefix(&result);
    match result.exit {
        ProcessExit::Exited(0) => {}
        ProcessExit::Exited(code) => {
            return Err(failure(
                Some(code),
                format!("the verifier exited with code {code}: {stderr}"),
            ));
        }
        #[cfg(unix)]
        ProcessExit::Signaled(signal) => {
            return Err(failure(
                None,
                format!("the verifier was stopped by signal {signal}: {stderr}"),
            ));
        }
        ProcessExit::TimedOut => {
            return Err(failure(
                None,
                format!(
                    "the verifier did not finish within {} seconds",
                    config.timeout_seconds
                ),
            ));
        }
    }

    let bytes = fs::read(output).map_err(|_| failure(Some(0), "the verifier wrote no pack"))?;
    if serde_json::from_slice::<MergeReadinessPack>(&bytes).is_err() {
        return Err(failure(
            Some(0),
            format!("the verifier's pack does not parse: {stderr}"),
        ));
    }
    Ok(())
}

/// Starts `config.program` with `args` in the workspace, `PATH` as its only
/// environment variable, under `limits` with the configured timeout, and
/// waits for it.
async fn spawn_verifier(
    config: &VerifyConfig,
    limits: &ProcessLimits,
    workspace: &Workspace,
    args: Vec<String>,
) -> Result<ProcessResult, VerifyFailure> {
    #[allow(clippy::disallowed_methods)]
    let path = std::env::var("PATH").unwrap_or_default();
    let spec = ProcessSpec {
        program: PathBuf::from(&config.program),
        args,
        env: BTreeMap::from([("PATH".to_owned(), path)]),
        stdin: None,
        working_directory: workspace.path.clone(),
        workspace_root: workspace.path.clone(),
        keep_stdin_open: false,
    };
    let limits = ProcessLimits {
        timeout: Duration::from_secs(config.timeout_seconds),
        ..limits.clone()
    };
    let process = spec
        .spawn()
        .await
        .map_err(|_| failure(None, format!("{} could not be started", config.program)))?;
    process
        .wait_with_capture(&limits, &SecretMaterial::new())
        .await
        .map_err(|_| failure(None, "the verifier could not be supervised"))
}

fn stderr_prefix(result: &ProcessResult) -> String {
    result
        .stderr
        .text
        .chars()
        .take(STDERR_PREFIX_CHARS)
        .collect()
}

#[cfg(test)]
#[path = "verify/tests.rs"]
mod tests;
