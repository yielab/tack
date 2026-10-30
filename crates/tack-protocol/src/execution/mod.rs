//! Harness-neutral execution protocol domain for runner protocol v1.
//!
//! This module deliberately has no transport, persistence, or vendor adapter
//! dependencies. It represents the immutable values exchanged by the runner
//! protocol and is the seam consumed by the API, database, and runner
//! layers.

mod capabilities;
mod lifecycle;
mod messages;
mod types;

// `CapabilityLimits` is exported here since it's the type of the public
// field `RunnerCapabilities::limits`, so downstream crates can name it.
pub use capabilities::{
    CapabilityLimits, CapabilitySupport, CapabilityValue, Concurrency, DocsLinks,
    EmbeddedCapabilitySnapshot, FeatureCapabilities, HarnessCapability, ModelListSource,
    ModelsReport, RunnerCapabilities, SignInReport, SignInState, TeamReport, TeamViolation,
};
pub use lifecycle::{
    LifecycleError, TransitionActor, lease_held, sources, terminal, validate_transition,
};
pub use messages::{
    ActiveAttempt, ArtifactContentResponse, ArtifactGrant, ArtifactManifestEntry,
    ArtifactManifestRequest, ArtifactManifestResponse, ArtifactUpload, AttemptTransitionRequest,
    AttemptTransitionResponse, CancelObservation, CancellationObservationRequest,
    CancellationObservationResponse, ClaimGranted, ClaimIdle, ClaimLease, ClaimRequest,
    ClaimResponse, CompletionRequest, CompletionResponse, DecisionAnswer, DecisionCreateRequest,
    DecisionCreateResponse, DecisionOption, DecisionPollRequest, DecisionPollResponse,
    DecisionRecommendation, DecisionUpdate, EnrollmentRequest, EnrollmentResponse,
    EventBatchRequest, EventBatchResponse, EventRecord, HeartbeatRequest, HeartbeatResponse,
    LeaseResult, RefreshRequest, RefreshResponse,
};
pub use types::{
    ActualExecution, AgentProfileId, AgentProfileSnapshot, Approvals, AttemptId, AttemptSnapshot,
    AutonomyLevel, CancellationRequestId, Checkpoint, ClaimRequestId, CompletionId,
    EnvironmentValue, ExecutionError, ExecutionRequestId, ExecutionRequestSnapshot, ExecutionState,
    FencingToken, HarnessKind, IdempotencyKey, ItemId, Measurement, MeasurementSource,
    PermissionPolicy, ProtocolError, ProtocolErrorEnvelope, ProtocolVersion, RecoveryDetails,
    RecoveryDisposition, RecoveryJournalState, RecoveryKey, RecoveryObservation,
    RecoveryObservationRequest, RecoveryObservationResponse, RepositorySnapshot, RunnerId,
    RunnerSelector, StableErrorCode, TaskDocument, UnknownVariant, Usage, WorkspaceId,
    fence_card_text,
};
