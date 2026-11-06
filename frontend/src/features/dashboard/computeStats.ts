import type { Item, WorkflowStatus } from '../../shared/types';
import type { ExecutionSummary } from '../../shared/execution';
import { cardRun } from '../../shared/runWithAgent/cardRun';

export interface DashboardStats {
  totalItems: number;
  byStatus: { name: string; count: number; category: string }[];
  byPriority: { critical: number; high: number; medium: number; low: number };
  byType: { name: string; count: number }[];
  completionRate: number;
  totalEstimate: number;
  completedEstimate: number;
  recentItems: number;
  doneItems: number;
  /** The top-level groups, the ones needing a person first, then the ones with the least left. */
  groups: Item[];
  throughput7: number;
  throughput30: number;
}

/** The work among the items. A group holds work and is not counted as work itself: its figures come from the work under it. */
export function workOf(items: readonly Item[]): Item[] {
  return items.filter((item) => item.node === 'work');
}

/** The groups with no group above them, those needing a person first, then those with the least left to do. */
function topGroups(items: readonly Item[]): Item[] {
  const held = new Set(items.map((item) => item.id));
  const left = (group: Item) => (group.progress?.total ?? 0) - (group.progress?.done ?? 0);
  return items
    .filter((item) => item.node === 'group' && !(item.parent_id && held.has(item.parent_id)))
    .sort(
      (a, b) =>
        (b.progress?.needs_you ?? 0) - (a.progress?.needs_you ?? 0)
        || left(a) - left(b),
    );
}

/** Pure aggregation of project items for the dashboard: counts, points and breakdowns cover work only,
 * and the groups are listed apart. `now` is injected so time-window metrics are deterministic in tests. */
export function computeDashboardStats(
  allItems: Item[],
  statuses: WorkflowStatus[],
  now: Date,
): DashboardStats {
  const items = workOf(allItems);
  const totalItems = items.length;

  const byStatus = statuses.map((s) => ({
    name: s.name,
    count: items.filter((i) => i.status === s.name).length,
    category: s.category,
  }));

  const byPriority = {
    critical: items.filter((i) => i.priority === 'critical').length,
    high: items.filter((i) => i.priority === 'high').length,
    medium: items.filter((i) => i.priority === 'medium').length,
    low: items.filter((i) => i.priority === 'low').length,
  };

  const typeSet = new Set(items.map((i) => i.item_type));
  const byType = Array.from(typeSet).map((name) => ({
    name,
    count: items.filter((i) => i.item_type === name).length,
  }));

  const doneStatusNames = new Set(byStatus.filter((s) => s.category === 'done').map((s) => s.name));
  const doneItems = items.filter((i) => doneStatusNames.has(i.status)).length;
  const completionRate = totalItems > 0 ? Math.round((doneItems / totalItems) * 100) : 0;

  const withEstimates = items.filter((i) => i.estimate && i.estimate > 0);
  const totalEstimate = withEstimates.reduce((sum, i) => sum + (i.estimate || 0), 0);
  const completedEstimate = withEstimates
    .filter((i) => doneStatusNames.has(i.status))
    .reduce((sum, i) => sum + (i.estimate || 0), 0);

  const daysAgo = (n: number) => {
    const d = new Date(now);
    d.setDate(d.getDate() - n);
    return d;
  };
  const since7 = daysAgo(7);
  const since30 = daysAgo(30);
  const recentItems = items.filter((i) => new Date(i.created_at) > since7).length;
  const completedSince = (since: Date) =>
    items.filter((i) => i.completed_at && new Date(i.completed_at) > since).length;

  return {
    totalItems,
    byStatus,
    byPriority,
    byType,
    completionRate,
    totalEstimate,
    completedEstimate,
    recentItems,
    doneItems,
    groups: topGroups(allItems),
    throughput7: completedSince(since7),
    throughput30: completedSince(since30),
  };
}

/** The outcomes the Overview's runs bar sorts a run into. Queued and running share one. */
export type RunOutcome = 'done' | 'review' | 'needs' | 'running' | 'failed' | 'cancelled';

export interface RunStats {
  /** Items whose latest run started in the last seven days, by outcome. */
  week: Record<RunOutcome, number>;
  /** Items that have a run at all; the runs row is hidden while this is zero. */
  withRuns: number;
  /** Items that need a person now: a question to answer, or a finished run ready for review. */
  needYou: number;
  /** Tokens are counted per attempt, which the Overview does not load, so they are never added up per team. */
  tokensByTeam: null;
}

/** Why the tokens-per-team tile is disabled. Read from the Overview's own loaded data, not from a request. */
export const TOKENS_BY_TEAM_UNAVAILABLE =
  'Tokens are counted per attempt, and this page does not load attempts, so they cannot be added up by team.';

/** The state of an item's latest run, in the board's own words; queued counts with running. */
function outcomeOf(summary: ExecutionSummary | undefined, needsReview: boolean): RunOutcome | undefined {
  const run = cardRun(summary, needsReview);
  // A run turned away before it started is no outcome of the work, so the week's counts leave it out.
  if (!run || run.state === 'notstarted') return undefined;
  return run.state === 'queued' ? 'running' : run.state;
}

/**
 * Pure aggregation of the project's runs for the Overview. Each item counts once, by its
 * latest run, since that is what the loaded run data holds. `now` is injected as above.
 */
export function computeRunStats(items: Item[], runs: ExecutionSummary[], now: Date): RunStats {
  const itemIds = new Set(items.map((i) => i.id));
  const latest = new Map<string, ExecutionSummary>();
  for (const run of runs) {
    if (!itemIds.has(run.item_id)) continue;
    const prior = latest.get(run.item_id);
    if (!prior || run.created_at > prior.created_at) latest.set(run.item_id, run);
  }

  const since7 = new Date(now);
  since7.setDate(since7.getDate() - 7);
  const week: Record<RunOutcome, number> = { done: 0, review: 0, needs: 0, running: 0, failed: 0, cancelled: 0 };
  let withRuns = 0;
  let needYou = 0;
  for (const item of items) {
    const summary = latest.get(item.id);
    const needsReview = item.needs_review === true;
    const outcome = outcomeOf(summary, needsReview);
    if (summary || needsReview) withRuns += 1;
    if (outcome === 'needs' || outcome === 'review') needYou += 1;
    if (summary && outcome && new Date(summary.created_at) > since7) week[outcome] += 1;
  }

  return { week, withRuns, needYou, tokensByTeam: null };
}
