//! Installing a project's dependencies in the copy of the code an agent works in.
//!
//! The command comes from the repository's lockfile unless the project replaced or turned off the
//! install (`tack_core::prepare`). It runs in the checkout with the runner's own environment,
//! network included; the agent's sandbox is not involved. What a lockfile installs into a folder
//! (`node_modules`, `.venv`) is kept once, under the runner's state directory, and linked into each
//! checkout, so the next attempt with the same lockfile does not install again.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

use tack_core::prepare::{PrepareSetting, Resolution, Why};

use crate::harness::{
    process::{ProcessExit, ProcessLimits, ProcessSpec},
    redact::SecretMaterial,
    sha256::sha256_hex,
};

/// Every folder a cached install can be linked as. The evidence of an attempt leaves out the ones
/// that are links, because a link is not the agent's work.
const LINKED_FOLDERS: [&str; 2] = ["node_modules", ".venv"];

/// Output kept from a command, per stream.
const OUTPUT_BYTES: usize = 1024 * 1024;

/// How many of the last lines of a failed command's output the reason carries.
const REASON_LINES: usize = 40;

/// Why the dependencies could not be prepared; the text is the attempt's reason.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct PrepareError(String);

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

/// The folders under `checkout` that are links to a cached install.
pub fn linked_folders(checkout: &Path) -> Vec<&'static str> {
    LINKED_FOLDERS
        .into_iter()
        .filter(|folder| {
            fs::symlink_metadata(checkout.join(folder))
                .is_ok_and(|metadata| metadata.file_type().is_symlink())
        })
        .collect()
}

/// The folder a detected install fills, when one is worth keeping: poetry and cargo keep their
/// packages in a cache of their own.
fn cached_folder(lockfile: &str) -> Option<&'static str> {
    match lockfile {
        "package-lock.json" | "pnpm-lock.yaml" | "yarn.lock" => Some("node_modules"),
        "uv.lock" => Some(".venv"),
        _ => None,
    }
}

/// Runs `command`, or links the install a previous attempt kept for the same lockfile.
async fn install(
    checkout: &Path,
    command: &str,
    lockfile: Option<&str>,
    state_dir: &Path,
    timeout: Duration,
) -> Result<(), PrepareError> {
    // A replaced command is the project's own and may do anything, so it always runs. A folder the
    // repository already holds is the project's own too.
    let kept = lockfile
        .and_then(|lockfile| Some((lockfile, cached_folder(lockfile)?)))
        .filter(|(_, folder)| fs::symlink_metadata(checkout.join(folder)).is_err());
    let Some((lockfile, folder)) = kept else {
        return run(checkout, command, timeout).await;
    };
    let Ok(lock) = fs::read(checkout.join(lockfile)) else {
        return run(checkout, command, timeout).await;
    };
    let cache = state_dir.join("prepare-cache");
    let entry = cache.join(sha256_hex(&lock));
    if link(&entry.join(folder), &checkout.join(folder)).is_ok() {
        tracing::info!("linked the dependencies kept for {lockfile}");
        return Ok(());
    }
    run(checkout, command, timeout).await?;
    keep(checkout, folder, &cache, &entry)
}

/// Moves the freshly installed `folder` into the cache as `entry` and links it back. The entry
/// appears whole or not at all: it is built beside the cache's other entries and renamed into
/// place, and when another attempt renamed first, theirs is used and this copy is dropped. A
/// folder that cannot be moved stays in the checkout, which still has its install.
fn keep(checkout: &Path, folder: &str, cache: &Path, entry: &Path) -> Result<(), PrepareError> {
    let staging = cache.join(format!(
        ".staging-{}",
        checkout
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    ));
    let installed = checkout.join(folder);
    let _ = fs::remove_dir_all(&staging);
    let moved =
        fs::create_dir_all(&staging).and_then(|()| fs::rename(&installed, staging.join(folder)));
    if let Err(error) = moved {
        tracing::warn!("the installed dependencies were not kept: {error}");
        let _ = fs::remove_dir_all(&staging);
        return Ok(());
    }
    if let Err(error) = fs::rename(&staging, entry)
        && !entry.join(folder).is_dir()
    {
        tracing::warn!("the installed dependencies were not kept: {error}");
        let given_back = fs::rename(staging.join(folder), &installed);
        let _ = fs::remove_dir_all(&staging);
        return given_back
            .map_err(|error| PrepareError(format!("Could not keep the dependencies: {error}")));
    }
    let _ = fs::remove_dir_all(&staging);
    link(&entry.join(folder), &installed)
        .map_err(|error| PrepareError(format!("Could not link the dependencies: {error}")))
}

/// Makes `to` a link to the directory `from`.
fn link(from: &Path, to: &Path) -> io::Result<()> {
    if !from.is_dir() {
        return Err(io::ErrorKind::NotFound.into());
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(from, to)
    }
    #[cfg(not(unix))]
    {
        let _ = to;
        Err(io::ErrorKind::Unsupported.into())
    }
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
