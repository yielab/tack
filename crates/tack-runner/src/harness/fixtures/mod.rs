//! Rust-side accessors for the shared fake harness fixture
//! (`fake_harness.sh`, documented in full at the top of that file).
//!
//! D1/D2/D3 drive this exact fixture from their own crash/fake tests: spawn
//! it through [`super::process::ProcessSpec`] with `TACK_FAKE_HARNESS_MODE`
//! (and mode-specific variables) set in `ProcessSpec::env`, exactly as this
//! module's own tests and `process.rs`'s tests already do. Nothing here is
//! specific to D4 — this is the intentionally-reusable half of the card.

use std::path::PathBuf;

/// Absolute path to the fixture script itself, resolved at compile time
/// against this crate's own manifest directory so it does not depend on the
/// process's current working directory at test time.
pub fn fake_harness_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/harness/fixtures/fake_harness.sh")
}

/// The `(program, args)` pair to put directly into
/// [`super::process::ProcessSpec`]. Always invokes the script through an
/// absolute `/bin/sh` rather than executing the file directly: this avoids
/// depending on the script's executable bit surviving every checkout/copy,
/// and sidesteps `PATH` lookup entirely (`ProcessSpec::spawn` always starts
/// the child from a cleared environment, so an unqualified program name has
/// no `PATH` to search).
pub fn fake_harness_command() -> (PathBuf, Vec<String>) {
    (
        PathBuf::from("/bin/sh"),
        vec![fake_harness_path().to_string_lossy().into_owned()],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_script_exists_and_is_executable_on_disk() {
        let path = fake_harness_path();
        let metadata = std::fs::metadata(&path)
            .unwrap_or_else(|_| panic!("fake harness fixture missing at {}", path.display()));
        assert!(metadata.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                metadata.permissions().mode() & 0o111,
                0,
                "fixture script should carry an executable bit, even though \
                 fake_harness_command() does not rely on it"
            );
        }
    }

    #[test]
    fn fake_harness_command_invokes_the_fixture_through_sh() {
        let (program, args) = fake_harness_command();
        assert_eq!(program, PathBuf::from("/bin/sh"));
        assert_eq!(
            args,
            vec![fake_harness_path().to_string_lossy().into_owned()]
        );
    }
}
