//! Rust-side accessors for the shared fake harness fixture (`fake_harness.sh`).
//! Reusable across harness kinds: spawn via `ProcessSpec` with `TACK_FAKE_HARNESS_MODE` set.

use std::path::PathBuf;

/// Absolute path to the fixture script, resolved at compile time.
pub fn fake_harness_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/harness/fixtures/fake_harness.sh")
}

/// `(program, args)` for `ProcessSpec` — invoked via `/bin/sh`, never directly.
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
