// Public surface of the shared execution API/state layer. The fleet/runner
// management UI, the "Run with agent" + activity surfaces, and anything
// added later should import from this barrel rather than reaching into
// individual files, so the boundary of what's public stays visible in one
// place.

export type {
  ExecutionState,
  RunnerSelector,
  CapabilitySupport,
  CapabilityValue,
  FeatureCapabilities,
  Concurrency,
  HarnessCapability,
  CapabilityLimits,
  RunnerCapabilities,
  MeasurementSource,
  Measurement,
  Usage,
  StableErrorCode,
  RunStep,
  RunStepBudget,
  RunStepPrompt,
  RunRecordSet,
  TeamStepReport,
  TeamStepState,
} from './types';
export { TERMINAL_EXECUTION_STATES, isTerminalExecutionState } from './types';

export type {
  ExecutionSummary,
  ExecutionListResult,
  CreateExecutionInput,
  CreateExecutionResult,
  CancelExecutionResult,
  ContinueExecutionResult,
  RequeueExecutionInput,
  RequeueExecutionResult,
  FleetSummary,
  FleetListResult,
  CreateFleetInput,
  CreateFleetResult,
  AgentProfileSummary,
  ProfileKind,
  ProfileHistory,
  AgentProfileListResult,
  CreateAgentProfileInput,
  CreateAgentProfileResult,
  EnrollRunnerInput,
  EnrollRunnerResult,
  RevokeRunnerResult,
  RevokeEnrollmentTokenResult,
  RunnerSummary,
  RunnerListResult,
} from './api';
export { executionsApi, fleetsApi, agentProfilesApi, runnersApi, teamOf } from './api';

export type {
  ModelProvenance,
  RunnerTimeCost,
  UsageEconomics,
  AttemptPullRequest,
  AttemptSummary,
  AttemptListResult,
  EventSummary,
  EventListResult,
  TerminalReason,
  ContinueOffer,
} from './attempts';
export {
  attemptsApi,
  readRunSteps,
  readRunRecords,
  isFolderRun,
  folderRunSummary,
  readContinueOffer,
  readKeptWork,
} from './attempts';

export type {
  DecisionOption,
  DecisionRecommendation,
  DecisionState,
  DecisionAnswer,
  DecisionResolvedBy,
  DecisionRecord,
  ResolveDecisionResult,
} from './decisions';
export {
  decisionsApi,
  decisionTokenStore,
  isDecisionTokenRejected,
  isDecisionExpired,
  isDecisionIdempotencyConflict,
  isDecisionNotFound,
  isDecisionInvalidOption,
} from './decisions';

export type { ArtifactRecord } from './artifacts';
export { artifactsApi, isArtifactNotFound, isArtifactContentNotVerified } from './artifacts';

export type { CapabilityGate, FeatureName } from './capabilities';
export {
  gateFeature,
  gateFeatureAcrossRunners,
  gateApiFeature,
  listReportedHarnessKinds,
  harnessProbeStatus,
  NO_DATA_REASON,
} from './capabilities';

export { VersionedCache, SequenceAllocator } from './cache';

export type { ExecutionInvalidationEvent, ExecutionRealtimeStatus, ExecutionRealtimeOptions, ExecutionRealtime } from './realtime';
export { createExecutionRealtime } from './realtime';

export type {
  NormalizedExecutionError,
  ListStatus,
  CancellationState,
  ExecutionRequestRecord,
  AttemptAvailability,
  ExecutionStore,
} from './store';
export { createExecutionStore } from './store';

export type { Routed } from './routing';
export { routingApi, childrenText } from './routing';
export type { MemoryEntry, TeamMemoryDecided, TeamBudget, TeamEntry, TeamMember, TeamMemory, TeamMethod, TeamMisfit, TeamNeed, TeamReport, TeamResumed, TeamStep } from './teams';
export { missingHarnessFix, missingTeamFix } from './missing';
export { teamsApi, teamChoices, supervisionText, stepText, networkText, queueText, memberReachText, webText, misfitText, notSentText, unmetNeeds, deliversText, firstLine, runnableFirst, methodsByType, isTeamOfOne, podsOnly, teamProposes, pausedText, resumedText, budgetSpent, resumeGate, memoryCommands, learntNewestFirst, MEMORY_DECIDE_REASON, plainName, SHARED_PROPOSALS_TEXT } from './teams';

export type { AgentCapabilityId, AgentCapabilityLine } from './agents';
export {
  AGENT_KINDS,
  AGENT_NAMES,
  agentName,
  agentCapabilityLines,
  agentCanSentence,
  pickHarness,
  profileAgentText,
  SINGLE_AGENT_TEXT,
  PER_TASK_AGENT_TEXT,
} from './agents';
