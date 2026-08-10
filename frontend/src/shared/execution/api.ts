// Wire-format boundary for the Part III operator execution/fleet/runner/
// profile surface (TODO.md III-E2). See `types.ts`'s header comment for why
// these shapes are hand-typed rather than imported from `../api/schema.gen`
// (every route's OpenAPI schema is currently `{}` — a real, cited gap, not a
// guess) and for the per-field provenance of the richer domain vocabulary.
// Every shape in *this* file is instead copied directly from the live Rust
// handler that produces it — cited per section below — since those handlers
// are real and already shipped, even though nothing generates a TS type
// from them yet.
//
// **Endpoints intentionally NOT wrapped here, because they don't exist:**
//   - `GET /runners` (list) — `crates/tack-api/src/handlers/runner_admin.rs`
//     only registers `POST /runners/enrollment`,
//     `POST /runners/{id}/enrollment-tokens/{token_id}/revoke`, and
//     `POST /runners/{id}/revoke`. There is no way to list runners or read
//     a runner's `capability_snapshot` back from the operator API today.
//   - `GET /runner-fleets/{id}` (single fleet + roster) — only
//     `POST`/`GET` (list) exist.
//   - Any attempt-level read (`GET /executions/{id}` returns only five
//     scalar columns — see `ExecutionSummary` below — never attempts,
//     events, decisions or artifacts).
// All three are flagged in `docs/agent-handoffs/part-iii/III-E2.md` as a
// schema/API change requested from another owner (E6, which TODO.md's Wave
// 4 dependency graph names as the integrator who fills in "route/spec/
// generated updates" after E1-E5 land). `capabilities.ts` and `cache.ts` are
// still built to consume the richer shapes so no further design work is
// needed once a read endpoint exists — only a new wrapper function here.
//
// **A wire inconsistency worth flagging alongside that gap:** `POST
// /executions/{id}/cancel`'s success response
// (`crates/tack-api/src/handlers/executions.rs`, `request_cancellation`,
// line ~515) hardcodes `"state":"cancellation_requested"` — a string that is
// NOT a member of `ExecutionState` (III.1.1's frozen ten-state lifecycle has
// no such state; cancellation intent is instead the separate
// `cancellation_requested_at` timestamp column `list_executions`/
// `get_execution` already expose). A client that calls `cancel()` and then
// re-fetches via `get()` will see the row's REAL current lifecycle state
// (e.g. still `"running"`) with `cancellation_requested_at` populated, not a
// `"cancellation_requested"` state. `CancelExecutionResult.state` is typed
// as a bare `string` below (never merged into an `ExecutionState`) and
// `store.ts`'s optimistic update only ever touches
// `cancellation_requested_at`, never `state`, for exactly this reason.

import { request, requestWithHeaders } from '../api/client';

// ── Executions (`crates/tack-api/src/handlers/executions.rs`) ─────────────

/**
 * The row shape returned by both `GET /executions` (`list_executions`) and
 * `GET /executions/{id}` (`get_execution`) — five columns, verbatim from
 * each handler's own `SELECT` (executions.rs lines ~426-436, ~443-462).
 * This is deliberately much thinner than III.1.2's full execution-request
 * snapshot (no selector, no agent profile, no requested harness/model, no
 * repository/permission/budget/timeout data) — the handlers do not select
 * those columns even though `execution_requests` stores them. Do not assume
 * a field exists here just because it's in `ExecutionRequestSnapshot`.
 */
export interface ExecutionSummary {
  request_id: string;
  item_id: string;
  /** The real current lifecycle state (`ExecutionState` from `types.ts`),
   *  kept as `string` here (not narrowed) so an operator-surface value this
   *  build doesn't recognise still renders instead of failing to parse —
   *  the frozen contract has no `Unknown` escape hatch, but a defensive
   *  client is cheap insurance against a server/client version skew. */
  state: string;
  cancellation_requested_at: string | null;
  created_at: string;
}

export interface ExecutionListResult {
  protocol_version: number;
  data: ExecutionSummary[];
}

/**
 * `POST /executions` request body — every field from `CreateExecution`
 * (executions.rs lines 126-147). `selector_kind` is constrained to exactly
 * `"exact_runner" | "fleet"` server-side (line 229); `"any"` (a real
 * `RunnerSelector` variant per `types.ts`) is rejected today.
 */
export interface CreateExecutionInput {
  item_id: string;
  idempotency_key: string;
  selector_kind: 'exact_runner' | 'fleet';
  selector_id: string;
  agent_profile_id: string;
  requested_harness_kind: string;
  requested_model_provider?: string | null;
  requested_model_id?: string | null;
  agent_profile_snapshot: unknown;
  repository_snapshot: unknown;
  permission_policy: unknown;
  budgets: unknown;
  environment: unknown;
  metadata: unknown;
  timeout_seconds: number;
  status_map_policy_id?: string | null;
}

/** `POST /executions` success body (executions.rs lines 409-415). `state` is
 *  always the literal `"queued"` on both a fresh create and a replay — this
 *  endpoint's one and only honest lifecycle claim, unlike cancel's (see
 *  header note). `replayed` distinguishes a fresh create from an
 *  idempotency-key hit that reused the original request. */
export interface CreateExecutionResult {
  protocol_version: number;
  request_id: string;
  state: 'queued';
  replayed: boolean;
}

/** `POST /executions/{id}/cancel` success body (executions.rs lines
 *  515-517). **`state` is a fixed acknowledgement string, not a member of
 *  `ExecutionState`** — see this file's header note. Treat it as opaque
 *  display text, never feed it into `ExecutionState`-typed logic. */
export interface CancelExecutionResult {
  protocol_version: number;
  request_id: string;
  state: string;
}

/** `POST /executions/{id}/requeue` request body — `RecoveryConfirmation`
 *  (executions.rs lines 520-524). Requires the operator's out-of-band
 *  `recovery_key` and a human `reason`; only valid from `needs_operator`. */
export interface RequeueExecutionInput {
  recovery_key: string;
  reason: string;
}

/** `POST /executions/{id}/requeue` success body (executions.rs lines
 *  552-554). `state` is always the literal `"queued"` here — a genuinely
 *  real lifecycle claim, since a successful requeue really does transition
 *  the request back to `queued` (III.1.1: `needs_operator -> queued`,
 *  operator-only). */
export interface RequeueExecutionResult {
  protocol_version: number;
  request_id: string;
  state: 'queued';
  recovered_from: 'needs_operator';
  replayed: boolean;
}

export const executionsApi = {
  /** Preserves response headers via `requestWithHeaders` even though the
   *  handler sets none beyond the defaults today — so a future
   *  fencing/ETag-style header lands here automatically instead of being
   *  silently dropped by a `data`-only wrapper (this card's "preserve
   *  headers" task; proven in `api.test.ts`). */
  list: () => requestWithHeaders<ExecutionListResult>('/executions'),
  get: (requestId: string) =>
    requestWithHeaders<ExecutionSummary>(`/executions/${encodeURIComponent(requestId)}`),
  create: (input: CreateExecutionInput) =>
    request<CreateExecutionResult>('/executions', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  cancel: (requestId: string) =>
    request<CancelExecutionResult>(`/executions/${encodeURIComponent(requestId)}/cancel`, {
      method: 'POST',
    }),
  requeue: (requestId: string, input: RequeueExecutionInput) =>
    request<RequeueExecutionResult>(`/executions/${encodeURIComponent(requestId)}/requeue`, {
      method: 'POST',
      body: JSON.stringify(input),
    }),
};

// ── Runner fleets (`crates/tack-api/src/handlers/runner_admin.rs`) ────────
//
// **Vocabulary note (III.0):** this `Fleet` (the `agent_fleets` table) is a
// completely different concept from `shared/orch/capabilities.ts`'s /
// `features/fleet/api.ts`'s `FleetEntry` (`GET /api/fleet`, Part II's
// per-project docket control-plane roster). Both happen to be named
// "fleet"; nothing here imports from or is compatible with that other
// module. A caller that means "docket's fleet view" wants
// `shared/orch`/`features/fleet`, not this file.

export interface FleetSummary {
  fleet_id: string;
  name: string;
  concurrency_limit: number | null;
  default_policy: unknown;
}

export interface FleetListResult {
  protocol_version: number;
  data: FleetSummary[];
}

export interface CreateFleetInput {
  name: string;
  concurrency_limit?: number | null;
  default_policy?: unknown;
}

export interface CreateFleetResult {
  protocol_version: number;
  fleet_id: string;
  name: string;
}

export const fleetsApi = {
  list: () => requestWithHeaders<FleetListResult>('/runner-fleets'),
  create: (input: CreateFleetInput) =>
    request<CreateFleetResult>('/runner-fleets', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
};

// ── Agent profiles ─────────────────────────────────────────────────────────

export interface AgentProfileSummary {
  agent_profile_id: string;
  name: string;
  instructions: string;
  tool_policy: unknown;
  limits: unknown;
}

export interface AgentProfileListResult {
  protocol_version: number;
  data: AgentProfileSummary[];
}

export interface CreateAgentProfileInput {
  name: string;
  instructions: string;
  tool_policy?: unknown;
  limits?: unknown;
}

export interface CreateAgentProfileResult {
  protocol_version: number;
  agent_profile_id: string;
  name: string;
}

export const agentProfilesApi = {
  list: () => requestWithHeaders<AgentProfileListResult>('/agent-profiles'),
  create: (input: CreateAgentProfileInput) =>
    request<CreateAgentProfileResult>('/agent-profiles', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
};

// ── Model profiles ─────────────────────────────────────────────────────────

export interface ModelProfileSummary {
  model_profile_id: string;
  name: string;
  model_provider: string;
  model_id: string;
  config_reference: string | null;
  enabled: boolean;
}

export interface ModelProfileListResult {
  protocol_version: number;
  data: ModelProfileSummary[];
}

export interface CreateModelProfileInput {
  name: string;
  model_provider: string;
  model_id: string;
  config_reference?: string | null;
}

export interface CreateModelProfileResult {
  protocol_version: number;
  model_profile_id: string;
  name: string;
  model_provider: string;
  model_id: string;
}

export const modelProfilesApi = {
  list: () => requestWithHeaders<ModelProfileListResult>('/model-profiles'),
  create: (input: CreateModelProfileInput) =>
    request<CreateModelProfileResult>('/model-profiles', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
};

// ── Runners (enrollment/revocation only — see header note on the missing
//    list endpoint) ─────────────────────────────────────────────────────────

/** `POST /runners/enrollment` request body — `CreatePendingRunner`
 *  (runner_admin.rs lines 100-113). */
export interface EnrollRunnerInput {
  name: string;
  labels?: unknown;
  total_capacity: number;
  available_capacity: number;
  capability_snapshot?: unknown;
  protocol_version?: number;
  enrollment_lifetime_seconds?: number;
}

/**
 * `POST /runners/enrollment` success body (runner_admin.rs lines 306-312).
 * **`enrollment_token` is a raw, one-time secret** — the handler's own doc
 * comment: "deliberately emitted once here and is never readable from
 * metadata, list, revocation, or runner responses." Callers of this API
 * must display/copy it immediately and MUST NOT persist it into
 * `store.ts`'s cache, `sessionStorage`, `localStorage`, or any logger — the
 * same discipline `TACK_RUNNER_ENROLLMENT_TOKEN` gets on the runner side
 * (root `CLAUDE.md`: "exchanged for a durable credential and never
 * persisted").
 */
export interface EnrollRunnerResult {
  protocol_version: number;
  runner_id: string;
  token_id: string;
  enrollment_token: string;
  expires_at: string;
}

export interface RevokeRunnerResult {
  protocol_version: number;
  runner_id: string;
  state: 'revoked';
}

export interface RevokeEnrollmentTokenResult {
  protocol_version: number;
  runner_id: string;
  token_id: string;
  state: 'revoked';
}

export const runnersApi = {
  enroll: (input: EnrollRunnerInput) =>
    request<EnrollRunnerResult>('/runners/enrollment', {
      method: 'POST',
      body: JSON.stringify(input),
    }),
  revokeRunner: (runnerId: string) =>
    request<RevokeRunnerResult>(`/runners/${encodeURIComponent(runnerId)}/revoke`, {
      method: 'POST',
    }),
  revokeEnrollmentToken: (runnerId: string, tokenId: string) =>
    request<RevokeEnrollmentTokenResult>(
      `/runners/${encodeURIComponent(runnerId)}/enrollment-tokens/${encodeURIComponent(tokenId)}/revoke`,
      { method: 'POST' },
    ),
};
