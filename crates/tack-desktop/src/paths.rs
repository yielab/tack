//! Computes the per-user data root and the four `TACK_*` folder locations
//! under it, plus the app's own settings.json path.
//!
//! The root is `dirs::data_dir()` + `tack` (lowercase on every OS), created
//! `0700` on Unix. Storage, runner state and the log file always live under
//! that root; only the database path can move, via a `settings.json`
//! override (see [`crate::first_run`]).

use std::path::{Path, PathBuf};

use crate::supervisor::ServerFolders;

const APP_DIR_NAME: &str = "tack";

#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    #[error("could not determine this OS's per-user data directory")]
    NoDataDir,
    #[error("failed to create the data directory {path}: {source}")]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
    #[cfg(unix)]
    #[error("failed to set permissions on {path}: {source}")]
    SetPermissions {
        path: PathBuf,
        source: std::io::Error,
    },
}

#[derive(Debug, Clone)]
pub struct DataPaths {
    pub root: PathBuf,
    pub settings_file: PathBuf,
}

impl DataPaths {
    /// Resolves the real per-OS root via `dirs::data_dir()` and ensures it
    /// exists. Fails closed (`NoDataDir`) rather than falling back to the
    /// current directory when the OS cannot answer.
    pub fn resolve() -> Result<Self, PathsError> {
        let base = dirs::data_dir().ok_or(PathsError::NoDataDir)?;
        Self::from_base(&base)
    }

    /// The join-and-create logic on its own, independent of `dirs::data_dir`,
    /// so it can be exercised against a representative base directory for
    /// any OS from a single test host.
    fn from_base(base: &Path) -> Result<Self, PathsError> {
        let root = root_from_base(base);
        ensure_dir(&root)?;
        let settings_file = root.join("settings.json");
        Ok(Self {
            root,
            settings_file,
        })
    }

    pub fn default_database_url(&self) -> String {
        format!("sqlite:{}/tack.db?mode=rwc", self.root.display())
    }

    pub fn storage_dir(&self) -> PathBuf {
        self.root.join("storage")
    }

    pub fn runner_state_dir(&self) -> PathBuf {
        self.root.join("runner")
    }

    pub fn log_file(&self) -> PathBuf {
        self.root.join("logs/tack.log")
    }

    /// Builds the four `TACK_*` folder variables for the sidecar. Applies
    /// `database_override` when present (the settings.json "use an existing
    /// tack.db" choice); storage, runner state and the log stay pinned under
    /// the root regardless, since only the database location is ever a
    /// user choice.
    pub fn server_folders(&self, database_override: Option<&Path>) -> ServerFolders {
        let database_url = match database_override {
            Some(path) => format!("sqlite:{}?mode=rwc", path.display()),
            None => self.default_database_url(),
        };
        ServerFolders {
            database_url,
            storage_dir: self.storage_dir(),
            runner_state_dir: self.runner_state_dir(),
            log_file: self.log_file(),
        }
    }
}

fn root_from_base(base: &Path) -> PathBuf {
    base.join(APP_DIR_NAME)
}

fn ensure_dir(path: &Path) -> Result<(), PathsError> {
    std::fs::create_dir_all(path).map_err(|source| PathsError::CreateDir {
        path: path.to_path_buf(),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(
            |source| PathsError::SetPermissions {
                path: path.to_path_buf(),
                source,
            },
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The join logic itself (this crate's own computation, not
    /// `dirs::data_dir`'s per-OS lookup) against a representative base
    /// directory for each OS `dirs::data_dir()` can return. Proves the
    /// pinned "lowercase tack on every OS" rule without needing a machine of
    /// each kind.
    #[test]
    fn root_appends_lowercase_tack_to_any_os_base() {
        assert_eq!(
            root_from_base(Path::new("/home/alice/.local/share")),
            PathBuf::from("/home/alice/.local/share/tack")
        );
        assert_eq!(
            root_from_base(Path::new("/Users/Alice/Library/Application Support")),
            PathBuf::from("/Users/Alice/Library/Application Support/tack")
        );
        assert_eq!(
            root_from_base(Path::new(r"C:\Users\Alice\AppData\Roaming")),
            PathBuf::from(r"C:\Users\Alice\AppData\Roaming").join("tack")
        );
    }

    #[test]
    fn from_base_creates_the_root_and_derives_the_four_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("data-home");
        let paths = DataPaths::from_base(&base).unwrap();

        assert!(paths.root.is_dir());
        assert_eq!(paths.root, base.join("tack"));
        assert_eq!(paths.settings_file, paths.root.join("settings.json"));
        assert_eq!(paths.storage_dir(), paths.root.join("storage"));
        assert_eq!(paths.runner_state_dir(), paths.root.join("runner"));
        assert_eq!(paths.log_file(), paths.root.join("logs/tack.log"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&paths.root).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700, "root must be 0700 on Unix");
        }
    }

    #[test]
    fn from_base_is_idempotent_when_the_root_already_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("data-home");
        DataPaths::from_base(&base).unwrap();
        let paths = DataPaths::from_base(&base).unwrap();
        assert!(paths.root.is_dir());
    }

    #[test]
    fn server_folders_defaults_to_the_pinned_database_under_root() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("data-home");
        let paths = DataPaths::from_base(&base).unwrap();

        let folders = paths.server_folders(None);
        assert_eq!(
            folders.database_url,
            format!("sqlite:{}/tack.db?mode=rwc", paths.root.display())
        );
        assert_eq!(folders.storage_dir, paths.storage_dir());
    }

    #[test]
    fn server_folders_applies_a_database_override_and_leaves_the_rest_pinned() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("data-home");
        let paths = DataPaths::from_base(&base).unwrap();
        let chosen = tmp.path().join("elsewhere/existing.db");

        let folders = paths.server_folders(Some(&chosen));
        assert_eq!(
            folders.database_url,
            format!("sqlite:{}?mode=rwc", chosen.display())
        );
        // Storage, runner state and the log are never affected by the
        // database override.
        assert_eq!(folders.storage_dir, paths.storage_dir());
        assert_eq!(folders.runner_state_dir, paths.runner_state_dir());
        assert_eq!(folders.log_file, paths.log_file());
    }
}
