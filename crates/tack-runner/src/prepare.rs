//! Installing a project's dependencies in the copy of the code an agent works in.
//!
//! The command comes from the repository's lockfile unless the project replaced or turned off the
//! install (`tack_protocol::prepare`). It runs in the checkout with the runner's own environment,
//! network included; the agent's sandbox is not involved. What npm, pnpm and yarn install into
//! `node_modules` is kept once, under the runner's state directory, and each checkout gets its own
//! tree of directories whose files are hard links to the kept ones, so the next attempt with the
//! same lockfile does not install again and the agent's sandbox, which sees only the checkout, can
//! read it.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use tack_protocol::execution::ExecutionRequestSnapshot;
use tack_protocol::prepare::{PrepareSetting, Resolution, Why};

use crate::harness::{
    process::{ProcessExit, ProcessLimits, ProcessSpec},
    redact::SecretMaterial,
    sha256::sha256_hex,
};

/// The folder a cached install fills.
const FOLDER: &str = "node_modules";

/// A file the runner puts in that folder to say it prepared it. The evidence of an attempt leaves
/// the folder out when it holds this file, because the install is not the agent's work.
const MARKER: &str = ".tack-prepared";

/// Output kept from a command, per stream.
const OUTPUT_BYTES: usize = 1024 * 1024;

/// How many of the last lines of a failed command's output the reason carries.
const REASON_LINES: usize = 40;

/// Why the dependencies could not be prepared; the text is the attempt's reason.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct PrepareError(String);

/// The tools that read and never run a command, compared case-insensitively.
const READ_ONLY_TOOLS: [&str; 5] = ["read", "grep", "glob", "webfetch", "websearch"];

/// The acceptance criteria that run something, and so need what the project installs.
const RUNNING_CRITERIA: [&str; 3] = ["command", "test", "metric"];

/// Why a run installs nothing, as the run's timeline says it.
pub const NO_INSTALL_REASON: &str = "No install: this profile runs no commands";

/// Whether `request` needs nothing installed: its tools are named and none of them runs a
/// command, and the item has no check that does. The tools are the run's own, else the profile's
/// for the run's agent; `*` or no list is not a list, and docket's pipeline decides for itself.
pub fn needs_no_install(request: &ExecutionRequestSnapshot) -> bool {
    let harness = request.requested_harness_kind.as_str();
    if harness == "docket" {
        return false;
    }
    let mut tools = request.permission_policy.tools.clone();
    if tools.is_empty() {
        tools = request.resolved_agent_profile.tool_policy["tools"][harness]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tool| tool.as_str().map(str::to_owned))
            .collect();
    }
    let reads_only = !tools.is_empty()
        && tools.iter().all(|tool| {
            READ_ONLY_TOOLS
                .iter()
                .any(|read| tool.eq_ignore_ascii_case(read))
        });
    let has_check = request
        .brief
        .as_ref()
        .and_then(|brief| brief["acceptance"].as_array())
        .into_iter()
        .flatten()
        .any(|criterion| {
            criterion["kind"]
                .as_str()
                .is_some_and(|kind| RUNNING_CRITERIA.contains(&kind))
        });
    reads_only && !has_check
}

/// Prepares `checkout` as `setting` says, within `timeout`. Nothing runs when the install is off
/// or the repository has no lockfile.
pub async fn prepare(
    checkout: &Path,
    setting: &PrepareSetting,
    state_dir: &Path,
    timeout: Duration,
) -> Result<(), PrepareError> {
    let files = root_files(checkout)
        .map_err(|error| PrepareError(format!("Could not read the checkout: {error}")))?;
    match setting.resolve(&files) {
        Resolution::Nothing(why) => {
            let why = match why {
                Why::Off => "the install is turned off",
                Why::NoLockfile => "the repository has no lockfile",
            };
            tracing::info!("nothing is installed: {why}");
            Ok(())
        }
        Resolution::Run { command, lockfile } => {
            install(checkout, &command, lockfile, state_dir, timeout).await
        }
    }
}

/// The names of the files at the checkout's root.
fn root_files(checkout: &Path) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(checkout)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(names)
}

/// The folders under `checkout` that the runner prepared.
pub fn prepared_folders(checkout: &Path) -> Vec<&'static str> {
    if checkout.join(FOLDER).join(MARKER).is_file() {
        vec![FOLDER]
    } else {
        Vec::new()
    }
}

/// Whether the install a lockfile implies is kept: poetry, uv and cargo keep their packages in a
/// cache of their own.
fn is_kept(lockfile: &str) -> bool {
    matches!(
        lockfile,
        "package-lock.json" | "pnpm-lock.yaml" | "yarn.lock"
    )
}

/// Runs `command`, or builds the install a previous attempt kept for the same lockfile.
async fn install(
    checkout: &Path,
    command: &str,
    lockfile: Option<&str>,
    state_dir: &Path,
    timeout: Duration,
) -> Result<(), PrepareError> {
    let installed = checkout.join(FOLDER);
    // A replaced command is the project's own and may do anything, so it always runs. A folder the
    // repository already holds is the project's own too.
    let kept = lockfile
        .filter(|lockfile| is_kept(lockfile))
        .filter(|_| fs::symlink_metadata(&installed).is_err());
    let Some(lockfile) = kept else {
        return run(checkout, command, timeout).await;
    };
    let Ok(lock) = fs::read(checkout.join(lockfile)) else {
        return run(checkout, command, timeout).await;
    };
    let cache = state_dir.join("prepare-cache");
    let entry = cache.join(sha256_hex(&lock));
    if link_tree(&entry.join(FOLDER), &installed).is_ok() {
        tracing::info!("reused the dependencies kept for {lockfile}");
        return mark(&installed);
    }
    // Nothing usable was kept, or it could not be linked: whatever was half built goes.
    let _ = fs::remove_dir_all(&installed);
    run(checkout, command, timeout).await?;
    keep(checkout, &cache, &entry);
    mark(&installed)
}

/// Keeps the install in the checkout as `entry`, built from hard links beside the cache's other
/// entries and renamed into place, so an entry appears whole or not at all. When another attempt
/// renamed first, theirs stays and this copy is dropped. The checkout keeps its own install either
/// way, so a failure here only means the next attempt installs again.
fn keep(checkout: &Path, cache: &Path, entry: &Path) {
    let staging = cache.join(format!(
        ".staging-{}",
        checkout
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    ));
    let _ = fs::remove_dir_all(&staging);
    let kept = fs::create_dir_all(&staging)
        .and_then(|()| link_tree(&checkout.join(FOLDER), &staging.join(FOLDER)))
        .and_then(|()| fs::rename(&staging, entry));
    if let Err(error) = kept {
        tracing::info!("the installed dependencies were not kept: {error}");
        let _ = fs::remove_dir_all(&staging);
    }
}

/// Says the runner prepared `installed`, with a file of its own (not a link to a kept one).
fn mark(installed: &Path) -> Result<(), PrepareError> {
    fs::write(installed.join(MARKER), "")
        .map_err(|error| PrepareError(format!("Could not prepare the dependencies: {error}")))
}

/// Builds `to` as a copy of the directory tree `from`: real directories, symlinks recreated, and
/// every file a hard link to the original.
fn link_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            link_tree(&entry.path(), &target)?;
        } else if kind.is_symlink() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(fs::read_link(entry.path())?, &target)?;
            #[cfg(not(unix))]
            return Err(io::ErrorKind::Unsupported.into());
        } else {
            fs::hard_link(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Runs `command` with `sh -c` in `checkout`, bounded by `timeout`.
async fn run(checkout: &Path, command: &str, timeout: Duration) -> Result<(), PrepareError> {
    let failed = |what: String, output: &str| {
        let tail = last_lines(output);
        let reason = format!("Could not prepare the dependencies: `{command}` {what}");
        PrepareError(if tail.is_empty() {
            reason
        } else {
            format!("{reason}\n{tail}")
        })
    };
    let spec = ProcessSpec {
        program: PathBuf::from("sh"),
        args: vec!["-c".to_owned(), command.to_owned()],
        env: std::env::vars().collect(),
        stdin: None,
        working_directory: checkout.to_path_buf(),
        workspace_root: checkout.to_path_buf(),
        keep_stdin_open: false,
    };
    let limits = ProcessLimits::new(OUTPUT_BYTES, OUTPUT_BYTES, timeout);
    let process = spec
        .spawn()
        .await
        .map_err(|_| failed("could not be started".to_owned(), ""))?;
    let result = process
        .wait_with_capture(&limits, &SecretMaterial::new())
        .await
        .map_err(|_| failed("could not be followed".to_owned(), ""))?;
    let output = format!("{}\n{}", result.stdout.text, result.stderr.text);
    match result.exit {
        ProcessExit::Exited(0) => Ok(()),
        ProcessExit::Exited(code) => Err(failed(format!("exited {code}"), &output)),
        #[cfg(unix)]
        ProcessExit::Signaled(signal) => {
            Err(failed(format!("was stopped by signal {signal}"), &output))
        }
        ProcessExit::TimedOut => Err(failed(
            format!("timed out after {} s", timeout.as_secs()),
            &output,
        )),
    }
}

/// The last lines of `output`, without the blank ones at either end.
fn last_lines(output: &str) -> String {
    let lines: Vec<&str> = output.trim().lines().collect();
    lines[lines.len().saturating_sub(REASON_LINES)..].join("\n")
}

#[cfg(test)]
mod tests;
