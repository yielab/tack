//! What the loops and the merge need from the part of Tack that still serves HTTP: the GitHub
//! calls, the embedded runner's secret store, the prill that checks a signature and receives a
//! pull request, the stored files of an attempt and the board's live event channel. `tack-api` implements it over its own state; a method goes away from here when
//! the thing it reaches moves into this crate.

use async_trait::async_trait;
use tack_core::autonomy::CiState;
use uuid::Uuid;

#[async_trait]
pub trait Host: Send + Sync {
    /// The token GitHub calls for this project use: its own reference when it resolves, else the
    /// board's.
    async fn github_token(&self, project_id: Uuid) -> Option<String>;

    /// The CI state of commit `sha` in `repo`, or why GitHub could not be read.
    async fn ci_state(
        &self,
        client: &reqwest::Client,
        token: &str,
        repo: &str,
        sha: &str,
    ) -> Result<CiState, String>;

    /// Read-only resolution of a stored `secret_reference` (`store:<name>` or
    /// `env:<VARIABLE>`). `None` when nothing is wired in to resolve one; `Err` carries the
    /// failure, never the value.
    async fn resolve_secret(&self, reference: &str) -> Option<Result<String, String>>;

    /// Called once GitHub's report on pull request `number` of `repo` is stored.
    async fn pull_request_observed(&self, repo: &str, number: i64);

    /// Merge pull request `number` only if its head is still `sha`; the merge commit's sha, or
    /// the sentence that says why GitHub refused.
    async fn merge_pull_request(
        &self,
        client: &reqwest::Client,
        token: &str,
        repo: &str,
        number: i64,
        sha: &str,
    ) -> Result<String, String>;

    /// Open the pull request of an attempt that pushed a branch. Every refusal is logged and
    /// leaves no row.
    async fn open_pull_request(&self, attempt_id: &str);

    /// Move the request's item to Done when the request's status policy says an accepted
    /// merge-readiness pack does.
    async fn apply_mrp_accepted(&self, request_id: &str);

    /// Whether prill accepts `envelope` signed by `public_key_pem` for `commit`: the key id it
    /// names, or its reason.
    async fn verify_attestation(
        &self,
        envelope: &[u8],
        public_key_pem: &str,
        commit: &str,
    ) -> Result<String, String>;

    /// The bytes of a stored artifact, `None` when they cannot be read.
    async fn read_artifact(&self, reference: &str) -> Option<Vec<u8>>;

    /// Tell the open boards that an item moved.
    fn item_moved(&self, project_id: Uuid, item_id: Uuid, old_status: String, new_status: String);
}
