//! The harness that measures Tack's runs with a real agent. It starts `tack serve
//! --with-runner` in a throwaway world, uses it over HTTP as an operator would and records
//! what the run left. It knows nothing of Tack's inside; the only thing that sets it apart from
//! the end-to-end tests is that the agent and the model are real.

pub mod api;
pub mod exam;
pub mod grade;
pub mod graders;
pub mod metrics;
pub mod operator;
pub mod record;
pub mod report;
pub mod task;
pub mod taxonomy;
pub mod trial;
pub mod world;

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

/// The `tack` binary to serve with: the one named, else `TACK_BIN`, else the one built beside
/// this program, else the first on `PATH`.
pub fn locate_tack_bin(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = explicit.or_else(|| std::env::var_os("TACK_BIN").map(PathBuf::from)) {
        return if path.is_file() {
            Ok(path)
        } else {
            bail!("{} is not a file", path.display())
        };
    }
    if let Ok(exe) = std::env::current_exe() {
        // `target/debug/tack-evals` and `target/debug/deps/<test>` both sit under the profile
        // directory the `tack` binary is built into.
        for dir in exe.ancestors().skip(1).take(3) {
            let candidate = dir.join("tack");
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    if let Some(found) = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join("tack"))
            .find(|candidate| candidate.is_file())
    }) {
        return Ok(found);
    }
    bail!("no `tack` binary found: build it (`cargo build -p tack-cli`) or pass --tack-bin")
}

/// Every task under `paths`, each a task directory or a directory of them.
pub fn load_tasks(paths: &[PathBuf]) -> Result<Vec<task::LoadedTask>> {
    let mut tasks = Vec::new();
    for path in paths {
        tasks.extend(task::LoadedTask::load_all(Path::new(path))?);
    }
    Ok(tasks)
}
