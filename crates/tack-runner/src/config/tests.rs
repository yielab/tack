use super::*;

#[test]
fn config_precedence_is_defaults_file_environment_then_cli() {
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

/// Mirrors the test above for the second known provider — both are
/// seeded disabled, independently of each other.
#[test]
fn anthropic_provider_defaults_to_disabled_with_expected_secret() {
    let config = RunnerConfig::defaults();
    let provider = config
        .providers
        .get(ANTHROPIC_CONFIG_KEY)
        .expect("default provider entry present");
    assert!(!provider.enabled);
    assert_eq!(provider.secret, DEFAULT_ANTHROPIC_SECRET);
}

/// Enabling one known provider through the environment must not
/// disturb the other's own (disabled) default — proves the two
/// providers' config overrides are independent, keyed maps rather than
/// one override clobbering the whole `providers` map.
#[test]
fn enabling_one_provider_does_not_affect_others_default() {
    let config = RunnerConfig::from_sources(RunnerConfigSources {
        environment: ConfigOverrides {
            providers: BTreeMap::from([(
                ANTHROPIC_CONFIG_KEY.to_owned(),
                ProviderOverride {
                    enabled: Some(true),
                    secret: Some("anthropic-secret".to_owned()),
                    base_url: None,
                },
            )]),
            ..ConfigOverrides::default()
        },
        ..RunnerConfigSources::default()
    })
    .expect("configuration should load");

    let anthropic = config
        .providers
        .get(ANTHROPIC_CONFIG_KEY)
        .expect("anthropic entry present");
    assert!(anthropic.enabled);
    assert_eq!(anthropic.secret, "anthropic-secret");

    let vercel = config
        .providers
        .get(VERCEL_AI_GATEWAY_CONFIG_KEY)
        .expect("vercel entry still present at its default");
    assert!(!vercel.enabled);
    assert_eq!(vercel.secret, DEFAULT_VERCEL_AI_GATEWAY_SECRET);
}

/// Mirrors `configuration_precedence_is_defaults_file_environment_then_cli`,
/// but for a provider entry specifically: proves the field-level merge
/// (environment overrides only `secret`, `enabled` still comes from the
/// file) rather than one override replacing the whole entry.
#[test]
fn provider_precedence_is_defaults_file_environment_then_cli() {
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
                    base_url: None,
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
fn an_unrecognized_provider_name_does_not_affect_the_known_ones() {
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

/// The verifier is prill unless the file says otherwise; `kind = "command"`
/// is the operator's own program, and a kind this build does not know is
/// rejected rather than run as something else.
#[test]
fn the_verify_table_defaults_to_prill_and_accepts_both_kinds() {
    let defaults = VerifyConfig::default();
    assert!(!defaults.enabled);
    assert_eq!(defaults.kind, VerifyKind::Prill);
    assert_eq!(defaults.program, "prill");
    assert!(defaults.args.is_empty());

    let config = RunnerConfig::from_sources(RunnerConfigSources {
        file_toml: Some(
            r#"
                [verify]
                enabled = true
                args = ["--image", "registry.invalid/verify@sha256:0"]
            "#,
        ),
        ..RunnerConfigSources::default()
    })
    .expect("a [verify] table without a kind is prill");
    assert_eq!(config.verify.kind, VerifyKind::Prill);
    assert_eq!(config.verify.program, "prill");
    assert_eq!(config.verify.args.len(), 2);

    let config = RunnerConfig::from_sources(RunnerConfigSources {
        file_toml: Some(
            r#"
                [verify]
                enabled = true
                kind = "command"
                program = "/opt/verifier/bin/verify"
            "#,
        ),
        ..RunnerConfigSources::default()
    })
    .expect("kind = \"command\" names the operator's own program");
    assert_eq!(config.verify.kind, VerifyKind::Command);
    assert_eq!(config.verify.program, "/opt/verifier/bin/verify");

    let error = RunnerConfig::from_sources(RunnerConfigSources {
        file_toml: Some(
            r#"
                [verify]
                kind = "assay"
            "#,
        ),
        ..RunnerConfigSources::default()
    })
    .expect_err("an unknown verify kind is rejected");
    assert_eq!(error, ConfigError::Invalid);
}

#[test]
fn openai_compatible_defaults_to_disabled_with_no_secret_and_no_base_url() {
    let config = RunnerConfig::defaults();
    let provider = config
        .providers
        .get(OPENAI_COMPATIBLE_CONFIG_KEY)
        .expect("default provider entry present");
    assert!(!provider.enabled);
    assert!(provider.secret.is_empty());
    assert_eq!(provider.base_url, None);
}

#[test]
fn openai_compatible_file_entry_carries_its_base_url() {
    let config = RunnerConfig::from_sources(RunnerConfigSources {
        file_toml: Some(
            r#"
                [provider.openai_compatible]
                enabled = true
                base_url = "http://127.0.0.1:8081"
            "#,
        ),
        ..RunnerConfigSources::default()
    })
    .expect("configuration should load");

    let provider = config
        .providers
        .get(OPENAI_COMPATIBLE_CONFIG_KEY)
        .expect("entry present");
    assert!(provider.enabled);
    assert_eq!(provider.base_url.as_deref(), Some("http://127.0.0.1:8081"));
    assert!(
        provider.secret.is_empty(),
        "no secret is the server-checks-none case, not an error"
    );
}

/// `(file, enabled by the environment)` rows: an enabled entry with no
/// `base_url` at all, one with a blank `base_url`, and one the environment
/// alone enables — each is refused at load, naming the table and the field.
#[test]
fn enabling_openai_compatible_without_a_base_url_is_refused_at_load() {
    for (file, enabled_by_environment) in [
        ("[provider.openai_compatible]\nenabled = true\n", false),
        (
            "[provider.openai_compatible]\nenabled = true\nbase_url = \"  \"\n",
            false,
        ),
        ("", true),
    ] {
        let error = RunnerConfig::from_sources(RunnerConfigSources {
            file_toml: Some(file),
            environment: ConfigOverrides {
                providers: BTreeMap::from([(
                    OPENAI_COMPATIBLE_CONFIG_KEY.to_owned(),
                    ProviderOverride {
                        enabled: enabled_by_environment.then_some(true),
                        secret: None,
                        base_url: None,
                    },
                )]),
                ..ConfigOverrides::default()
            },
            ..RunnerConfigSources::default()
        })
        .expect_err("an enabled entry with no base_url must be refused");
        assert_eq!(
            error,
            ConfigError::ProviderBaseUrlMissing {
                provider: OPENAI_COMPATIBLE_CONFIG_KEY.to_owned()
            },
            "{file:?}"
        );
        let message = error.to_string();
        assert!(
            message.contains("[provider.openai_compatible]") && message.contains("base_url"),
            "{message}"
        );
    }
}

#[test]
fn a_base_url_from_the_environment_completes_a_file_enabled_entry() {
    let config = RunnerConfig::from_sources(RunnerConfigSources {
        file_toml: Some("[provider.openai_compatible]\nenabled = true\n"),
        environment: ConfigOverrides {
            providers: BTreeMap::from([(
                OPENAI_COMPATIBLE_CONFIG_KEY.to_owned(),
                ProviderOverride {
                    enabled: None,
                    secret: Some("store:local/key".to_owned()),
                    base_url: Some("http://127.0.0.1:8081".to_owned()),
                },
            )]),
            ..ConfigOverrides::default()
        },
        ..RunnerConfigSources::default()
    })
    .expect("configuration should load");

    let provider = config
        .providers
        .get(OPENAI_COMPATIBLE_CONFIG_KEY)
        .expect("entry present");
    assert!(provider.enabled);
    assert_eq!(provider.base_url.as_deref(), Some("http://127.0.0.1:8081"));
    assert_eq!(provider.secret, "store:local/key");
}

/// Mutates process-wide variables: safe under `cargo nextest` (one process
/// per test), never under a bare `cargo test` alongside this module's
/// other tests.
#[test]
fn provider_environment_variables_are_read_per_provider() {
    let variables = [
        ("TACK_RUNNER_PROVIDER_OPENAI_COMPATIBLE_ENABLED", "true"),
        (
            "TACK_RUNNER_PROVIDER_OPENAI_COMPATIBLE_BASE_URL",
            "http://127.0.0.1:8081",
        ),
        (
            "TACK_RUNNER_PROVIDER_OPENAI_COMPATIBLE_SECRET",
            "openai-compatible/local",
        ),
        // A hosted provider's host is the vendor's: this one is never read.
        (
            "TACK_RUNNER_PROVIDER_VERCEL_AI_GATEWAY_BASE_URL",
            "http://127.0.0.1:1",
        ),
        ("TACK_RUNNER_PROVIDER_ANTHROPIC_ENABLED", "1"),
    ];
    for (name, value) in variables {
        unsafe { std::env::set_var(name, value) };
    }
    let overrides = RunnerConfig::environment_overrides();
    for (name, _) in variables {
        unsafe { std::env::remove_var(name) };
    }

    assert_eq!(
        overrides.providers.get(OPENAI_COMPATIBLE_CONFIG_KEY),
        Some(&ProviderOverride {
            enabled: Some(true),
            secret: Some("openai-compatible/local".to_owned()),
            base_url: Some("http://127.0.0.1:8081".to_owned()),
        })
    );
    assert_eq!(
        overrides.providers.get(ANTHROPIC_CONFIG_KEY),
        Some(&ProviderOverride {
            enabled: Some(true),
            secret: None,
            base_url: None,
        })
    );
    assert!(
        !overrides
            .providers
            .contains_key(VERCEL_AI_GATEWAY_CONFIG_KEY),
        "{:?}",
        overrides.providers
    );
}
