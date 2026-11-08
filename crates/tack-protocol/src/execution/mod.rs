//! Harness-neutral execution protocol domain for runner protocol v1.
//!
//! This module deliberately has no transport, persistence, or vendor adapter
//! dependencies. It represents the immutable values exchanged by the runner
//! protocol and is the seam consumed by the API, database, and runner
//! layers.

mod activity;
mod capabilities;
mod lifecycle;
mod messages;
mod operations;
mod prompt_templates;
mod types;

// `CapabilityLimits` is exported here since it's the type of the public
// field `RunnerCapabilities::limits`, so downstream crates can name it.
pub use activity::{ActivityState, ActivityTask, CurrentStep, FinishedStep, TeamActivity, WaitsOn};
pub use capabilities::{
    CapabilityLimits, CapabilitySupport, CapabilityValue, Concurrency, DocsLinks,
    EmbeddedCapabilitySnapshot, FeatureCapabilities, HarnessCapability, MemoryEntry,
    MemoryEvidence, ModelListSource, ModelsReport, NEED_MODEL, NEED_SIGN_IN, ParityCell,
    ParityColumn, ParityLine, ParityRule, RunnerCapabilities, SignInReport, SignInState,
    TeamBudget, TeamDecides, TeamDelivers, TeamFacts, TeamMember, TeamMemory, TeamMethod,
    TeamMisfit, TeamNeed, TeamNeeds, TeamNetwork, TeamPause, TeamQueue, TeamRecordSet, TeamReport,
    TeamSearch, TeamSearchOption, TeamSearchProvider, TeamStep, TeamStepCheck, TeamStepKind,
    TeamViolation, TeamWebMode, TeamWebRead, WITHOUT_LIVE_USAGE, WorksIn,
    reports_usage_while_running, team_misfits, validate_parity,
};
pub use lifecycle::{
    LifecycleError, TransitionActor, lease_held, sources, terminal, validate_transition,
};
pub use messages::{
    ActiveAttempt, ActivityReport, ActivityResponse, ArtifactContentResponse, ArtifactGrant,
    ArtifactManifestEntry, ArtifactManifestRequest, ArtifactManifestResponse, ArtifactUpload,
    AttemptTransitionRequest, AttemptTransitionResponse, CancelObservation,
    CancellationObservationRequest, CancellationObservationResponse, ClaimGranted, ClaimIdle,
    ClaimLease, ClaimRequest, ClaimResponse, CompletionRequest, CompletionResponse, DecisionAnswer,
    DecisionAnsweredElsewhereRequest, DecisionAnsweredElsewhereResponse, DecisionCreateRequest,
    DecisionCreateResponse, DecisionOption, DecisionPollRequest, DecisionPollResponse,
    DecisionRecommendation, DecisionUpdate, EnrollmentRequest, EnrollmentResponse,
    EventBatchRequest, EventBatchResponse, EventRecord, HeartbeatRequest, HeartbeatResponse,
    LeaseResult, RefreshRequest, RefreshResponse,
};
pub use operations::{
    MemoryDecision, OperationOutcome, OperationPollRequest, OperationPollResponse,
    OperationResultRequest, OperationResultResponse, RunnerOperation, TeamOperation,
};
pub use prompt_templates::{
    BRIEF_HEADING, COMMENT_LABEL, DEFINITION_OF_DONE_HEADING, PROMPT_TEMPLATE_SHA,
    PROMPT_TEMPLATES, template_fingerprint,
};
pub use types::{
    ActualExecution, AgentProfileId, AgentProfileSnapshot, Approvals, AttemptId, AttemptSnapshot,
    AutonomyLevel, CancellationRequestId, Checkpoint, ClaimRequestId, CompletionId, EgressMode,
    EnvironmentValue, ExecutionError, ExecutionRequestId, ExecutionRequestSnapshot, ExecutionState,
    FencingToken, HarnessKind, IdempotencyKey, ItemId, Measurement, MeasurementSource,
    PermissionPolicy, ProtocolError, ProtocolErrorEnvelope, ProtocolVersion, RecoveryDetails,
    RecoveryDisposition, RecoveryJournalState, RecoveryKey, RecoveryObservation,
    RecoveryObservationRequest, RecoveryObservationResponse, RepositorySnapshot, RunnerId,
    RunnerSelector, StableErrorCode, TaskDocument, UnknownVariant, Usage, WorkspaceId,
    fence_card_text,
};
