// Pure, framework-agnostic display logic for one attempt's model provenance
// and usage economics. Kept separate from `AttemptList.tsx` for the same
// reason `shared.ts` is separate from `RunWithAgentModal.tsx` — unit
// testable without mounting anything, and a single place every consumer
// (`AttemptList.tsx`, its tests) reads through so "Not measured" can never
// silently drift into "$0.00" at a second call site.
//
// **The load-bearing rule this whole file exists to enforce (CLAUDE.md
// rule 1, "unsupported is typed, unknown is explicit, unmeasured is
// nullable"):** `usage_economics.runner_time_cost.cost_usd_estimated`
// is `{value: null, source: "not_measured"}` in *every real response this
// API returns today* — no runner infra cost-rate is stored anywhere in this
// schema. `formatUsdMeasurement` below renders that literal, unmistakable
// text `"Not measured"` — never `$0.00`, `—`, `0`, or a blank cell, which
// would each be a lie about real money. `absent_usage_never_serializes_as_zero`
// (`crates/tack-orch/src/usage_provenance.rs`) is the backend half of this
// same guarantee; this file is the frontend half.

import type { AttemptSummary, ModelProvenance, RunnerTimeCost, UsageEconomics } from '../execution/attempts';
import type { Measurement } from '../execution/types';
import { HARNESS_KINDS, describeExecutionState, type StateTone } from './shared';

/** The exact literal every unmeasured usage figure must render. Never
 *  interpolated or abbreviated differently at a second call site — every
 *  caller that needs this text imports this constant rather than retyping
 *  the string, so a future edit can never accidentally introduce a second,
 *  slightly different spelling. */
export const NOT_MEASURED_TEXT = 'Not measured';

/**
 * Renders a `Measurement<number>` dollar figure honestly. `source ===
 * 'not_measured'` is checked FIRST and unconditionally returns
 * {@link NOT_MEASURED_TEXT} regardless of `value` (defensive: a
 * `not_measured` source should always carry `value: null` per the backend's
 * own guarantee, but this function does not trust that pairing blindly — a
 * `null` value on its own, whatever the `source` says, is also treated as
 * "not measured" rather than risking a `NaN`/blank render). A real `0` with
 * a `measured`/`estimated` source is a genuine, distinct fact and renders as
 * a real `$0.00...`, never collapsed into the same text as "unmeasured".
 */
export function formatUsdMeasurement(measurement: Measurement<number>): string {
  if (measurement.source === 'not_measured' || measurement.value === null) {
    return NOT_MEASURED_TEXT;
  }
  const decimals = Math.abs(measurement.value) > 0 && Math.abs(measurement.value) < 0.01 ? 4 : 2;
  const dollars = `$${measurement.value.toFixed(decimals)}`;
  const provenanceLabel = measurement.source === 'measured' ? 'measured' : 'estimated';
  return `${dollars} (${provenanceLabel})`;
}

/**
 * `wall_clock_ms` is a plain derivable fact, never itself a `Measurement`
 * (there is no "estimated" wall clock — `usage_provenance.rs`'s own doc
 * comment). `null` means "not yet known" (the attempt hasn't reported both
 * `started_at`/`ended_at`), a genuinely different reason for absence than
 * "not measured" — this function's wording is deliberately distinct from
 * {@link NOT_MEASURED_TEXT} so the two absence reasons are never visually
 * conflated.
 */
export function formatWallClock(wallClockMs: number | null, terminal = false): string {
  if (wallClockMs === null) return terminal ? 'Did not start' : 'Unknown — attempt has not finished yet';
  const totalSeconds = Math.round(wallClockMs / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const parts: string[] = [];
  if (hours > 0) parts.push(`${hours}h`);
  if (hours > 0 || minutes > 0) parts.push(`${minutes}m`);
  parts.push(`${seconds}s`);
  return parts.join(' ');
}

export interface RunnerTimeCostDisplay {
  wallClock: string;
  costUsd: string;
}

export function formatRunnerTimeCost(cost: RunnerTimeCost, terminal = false): RunnerTimeCostDisplay {
  return {
    wallClock: formatWallClock(cost.wall_clock_ms, terminal),
    costUsd: formatUsdMeasurement(cost.cost_usd_estimated),
  };
}

export interface UsageEconomicsDisplay {
  modelTokenCostUsd: string;
  runnerTime: RunnerTimeCostDisplay;
}

/** Never sums the two dollar dimensions — `UsageEconomics`'s own doc
 *  comment: they are independently provenanced and must stay visibly
 *  separate line items, not folded into one "total cost" that would imply a
 *  precision neither figure actually has. */
export function formatUsageEconomics(usage: UsageEconomics, terminal = false): UsageEconomicsDisplay {
  return {
    modelTokenCostUsd: formatUsdMeasurement(usage.model_token_cost_usd_estimated),
    runnerTime: formatRunnerTimeCost(usage.runner_time_cost, terminal),
  };
}

// ─── What a run used ────────────────────────────────────────────────────────

/** One attempt's usage, read from `attempt.usage` (the runner's report) and
 *  `usage_economics`. Every figure is `null` when the harness didn't report it. */
export interface AttemptUsage {
  /** Every input token the model read, cache reads and writes included. */
  tokensIn: number | null;
  tokensOut: number | null;
  cacheRead: number | null;
  cacheWrite: number | null;
  modelCalls: number | null;
  /** The models the run used, the main one first. */
  models: string[];
  cost: Measurement<number>;
  wallClockMs: number | null;
}

const num = (v: unknown): number | null => (typeof v === 'number' && Number.isFinite(v) ? v : null);
const obj = (v: unknown): Record<string, unknown> => (v && typeof v === 'object' ? (v as Record<string, unknown>) : {});

export function readAttemptUsage(attempt: AttemptSummary): AttemptUsage {
  const usage = obj(attempt.usage);
  const value = (key: string) => num(obj(usage[key]).value);
  let tokensIn = value('tokens_in');
  let cacheRead = num(usage.cache_read_tokens);
  let cacheWrite = num(usage.cache_write_tokens);
  let modelCalls = num(usage.model_calls);
  let models = Array.isArray(usage.models) ? usage.models.filter((m): m is string => typeof m === 'string') : [];

  // A claude-code run recorded before the runner read the cache: its
  // `tokens_in` counts only uncached input, and the full figures are still in
  // the result line it kept as its terminal reason.
  const reason = obj(attempt.terminal_reason);
  if (reason.type === 'result' && !('cache_read_tokens' in usage) && !('model_calls' in usage)) {
    const vendor = obj(reason.usage);
    cacheRead = num(vendor.cache_read_input_tokens);
    cacheWrite = num(vendor.cache_creation_input_tokens);
    if (tokensIn !== null) tokensIn += (cacheRead ?? 0) + (cacheWrite ?? 0);
    modelCalls = num(reason.num_turns);
    models = Object.keys(obj(reason.modelUsage));
  }
  if (models.length === 0) {
    const observed = obj(attempt.actual_execution).model_id;
    if (typeof observed === 'string' && observed !== '' && observed !== 'unknown') models = [observed];
  }
  return {
    tokensIn,
    tokensOut: value('tokens_out'),
    cacheRead,
    cacheWrite,
    modelCalls,
    models,
    cost: attempt.usage_economics.model_token_cost_usd_estimated,
    wallClockMs: attempt.usage_economics.runner_time_cost.wall_clock_ms,
  };
}

/** `12`, `3.3K`, `286K`, `1.08M`. */
export function formatTokenCount(n: number): string {
  const trim = (s: string) => s.replace(/\.0+$/, '').replace(/(\.\d*?)0+$/, '$1');
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${trim((n / 1000).toFixed(n < 10_000 ? 1 : 0))}K`;
  return `${trim((n / 1_000_000).toFixed(n < 10_000_000 ? 2 : 1))}M`;
}

/** A harness's own dollar figure is its estimate (claude-code prices tokens at
 *  list price), never a bill — so it is always shown as approximate. */
export function formatApproxCost(cost: Measurement<number>): string {
  if (cost.source === 'not_measured' || cost.value === null) return NOT_MEASURED_TEXT;
  const decimals = Math.abs(cost.value) > 0 && Math.abs(cost.value) < 0.01 ? 4 : 2;
  return `≈ $${cost.value.toFixed(decimals)}`;
}

export function harnessName(kind: string | null | undefined): string {
  if (!kind) return 'the agent';
  return HARNESS_KINDS.find((h) => h.value === kind)?.label ?? kind;
}

// ─── Model provenance ───────────────────────────────────────────────────────

export interface ModelProvenanceDisplay {
  label: string;
  detail: string;
  tone: StateTone;
}

/**
 * `null` means the attempt has not yet reported `actual_execution` — a
 * "hasn't happened yet" state, deliberately worded differently from
 * {@link NOT_MEASURED_TEXT} ("won't be measured"), since the two are not the
 * same fact: this one may still resolve to a real value once the attempt
 * completes.
 */
export function describeModelProvenance(
  provenance: ModelProvenance | null,
  observationSource?: string | null,
): ModelProvenanceDisplay {
  if (provenance === null) {
    return { label: 'Not yet reported', detail: 'This attempt has not reported its actual execution yet.', tone: 'neutral' };
  }
  switch (provenance.kind) {
    case 'matched':
      if (observationSource !== 'observed' && observationSource !== 'confirmed_from_output') {
        return {
          label: 'Requested, not confirmed',
          detail: `Requested ${provenance.provider} / ${provenance.model_id} (not confirmed by the run)`,
          tone: 'neutral',
        };
      }
      return {
        label: 'Matched request',
        detail: `Ran on ${provenance.provider} / ${provenance.model_id}, as requested.`,
        tone: 'success',
      };
    case 'auto_select_observed':
      return {
        label: 'Auto-selected',
        detail: `The request allowed auto-selection; the runner chose ${provenance.actual_provider} / ${provenance.actual_model_id}.`,
        tone: 'info',
      };
    case 'mismatched':
      return {
        label: 'Mismatched request',
        detail:
          `Requested ${provenance.requested_provider} / ${provenance.requested_model_id}, ` +
          `but ran on ${provenance.actual_provider} / ${provenance.actual_model_id}.`,
        tone: 'warning',
      };
    /* istanbul ignore next -- defensive: ModelProvenance is a closed union
       today; an unrecognised `kind` still renders instead of throwing. */
    default: {
      const unknown = provenance as { kind: string };
      return { label: 'Unrecognised provenance', detail: `Unknown kind: ${unknown.kind}`, tone: 'neutral' };
    }
  }
}

// ─── Outcome ────────────────────────────────────────────────────────────────

/** The size of the patch an attempt left, or 0 when it left none. */
export function attemptPatchBytes(terminalReason: unknown): number {
  const arts = obj(terminalReason).artifacts;
  const patch = (Array.isArray(arts) ? arts : []).find((a) => obj(a).kind === 'patch');
  return num(obj(patch).size_bytes) ?? 0;
}

/** Whether a person still has to accept or reject the attempt: a terminal
 *  attempt that left a patch or a kept workspace (the backend's `needs_review`). */
export function isReviewableAttempt(attempt: Pick<AttemptSummary, 'state' | 'terminal_reason'>): boolean {
  return (
    isTerminalAttemptState(attempt.state) &&
    (attemptPatchBytes(attempt.terminal_reason) > 0 || typeof obj(attempt.terminal_reason).workspace_kept_at === 'string')
  );
}

/** The one status an attempt shows: its review verdict, then what its outcome
 *  says, then its lifecycle state. The run's heading shows its latest
 *  attempt's, so a run never shows two states at once. */
export function describeAttemptStatus(
  attempt: Pick<AttemptSummary, 'state' | 'terminal_reason'>,
  review: AttemptSummary['review'] | undefined = null,
): { label: string; tone: StateTone } {
  if (isReviewableAttempt(attempt)) {
    if (review) return review.verdict === 'accepted' ? { label: 'Accepted', tone: 'success' } : { label: 'Rejected', tone: 'neutral' };
    return attemptPatchBytes(attempt.terminal_reason) > 0
      ? { label: 'Finished — needs your review', tone: 'warning' }
      : { label: 'Finished — changes could not be read, needs your review', tone: 'warning' };
  }
  const badge = describeAttemptOutcome(attempt.state, attempt.terminal_reason).badge;
  if (badge) return { label: badge.label, tone: badge.tone };
  const state = describeExecutionState(attempt.state);
  return { label: state.label, tone: state.tone };
}

const TERMINAL_ATTEMPT_STATES = new Set(['succeeded', 'failed', 'cancelled', 'lost']);

/** Whether an attempt state is final (includes `lost`). */
export function isTerminalAttemptState(state: string): boolean {
  return TERMINAL_ATTEMPT_STATES.has(state);
}

export interface AttemptOutcome {
  /** The "Why it stopped" line, for a failed/cancelled/lost attempt only. */
  whyStopped: string | null;
  /** An override for the `succeeded` badge, when the run's artifacts qualify it. */
  badge: { label: string; tone: StateTone; detail?: string } | null;
}

/** `terminal_reason` is untyped on the wire; read it defensively. */
export function describeAttemptOutcome(state: string, terminalReason: unknown): AttemptOutcome {
  const reason = (terminalReason && typeof terminalReason === 'object' ? terminalReason : {}) as {
    code?: unknown;
    message?: unknown;
    artifacts?: unknown;
    workspace_kept_at?: unknown;
  };
  const str = (v: unknown) => (typeof v === 'string' && v !== '' ? v : null);
  let whyStopped: string | null = null;
  if (state === 'failed' || state === 'cancelled' || state === 'lost') {
    const text = str(reason.message) ?? str(reason.code) ?? 'No reason was reported';
    whyStopped = reason.code === 'harness_rejected' ? `Did not start — ${text}` : text;
  }
  let badge: AttemptOutcome['badge'] = null;
  if (state === 'succeeded') {
    const patch = (Array.isArray(reason.artifacts) ? reason.artifacts : []).find(
      (a) => a && typeof a === 'object' && (a as { kind?: unknown }).kind === 'patch',
    ) as { size_bytes?: unknown } | undefined;
    const patchBytes = typeof patch?.size_bytes === 'number' ? patch.size_bytes : null;
    const kept = str(reason.workspace_kept_at);
    if (patchBytes !== null && patchBytes > 0) {
      badge = { label: 'Succeeded', tone: 'success' };
    } else if (kept) {
      badge = { label: 'Finished — changes could not be read', tone: 'warning', detail: `Workspace kept at ${kept}` };
    } else if (patchBytes === 0) {
      badge = { label: 'Finished — no changes recorded', tone: 'warning', detail: 'The agent ended without changing any files. What it said is below.' };
    }
  }
  return { whyStopped, badge };
}
