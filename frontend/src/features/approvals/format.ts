// Pure formatting/interpretation helpers for the approvals inbox — kept
// isolated and unit-tested like every other feature's `format.ts`
// (`features/fleet/format.ts`, `shared/dispatch/format.ts`,
// `shared/agentActivity/format.ts`).

import type { PendingApproval } from './api';

/** Human-relative elapsed time since `iso`, with no "just now"/"ago" — the
 *  inbox shows this as "Waiting {elapsedSince(...)}" so the wording only
 *  needs to live in one place. Falls back to "unknown" for an unparseable
 *  timestamp rather than blank/NaN. Duplicated from
 *  `features/fleet/format.ts#relativeTime`'s algorithm (not imported — see
 *  `api.ts`'s note on why this module can't reach into `features/fleet/**`,
 *  `architecture.test.ts`'s cross-feature-import rule). */
export function elapsedSince(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return 'unknown';
  const diffSec = Math.max(0, Math.round((Date.now() - then) / 1000));
  if (diffSec < 5) return 'just now';
  if (diffSec < 60) return `${diffSec}s`;
  const min = Math.round(diffSec / 60);
  if (min < 60) return `${min}m`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr}h`;
  const day = Math.round(hr / 24);
  return `${day}d`;
}

/** The correlated-item label for a row, or the honest "uncorrelated" state
 *  — never blank, never a guess. An uncorrelated approval (`item_id: null`)
 *  is the one this whole inbox exists to surface (`api.ts`'s header
 *  comment), so its absence must read as a deliberate, named state, not a
 *  missing-data glitch. */
export function correlatedItemLabel(row: PendingApproval): string {
  if (row.item_title) return row.item_title;
  return 'Uncorrelated — not attributed to a Tack item';
}

/** Display label for the requesting agent — docket's `role` field, or an
 *  honest placeholder rather than a blank cell. */
export function agentLabel(row: PendingApproval): string {
  return row.agent && row.agent.trim() ? row.agent : 'unknown agent';
}

/** Display label for the gated action text, or an honest placeholder. */
export function actionLabel(row: PendingApproval): string {
  return row.action && row.action.trim() ? row.action : '(no action description)';
}
