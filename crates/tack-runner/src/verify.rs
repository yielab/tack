//! Runs the operator's verifier over one succeeded attempt's evidence.
//!
//! The verifier is a program the operator names in the runner's `[verify]`
//! table; the board never runs it. [`run`] copies the staged evidence into
//! `<scratch>/evidence`, runs `<program> <args…> --evidence <scratch>/evidence
//! --workspace <workspace> --output <scratch>/evidence/mrp.json` under the
//! runner's process limits and the configured timeout with an environment of
//! `PATH` only, reads the pack it wrote through [`tack_core::mrp`], and stages
//! `mrp.json` (kind `mrp`) for the engine's ordinary upload. Every failure is
//! a [`VerifyFailure`]; none of them changes the attempt's outcome.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use tack_core::mrp::MergeReadinessPack;

use crate::{
    client::workspace::Workspace,
    config::VerifyConfig,
    harness::{
        artifact::ArtifactStager,
        process::{ProcessExit, ProcessLimits, ProcessSpec},
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

    let mut args = config.args.clone();
    args.extend([
        "--evidence".to_owned(),
        evidence.display().to_string(),
        "--workspace".to_owned(),
        workspace.path.display().to_string(),
        "--output".to_owned(),
        output.display().to_string(),
    ]);
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
    let result = process
        .wait_with_capture(&limits, &SecretMaterial::new())
        .await
        .map_err(|_| failure(None, "the verifier could not be supervised"))?;
    let stderr: String = result
        .stderr
        .text
        .chars()
        .take(STDERR_PREFIX_CHARS)
        .collect();
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

    let bytes = fs::read(&output).map_err(|_| failure(Some(0), "the verifier wrote no pack"))?;
    if serde_json::from_slice::<MergeReadinessPack>(&bytes).is_err() {
        return Err(failure(
            Some(0),
            format!("the verifier's pack does not parse: {stderr}"),
        ));
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

#[cfg(test)]
#[path = "verify/tests.rs"]
mod tests;
