// Wire-format boundary for the per-request attempt/event surface:
// `GET /executions/{id}/attempts` and `GET
// /executions/{id}/attempts/{attempt_number}/events`
// (`crates/tack-api/src/handlers/executions.rs`,
// `list_execution_attempts`/`list_execution_attempt_events`). This file is
// that wiring's wire-format layer; `store.ts#loadAttempts`/`attemptsFor` is
// the reactive layer built on top of it.
//
// Every shape below is copied field-for-field from the real handler
// (`AttemptSummary`/`AttemptListResponse`/`EventSummary`/`EventListResponse`
// in `executions.rs`), the same hand-typed-from-Rust convention `api.ts`'s
// header comment documents for this whole domain (this domain's OpenAPI
// schemas type `model_provenance`/`usage_economics` via
// `#[schema(value_type = ...)]`, pointing at the real `tack-protocol` types).
//
// `model_provenance` (nullable — `null` until the attempt reports
// `actual_execution`) and `usage_economics` (always present) on
// `AttemptSummary` carry `tack_protocol::usage_provenance::{ModelProvenance,
// UsageEconomics, RunnerTimeCost}`; the generated document describes them
// straight from those types.

import { requestWithHeaders } from '../api/client';
import type { components } from '../api/schema.gen';
import type { Measurement } from './types';

/** `executions.rs`'s `TerminalReason`: the five named keys the UI reads, plus whatever else the runner sent. */
export type TerminalReason = components['schemas']['TerminalReason'];

// ─── Model provenance (`usage_provenance.rs::ModelProvenance`) ─────────────

/**
 * The model the request named and the one the attempt ran on, both in full and
 * never reconciled; `source` says who named the served one (`reported`,
 * `requested` or `agent_default`).
 */
export type ModelProvenance = components['schemas']['ModelProvenance'];

// ─── Pull request (`executions.rs::AttemptPullRequest`) ────────────────────

/**
 * A pull request opened by the run, when one was created. `null` when no PR
 * was opened (verification declined, branch push declined, or the runner
 * doesn't support GitHub integration).
 */
export interface AttemptPullRequest {
  number: number;
  state: string;
  url: string;
}

// ─── Usage economics (`usage_provenance.rs::UsageEconomics`) ───────────────

/**
 * The runner-witnessed wall-clock dimension — a plain fact, never itself a
 * `Measurement` (there is no "estimated" wall clock; see
 * `usage_provenance.rs`'s own doc comment). `wall_clock_ms` is `null` only
 * until both `started_at`/`ended_at` are known; `cost_usd_estimated` is a
 * `Measurement` because turning that duration into a dollar figure needs an
 * infra rate no deployment configures today — every real response has this
 * at `{value: null, source: 'not_measured'}`.
 */
export interface RunnerTimeCost {
  wall_clock_ms: number | null;
  cost_usd_estimated: Measurement<number>;
}

/**
 * Two independently-provenanced dollar dimensions, deliberately never
 * summed into one figure: the harness/vendor's own self-reported token
 * cost, and a derived runner-infra-time cost. Always present on an
 * `AttemptSummary` (unlike `model_provenance`), because both sub-fields
 * degrade to an honest `not_measured` rather than needing to be absent
 * themselves.
 */
export interface UsageEconomics {
  model_token_cost_usd_estimated: Measurement<number>;
  runner_time_cost: RunnerTimeCost;
}

// ─── Merge (`executions.rs::AttemptMerge`) ─────────────────────────────────

/** Who merged an attempt's pull request, when it is merged. */
export type MergedBy = components['schemas']['MergedBy'];

/** The Merge button's state: `reason` says why it is disabled, or what held its last press. */
export type AttemptMerge = components['schemas']['AttemptMerge'];

// ─── Attempts (`GET /executions/{id}/attempts`) ────────────────────────────

/**
 * One attempt, exactly as `executions.rs`'s `AttemptSummary` serializes it.
 * `actual_execution`/`usage` stay `unknown` (untyped JSON) here for the same
 * reason the Rust struct types them as a bare `Value`: the real types
 * (`tack_orch::execution::{ActualExecution, Usage}`) live in a crate this
 * frontend has no generated bridge to yet — this file only needs enough shape
 * to drive `model_provenance`/`usage_economics` display, which are already
 * fully typed above. `terminal_reason` is typed from the generated schema.
 */
export interface AttemptSummary {
  attempt_id: string;
  request_id: string;
  attempt_number: number;
  runner_id: string;
  fencing_token: number;
  state: string;
  lease_issued_at: string;
  lease_expires_at: string;
  last_heartbeat_at: string | null;
  event_checkpoint: string | null;
  completion_id: string | null;
  workspace_id: string | null;
  base_revision: string | null;
  actual_execution: unknown | null;
  terminal_reason: TerminalReason | null;
  usage: unknown | null;
  /** New input, cache writes and output, the figure a token limit counts; `null` when nothing was measured. */
  counted_tokens?: number | null;
  started_at: string | null;
  ended_at: string | null;
  created_at: string;
  updated_at: string;
  /** `null` while the attempt has not yet reported `actual_execution`. */
  model_provenance: ModelProvenance | null;
  usage_economics: UsageEconomics;
  /** `null` when no pull request was opened by the run. */
  pull_request: AttemptPullRequest | null;
  /** The Merge button's state. Absent from a server that predates it. */
  merge?: AttemptMerge;
  /** Names (never values) of the environment variables the runner gave the harness, as it reported at start. */
  environment_names?: string[];
  /** What the harness ran inside, as the runner reported at start; `null` when it did not say. */
  sandbox?: string | null;
  /** `null` until a person accepts or rejects the attempt's result. */
  review: { verdict: 'accepted' | 'rejected'; note: string | null; reviewed_at: string } | null;
}

export interface AttemptListResult {
  protocol_version: number;
  data: AttemptSummary[];
}

// ─── Events (`GET /executions/{id}/attempts/{n}/events`) ──────────────────

/**
 * One normalized timeline event, exactly as `executions.rs`'s `EventSummary`
 * serializes it. `kind`/`source` stay plain strings (matching
 * `docs/contracts/runner-v1/event-batch.request.json`'s own free-form
 * vocabulary — `"message"`/`"progress"` in the frozen example, never a fixed
 * enum); `payload` is the harness/runner's own arbitrary JSON, rendered
 * defensively by `attemptFormat.ts#describeEventPayload`.
 */
export interface EventSummary {
  event_id: string;
  sequence: number;
  source: string;
  kind: string;
  payload: unknown;
  occurred_at: string;
  created_at: string;
}

export interface EventListResult {
  protocol_version: number;
  data: EventSummary[];
}

export const attemptsApi = {
  list: (requestId: string) =>
    requestWithHeaders<AttemptListResult>(`/executions/${encodeURIComponent(requestId)}/attempts`),
  events: (requestId: string, attemptNumber: number) =>
    requestWithHeaders<EventListResult>(
      `/executions/${encodeURIComponent(requestId)}/attempts/${encodeURIComponent(String(attemptNumber))}/events`,
    ),
  /** `POST /attempts/{attempt_id}/merge` — 200 `{merged, merge_sha}`; a refusal is a 409 whose message says why. */
  merge: (attemptId: string) =>
    requestWithHeaders<{ merged: boolean; merge_sha?: string | null }>(
      `/attempts/${encodeURIComponent(attemptId)}/merge`,
      { method: 'POST' },
    ),
  review: (requestId: string, attemptNumber: number, body: { verdict: 'accepted' | 'rejected'; note?: string }) =>
    requestWithHeaders<unknown>(
      `/executions/${encodeURIComponent(requestId)}/attempts/${encodeURIComponent(String(attemptNumber))}/review`,
      { method: 'POST', body: JSON.stringify(body) },
    ),
};
