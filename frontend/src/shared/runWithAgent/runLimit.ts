import { readContinueOffer } from '../execution';

/** A running run's token count turns amber at this share of its limit. */
const RUN_LIMIT_WARNING = 0.8;

/** Whether `used` has reached the warning share of `limit`; false without a limit. */
export function isNearLimit(used: number | null | undefined, limit: number | null | undefined): boolean {
  return typeof used === 'number' && typeof limit === 'number' && limit > 0 && used / limit >= RUN_LIMIT_WARNING;
}

/** Whether an attempt's `terminal_reason` says its token limit stopped it: the runner's
 *  `token_limit`, or docket's `budget` when the hop it stopped at carries no step bound of its
 *  own (the shape docket sent before Phase 87's `budgetOutcome`, or a hop whose bound really is
 *  `tokens` — Tack's own limit, read above). A step's own round, tool-call or time budget
 *  (`code: "step_budget"`, or a `budget` hop whose bound is one of those) is never a token
 *  stop — see `docs/closed-cycles/plans/phase-87.md`, decision 1. */
export function stoppedByLimit(terminalReason: unknown): boolean {
  const reason = (terminalReason ?? {}) as { code?: unknown; reason?: unknown; steps?: unknown };
  const says = (word: string) => reason.code === word || reason.reason === word;
  if (says('token_limit')) return true;
  if (says('step_budget')) return false;
  if (!says('budget')) return false;
  const steps = Array.isArray(reason.steps) ? reason.steps : [];
  const last = steps[steps.length - 1] as { budget?: { bound?: unknown } } | null | undefined;
  const bound = last && typeof last === 'object' ? last.budget?.bound : undefined;
  return bound === undefined || bound === null || bound === 'tokens';
}

/** Whether `terminal_reason` already carries docket's `next.continue` (Phase 87's
 *  `budgetOutcome`) — regardless of whether it reads `possible: true` or `false`. When it does,
 *  `ContinueOrStartOver` owns the "stopped" telling for this attempt (decision 5: "Start over
 *  from this run's work, with twice the limit" **replaces** "Run again with more" for a tokens
 *  stop that kept work); `RunLimitNote`'s own "Stopped at its limit" block must not also show,
 *  or the two repeat each other. A docket too old to send `next` at all leaves this false, and
 *  `RunLimitNote` keeps telling that stop exactly as it always has. */
export function hasBudgetStopOffer(terminalReason: unknown): boolean {
  return readContinueOffer(terminalReason) !== null;
}

/** The limit "Run again with more" starts the Run dialog with: twice the old one. */
export const doubled = (limit: number): number => limit * 2;

/** A token count in groups of three, separated by a space ("1 600 000"). */
export const spaced = (n: number): string => String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ' ');
