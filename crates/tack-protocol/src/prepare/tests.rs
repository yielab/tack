use super::*;

fn detected(files: &[&str]) -> Option<(&'static str, &'static str)> {
    detect(files).map(|d| (d.command, d.lockfile))
}

#[test]
fn each_lockfile_implies_its_command() {
    for (file, command) in [
        ("package-lock.json", "npm ci"),
        ("pnpm-lock.yaml", "pnpm install --frozen-lockfile"),
        ("yarn.lock", "yarn install --immutable"),
        ("uv.lock", "uv sync --frozen"),
        ("poetry.lock", "poetry install"),
        ("Cargo.lock", "cargo fetch"),
    ] {
        assert_eq!(detected(&["README.md", file, "src"]), Some((command, file)));
    }
}

#[test]
fn the_first_lockfile_in_the_order_wins() {
    assert_eq!(
        detected(&["Cargo.lock", "yarn.lock", "package-lock.json"]),
        Some(("npm ci", "package-lock.json"))
    );
    assert_eq!(
        detected(&["poetry.lock", "uv.lock"]),
        Some(("uv sync --frozen", "uv.lock"))
    );
}

#[test]
fn no_lockfile_detects_nothing() {
    assert_eq!(detected(&[]), None);
    assert_eq!(detected(&["package.json", "README.md"]), None);
}

#[test]
fn only_a_bare_file_name_counts() {
    assert_eq!(
        detected(&["frontend/package-lock.json", "Cargo.lock.bak"]),
        None
    );
}

#[test]
fn owned_names_work_as_well_as_borrowed() {
    let files = vec!["uv.lock".to_string()];
    assert_eq!(detect(&files).map(|d| d.command), Some("uv sync --frozen"));
}

#[test]
fn the_stored_value_reads_as_detect_off_or_replace() {
    assert_eq!(PrepareSetting::from_stored(None), PrepareSetting::Detect);
    assert_eq!(PrepareSetting::from_stored(Some("")), PrepareSetting::Off);
    assert_eq!(PrepareSetting::from_stored(Some("  ")), PrepareSetting::Off);
    assert_eq!(
        PrepareSetting::from_stored(Some(" make deps ")),
        PrepareSetting::Replace {
            command: "make deps".into()
        }
    );
}

#[test]
fn detect_runs_what_the_lockfile_implies() {
    assert_eq!(
        PrepareSetting::Detect.resolve(&["pnpm-lock.yaml"]),
        Resolution::Run {
            command: "pnpm install --frozen-lockfile".into(),
            lockfile: Some("pnpm-lock.yaml"),
        }
    );
}

#[test]
fn detect_without_a_lockfile_installs_nothing() {
    assert_eq!(
        PrepareSetting::Detect.resolve(&["README.md"]),
        Resolution::Nothing(Why::NoLockfile)
    );
}

#[test]
fn off_installs_nothing_even_with_a_lockfile() {
    assert_eq!(
        PrepareSetting::Off.resolve(&["package-lock.json"]),
        Resolution::Nothing(Why::Off)
    );
}

#[test]
fn a_replacement_runs_with_or_without_a_lockfile() {
    let setting = PrepareSetting::Replace {
        command: "make deps".into(),
    };
    let expected = Resolution::Run {
        command: "make deps".into(),
        lockfile: None,
    };
    assert_eq!(setting.resolve(&["package-lock.json"]), expected);
    assert_eq!(setting.resolve::<&str>(&[]), expected);
}

#[test]
fn a_command_is_trimmed_and_must_be_one_line() {
    assert_eq!(clean_command("  npm ci \t"), Ok("npm ci".to_string()));
    assert_eq!(clean_command("   "), Ok(String::new()));
    assert!(clean_command("npm ci\nrm -rf /").is_err());
    assert!(
        clean_command("npm ci\r").is_ok(),
        "a trailing return is whitespace"
    );
    assert!(clean_command("a\r\nb").is_err());
}

#[test]
fn the_setting_travels_as_a_tagged_object() {
    let value = serde_json::to_value(PrepareSetting::Replace {
        command: "make deps".into(),
    })
    .unwrap();
    assert_eq!(
        value,
        serde_json::json!({"mode": "replace", "command": "make deps"})
    );
    let off: PrepareSetting = serde_json::from_value(serde_json::json!({"mode": "off"})).unwrap();
    assert_eq!(off, PrepareSetting::Off);
}
