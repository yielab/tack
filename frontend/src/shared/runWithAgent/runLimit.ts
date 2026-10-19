/** A running run's token count turns amber at this share of its limit. */
export const RUN_LIMIT_WARNING = 0.8;

/** Whether `used` has reached the warning share of `limit`; false without a limit. */
export function isNearLimit(used: number | null | undefined, limit: number | null | undefined): boolean {
  return typeof used === 'number' && typeof limit === 'number' && limit > 0 && used / limit >= RUN_LIMIT_WARNING;
}

/** Whether an attempt's `terminal_reason` says its token limit stopped it: the runner's
 *  `token_limit`, or docket's `budget`. */
export function stoppedByLimit(terminalReason: unknown): boolean {
  const reason = (terminalReason ?? {}) as { code?: unknown; reason?: unknown };
  return ['token_limit', 'budget'].some((word) => reason.code === word || reason.reason === word);
}

/** The limit "Run again with more" starts the Run dialog with: twice the old one. */
export const doubled = (limit: number): number => limit * 2;

/** A token count in groups of three, separated by a space ("1 600 000"). */
export const spaced = (n: number): string => String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ' ');
