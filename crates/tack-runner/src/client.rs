use std::{fmt, time::Duration};

use async_trait::async_trait;

use crate::{EnrollmentCredential, RunnerError, Shutdown};

#[path = "engine.rs"]
pub mod engine;
#[path = "journal.rs"]
pub mod journal;
#[path = "workspace.rs"]
pub mod workspace;

pub use engine::{
    CancelObservation, EngineError, HarnessAdapter, HarnessError, HarnessOutcome, LocalRunHandle,
    RecoveryObservation, RunCycle, RunnerEngine,
};
pub use journal::{AttemptJournal, JournalError, JournalState, OwnerOnlyJournal, WorkspaceJournal};
pub use workspace::{
    CleanupResult, UnavailableWorktreeProvisioner, Workspace, WorkspaceError, WorkspaceManager,
    WorktreeProvisioner,
};

macro_rules! opaque_value {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

opaque_value!(
    RunnerId,
    "An opaque runner identity used by this local client seam."
);
opaque_value!(AttemptId, "An opaque execution attempt identity.");
opaque_value!(ClaimRequestId, "An opaque claim idempotency identity.");
opaque_value!(CompletionId, "An opaque completion idempotency identity.");
opaque_value!(
    CancellationRequestId,
    "An opaque cancellation idempotency identity."
);
opaque_value!(WorkspaceId, "An opaque isolated workspace identity.");
opaque_value!(Checkpoint, "An opaque event-stream checkpoint.");
opaque_value!(
    Timestamp,
    "An RFC 3339 timestamp preserved without local interpretation."
);

/// A lease token is intentionally not interchangeable with an opaque ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct FencingToken(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptState {
    Leased,
    Preparing,
    Running,
    WaitingDecision,
    Succeeded,
    Failed,
    Cancelled,
    Lost,
    NeedsOperator,
}

/// Runner protocol boundary retained from the B3 lifecycle skeleton.
#[async_trait]
pub trait RunnerProtocolClient: Send + Sync {
    async fn serve(&self, shutdown: Shutdown) -> Result<(), RunnerError>;
}

/// Credential returned once from enrollment. Formatting is always redacted.
#[derive(Clone, PartialEq, Eq)]
pub struct RunnerCredential(String);

impl RunnerCredential {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RunnerCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RunnerCredential([REDACTED])")
    }
}

impl fmt::Display for RunnerCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Clone)]
pub struct RunnerSession {
    pub runner_id: RunnerId,
    credential: RunnerCredential,
}

impl RunnerSession {
    pub fn new(runner_id: RunnerId, credential: RunnerCredential) -> Self {
        Self {
            runner_id,
            credential,
        }
    }

    pub fn credential(&self) -> &RunnerCredential {
        &self.credential
    }
}

impl fmt::Debug for RunnerSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RunnerSession")
            .field("runner_id", &self.runner_id)
            .field("credential", &self.credential)
            .finish()
    }
}

/// The small, immutable repository subset needed to reserve a local worktree.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RepositorySpec {
    pub remote: String,
    pub base_revision: String,
}

#[derive(Debug, Clone)]
pub struct EnrollmentRequest {
    pub runner_name: String,
    pub runner_version: String,
}

#[derive(Debug, Clone)]
pub struct EnrollmentResponse {
    pub session: RunnerSession,
    pub heartbeat_interval: Duration,
    pub lease_duration: Duration,
}

#[derive(Debug, Clone)]
pub struct ClaimRequest {
    pub claim_request_id: ClaimRequestId,
    pub available_capacity: u32,
    pub wait: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AttemptLease {
    pub attempt_id: AttemptId,
    pub runner_id: RunnerId,
    pub fencing_token: FencingToken,
    pub attempt_number: u32,
    pub state: AttemptState,
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,
}

#[derive(Debug, Clone)]
pub struct ClaimedWork {
    pub claim_request_id: ClaimRequestId,
    pub lease: AttemptLease,
    pub repository: RepositorySpec,
}

#[derive(Debug, Clone)]
pub enum ClaimResult {
    Work(ClaimedWork),
    NoWork {
        retry_after: Duration,
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct ActiveAttempt {
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub state: AttemptState,
    pub journal_state: JournalState,
    pub last_event_checkpoint: Option<Checkpoint>,
}

#[derive(Debug, Clone)]
pub struct HeartbeatRequest {
    pub heartbeat_id: String,
    pub available_capacity: u32,
    pub active_attempts: Vec<ActiveAttempt>,
}

#[derive(Debug, Clone)]
pub struct LeaseResult {
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub cancellation_requested: bool,
}

#[derive(Debug, Clone)]
pub struct HeartbeatResponse {
    pub lease_results: Vec<LeaseResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartPhase {
    Preparing,
    ProcessObservedRunning,
}

#[derive(Debug, Clone)]
pub struct StartReport {
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub phase: StartPhase,
    pub process_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CompletionReport {
    pub completion_id: CompletionId,
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub outcome: HarnessOutcome,
}

#[derive(Debug, Clone)]
pub struct CancellationReport {
    pub cancellation_request_id: CancellationRequestId,
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub observation: CancelObservation,
}

#[derive(Debug, Clone)]
pub struct RecoveryReport {
    pub attempt_id: AttemptId,
    pub fencing_token: FencingToken,
    pub observation: RecoveryObservation,
}

/// Typed transport seam. The frozen fixtures specify payloads but not an
/// operation-path map, so C3 must not invent an HTTP route authority ahead of
/// C2/C5 integration.
#[async_trait]
pub trait PullProtocol: Send + Sync {
    async fn enroll(
        &self,
        enrollment_credential: &EnrollmentCredential,
        request: EnrollmentRequest,
    ) -> Result<EnrollmentResponse, ProtocolClientError>;

    async fn claim(
        &self,
        session: &RunnerSession,
        request: ClaimRequest,
    ) -> Result<ClaimResult, ProtocolClientError>;

    async fn heartbeat(
        &self,
        session: &RunnerSession,
        request: HeartbeatRequest,
    ) -> Result<HeartbeatResponse, ProtocolClientError>;

    async fn report_start(
        &self,
        session: &RunnerSession,
        report: StartReport,
    ) -> Result<(), ProtocolClientError>;

    async fn report_completion(
        &self,
        session: &RunnerSession,
        report: CompletionReport,
    ) -> Result<(), ProtocolClientError>;

    async fn report_cancellation(
        &self,
        session: &RunnerSession,
        report: CancellationReport,
    ) -> Result<(), ProtocolClientError>;

    async fn observe_recovery(
        &self,
        session: &RunnerSession,
        report: RecoveryReport,
    ) -> Result<(), ProtocolClientError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolClientError {
    #[error("stale_lease")]
    StaleLease,
    #[error("runner_revoked")]
    RunnerRevoked,
    #[error("protocol rejected the request")]
    Rejected,
    #[error("runner protocol transport failed")]
    Transport,
}

/// Fails explicitly until a route-owning integration supplies a transport.
#[derive(Debug, Default)]
pub struct UnavailableProtocolClient;

#[async_trait]
impl RunnerProtocolClient for UnavailableProtocolClient {
    async fn serve(&self, _shutdown: Shutdown) -> Result<(), RunnerError> {
        Err(RunnerError::ProtocolUnavailable)
    }
}
