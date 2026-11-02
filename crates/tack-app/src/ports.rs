//! What the loops and the merge need from the part of Tack that still serves HTTP, as small
//! ports: the prill that checks a signature and receives a pull request ([`Verifier`]), the
//! stored files of an attempt ([`Artifacts`]) and the board's live event channel
//! ([`DomainEvents`]). `tack-api` implements each over its own state; a port stays a trait only
//! where its implementor names an HTTP type this crate may not (`ArtifactStorage`'s body
//! stream, the WebSocket channel, the prill process the API runs) and a test can stand a fake
//! in its place. The GitHub calls are [`crate::github_sync::GitHub`], the embedded runner's
//! secret store is [`LocalRunnerControl`]: both live in this crate, so neither is a port.

use std::sync::Arc;

use async_trait::async_trait;

use crate::events::DomainEvent;
use crate::github_sync::GitHub;
use crate::local_runner::LocalRunnerControl;

/// What the board says when it cannot start prill; every caller reads it from here.
pub const NOT_INSTALLED: &str = "The verifier is not installed on the board. An administrator sets it up: see the Administrator Guide";

/// Why prill did not accept an attestation. The text is what the merge record and the item show.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VerifierError {
    #[error("{}", NOT_INSTALLED)]
    NotInstalled,
    #[error("the board could not stage the attestation")]
    CouldNotStage,
    /// prill ran and refused; carries its reason.
    #[error("{0}")]
    Refused(String),
}

/// The prill the board runs.
#[async_trait]
pub trait Verifier: Send + Sync {
    /// Whether prill accepts `envelope` signed by `public_key_pem` for `commit`: the key id it
    /// names, or why not.
    async fn verify_attestation(
        &self,
        envelope: &[u8],
        public_key_pem: &str,
        commit: &str,
    ) -> Result<String, VerifierError>;

    /// Called once GitHub's report on pull request `number` of `repo` is stored: tell prill what
    /// became of it. A failure is logged by the implementor and tried again on the next tick.
    async fn pull_request_observed(&self, repo: &str, number: i64);
}

/// The stored files of an attempt.
#[async_trait]
pub trait Artifacts: Send + Sync {
    /// The bytes behind `reference`, `None` when they cannot be read.
    async fn read(&self, reference: &str) -> Option<Vec<u8>>;
}

/// The board's event channel: where every consumer, the open boards among them, hears what
/// happened (see [`crate::events`]).
pub trait DomainEvents: Send + Sync {
    /// Tell every listener that `event` became true. It never blocks and never fails.
    fn emit(&self, event: DomainEvent);

    /// A receiver of everything emitted from now on, for a consumer that reads events on its
    /// own schedule; `None` for a channel nobody can listen to.
    fn subscribe(&self) -> Option<tokio::sync::broadcast::Receiver<DomainEvent>> {
        None
    }
}

/// Everything a tick or a merge reaches outside the store, built once per pass by the caller.
#[derive(Clone)]
pub struct Ports {
    pub github: GitHub,
    /// The embedded runner, which holds the secret store GitHub token references resolve
    /// through; `None` when none is wired in.
    pub runner: Option<Arc<dyn LocalRunnerControl>>,
    pub verifier: Arc<dyn Verifier>,
    pub artifacts: Arc<dyn Artifacts>,
    pub events: Arc<dyn DomainEvents>,
}
