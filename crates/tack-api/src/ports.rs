//! The ports `tack-app` calls back into, over the API's own state: the prill the board runs, the
//! artifact store and the board's event channel (`tack_app::ports`), and the GitHub the board
//! is configured with. An event goes to the in-process bus every consumer subscribes to, and,
//! when it has a message the open boards understand, to the WebSocket as that message.

use std::sync::Arc;

use async_trait::async_trait;
use tack_app::events::{DomainEvent, Fact};
use tack_app::github_sync::GitHub;
use tack_app::ports::{Artifacts, DomainEvents, Ports, Verifier, VerifierError};
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::handlers::runner_protocol::artifact_storage::ArtifactStorage;
use crate::handlers::websocket::{self, BoardEvent};
use crate::router::AppState;

/// What `tack-app` reaches through [`Ports`], over a running board's state.
#[derive(Clone)]
struct Adapter(AppState);

impl AppState {
    /// The GitHub this board is configured with: its API root and the environment's token.
    pub fn github(&self) -> GitHub {
        self.config.loops.github()
    }

    /// A project's GitHub token: its own `github_token_ref` resolved through the embedded
    /// runner's secret store, else the environment's. See [`GitHub::token_for_project`].
    pub async fn github_token(&self, project_id: Uuid) -> Option<String> {
        self.github()
            .token_for_project(&self.repo, self.local_runner.as_deref(), project_id)
            .await
    }

    /// The [`Ports`] `tack-app`'s loops and merge call into.
    pub fn ports(&self) -> Ports {
        let adapter = Arc::new(Adapter(self.clone()));
        Ports {
            github: self.github(),
            runner: self.local_runner.clone(),
            verifier: adapter.clone(),
            artifacts: adapter.clone(),
            events: adapter,
        }
    }

    /// The board's event channel, over this state's bus and WebSocket.
    pub fn events(&self) -> Arc<dyn DomainEvents> {
        Arc::new(Adapter(self.clone()))
    }

    /// The stored files of the attempts this board holds.
    pub(crate) fn artifact_storage(&self) -> ArtifactStorage {
        ArtifactStorage::new(format!("{}/execution-artifacts", self.config.storage_dir))
    }
}

#[async_trait]
impl Verifier for Adapter {
    async fn verify_attestation(
        &self,
        envelope: &[u8],
        public_key_pem: &str,
        commit: &str,
    ) -> Result<String, VerifierError> {
        crate::prill::attest_verify(&self.0.prill().await, envelope, public_key_pem, commit).await
    }

    async fn pull_request_observed(&self, repo: &str, number: i64) {
        crate::prill_store::observed(&self.0, repo, number).await;
    }
}

#[async_trait]
impl Artifacts for Adapter {
    async fn read(&self, reference: &str) -> Option<Vec<u8>> {
        let mut file = self
            .0
            .artifact_storage()
            .open_for_read(reference)
            .await
            .ok()?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).await.ok()?;
        Some(bytes)
    }
}

impl DomainEvents for Adapter {
    fn emit(&self, event: DomainEvent) {
        if let Some(message) = board_message(&event) {
            websocket::broadcast_event(&self.0, message);
        }
        self.0.event_bus.emit(event);
    }

    fn subscribe(&self) -> Option<tokio::sync::broadcast::Receiver<DomainEvent>> {
        Some(self.0.event_bus.subscribe())
    }
}

/// What the open boards are told of an event: the message they have always understood, or
/// nothing for an event they have no message for (a run, a decision).
fn board_message(event: &DomainEvent) -> Option<BoardEvent> {
    let project_id = event.project_id;
    Some(match &event.fact {
        Fact::ItemCreated { item_id, status } => BoardEvent::ItemCreated {
            project_id,
            item_id: *item_id,
            status: status.clone(),
        },
        Fact::ItemUpdated { item_id, status } => BoardEvent::ItemUpdated {
            project_id,
            item_id: *item_id,
            old_status: Some(status.clone()),
            new_status: status.clone(),
        },
        Fact::ItemMoved { item_id, from, to } => BoardEvent::ItemUpdated {
            project_id,
            item_id: *item_id,
            old_status: Some(from.clone()),
            new_status: to.clone(),
        },
        Fact::ItemDeleted { item_id } => BoardEvent::ItemDeleted {
            project_id,
            item_id: *item_id,
        },
        Fact::RunStarted { .. }
        | Fact::RunFinished { .. }
        | Fact::RunFailed { .. }
        | Fact::RunWaiting { .. }
        | Fact::DecisionOpened { .. }
        | Fact::DecisionResolved { .. } => return None,
    })
}
