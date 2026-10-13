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
