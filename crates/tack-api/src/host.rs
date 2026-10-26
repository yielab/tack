//! `tack_app::Host` over the API's own state: the GitHub calls, prill, the artifact store and the
//! board's event channel that `tack-app`'s loops and merge reach through it.

use async_trait::async_trait;
use tack_app::host::Host;
use tack_core::autonomy::CiState;
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use crate::handlers::runner_protocol::artifact_storage::ArtifactStorage;
use crate::handlers::websocket::{self, BoardEvent};
use crate::router::AppState;

/// The host of a running board.
#[derive(Clone)]
pub struct AppHost(AppState);

impl AppHost {
    pub fn new(state: AppState) -> Self {
        Self(state)
    }

    fn storage(&self) -> ArtifactStorage {
        ArtifactStorage::new(format!("{}/execution-artifacts", self.0.config.storage_dir))
    }
}

impl AppState {
    /// The [`Host`] `tack-app` calls back into.
    pub fn host(&self) -> AppHost {
        AppHost::new(self.clone())
    }
}

#[async_trait]
impl Host for AppHost {
    async fn github_token(&self, project_id: Uuid) -> Option<String> {
        crate::github_sync::github_token_for_project(&self.0, project_id).await
    }

    async fn ci_state(
        &self,
        client: &reqwest::Client,
        token: &str,
        repo: &str,
        sha: &str,
    ) -> Result<CiState, String> {
        let base = self.0.config.github_api_base.as_str();
        crate::github_sync::ci_state(client, base, token, repo, sha)
            .await
            .map_err(|error| error.to_string())
    }

    async fn merge_pull_request(
        &self,
        client: &reqwest::Client,
        token: &str,
        repo: &str,
        number: i64,
        sha: &str,
    ) -> Result<String, String> {
        let base = self.0.config.github_api_base.as_str();
        crate::github_sync::merge_pull_request(client, base, token, repo, number, sha)
            .await
            .map_err(|refused| refused.0)
    }

    async fn open_pull_request(&self, attempt_id: &str) {
        crate::handlers::pull_requests::open_pull_request_for_attempt(
            self.0.clone(),
            &self.storage(),
            attempt_id,
        )
        .await;
    }

    async fn apply_mrp_accepted(&self, request_id: &str) {
        let _ = crate::handlers::mrp::apply_mrp_accepted(&self.0, request_id).await;
    }

    async fn verify_attestation(
        &self,
        envelope: &[u8],
        public_key_pem: &str,
        commit: &str,
    ) -> Result<String, String> {
        crate::prill::attest_verify(
            &self.0.prill_program().await,
            envelope,
            public_key_pem,
            commit,
        )
        .await
    }

    async fn read_artifact(&self, reference: &str) -> Option<Vec<u8>> {
        let mut file = self.storage().open_for_read(reference).await.ok()?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).await.ok()?;
        Some(bytes)
    }

    fn item_moved(&self, project_id: Uuid, item_id: Uuid, old_status: String, new_status: String) {
        websocket::broadcast_event(
            &self.0,
            BoardEvent::ItemUpdated {
                project_id,
                item_id,
                old_status: Some(old_status),
                new_status,
            },
        );
    }
}
