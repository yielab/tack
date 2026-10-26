//! Shared fixtures for `tack-runner`'s integration tests.

/// A uniquely prefixed temporary directory, deleted when the returned guard
/// drops — including when an assertion panics first.
pub fn temp_dir(label: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(label)
        .tempdir()
        .expect("temporary directory")
}
