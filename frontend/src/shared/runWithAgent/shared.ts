// Pure, framework-agnostic logic behind the "Run with agent" UI.
//
// Kept separate from the SolidJS components below so that payload
// construction, capability gating, and default-provenance logic are unit
// testable without mounting anything — and, more importantly, so that
// `RunWithAgentModal.tsx` (the ONE shared modal all three surfaces mount) has
// a single, provably-shared code path from form state to wire body: all
// three surfaces (Board, item-detail, Sprint) create the same payload shape
// when launching a run, with no divergent DTOs between entry points.
// Board/item-detail/Sprint never build a `CreateExecutionInput` themselves,
// they only collect a `RunWithAgentFormValues` and hand it to the one
// modal, which calls {@link buildCreateExecutionInput} exactly once.
//
// This module is about the neutral execution domain (`ExecutionRequest` via
// `tack-runner`) exclusively — it has no other "run an agent" concept to
// stay compatible with.

import type {
  CreateExecutionInput,
  RunnerCapabilities,
  HarnessCapability,
} from '../execution';

// ─── Harness kinds ──────────────────────────────────────────────────────────

/**
 * The in-tree v1 harness adapters (`crates/tack-runner/src/harness/`), with their real,
 * verified `harness_kind` wire values — read directly from each adapter's own
 * constant, not guessed:
 *   - `codex.rs`'s `CODEX_HARNESS_KIND = "codex"`
 *   - `claude_code.rs`'s `HARNESS_KIND = "claude-code"` (a HYPHEN)
 *   - `docket.rs`'s descriptor `kind: "docket"`
 *   - `opencode.rs`'s descriptor `kind: "opencode"`
 *
 * This list is not the whole harness vocabulary — a runner's own
 * `harness_kind` is an opaque string (`HarnessKind::Other(String)` on the
 * runner side), so a runner can report a kind not in this list; it is just
 * not one this build has a bundled adapter for.
 *
 * Worth calling out explicitly: `crates/tack-cli/src/execution.rs`'s own
 * unit tests use `"claude_code"` (an UNDERSCORE) as an arbitrary example
 * value for what is, on the wire, an untyped `String` field
 * (`CreateExecution::requested_harness_kind`) — that is a test fixture
 * string, not the runner's real reported `harness_kind`. Copying the
 * underscore form here would silently make Claude Code unselectable against
 * any runner that actually reports itself as `"claude-code"`, since matching
 * against a real runner's capability report is a byte-exact string compare
 * . Verified by reading the
 * harness adapter source files directly, not by inference.
 */
export const HARNESS_KINDS: ReadonlyArray<{ value: string; label: string }> = [
  { value: 'codex', label: 'Codex' },
  { value: 'claude-code', label: 'Claude Code' },
  { value: 'docket', label: 'Docket' },
  { value: 'opencode', label: 'OpenCode' },
];

// ─── Payload construction ───────────────────────────────────────────────────

/**
 * Everything the modal collects from an operator, in a shape independent of
 * any particular form widget. The profile travels by id only: the server
 * loads it and records what the run used.
 */
export interface RunWithAgentFormValues {
  itemId: string;
  selectorKind: 'fleet' | 'exact_runner';
  selectorId: string;
  agentProfileId: string;
  harnessKind: string;
  /** The one model string the agent is asked to use, sent verbatim. `null`
   *  names none: the project's default applies when this is its default
   *  agent, otherwise the agent uses its own. */
  model: string | null;
  timeoutSeconds: number;
  allowNetwork: boolean;
  tools: string[];
  /** `'auto'` (default) omits `permission_policy.approvals` on the wire;
   *  `'ask'` sends it as `"ask"`. Mirrors `PermissionPolicy::approvals`
   *  (`crates/tack-orch/src/execution/types.rs`): absent means `Auto`, so
   *  this module never sends a fabricated `"auto"` literal — see
   *  {@link buildCreateExecutionInput}. */
  approvals: 'auto' | 'ask';
  /** Absent: the server fills the snapshot from the item's project. */
  repository?: { kind: string; remote: string; baseRevision: string; subdirectory: string | null };
  idempotencyKey: string;
  /** Sent only as `false`: a request can decline what the runner has, never enable it. */
  verify?: boolean;
  pushBranch?: boolean;
}

/**
 * The one function every entry point calls to go from collected form state
 * to the exact `POST /executions` body (`CreateExecutionInput`,
 * `shared/execution/api.ts`, itself copied field-for-field from
 * `crates/tack-api/src/handlers/executions.rs`'s `CreateExecution`).
 *
 * The profile is not sent: the server loads it by `agent_profile_id` and
 * records what it ran. The nested `repository_snapshot`/`permission_policy`
 * objects are untyped (`unknown`) on the wire-boundary
 * type: `docs/openapi.json`'s `CreateExecution` schema leaves each as a bare
 * description with no `type`/`properties`, because the real Rust structs one
 * layer deeper (`crates/tack-orch/src/execution/types.rs`) live in a crate
 * that does not derive `ToSchema`. They are NOT arbitrary here, though:
 * their shape is copied field-for-field from those structs:
 *   - `RepositorySnapshot { kind, remote, base_revision, subdirectory }`
 *   - `PermissionPolicy { tools, network, approvals }` — `approvals` is
 *     omitted entirely for `'auto'`, matching the Rust field's own
 *     `skip_serializing_if = "Option::is_none"`: absent means `Auto`, never
 *     a redundant `"auto"` literal sent over the wire
 * A live test against a real server confirmed defaulting
 * `permission_policy` to `{}` gets a 400 `missing
 * field \`network\`` — this module's fields exist specifically because that
 * mistake is already documented. `budgets`/`environment`/`metadata` are left
 * at `{}`, which that same test confirmed is genuinely safe to default
 * (untyped `Value` fields end-to-end).
 */
export function buildCreateExecutionInput(values: RunWithAgentFormValues): CreateExecutionInput {
  return {
    item_id: values.itemId,
    idempotency_key: values.idempotencyKey,
    selector_kind: values.selectorKind,
    selector_id: values.selectorId,
    agent_profile_id: values.agentProfileId,
    requested_harness_kind: values.harnessKind,
    requested_model: values.model,
    repository_snapshot: values.repository && {
      kind: values.repository.kind,
      remote: values.repository.remote,
      base_revision: values.repository.baseRevision,
      subdirectory: values.repository.subdirectory,
    },
    permission_policy: {
      tools: values.tools,
      network: values.allowNetwork,
      ...(values.approvals === 'ask' ? { approvals: 'ask' } : {}),
    },
    budgets: {},
    environment: {},
    metadata: {},
    timeout_seconds: values.timeoutSeconds,
    status_map_policy_id: null,
    ...(values.verify === false ? { verify: false } : {}),
    ...(values.pushBranch === false ? { push_branch: false } : {}),
  };
}

/** A fresh idempotency key for one create attempt — matches the existing
 *  frontend precedent for client-generated ids (`ActivityTab.tsx`,
 *  `CreateItemModal.tsx`'s subtask ids both use `crypto.randomUUID()`), and
 *  E5's CLI default ("a freshly-generated default" the operator may
 *  override to opt into safe retry). This module never reuses a key across
 *  two different submissions of the same open modal. */
export function generateIdempotencyKey(): string {
  return crypto.randomUUID();
}

// ─── The model a run uses ───────────────────────────────────────────────────

/** The model a run will use: the request's own, else the project's default model
 *  when the run's agent is the project's default agent, else none (the agent
 *  chooses). The same rule the server applies when it creates the request. */
export function resolveRunModel(
  requested: string | null,
  harnessKind: string,
  project: { default_harness?: string | null; default_model?: string | null } | null | undefined,
): string | null {
  if (requested) return requested;
  if (project?.default_model && project.default_harness === harnessKind) return project.default_model;
  return null;
}

// ─── Capability gating ──────────────────────────────────────────────────────

export interface ModelGate {
  /** Whether the submit control should allow this selection through. */
  allowed: boolean;
  /** Always present — never a silent disable. */
  reason: string;
  /** A concrete next step the operator can take right now, when the block is fixable
   *  from a page that exists: setting the project's default model. Omitted otherwise. */
  fix?: { label: string; href: string };
}

/**
 * The submit-gate for the model: disabled and reasoned, never merely rejected
 * server-side. A run with a model is allowed; a run with none is allowed unless
 * every runner reporting the harness says `requires_model` (it
 * will not pick one itself), which the scheduler would leave queued.
 */
export function gateHarnessModelSelection(
  capabilities: RunnerCapabilities[],
  harnessKind: string,
  model: string | null,
  projectSettingsHref?: string,
): ModelGate {
  if (capabilities.length === 0) return { allowed: false, reason: 'no runner capability data available' };
  if (model) return { allowed: true, reason: `Runs on ${model}` };
  const reporting = capabilities.flatMap((c) => c.harnesses).filter((h) => h.harness_kind === harnessKind);
  if (reporting.length > 0 && reporting.every((h) => h.requires_model === true)) {
    return {
      allowed: false,
      reason: `${harnessKind} needs a model named. Choose one below, or set a default model.`,
      fix: projectSettingsHref ? { label: 'Set a default model for this project', href: projectSettingsHref } : undefined,
    };
  }
  return { allowed: true, reason: "Uses the agent's own model" };
}

// ─── Lifecycle-state display ────────────────────────────────────────────────

export type StateTone = 'neutral' | 'primary' | 'success' | 'warning' | 'danger' | 'info';

const STATE_LABEL: Record<string, string> = {
  queued: 'Queued',
  leased: 'Starting',
  preparing: 'Preparing',
  running: 'Running',
  waiting_decision: 'Asking you',
  succeeded: 'Finished',
  failed: 'Failed',
  cancelled: 'Cancelled',
  lost: 'Lost',
  needs_operator: 'Needs operator',
};

const STATE_TONE: Record<string, StateTone> = {
  queued: 'neutral',
  leased: 'info',
  preparing: 'info',
  running: 'primary',
  waiting_decision: 'warning',
  succeeded: 'success',
  failed: 'danger',
  cancelled: 'neutral',
  lost: 'danger',
  needs_operator: 'warning',
};

/**
 * `ExecutionSummary.state` is kept as a plain `string` on the wire type
 * (`shared/execution/api.ts`'s own doc comment: "so an operator-surface
 * value this build doesn't recognise still renders instead of failing to
 * parse"). This is the display-layer half of that same defensiveness — an
 * unrecognised value still renders (as itself, neutral tone), it never
 * throws and never silently disappears.
 */
export function describeExecutionState(state: string): { label: string; tone: StateTone; known: boolean } {
  const label = STATE_LABEL[state];
  if (label) return { label, tone: STATE_TONE[state], known: true };
  return { label: state, tone: 'neutral', known: false };
}

const TERMINAL_STATES = new Set(['succeeded', 'failed', 'cancelled']);

/** Whether the request is in a terminal state — a plain-string-safe
 *  restatement of `shared/execution/types.ts#isTerminalExecutionState`,
 *  which takes a narrowed `ExecutionState`, not the defensively-widened
 *  `string` this module works with (see `describeExecutionState`'s note). */
export function isTerminalStateString(state: string): boolean {
  return TERMINAL_STATES.has(state);
}

const PR_STATE_LABEL: Record<string, string> = {
  open: 'Open',
  merged: 'Merged',
  closed: 'Closed',
  reverted: 'Reverted',
};

const PR_STATE_TONE: Record<string, StateTone> = {
  open: 'primary',
  merged: 'success',
  closed: 'neutral',
  reverted: 'warning',
};

/**
 * Pull request state display — mirrors the execution state pattern.
 * Like `describeExecutionState`, this handles defensively against
 * unrecognised values, rendering them with a neutral tone.
 */
export function describePullRequestState(state: string): { label: string; tone: StateTone } {
  const label = PR_STATE_LABEL[state];
  if (label) return { label, tone: PR_STATE_TONE[state] };
  return { label: state, tone: 'neutral' };
}

/**
 * A small, self-contained relative-time formatter, local to the execution
 * UI rather than shared, since it is ~12 lines and this is its only caller.
 */
export function relativeTimeFromIso(iso: string | null | undefined): string {
  if (!iso) return 'unknown';
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return 'unknown';
  const diffSec = Math.round((Date.now() - then) / 1000);
  if (diffSec < 5) return 'just now';
  if (diffSec < 60) return `${diffSec}s ago`;
  const min = Math.round(diffSec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.round(hr / 24);
  if (day < 30) return `${day}d ago`;
  return new Date(iso).toLocaleDateString();
}

// ─── Target selection ("Where it runs") ─────────────────────────────────────

/** `agent_runners.state` values that mean the runner is live and could
 *  actually claim work — `runner_admin.rs::list_runners` reports
 *  `'pending_enrollment' | 'active' | 'revoked'` (plus any future value,
 *  which this treats as inactive rather than guessing). */
export function isActiveRunnerState(state: string): boolean {
  return state === 'active';
}

/** Whether the target picker should stay hidden and the one active runner
 *  used directly, with no free-text id anywhere — hidden when exactly one
 *  machine is active, the common case. More than one active runner, or a
 *  fleet also existing as an alternative, means a real choice exists and
 *  the picker must render. */
export function shouldHideTargetPicker(activeRunnerCount: number, fleetCount: number): boolean {
  return activeRunnerCount === 1 && fleetCount === 0;
}

/**
 * Whether the product should show its "agent execution is off" state
 * instead of the run form.
 *
 * `/api/local-runner` exists and reports whether the embedded runner
 * (`tack serve --with-runner`) is on, but that only answers for the
 * embedded case — a remote runner can supply execution capacity with the
 * embedded one off. `/api/executions` and its sibling routes are mounted
 * unconditionally regardless of either (`frontend/e2e/run-with-agent.spec.ts`'s
 * own header note calls this "the always-on operator surface"), so this
 * function still uses the more general, directly-observable signal: zero
 * active runners (local or remote) reads as "execution is off," since no
 * runner ever enrolls until one is started.
 */
export function isExecutionOff(activeRunnerCount: number): boolean {
  return activeRunnerCount === 0;
}

// ─── Approvals ("Ask me before acting") ─────────────────────────────────────

/**
 * Whether a harness attests it can pause a run and ask the operator before
 * it acts, unlocking the "Ask me" approvals choice — same rule and same
 * shape as the other attestations: only `support === 'supported'`
 * counts, `'advisory'` and an absent `decisions` attestation (an older
 * runner, or the shared fake probe) both mean "not attested" and are
 * rejected identically to `'unsupported'`, matching
 * `crates/tack-orch/src/scheduler/select.rs`'s own treatment of this exact
 * field.
 */
/** The tools a profile gives one harness (`tool_policy.tools[harness]`; `['*']` is every
 *  tool), or `[]` when it names none. A run whose own tool list is blank uses these. */
export function profileToolsFor(toolPolicy: unknown, harness: string): string[] {
  const tools = (toolPolicy as { tools?: Record<string, unknown> } | null)?.tools?.[harness];
  return Array.isArray(tools) ? tools.filter((t): t is string => typeof t === 'string' && t.trim() !== '') : [];
}

export function isDecisionsAttested(harness: HarnessCapability | undefined): boolean {
  return harness?.decisions?.support === 'supported';
}

/** Props of the one shared "Run with agent" modal (Board, item-detail, Sprint). */
export interface RunWithAgentModalProps {
  isOpen: boolean;
  onClose: () => void;
  itemId: string;
  itemTitle: string;
  /** The item's project, whose model default the submit gate reads. */
  projectId: string;
  /** Called with the new request id after a successful create. */
  onCreated?: (requestId: string) => void;
  /** Tests only: replaces the capabilities the submit gate reads. The target
   *  picker always reads the live `GET /runners`. */
  capabilities?: () => RunnerCapabilities[];
}
