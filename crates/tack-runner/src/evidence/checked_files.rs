//! The files that decide how a piece of work is checked.
//!
//! An agent that changes one of these changes the question the verifier
//! answers, so the pack names each such change on its own instead of leaving
//! it among the rest of the diff.

/// The heading the pack gives these changes.
pub const SECTION: &str = "The agent changed how it is checked";

/// Whether `path` (workspace-relative, `/`-separated) is one of the files
/// that decide how the work is checked: everything under `.github/`, any
/// file or directory with `test` in its name, and `package.json` (where the
/// scripts live), `Cargo.toml`, `Makefile`, `justfile` and `prill.yaml` at any
/// depth.
pub fn matches(path: &str) -> bool {
    let path = path.trim_start_matches("./");
    if path.starts_with(".github/") {
        return true;
    }
    let lower = path.to_ascii_lowercase();
    if lower.split('/').any(|segment| segment.contains("test")) {
        return true;
    }
    let name = lower.rsplit('/').next().unwrap_or_default();
    matches!(
        name,
        "package.json" | "cargo.toml" | "makefile" | "justfile" | "prill.yaml"
    )
}
