use std::{collections::BTreeMap, fmt, path::PathBuf};

use serde::Deserialize;

use crate::{ConfigError, RunnerError};

pub const DEFAULT_API_BASE_URL: &str = "http://127.0.0.1:3210/api/runner/v1";
pub const DEFAULT_RUNNER_ID: &str = "local-runner";
pub const DEFAULT_STATE_DIR: &str = ".tack-runner";

/// The provider name recorded everywhere this system records or requests a
/// model's route (`ModelProvider`, `requested_model_provider`, a catalog
/// entry's `discovery`) — ADR 0061 decision 4.
pub const VERCEL_AI_GATEWAY_PROVIDER: &str = "vercel-ai-gateway";

/// The `[provider.<name>]` table name and `RunnerConfig::providers` map key
/// for the provider above. Kept distinct from [`VERCEL_AI_GATEWAY_PROVIDER`]
/// because a config section header and a wire-level provider name follow
/// different spelling conventions in this project (underscore vs. hyphen).
pub const VERCEL_AI_GATEWAY_CONFIG_KEY: &str = "vercel_ai_gateway";

/// Default `secret` entry name for the provider above — the runner-local
/// secret store name a fresh install resolves with no configuration.
/// `SecretStore::resolve` does not append `/default` on its own, so this
/// must spell the full entry name.
pub const DEFAULT_VERCEL_AI_GATEWAY_SECRET: &str = "vercel-ai-gateway/default";

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
    /// Keyed the same as [`RunnerConfig::providers`]. Only a field actually
    /// present here is merged — an override never clears a provider's other
    /// field, matching [`ConfigOverrides`]'s own `Option`-means-untouched
    /// rule for every other member.
    pub providers: BTreeMap<String, ProviderOverride>,
}

/// A partial [`ProviderConfig`] update: `None` leaves the field untouched,
/// mirroring every other member of [`ConfigOverrides`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProviderOverride {
    pub enabled: Option<bool>,
    pub secret: Option<String>,
}

/// Whether a provider is on, and where its credential lives — the only two
/// user-configurable facts about it. Endpoint URLs and credential env-var
/// names are not configuration; they are fixed per-provider data in
/// `crate::provider::known_endpoint`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderConfig {
    pub enabled: bool,
    pub secret: String,
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
    /// Configured provider endpoints, keyed by `[provider.<name>]` table
    /// name. One entry is seeded by [`RunnerConfig::defaults`]
    /// ([`VERCEL_AI_GATEWAY_CONFIG_KEY`], disabled); a name absent from
    /// this map has no configured endpoint at all, not merely a disabled
    /// one — the harness's own subscription/login mode applies.
    pub providers: BTreeMap<String, ProviderConfig>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    api_base_url: Option<String>,
    runner_id: Option<String>,
    state_dir: Option<PathBuf>,
    enrollment_credential: Option<String>,
    /// `[provider.<name>]` tables. Free-form key: a name this build does
    /// not recognize simply sits unused rather than failing to parse — the
    /// dedicated `enabled`/`secret` fields inside each table still reject
    /// an unknown field via `deny_unknown_fields` on
    /// [`ProviderFileConfig`].
    provider: Option<BTreeMap<String, ProviderFileConfig>>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderFileConfig {
    enabled: Option<bool>,
    secret: Option<String>,
}

impl RunnerConfig {
    pub fn defaults() -> Self {
        let mut providers = BTreeMap::new();
        providers.insert(
            VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
            ProviderConfig {
                enabled: false,
                secret: DEFAULT_VERCEL_AI_GATEWAY_SECRET.to_owned(),
            },
        );
        Self {
            api_base_url: DEFAULT_API_BASE_URL.to_owned(),
            runner_id: DEFAULT_RUNNER_ID.to_owned(),
            state_dir: PathBuf::from(DEFAULT_STATE_DIR),
            enrollment_credential: None,
            providers,
        }
    }

    /// Applies defaults, configuration file, environment, then command-line
    /// values. Supplying source values explicitly keeps this deterministic and
    /// testable without global environment mutation.
    pub fn from_sources(sources: RunnerConfigSources<'_>) -> Result<Self, ConfigError> {
        let mut config = Self::defaults();

        if let Some(contents) = sources.file_toml {
            let file: FileConfig = toml::from_str(contents).map_err(|_| ConfigError::Invalid)?;
            let providers = file
                .provider
                .unwrap_or_default()
                .into_iter()
                .map(|(name, entry)| {
                    (
                        name,
                        ProviderOverride {
                            enabled: entry.enabled,
                            secret: entry.secret,
                        },
                    )
                })
                .collect();
            config.apply(ConfigOverrides {
                api_base_url: file.api_base_url,
                runner_id: file.runner_id,
                state_dir: file.state_dir,
                enrollment_credential: file.enrollment_credential.map(EnrollmentCredential::new),
                providers,
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
        let mut providers = BTreeMap::new();
        let enabled = std::env::var("TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_ENABLED")
            .ok()
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"));
        let secret = std::env::var("TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_SECRET").ok();
        if enabled.is_some() || secret.is_some() {
            providers.insert(
                VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
                ProviderOverride { enabled, secret },
            );
        }
        ConfigOverrides {
            api_base_url: std::env::var("TACK_RUNNER_API_URL").ok(),
            runner_id: std::env::var("TACK_RUNNER_ID").ok(),
            state_dir: std::env::var_os("TACK_RUNNER_STATE_DIR").map(PathBuf::from),
            enrollment_credential: std::env::var("TACK_RUNNER_ENROLLMENT_TOKEN")
                .ok()
                .map(EnrollmentCredential::new),
            providers,
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
        for (name, provider_override) in overrides.providers {
            let entry = self
                .providers
                .entry(name)
                .or_insert_with(|| ProviderConfig {
                    enabled: false,
                    secret: String::new(),
                });
            if let Some(enabled) = provider_override.enabled {
                entry.enabled = enabled;
            }
            if let Some(secret) = provider_override.secret {
                entry.secret = secret;
            }
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
                providers: BTreeMap::new(),
            },
            command_line: ConfigOverrides {
                api_base_url: Some("https://cli.invalid".into()),
                runner_id: Some("cli".into()),
                state_dir: Some(PathBuf::from("cli-state")),
                enrollment_credential: Some(EnrollmentCredential::new("cli-secret")),
                providers: BTreeMap::new(),
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

    #[test]
    fn provider_defaults_to_disabled_with_the_expected_secret_name() {
        let config = RunnerConfig::defaults();
        let provider = config
            .providers
            .get(VERCEL_AI_GATEWAY_CONFIG_KEY)
            .expect("default provider entry present");
        assert!(!provider.enabled);
        assert_eq!(provider.secret, DEFAULT_VERCEL_AI_GATEWAY_SECRET);
    }

    /// Mirrors `configuration_precedence_is_defaults_file_environment_then_cli`,
    /// but for a provider entry specifically: proves the field-level merge
    /// (environment overrides only `secret`, `enabled` still comes from the
    /// file) rather than one override replacing the whole entry.
    #[test]
    fn provider_config_precedence_is_defaults_file_environment_then_cli() {
        let config = RunnerConfig::from_sources(RunnerConfigSources {
            file_toml: Some(
                r#"
                    [provider.vercel_ai_gateway]
                    enabled = true
                    secret = "file-secret"
                "#,
            ),
            environment: ConfigOverrides {
                providers: BTreeMap::from([(
                    VERCEL_AI_GATEWAY_CONFIG_KEY.to_owned(),
                    ProviderOverride {
                        enabled: None,
                        secret: Some("environment-secret".to_owned()),
                    },
                )]),
                ..ConfigOverrides::default()
            },
            command_line: ConfigOverrides::default(),
        })
        .expect("configuration should load");

        let provider = config
            .providers
            .get(VERCEL_AI_GATEWAY_CONFIG_KEY)
            .expect("provider entry present");
        assert!(
            provider.enabled,
            "the file enabled it and nothing later touched that field"
        );
        assert_eq!(
            provider.secret, "environment-secret",
            "environment overrides the file's secret"
        );
    }

    #[test]
    fn unknown_field_inside_a_provider_table_is_rejected() {
        let error = RunnerConfig::from_sources(RunnerConfigSources {
            file_toml: Some(
                r#"
                    [provider.vercel_ai_gateway]
                    enabled = true
                    bogus = "nope"
                "#,
            ),
            ..RunnerConfigSources::default()
        })
        .expect_err("an unknown field inside a provider table must be rejected");
        assert_eq!(error, ConfigError::Invalid);
    }

    /// A provider name this build does not recognize must not fail
    /// configuration loading — it simply sits unused, and the real
    /// `vercel_ai_gateway` entry stays at its (disabled) default.
    #[test]
    fn an_unrecognized_provider_name_does_not_fail_loading_or_affect_the_known_one() {
        let config = RunnerConfig::from_sources(RunnerConfigSources {
            file_toml: Some(
                r#"
                    [provider.some_future_gateway]
                    enabled = true
                    secret = "irrelevant"
                "#,
            ),
            ..RunnerConfigSources::default()
        })
        .expect("an unrecognized provider name must not fail configuration loading");

        assert!(
            config
                .providers
                .get("some_future_gateway")
                .expect("the entry is still recorded")
                .enabled
        );
        let vercel = config
            .providers
            .get(VERCEL_AI_GATEWAY_CONFIG_KEY)
            .expect("the known provider's default entry is untouched");
        assert!(!vercel.enabled);
    }
}
