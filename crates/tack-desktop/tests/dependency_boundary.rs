//! Proves the two dependency-boundary rules this crate exists to enforce:
//! `tack-desktop` never depends on the server crates, and `tack-cli` never
//! pulls in a webview or GTK dependency. Both invoke real `cargo`
//! subcommands against the workspace this crate lives in, so a regression
//! here is a regression a human would see running the same commands by hand.

use std::process::Command;

/// No crate under `crates/tack-desktop` may depend on `tack-api`, `tack-db`,
/// `tack-orch` or `tack-runner` — this crate supervises the bundled `tack`
/// binary as a subprocess and speaks to it over HTTP only.
#[test]
fn tack_desktop_never_links_the_server_crates() {
    let metadata = cargo_metadata::MetadataCommand::new()
        .no_deps()
        .other_options(vec!["--offline".to_string()])
        .exec()
        .expect("cargo metadata should succeed for this workspace");

    let package = metadata
        .packages
        .iter()
        .find(|p| p.name.as_str() == "tack-desktop")
        .expect("tack-desktop must be a workspace member");

    let forbidden = ["tack-api", "tack-db", "tack-orch", "tack-runner"];
    let found: Vec<&str> = package
        .dependencies
        .iter()
        .map(|d| d.name.as_str())
        .filter(|name| forbidden.contains(name))
        .collect();

    assert!(
        found.is_empty(),
        "tack-desktop must never depend on the server crates, found: {found:?}"
    );
}

/// No webview or GTK dependency may enter an existing crate. `tack-cli` is
/// the one that matters most: its release build is a static musl binary for
/// headless hosts, and it must stay linkable there.
#[test]
fn tack_cli_stays_free_of_webview_and_gtk_dependencies() {
    // `--manifest-path` is required: this crate is its own workspace (see its
    // Cargo.toml), so `-p tack-cli` would not resolve from here otherwise.
    let output = Command::new("cargo")
        .args([
            "tree",
            "--manifest-path",
            "../../Cargo.toml",
            "-p",
            "tack-cli",
            "-e",
            "normal",
            "--offline",
        ])
        .output()
        .expect("cargo tree should run");
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let tree = String::from_utf8_lossy(&output.stdout).to_lowercase();
    let hits: Vec<&str> = tree
        .lines()
        .filter(|line| line.contains("tauri") || line.contains("webkit") || line.contains("gtk"))
        .collect();

    assert!(
        hits.is_empty(),
        "tack-cli's dependency tree must never mention tauri/webkit/gtk, found:\n{}",
        hits.join("\n")
    );
}

/// This crate duplicates `version` from the root `[workspace.package]` instead
/// of inheriting it, because it is not a member of that workspace. Duplication
/// that nothing checks is duplication that drifts, and a desktop bundle
/// claiming a version the server does not have is a support problem, so check
/// it. `tauri.conf.json` carries the same string a third time.
#[test]
fn the_desktop_version_matches_the_server_workspace() {
    let root = std::fs::read_to_string("../../Cargo.toml").expect("root manifest is readable");
    let workspace_version = root
        .split("[workspace.package]")
        .nth(1)
        .and_then(|section| section.lines().find(|l| l.starts_with("version = ")))
        .and_then(|line| line.split('"').nth(1))
        .expect("root [workspace.package] declares a version")
        .to_owned();

    assert_eq!(
        env!("CARGO_PKG_VERSION"),
        workspace_version,
        "tack-desktop's version drifted from the root workspace"
    );

    let tauri_config =
        std::fs::read_to_string("tauri.conf.json").expect("tauri.conf.json is readable");
    let declared: serde_json::Value =
        serde_json::from_str(&tauri_config).expect("tauri.conf.json is valid JSON");
    assert_eq!(
        declared["version"].as_str(),
        Some(workspace_version.as_str()),
        "tauri.conf.json's version drifted from the root workspace"
    );
}
