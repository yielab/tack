use std::{collections::BTreeMap, fmt, path::PathBuf};

use serde::Deserialize;

use crate::{ConfigError, RunnerError};

pub const DEFAULT_API_BASE_URL: &str = "http://127.0.0.1:3210/api/runner/v1";
pub const DEFAULT_RUNNER_ID: &str = "local-runner";
pub const DEFAULT_STATE_DIR: &str = ".tack-runner";

/// A credential whose normal formatting is always redacted.
#[derive(Clone, PartialEq, Eq)]
pub struct EnrollmentCredential(String);

impl EnrollmentCredential {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the secret only to the protocol implementation that must use it.
    /// Callers must never put this value into a log, error, or command line.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for EnrollmentCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EnrollmentCredential([REDACTED])")
    }
}

impl fmt::Display for EnrollmentCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConfigOverrides {
    pub api_base_url: Option<String>,
    pub runner_id: Option<String>,
    pub state_dir: Option<PathBuf>,
    pub enrollment_credential: Option<EnrollmentCredential>,
}

#[derive(Default)]
pub struct RunnerConfigSources<'a> {
    pub file_toml: Option<&'a str>,
    pub environment: ConfigOverrides,
    pub command_line: ConfigOverrides,
}

/// Safe runner configuration. `Debug` inherits credential redaction from
/// [`EnrollmentCredential`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunnerConfig {
    pub api_base_url: String,
    pub runner_id: String,
    pub state_dir: PathBuf,
    pub enrollment_credential: Option<EnrollmentCredential>,
    /// The `[verify]` table: a program run after a succeeded attempt.
    pub verify: VerifyConfig,
    /// The `[git]` table: whether a succeeded attempt's branch is pushed.
    pub git: GitConfig,
    /// The `[secrets]` table.
    pub secrets: SecretsConfig,
    /// The `[environment]` table: variables every run gets, whatever its
    /// request says. A request cannot override one.
    pub environment: BTreeMap<String, EnvValue>,
}

/// The `[secrets]` table.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SecretsConfig {
    /// Secret-store names a request's `secret_reference` may name. Empty
    /// means a request may reference none.
    pub request_may_reference: Vec<String>,
}

/// One `[environment]` entry: a literal `value`, or a `secret_reference`
/// (`store:<name>` or `env:<VARIABLE>`) resolved once at startup.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvValue {
    pub value: Option<String>,
    pub secret_reference: Option<String>,
}

/// The `[git]` table. Off by default; no environment variable sets it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GitConfig {
    pub push_branches: bool,
    pub branch_prefix: String,
    pub author: String,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            push_branches: false,
            branch_prefix: "tack/".to_owned(),
            author: "Tack Runner <tack-runner@localhost>".to_owned(),
        }
    }
}

/// How the runner talks to the program in its `[verify]` table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerifyKind {
    /// The program is prill: the runner snapshots the workspace, writes
    /// prill's own input documents, runs `<program> verify …` and translates
    /// the `mrp/v1` pack it prints into `mrp-v1`.
    Prill,
    /// The program reads the staged evidence directory itself and writes an
    /// `mrp-v1` pack where `--output` says.
    Command,
}

impl std::fmt::Display for VerifyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Prill => write!(f, "prill"),
            Self::Command => write!(f, "command"),
        }
    }
}

/// The `[verify]` table. Off by default; no environment variable sets it.
/// `args` go after the flags the runner adds for `kind = "prill"` (so an
/// `--image` or `--no-sandbox` reaches prill) and before them for
/// `kind = "command"`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VerifyConfig {
    pub enabled: bool,
    pub kind: VerifyKind,
    pub program: String,
    pub args: Vec<String>,
    pub timeout_seconds: u64,
    /// `kind = "prill"` only: the private key prill signs the pack with
    /// (`--sign <path>`). Unset, packs are unsigned. A refusal to sign fails
    /// the verification; the runner never retries without the key.
    pub sign_key: Option<String>,
}

impl Default for VerifyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            kind: VerifyKind::Prill,
            program: "prill".to_owned(),
            args: Vec::new(),
            timeout_seconds: 1800,
            sign_key: None,
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    api_base_url: Option<String>,
    runner_id: Option<String>,
    state_dir: Option<PathBuf>,
    enrollment_credential: Option<String>,
    verify: Option<VerifyConfig>,
    git: Option<GitConfig>,
    secrets: Option<SecretsConfig>,
    environment: Option<BTreeMap<String, EnvValue>>,
}

impl RunnerConfig {
    pub fn defaults() -> Self {
        Self {
            api_base_url: DEFAULT_API_BASE_URL.to_owned(),
            runner_id: DEFAULT_RUNNER_ID.to_owned(),
            state_dir: PathBuf::from(DEFAULT_STATE_DIR),
            enrollment_credential: None,
            verify: VerifyConfig::default(),
            git: GitConfig::default(),
            secrets: SecretsConfig::default(),
            environment: BTreeMap::new(),
        }
    }

    /// Applies defaults, configuration file, environment, then command-line
    /// values. Supplying source values explicitly keeps this deterministic and
    /// testable without global environment mutation.
    pub fn from_sources(sources: RunnerConfigSources<'_>) -> Result<Self, ConfigError> {
        let mut config = Self::defaults();

        if let Some(contents) = sources.file_toml {
            let file: FileConfig = toml::from_str(contents).map_err(|_| {
                let names_provider = toml::from_str::<toml::Table>(contents)
                    .is_ok_and(|table| table.contains_key("provider"));
                if names_provider {
                    ConfigError::ProviderTable
                } else {
                    ConfigError::Invalid
                }
            })?;
            if let Some(verify) = file.verify {
                config.verify = verify;
            }
            if let Some(git) = file.git {
                config.git = git;
            }
            if let Some(secrets) = file.secrets {
                config.secrets = secrets;
            }
            if let Some(environment) = file.environment {
                config.environment = environment;
            }
            config.apply(ConfigOverrides {
                api_base_url: file.api_base_url,
                runner_id: file.runner_id,
                state_dir: file.state_dir,
                enrollment_credential: file.enrollment_credential.map(EnrollmentCredential::new),
            });
        }
        config.apply(sources.environment);
        config.apply(sources.command_line);

        if config.runner_id.trim().is_empty() {
            return Err(ConfigError::EmptyRunnerId);
        }
        Ok(config)
    }

    pub fn environment_overrides() -> ConfigOverrides {
        ConfigOverrides {
            api_base_url: std::env::var("TACK_RUNNER_API_URL").ok(),
            runner_id: std::env::var("TACK_RUNNER_ID").ok(),
            state_dir: std::env::var_os("TACK_RUNNER_STATE_DIR").map(PathBuf::from),
            enrollment_credential: std::env::var("TACK_RUNNER_ENROLLMENT_TOKEN")
                .ok()
                .map(EnrollmentCredential::new),
        }
    }

    pub fn require_enrollment_credential(&self) -> Result<&EnrollmentCredential, RunnerError> {
        self.enrollment_credential
            .as_ref()
            .ok_or(RunnerError::MissingEnrollmentCredential)
    }

    /// Where the file-backend secret store keeps its owner-only file when no
    /// platform credential store answers. Always under `state_dir`, so it
    /// moves with `--state-dir`/`TACK_RUNNER_STATE_DIR` exactly like the
    /// enrolled session file does.
    pub fn secret_store_path(&self) -> PathBuf {
        self.state_dir.join("secrets.json")
    }

    fn apply(&mut self, overrides: ConfigOverrides) {
        if let Some(value) = overrides.api_base_url {
            self.api_base_url = value;
        }
        if let Some(value) = overrides.runner_id {
            self.runner_id = value;
        }
        if let Some(value) = overrides.state_dir {
            self.state_dir = value;
        }
        if let Some(value) = overrides.enrollment_credential {
            self.enrollment_credential = Some(value);
        }
    }
}

#[cfg(test)]
#[path = "config/tests.rs"]
mod tests;
