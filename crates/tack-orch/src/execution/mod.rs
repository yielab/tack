//! Harness-neutral execution protocol domain for runner protocol v1.
//!
//! This module deliberately has no transport, persistence, or vendor adapter
//! dependencies. It represents the immutable values exchanged by the runner
//! protocol and is the seam consumed by the later API, database, and runner
//! cards.

mod capabilities;
mod lifecycle;
mod types;

// III-H1 added `CapabilityLimits` below: it is the type of the public field
// `RunnerCapabilities::limits`, but was the only capability type this list
// omitted — so no downstream crate could name it to construct a capability
// report. Purely an export-list omission; no type, field or behaviour changed.
pub use capabilities::{
    CapabilityLimits, CapabilitySupport, CapabilityValue, Concurrency, EmbeddedCapabilitySnapshot,
    FeatureCapabilities, HarnessCapability, ModelCombination, RunnerCapabilities,
};
pub use lifecycle::{LifecycleError, TransitionActor, validate_transition};
pub use types::{
    ActualExecution, ActualModelId, ActualModelProvider, AgentProfileId, AgentProfileSnapshot,
    ArtifactId, AttemptId, AttemptSnapshot, CancellationRequestId, Checkpoint, ClaimRequestId,
    CompletionId, DecisionId, EnvironmentValue, EventId, ExecutionError, ExecutionRequestId,
    ExecutionRequestSnapshot, ExecutionState, FencingToken, HarnessKind, IdempotencyKey, ItemId,
    Measurement, MeasurementSource, ModelId, ModelProvider, PermissionPolicy, ProtocolError,
    ProtocolErrorEnvelope, ProtocolVersion, RecoveryDetails, RecoveryDisposition,
    RecoveryJournalState, RecoveryKey, RecoveryObservation, RecoveryObservationRequest,
    RecoveryObservationResponse, RepositorySnapshot, RequestedModelId, RequestedModelProvider,
    RunnerId, RunnerSelector, StableErrorCode, Usage, WorkspaceId,
};
