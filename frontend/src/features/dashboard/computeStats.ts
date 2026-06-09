import type { Item, WorkflowStatus } from '../../shared/types';

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
  throughput7: number;
  throughput30: number;
}

const typeKey = (i: Item) =>
  typeof i.item_type === 'string' ? i.item_type : i.item_type.custom;

/** Pure aggregation of project items for the dashboard (T-514). `now` is
 * injected so time-window metrics are deterministic in tests. */
export function computeDashboardStats(
  items: Item[],
  statuses: WorkflowStatus[],
  now: Date,
): DashboardStats {
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

  const typeSet = new Set(items.map(typeKey));
  const byType = Array.from(typeSet).map((name) => ({
    name,
    count: items.filter((i) => typeKey(i) === name).length,
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
    throughput7: completedSince(since7),
    throughput30: completedSince(since30),
  };
}
