//! The leases this runner holds right now, shared by the attempts that hold
//! them and the one ticker that renews them all with a single heartbeat.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use tokio::sync::watch;

use crate::client::{
    ActiveAttempt, AttemptId, AttemptJournal, AttemptState, Checkpoint, FencingToken, JournalState,
    LeaseResult,
};

struct Entry {
    fencing_token: FencingToken,
    journal_state: JournalState,
    last_event_checkpoint: Option<Checkpoint>,
    cancel: watch::Sender<bool>,
}

struct Registry {
    total: u32,
    entries: Mutex<BTreeMap<AttemptId, Entry>>,
}

/// A shared registry of the leases in flight. Clones see the same attempts.
#[derive(Clone)]
pub struct ActiveAttempts {
    registry: Arc<Registry>,
}

impl ActiveAttempts {
    /// A registry for a runner that works up to `total` attempts at once.
    pub fn new(total: u32) -> Self {
        Self {
            registry: Arc::new(Registry {
                total,
                entries: Mutex::new(BTreeMap::new()),
            }),
        }
    }

    fn entries(&self) -> std::sync::MutexGuard<'_, BTreeMap<AttemptId, Entry>> {
        self.registry
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Records the lease `record` holds. The receiver turns `true` when the
    /// server asks for the attempt to be cancelled.
    pub fn register(&self, record: &AttemptJournal) -> watch::Receiver<bool> {
        let (cancel, watcher) = watch::channel(false);
        self.entries().insert(
            record.attempt_id.clone(),
            Entry {
                fencing_token: record.fencing_token,
                journal_state: record.state,
                last_event_checkpoint: record.last_event_checkpoint.clone(),
                cancel,
            },
        );
        watcher
    }

    /// Brings the journal state and checkpoint the next heartbeat carries up
    /// to date with `record`.
    pub fn sync(&self, record: &AttemptJournal) {
        if let Some(entry) = self.entries().get_mut(&record.attempt_id) {
            entry.journal_state = record.state;
            entry.last_event_checkpoint = record.last_event_checkpoint.clone();
        }
    }

    pub fn remove(&self, attempt_id: &AttemptId) {
        self.entries().remove(attempt_id);
    }

    /// How many attempts hold a lease.
    pub fn len(&self) -> usize {
        self.entries().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries().is_empty()
    }

    /// The most attempts this runner works at once.
    pub fn total(&self) -> u32 {
        self.registry.total
    }

    /// Attempts this runner can still take on, as a heartbeat reports it.
    pub fn free(&self) -> u32 {
        let held = u32::try_from(self.len()).unwrap_or(u32::MAX);
        self.registry.total.saturating_sub(held)
    }

    /// Every lease held, as a heartbeat's `active_attempts`.
    pub fn snapshot(&self) -> Vec<ActiveAttempt> {
        self.entries()
            .iter()
            .map(|(attempt_id, entry)| ActiveAttempt {
                attempt_id: attempt_id.clone(),
                fencing_token: entry.fencing_token,
                state: AttemptState::Running,
                journal_state: entry.journal_state,
                last_event_checkpoint: entry.last_event_checkpoint.clone(),
            })
            .collect()
    }

    /// Turns each lease result that asks for cancellation into a signal to
    /// the attempt it names, and only that attempt.
    pub fn apply(&self, lease_results: &[LeaseResult]) {
        let entries = self.entries();
        for result in lease_results.iter().filter(|r| r.cancellation_requested) {
            if let Some(entry) = entries
                .get(&result.attempt_id)
                .filter(|entry| entry.fencing_token == result.fencing_token)
            {
                entry.cancel.send_replace(true);
            }
        }
    }
}
