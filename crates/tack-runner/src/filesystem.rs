use std::{fs, path::Path};

use crate::RunnerError;
use crate::error::IoKind;

/// Filesystem boundary for local runner state. Future workspace and journal
/// implementations use this seam instead of reaching into global paths.
pub trait RunnerFilesystem: Send + Sync {
    fn prepare_state_dir(&self, path: &Path) -> Result<(), RunnerError>;
}

#[derive(Debug, Default)]
pub struct LocalFilesystem;

impl RunnerFilesystem for LocalFilesystem {
    fn prepare_state_dir(&self, path: &Path) -> Result<(), RunnerError> {
        fs::create_dir_all(path).map_err(|failure| RunnerError::Filesystem(IoKind(failure.kind())))
    }
}
