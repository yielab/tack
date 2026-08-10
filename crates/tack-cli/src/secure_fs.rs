//! Atomic, owner-only file writes for anything on disk that carries a
//! credential: the bearer token in `~/.tackrc` (`config::save`) and a saved
//! runner enrollment token (`tack runner enroll --out`).
//!
//! Mirrors `tack-runner`'s `journal.rs` pattern (write-temp, `fsync`, rename
//! into place, then re-assert owner-only permissions) rather than inventing a
//! second one: write directly to the destination and a crash mid-write can
//! leave a torn file readable by whatever the process umask allowed; write to
//! a sibling temp file created with `0600` from the start and `rename` over
//! the target, and the destination is either the old complete contents or
//! the new complete contents, never partial, and never briefly
//! world/group-readable.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

/// Write `bytes` to `path` atomically, creating or replacing it, with
/// owner-only (`0600`) permissions on every platform this compiles for.
pub fn write_owner_only_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    let temp_path = dir.join(format!(
        ".{}.{}.tmp",
        file_name.to_string_lossy(),
        std::process::id()
    ));

    {
        let mut file = open_owner_only_new(&temp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    // Best-effort cleanup if the rename below fails partway.
    let result = fs::rename(&temp_path, path).and_then(|()| make_owner_only(path));
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

#[cfg(unix)]
fn open_owner_only_new(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_owner_only_new(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    make_owner_only(path)?;
    Ok(file)
}

#[cfg(unix)]
fn make_owner_only(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn make_owner_only(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_full_content() {
        let dir = std::env::temp_dir().join(format!("tack-securefs-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secret.toml");

        write_owner_only_atomic(&path, b"token = \"abc\"\n").unwrap();

        let read = fs::read_to_string(&path).unwrap();
        assert_eq!(read, "token = \"abc\"\n");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn overwrite_replaces_content_and_leaves_no_temp_file() {
        let dir = std::env::temp_dir().join(format!(
            "tack-securefs-test-overwrite-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secret.toml");

        write_owner_only_atomic(&path, b"first\n").unwrap();
        write_owner_only_atomic(&path, b"second\n").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "second\n");
        let leftover: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftover.is_empty(), "temp file left behind: {leftover:?}");
        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir =
            std::env::temp_dir().join(format!("tack-securefs-test-perms-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secret.toml");

        write_owner_only_atomic(&path, b"token = \"abc\"\n").unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "expected 0600, got {:o}", mode & 0o777);
        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn overwriting_a_looser_permission_file_still_ends_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir =
            std::env::temp_dir().join(format!("tack-securefs-test-loose-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secret.toml");

        // Simulate a pre-existing world-readable file (e.g. written before
        // this module existed, or under a permissive umask).
        fs::write(&path, b"old\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        write_owner_only_atomic(&path, b"new\n").unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(fs::read_to_string(&path).unwrap(), "new\n");
        fs::remove_dir_all(&dir).ok();
    }
}
