use std::process::Command;

fn runner() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tack-runner"))
}

#[test]
fn help_is_available_without_configuration() {
    let output = runner().arg("--help").output().expect("runner starts");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help is UTF-8");
    assert!(stdout.contains("Local pull-based Tack runner"));
}

#[test]
fn missing_enrollment_credential_fails_without_echoing_configuration() {
    let output = runner()
        .env_remove("TACK_RUNNER_ENROLLMENT_TOKEN")
        .output()
        .expect("runner starts");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    assert!(stderr.contains("runner enrollment credential is required"));
    assert!(!stderr.contains("TACK_RUNNER_ENROLLMENT_TOKEN"));
    assert!(!stderr.contains("enrollment_token"));
}
