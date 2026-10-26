//! The command that installs a project's dependencies before an agent works in its copy of the code.
//!
//! The command comes from the lockfile at the repository's root, unless the project turned the
//! install off or replaced the command. Everything here is a function of file names; reading the
//! repository, and running the command, belong to the callers.

use serde::{Deserialize, Serialize};

/// Lockfile name and the command it implies, in the order they win when a repository has several.
const LOCKFILES: [(&str, &str); 6] = [
    ("package-lock.json", "npm ci"),
    ("pnpm-lock.yaml", "pnpm install --frozen-lockfile"),
    ("yarn.lock", "yarn install --immutable"),
    ("uv.lock", "uv sync --frozen"),
    ("poetry.lock", "poetry install"),
    ("Cargo.lock", "cargo fetch"),
];

/// An install command found from a lockfile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detected {
    pub command: &'static str,
    /// The file the command came from.
    pub lockfile: &'static str,
}

/// The install command for the first lockfile found among the names of a repository's root files.
/// Directories and paths below the root do not count; only a bare file name does.
pub fn detect<S: AsRef<str>>(root_files: &[S]) -> Option<Detected> {
    LOCKFILES.iter().find_map(|&(lockfile, command)| {
        root_files
            .iter()
            .any(|name| name.as_ref() == lockfile)
            .then_some(Detected { command, lockfile })
    })
}

/// What a project says about its install.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum PrepareSetting {
    /// Use the command the lockfile implies.
    #[default]
    Detect,
    /// Install nothing.
    Off,
    /// Run this command instead of the detected one.
    Replace { command: String },
}

impl PrepareSetting {
    /// Reads the stored value: absent detects, empty turns the install off, any other text replaces.
    pub fn from_stored(stored: Option<&str>) -> Self {
        match stored.map(str::trim) {
            None => Self::Detect,
            Some("") => Self::Off,
            Some(command) => Self::Replace {
                command: command.to_owned(),
            },
        }
    }

    /// The value to store; the inverse of [`from_stored`](Self::from_stored).
    pub fn to_stored(&self) -> Option<String> {
        match self {
            Self::Detect => None,
            Self::Off => Some(String::new()),
            Self::Replace { command } => Some(command.clone()),
        }
    }

    /// What runs for a repository whose root holds `root_files`, or why nothing does.
    pub fn resolve<S: AsRef<str>>(&self, root_files: &[S]) -> Resolution {
        match self {
            Self::Off => Resolution::Nothing(Why::Off),
            Self::Replace { command } => Resolution::Run {
                command: command.clone(),
                lockfile: None,
            },
            Self::Detect => match detect(root_files) {
                Some(found) => Resolution::Run {
                    command: found.command.to_owned(),
                    lockfile: Some(found.lockfile),
                },
                None => Resolution::Nothing(Why::NoLockfile),
            },
        }
    }
}

/// What a run does about its install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Run {
        command: String,
        /// The lockfile a detected command came from; none for a replaced one.
        lockfile: Option<&'static str>,
    },
    Nothing(Why),
}

/// Why no install runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    Off,
    NoLockfile,
}

/// A command as a person typed it, ready to store: trimmed, and one line. An empty command stays
/// empty, which is how the install is turned off.
pub fn clean_command(raw: &str) -> Result<String, &'static str> {
    let trimmed = raw.trim();
    if trimmed.contains(['\n', '\r']) {
        Err("The install command must be a single line")
    } else {
        Ok(trimmed.to_owned())
    }
}

#[cfg(test)]
mod tests;
