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
    pub max_concurrent_attempts: Option<u32>,
    /// A value in this source that could not be read at all; the load is
    /// refused naming it.
    pub invalid: Option<ConfigError>,
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
    /// How many attempts this runner works at once. At least 1; a folder
    /// worked in place still runs one at a time whatever this says.
    pub max_concurrent_attempts: u32,
    /// The `[verify]` table: a program run after a succeeded attempt.
    pub verify: VerifyConfig,
    /// The `[git]` table: whether a succeeded attempt's branch is pushed.
    pub git: GitConfig,
    /// The `[secrets]` table.
    pub secrets: SecretsConfig,
    /// The `[environment]` table: variables every run gets, whatever its
    /// request says. A request cannot override one.
    pub environment: BTreeMap<String, EnvValue>,
    /// The `[docket]` table: where this runner reaches docket's operator API.
    pub docket: DocketConfig,
}

/// The `[docket]` table. docket runs as a daemon beside the runner and the
/// runner speaks to it over HTTP; nothing here makes the runner spawn a
/// model call of its own.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DocketConfig {
    /// The daemon's `DOCKET_HOME`; the operator's own when unset.
    pub home: Option<PathBuf>,
    /// The port the daemon listens on, on loopback.
    pub port: u16,
    /// The file holding the daemon's bearer token.
    pub token_file: Option<PathBuf>,
    /// The docket binary; naming one registers the harness.
    pub binary: Option<PathBuf>,
    /// A daemon that is already running at this URL. When set, the runner
    /// uses it as it is and spawns nothing.
    pub base_url: Option<String>,
}

impl Default for DocketConfig {
    fn default() -> Self {
        Self {
            home: None,
            port: 7399,
            token_file: None,
            binary: None,
            base_url: None,
        }
    }
}

impl DocketConfig {
    /// Where the operator API answers: `base_url`, else loopback on `port`.
    pub fn url(&self) -> String {
        self.base_url
            .as_deref()
            .map(|url| url.trim_end_matches('/').to_owned())
            .unwrap_or_else(|| format!("http://127.0.0.1:{}", self.port))
    }

    /// Whether this runner is set up to use docket at all.
    pub fn configured(&self) -> bool {
        self.binary.is_some() || self.base_url.is_some()
    }
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
    max_concurrent_attempts: Option<u32>,
    verify: Option<VerifyConfig>,
    git: Option<GitConfig>,
    secrets: Option<SecretsConfig>,
    environment: Option<BTreeMap<String, EnvValue>>,
    docket: Option<DocketConfig>,
}

impl RunnerConfig {
    pub fn defaults() -> Self {
        Self {
            api_base_url: DEFAULT_API_BASE_URL.to_owned(),
            runner_id: DEFAULT_RUNNER_ID.to_owned(),
            state_dir: PathBuf::from(DEFAULT_STATE_DIR),
            enrollment_credential: None,
            max_concurrent_attempts: 1,
            verify: VerifyConfig::default(),
            git: GitConfig::default(),
            secrets: SecretsConfig::default(),
            environment: BTreeMap::new(),
            docket: DocketConfig::default(),
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
            if let Some(docket) = file.docket {
                config.docket = docket;
            }
            config.apply(ConfigOverrides {
                api_base_url: file.api_base_url,
                runner_id: file.runner_id,
                state_dir: file.state_dir,
                enrollment_credential: file.enrollment_credential.map(EnrollmentCredential::new),
                max_concurrent_attempts: file.max_concurrent_attempts,
                invalid: None,
            });
        }
        if let Some(error) = sources.environment.invalid.clone() {
            return Err(error);
        }
        config.apply(sources.environment);
        config.apply(sources.command_line);

        if config.runner_id.trim().is_empty() {
            return Err(ConfigError::EmptyRunnerId);
        }
        if config.max_concurrent_attempts < 1 {
            return Err(ConfigError::InvalidField {
                field: "max_concurrent_attempts",
                reason: "must be at least 1".to_owned(),
            });
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
            ..Self::attempts_from_environment(
                std::env::var("TACK_RUNNER_MAX_CONCURRENT_ATTEMPTS")
                    .ok()
                    .as_deref(),
            )
        }
    }

    /// The overrides `TACK_RUNNER_MAX_CONCURRENT_ATTEMPTS` carries: its
    /// number, or the refusal for a value that is not one.
    pub fn attempts_from_environment(raw: Option<&str>) -> ConfigOverrides {
        let mut overrides = ConfigOverrides::default();
        if let Some(raw) = raw {
            match raw.trim().parse() {
                Ok(value) => overrides.max_concurrent_attempts = Some(value),
                Err(_) => {
                    overrides.invalid = Some(ConfigError::InvalidField {
                        field: "max_concurrent_attempts",
                        reason: "TACK_RUNNER_MAX_CONCURRENT_ATTEMPTS must be a whole number"
                            .to_owned(),
                    });
                }
            }
        }
        overrides
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
        if let Some(value) = overrides.max_concurrent_attempts {
            self.max_concurrent_attempts = value;
        }
    }
}

#[cfg(test)]
#[path = "config/tests.rs"]
mod tests;
