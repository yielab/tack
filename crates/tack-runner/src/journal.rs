//! Owner-only local attempt journal.
//!
//! A record is made durable before an adapter is allowed to start a process.
//! It contains no runner credential or raw harness environment.

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use thiserror::Error;

use super::{AttemptId, AttemptLease, Checkpoint, FencingToken, RunnerId, WorkspaceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalState {
    Prepared,
    ProcessObservedRunning,
    CancellationRequested,
    RecoveryObserved,
    Reported,
    Quarantined,
}

impl JournalState {
    pub const fn is_unresolved(self) -> bool {
        matches!(
            self,
            Self::Prepared | Self::ProcessObservedRunning | Self::CancellationRequested
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceJournal {
    pub workspace_id: WorkspaceId,
    pub path: PathBuf,
    pub base_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AttemptJournal {
    pub attempt_id: AttemptId,
    pub runner_id: RunnerId,
    pub fencing_token: FencingToken,
    pub workspace: WorkspaceJournal,
    pub state: JournalState,
    pub process_id: Option<String>,
    pub last_event_checkpoint: Option<Checkpoint>,
}

impl AttemptJournal {
    pub fn prepared(lease: &AttemptLease, workspace: WorkspaceJournal) -> Self {
        Self {
            attempt_id: lease.attempt_id.clone(),
            runner_id: lease.runner_id.clone(),
            fencing_token: lease.fencing_token,
            workspace,
            state: JournalState::Prepared,
            process_id: None,
            last_event_checkpoint: None,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum JournalError {
    #[error("runner journal could not be initialized")]
    Initialization,
    #[error("runner journal record already exists")]
    AlreadyExists,
    #[error("runner journal record is missing")]
    Missing,
    #[error("runner journal could not be serialized")]
    Serialization,
    #[error("runner journal is malformed")]
    Malformed,
    #[error("runner journal operation failed")]
    Io,
}

/// Durable storage rooted in local runner state. File names are hex-encoded
/// opaque attempt IDs, so input cannot traverse outside the journal directory.
#[derive(Debug, Clone)]
pub struct OwnerOnlyJournal {
    root: PathBuf,
}

impl OwnerOnlyJournal {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn journal_path(&self, attempt_id: &AttemptId) -> PathBuf {
        self.journal_dir()
            .join(format!("{}.toml", encode_id(attempt_id.as_str())))
    }

    pub fn persist_before_spawn(&self, record: &AttemptJournal) -> Result<(), JournalError> {
        self.ensure_layout()?;
        let bytes = toml::to_string(record).map_err(|_| JournalError::Serialization)?;
        atomic_create_private(&self.journal_path(&record.attempt_id), bytes.as_bytes())
    }

    pub fn update(&self, record: &AttemptJournal) -> Result<(), JournalError> {
        self.ensure_layout()?;
        let path = self.journal_path(&record.attempt_id);
        if !path.exists() {
            return Err(JournalError::Missing);
        }
        let bytes = toml::to_string(record).map_err(|_| JournalError::Serialization)?;
        atomic_replace_private(&path, bytes.as_bytes())
    }

    pub fn load(&self, attempt_id: &AttemptId) -> Result<AttemptJournal, JournalError> {
        self.load_path(&self.journal_path(attempt_id))
    }

    pub fn unresolved(&self) -> Result<Vec<AttemptJournal>, JournalError> {
        self.ensure_layout()?;
        let mut records = Vec::new();
        for entry in fs::read_dir(self.journal_dir()).map_err(|_| JournalError::Io)? {
            let entry = entry.map_err(|_| JournalError::Io)?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("toml") {
                continue;
            }
            if fs::symlink_metadata(&path)
                .map_err(|_| JournalError::Io)?
                .file_type()
                .is_symlink()
            {
                return Err(JournalError::Malformed);
            }
            let record = self.load_path(&path)?;
            if record.state.is_unresolved() {
                records.push(record);
            }
        }
        records.sort_by(|left, right| left.attempt_id.as_str().cmp(right.attempt_id.as_str()));
        Ok(records)
    }

    /// Preserve uncertain process ownership evidence rather than deleting it.
    pub fn quarantine(&self, mut record: AttemptJournal) -> Result<(), JournalError> {
        record.state = JournalState::Quarantined;
        self.update(&record)?;
        let destination = self
            .quarantine_dir()
            .join(format!("{}.toml", encode_id(record.attempt_id.as_str())));
        fs::rename(self.journal_path(&record.attempt_id), destination)
            .map_err(|_| JournalError::Io)?;
        sync_directory(&self.quarantine_dir())
    }

    fn load_path(&self, path: &Path) -> Result<AttemptJournal, JournalError> {
        let contents = fs::read_to_string(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                JournalError::Missing
            } else {
                JournalError::Io
            }
        })?;
        toml::from_str(&contents).map_err(|_| JournalError::Malformed)
    }

    fn journal_dir(&self) -> PathBuf {
        self.root.join("journal")
    }

    fn quarantine_dir(&self) -> PathBuf {
        self.root.join("quarantine")
    }

    fn ensure_layout(&self) -> Result<(), JournalError> {
        for directory in [&self.root, &self.journal_dir(), &self.quarantine_dir()] {
            fs::create_dir_all(directory).map_err(|_| JournalError::Initialization)?;
            make_owner_only(directory).map_err(|_| JournalError::Initialization)?;
        }
        Ok(())
    }
}

fn encode_id(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn atomic_create_private(path: &Path, bytes: &[u8]) -> Result<(), JournalError> {
    let temporary = write_private_temporary(path, bytes)?;
    match fs::hard_link(&temporary, path) {
        Ok(()) => {
            fs::remove_file(&temporary).map_err(|_| JournalError::Io)?;
            sync_directory(path.parent().ok_or(JournalError::Io)?)
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                Err(JournalError::AlreadyExists)
            } else {
                Err(JournalError::Io)
            }
        }
    }
}

fn atomic_replace_private(path: &Path, bytes: &[u8]) -> Result<(), JournalError> {
    let temporary = write_private_temporary(path, bytes)?;
    fs::rename(&temporary, path).map_err(|_| JournalError::Io)?;
    make_owner_only(path).map_err(|_| JournalError::Io)?;
    sync_directory(path.parent().ok_or(JournalError::Io)?)
}

fn write_private_temporary(path: &Path, bytes: &[u8]) -> Result<PathBuf, JournalError> {
    let name = path.file_name().ok_or(JournalError::Io)?;
    let temporary = path.with_file_name(format!(
        ".{}.{}.tmp",
        name.to_string_lossy(),
        std::process::id()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| JournalError::Io)?;
    make_owner_only(&temporary).map_err(|_| JournalError::Io)?;
    file.write_all(bytes).map_err(|_| JournalError::Io)?;
    file.sync_all().map_err(|_| JournalError::Io)?;
    Ok(temporary)
}

fn sync_directory(path: &Path) -> Result<(), JournalError> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| JournalError::Io)
}

#[cfg(unix)]
fn make_owner_only(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(
        path,
        fs::Permissions::from_mode(if path.is_file() { 0o600 } else { 0o700 }),
    )
}

#[cfg(not(unix))]
fn make_owner_only(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;
    use crate::client::{AttemptState, Timestamp};

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    fn temporary_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "tack-runner-journal-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::SeqCst)
        ))
    }

    fn record() -> AttemptJournal {
        let lease = AttemptLease {
            attempt_id: AttemptId::new("att/opaque"),
            runner_id: RunnerId::new("runner"),
            fencing_token: FencingToken(7),
            attempt_number: 1,
            state: AttemptState::Leased,
            issued_at: Timestamp::new("2026-08-06T12:20:00Z"),
            expires_at: Timestamp::new("2026-08-06T12:21:00Z"),
        };
        AttemptJournal::prepared(
            &lease,
            WorkspaceJournal {
                workspace_id: WorkspaceId::new("ws"),
                path: PathBuf::from("workspace"),
                base_revision: "revision".into(),
            },
        )
    }

    #[test]
    fn pre_spawn_journal_is_atomic_owner_only_and_recoverable() {
        let root = temporary_root();
        let journal = OwnerOnlyJournal::new(&root);
        let record = record();
        journal
            .persist_before_spawn(&record)
            .expect("persist journal");

        assert_eq!(
            journal.load(&record.attempt_id).expect("load journal"),
            record
        );
        assert_eq!(
            journal.unresolved().expect("recovery scan"),
            vec![record.clone()]
        );
        assert!(matches!(
            journal.persist_before_spawn(&record),
            Err(JournalError::AlreadyExists)
        ));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(journal.journal_path(&record.attempt_id))
                .expect("journal metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0, "journal is owner-only");
        }
        fs::remove_dir_all(root).expect("remove temporary journal root");
    }
}
