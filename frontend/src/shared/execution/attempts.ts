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
import type { Measurement, RunRecordSet, RunStep, RunStepBudget, RunStepPrompt } from './types';

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
 * (`tack_protocol::execution::{ActualExecution, Usage}`) live in a crate this
 * frontend has no generated bridge to yet — this file only needs enough shape
 * to drive `model_provenance`/`usage_economics` display, which are already
 * fully typed above. `terminal_reason` is typed from the generated schema.
 */
/** A person's verdict on one attempt; a rejection always carries its reason. `reviewed_by` is the
 *  principal who gave it (`<kind>:<id>`), `null` for a review recorded before the reviewer was kept. */
export interface AttemptReview {
  verdict: 'accepted' | 'rejected';
  reason: string | null;
  reviewed_by: string | null;
  reviewed_at: string;
}

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
  /** The attempt ended without ever starting work: the computer refused it or the team's ceiling did.
   *  The server decides it (one rule); a screen reads this and never matches the reason's text. */
  never_started?: boolean;
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
  /** The "Run again at L3" button's state; present only on an attempt that ended asking for a person. */
  rerun_at_l3?: { enabled: boolean; reason: string | null };
  /** Names (never values) of the environment variables the runner gave the harness, as it reported at start. */
  environment_names?: string[];
  /** What the harness ran inside, as the runner reported at start; `null` when it did not say. */
  sandbox?: string | null;
  /** `null` until a person accepts or rejects the attempt's result. */
  review: AttemptReview | null;
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

// ─── What the run was made of (`terminal_reason`: `steps`, `records`, `outside_repository`) ──

const isRecord = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v);
const plainText = (v: unknown): string | null => (typeof v === 'string' && v.trim() !== '' ? v : null);
const plainTexts = (v: unknown): string[] =>
  Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string' && x.trim() !== '') : [];

/** A hop's `budget` (Phase 87, `budgetOutcome`): `null` for a hop that ran clean, or when the
 *  value on the wire is not the shape docket sends. */
function readHopBudget(value: unknown): RunStepBudget | null {
  if (!isRecord(value)) return null;
  const bound = plainText(value.bound);
  if (bound === null || typeof value.limit !== 'number' || typeof value.used !== 'number') return null;
  return {
    bound,
    limit: value.limit,
    used: value.used,
    source: plainText(value.source) ?? '',
    treeChanged: value.treeChanged === true,
    raise: plainText(value.raise),
  };
}

/** The hops of a docket task, in order; `undefined` when the run has none (a run from before the
 *  field existed, or a task with no hops). Read defensively: a key the run lacks reads as absent. */
export function readRunSteps(terminalReason: unknown): RunStep[] | undefined {
  const raw = isRecord(terminalReason) ? terminalReason.steps : undefined;
  if (!Array.isArray(raw)) return undefined;
  return raw.filter(isRecord).map((step) => ({
    role: plainText(step.role),
    model: plainText(step.model),
    class: plainText(step.class),
    family: plainText(step.family),
    // A budget hop's own report is never a verdict, even when the wire still carries one.
    verdict: step.report === true ? null : plainText(step.verdict),
    ok: typeof step.ok === 'boolean' ? step.ok : null,
    summary: plainText(step.summary),
    prompts: Array.isArray(step.prompts)
      ? step.prompts.filter(isRecord).map((p): RunStepPrompt => ({
          layer: plainText(p.layer) ?? '',
          name: plainText(p.name) ?? '',
          version: typeof p.version === 'string' ? p.version : '',
          sha256: plainText(p.sha256) ?? '',
          scope: plainText(p.scope) ?? '',
          tokens: typeof p.tokens === 'number' ? p.tokens : null,
        }))
      : [],
    assumptions: plainTexts(step.assumptions),
    questions: plainTexts(step.questions),
    startedAt: plainText(step.startedAt),
    finishedAt: plainText(step.finishedAt),
    cycle: typeof step.cycle === 'number' ? step.cycle : null,
    budget: readHopBudget(step.budget),
    report: step.report === true,
  }));
}

/** The record sets the run wrote; `undefined` when it wrote none. */
export function readRunRecords(terminalReason: unknown): RunRecordSet[] | undefined {
  const raw = isRecord(terminalReason) ? terminalReason.records : undefined;
  if (!Array.isArray(raw)) return undefined;
  return raw.filter(isRecord).map((set) => ({
    set: plainText(set.set) ?? '',
    columns: plainTexts(set.columns),
    rows: Array.isArray(set.rows) ? set.rows.filter(isRecord) : [],
    cut: set.cut === true,
  }));
}

/** Whether the run was a folder team's: docket's task is done and there was no repository to change. */
export function isFolderRun(terminalReason: unknown): boolean {
  return isRecord(terminalReason) && terminalReason.outside_repository === true;
}

/** A folder team's result: what its last step said it did. */
export function folderRunSummary(terminalReason: unknown): string | null {
  return isRecord(terminalReason) ? plainText(terminalReason.summary) : null;
}

// ─── A step's own budget stop (`code: "step_budget"` or `"budget"` with `next`, Phase 87) ────

/** Whether a new request may follow this run's own docket task instead of starting fresh
 *  (decision 6), and if not, why, in the backend's own words. Read from `terminal_reason.next.
 *  continue`, which docket.rs writes only for a step's own budget stop (code `step_budget` or
 *  `budget`); `null` for every other ending, including one from a docket too old to say. */
export interface ContinueOffer {
  possible: boolean;
  why: string | null;
}

export function readContinueOffer(terminalReason: unknown): ContinueOffer | null {
  const next = isRecord(terminalReason) && isRecord(terminalReason.next) ? terminalReason.next : undefined;
  const c = next && isRecord(next.continue) ? next.continue : undefined;
  if (!c || typeof c.possible !== 'boolean') return null;
  return { possible: c.possible, why: plainText(c.why) };
}

/** The branch a budget stop kept its work on, read straight off `terminal_reason` the same way a
 *  `done` run's is (`branch`/`head_commit`, decision 3); `null` when the stop pinned nothing —
 *  docket keeps the tree only for an Implementer hop. */
export function readKeptWork(terminalReason: unknown): { branch: string } | null {
  if (!isRecord(terminalReason)) return null;
  const branch = plainText(terminalReason.branch);
  return branch !== null && plainText(terminalReason.head_commit) !== null ? { branch } : null;
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
  review: (requestId: string, attemptNumber: number, body: { verdict: 'accepted' | 'rejected'; reason?: string }) =>
    requestWithHeaders<unknown>(
      `/executions/${encodeURIComponent(requestId)}/attempts/${encodeURIComponent(String(attemptNumber))}/review`,
      { method: 'POST', body: JSON.stringify(body) },
    ),
};
