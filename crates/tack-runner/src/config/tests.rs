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
            max_concurrent_attempts: None,
            invalid: None,
            process: None,
        },
        command_line: ConfigOverrides {
            api_base_url: Some("https://cli.invalid".into()),
            runner_id: Some("cli".into()),
            state_dir: Some(PathBuf::from("cli-state")),
            enrollment_credential: Some(EnrollmentCredential::new("cli-secret")),
            max_concurrent_attempts: None,
            invalid: None,
            process: None,
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
fn a_saved_session_or_a_credential_lets_the_runner_start_and_neither_does_not() {
    let directory = tempfile::tempdir().expect("state dir");
    let mut config = RunnerConfig::defaults();
    config.state_dir = directory.path().to_path_buf();

    assert!(matches!(
        config.require_session_or_enrollment_credential(),
        Err(RunnerError::MissingEnrollmentCredential)
    ));

    config.enrollment_credential = Some(EnrollmentCredential::new("a-credential"));
    config
        .require_session_or_enrollment_credential()
        .expect("a credential is enough");

    config.enrollment_credential = None;
    std::fs::write(
        directory.path().join("session.json"),
        r#"{"runner_id":"runr_1","runner_credential":"c","credential_expires_at":"2026-11-04T12:00:00Z"}"#,
    )
    .expect("session is written");
    config
        .require_session_or_enrollment_credential()
        .expect("a saved session is enough");

    std::fs::write(directory.path().join("session.json"), "{}").expect("session is damaged");
    assert!(
        config.require_session_or_enrollment_credential().is_err(),
        "a session that cannot be read is no session"
    );
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
fn a_provider_table_is_refused() {
    let error = RunnerConfig::from_sources(RunnerConfigSources {
        file_toml: Some(
            r#"
                [provider.vercel_ai_gateway]
                enabled = true
            "#,
        ),
        ..RunnerConfigSources::default()
    })
    .expect_err("a [provider.*] table must be refused");
    assert_eq!(error, ConfigError::ProviderTable);
    assert!(error.to_string().contains("[provider]"));
}

/// A key the file does not have is named by its key; a `[table]` says where the runner's settings
/// go; a value never appears in the message, whatever the key held.
#[test]
fn an_unknown_key_is_named_and_never_its_value() {
    let secret = "do-not-echo-this-credential";
    for (toml, expected) in [
        (
            "[runner]\nenrollment_credential = \"do-not-echo-this-credential\"".to_owned(),
            "unknown table [runner]: the runner's settings go at the top of the file",
        ),
        (
            "runer_id = \"do-not-echo-this-credential\"".to_owned(),
            "unknown key runer_id",
        ),
    ] {
        let error = RunnerConfig::from_sources(RunnerConfigSources {
            file_toml: Some(&toml),
            ..RunnerConfigSources::default()
        })
        .expect_err("an unknown key must be refused");
        let text = error.to_string();
        assert!(text.contains(expected), "{text}");
        assert!(!text.contains(secret), "{text}");
    }
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
    assert_eq!(config.verify.sign_key, None);

    let config = RunnerConfig::from_sources(RunnerConfigSources {
        file_toml: Some(
            r#"
                [verify]
                enabled = true
                sign_key = "/home/person/.config/prill/prill.key"
            "#,
        ),
        ..RunnerConfigSources::default()
    })
    .expect("a [verify] table may name a signing key");
    assert_eq!(
        config.verify.sign_key.as_deref(),
        Some("/home/person/.config/prill/prill.key")
    );

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
fn max_concurrent_attempts_defaults_to_one_is_refused_below_one_and_follows_precedence() {
    let overrides = |value: Option<u32>| ConfigOverrides {
        max_concurrent_attempts: value,
        ..ConfigOverrides::default()
    };
    let refused = |reason: &str| {
        Some(ConfigError::InvalidField {
            field: "max_concurrent_attempts",
            reason: reason.to_owned(),
        })
    };
    // (label, file, raw environment value, command line, what loads, the
    // refusal when it is refused)
    let rows = [
        ("default", None, None, None, Some(1), None),
        (
            "file",
            Some("max_concurrent_attempts = 3"),
            None,
            None,
            Some(3),
            None,
        ),
        (
            "environment beats file",
            Some("max_concurrent_attempts = 3"),
            Some("4"),
            None,
            Some(4),
            None,
        ),
        (
            "flag beats environment",
            Some("max_concurrent_attempts = 3"),
            Some("4"),
            Some(2),
            Some(2),
            None,
        ),
        (
            "zero in the file",
            Some("max_concurrent_attempts = 0"),
            None,
            None,
            None,
            refused("must be at least 1"),
        ),
        (
            "zero on the command line",
            None,
            Some("4"),
            Some(0),
            None,
            refused("must be at least 1"),
        ),
        (
            "an environment value that is not a number",
            Some("max_concurrent_attempts = 3"),
            Some("many"),
            None,
            None,
            refused("TACK_RUNNER_MAX_CONCURRENT_ATTEMPTS must be a whole number"),
        ),
    ];
    for (label, file, environment, command_line, expected, refusal) in rows {
        let loaded = RunnerConfig::from_sources(RunnerConfigSources {
            file_toml: file,
            environment: RunnerConfig::attempts_from_environment(environment),
            command_line: overrides(command_line),
        })
        .map(|config| config.max_concurrent_attempts);
        assert_eq!(loaded.clone().ok(), expected, "{label}");
        if let Some(refusal) = refusal {
            assert_eq!(loaded, Err(refusal), "{label}");
        }
    }
}

#[test]
fn the_docket_table_reads_and_defaults_to_the_loopback_port() {
    let load = |toml: Option<&str>| {
        RunnerConfig::from_sources(RunnerConfigSources {
            file_toml: toml,
            ..RunnerConfigSources::default()
        })
    };
    let unset = load(None).expect("defaults").docket;
    assert_eq!(unset.port, 7399);
    assert_eq!(unset.url(), "http://127.0.0.1:7399");
    assert!(!unset.configured());

    let set = load(Some(
        "[docket]\nport = 7400\ntoken_file = \"/run/docket.token\"\nbinary = \"/usr/bin/docket\"\n",
    ))
    .expect("a docket table")
    .docket;
    assert_eq!(set.url(), "http://127.0.0.1:7400");
    assert_eq!(set.token_file, Some("/run/docket.token".into()));
    assert!(set.configured());

    let fake = load(Some("[docket]\nbase_url = \"http://127.0.0.1:9/\"\n"))
        .expect("a base url")
        .docket;
    assert_eq!(fake.url(), "http://127.0.0.1:9");
    assert!(fake.configured());

    assert!(load(Some("[docket]\nprovider = \"x\"\n")).is_err());
}
