use std::{fmt, path::PathBuf};

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
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    api_base_url: Option<String>,
    runner_id: Option<String>,
    state_dir: Option<PathBuf>,
    enrollment_credential: Option<String>,
}

impl RunnerConfig {
    pub fn defaults() -> Self {
        Self {
            api_base_url: DEFAULT_API_BASE_URL.to_owned(),
            runner_id: DEFAULT_RUNNER_ID.to_owned(),
            state_dir: PathBuf::from(DEFAULT_STATE_DIR),
            enrollment_credential: None,
        }
    }

    /// Applies defaults, configuration file, environment, then command-line
    /// values. Supplying source values explicitly keeps this deterministic and
    /// testable without global environment mutation.
    pub fn from_sources(sources: RunnerConfigSources<'_>) -> Result<Self, ConfigError> {
        let mut config = Self::defaults();

        if let Some(contents) = sources.file_toml {
            let file: FileConfig = toml::from_str(contents).map_err(|_| ConfigError::Invalid)?;
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
mod tests {
    use super::*;

    #[test]
    fn configuration_precedence_is_defaults_file_environment_then_cli() {
        let config = RunnerConfig::from_sources(RunnerConfigSources {
            file_toml: Some(
                r#"
                    api_base_url = "https://file.invalid"
                    runner_id = "file"
                    state_dir = "file-state"
                    enrollment_credential = "file-secret"
                "#,
            ),
            environment: ConfigOverrides {
                api_base_url: Some("https://environment.invalid".into()),
                runner_id: Some("environment".into()),
                state_dir: Some(PathBuf::from("environment-state")),
                enrollment_credential: Some(EnrollmentCredential::new("environment-secret")),
            },
            command_line: ConfigOverrides {
                api_base_url: Some("https://cli.invalid".into()),
                runner_id: Some("cli".into()),
                state_dir: Some(PathBuf::from("cli-state")),
                enrollment_credential: Some(EnrollmentCredential::new("cli-secret")),
            },
        })
        .expect("configuration should load");

        assert_eq!(config.api_base_url, "https://cli.invalid");
        assert_eq!(config.runner_id, "cli");
        assert_eq!(config.state_dir, PathBuf::from("cli-state"));
        assert_eq!(config.enrollment_credential.unwrap().expose(), "cli-secret");
    }

    #[test]
    fn credentials_are_redacted_from_debug_and_missing_error() {
        let secret = "enrollment-credential-must-not-appear";
        let config = RunnerConfig::from_sources(RunnerConfigSources {
            command_line: ConfigOverrides {
                enrollment_credential: Some(EnrollmentCredential::new(secret)),
                ..ConfigOverrides::default()
            },
            ..RunnerConfigSources::default()
        })
        .expect("configuration should load");

        assert!(!format!("{config:?}").contains(secret));
        assert!(!format!("{:?}", config.enrollment_credential).contains(secret));
        let missing = RunnerConfig::defaults()
            .require_enrollment_credential()
            .expect_err("credential is absent");
        assert!(!missing.to_string().contains(secret));
    }

    #[test]
    fn invalid_toml_does_not_echo_credential_like_source_text() {
        let secret = "do-not-echo-this-credential";
        let error = RunnerConfig::from_sources(RunnerConfigSources {
            file_toml: Some("enrollment_credential = [do-not-echo-this-credential"),
            ..RunnerConfigSources::default()
        })
        .expect_err("invalid TOML should fail");

        assert_eq!(error, ConfigError::Invalid);
        assert!(!error.to_string().contains(secret));
    }
}
