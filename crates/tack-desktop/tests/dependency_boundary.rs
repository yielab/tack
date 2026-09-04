//! Proves the two dependency rules this card exists to establish (§VII.1
//! rules 1–2). Both invoke real `cargo` subcommands against the workspace
//! this crate lives in, so a regression here is a regression a human would
//! see running the same commands by hand.

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
    let output = Command::new("cargo")
        .args(["tree", "-p", "tack-cli", "-e", "normal", "--offline"])
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
