import { describe, it, expect } from 'vitest';
import { computeDashboardStats } from './computeStats';
import type { Item, WorkflowStatus } from '../../shared/types';

const STATUSES: WorkflowStatus[] = [
  { name: 'todo', category: 'todo', order: 0 },
  { name: 'doing', category: 'in_progress', order: 1 },
  { name: 'done', category: 'done', order: 2 },
];

const NOW = new Date('2025-06-15T00:00:00Z');
const daysBefore = (n: number) => {
  const d = new Date(NOW);
  d.setDate(d.getDate() - n);
  return d.toISOString();
};

const mk = (over: Partial<Item>): Item =>
  ({
    id: Math.random().toString(36).slice(2),
    item_type: 'task',
    status: 'todo',
    priority: 'medium',
    tags: [],
    created_at: daysBefore(1),
    ...over,
  } as unknown as Item);

describe('computeDashboardStats', () => {
  it('aggregates counts, completion %, WIP, throughput for a fixture set', () => {
    const items: Item[] = [
      mk({ status: 'todo', priority: 'high', estimate: 3 }),
      mk({ status: 'doing', priority: 'critical', estimate: 2 }),
      mk({ status: 'done', priority: 'low', estimate: 5, completed_at: daysBefore(2) }),
      mk({ status: 'done', priority: 'medium', completed_at: daysBefore(20) }),
    ];

    const s = computeDashboardStats(items, STATUSES, NOW);

    expect(s.totalItems).toBe(4);
    // WIP per column
    expect(s.byStatus).toEqual([
      { name: 'todo', count: 1, category: 'todo' },
      { name: 'doing', count: 1, category: 'in_progress' },
      { name: 'done', count: 2, category: 'done' },
    ]);
    expect(s.doneItems).toBe(2);
    expect(s.completionRate).toBe(50); // 2/4
    expect(s.byPriority).toEqual({ critical: 1, high: 1, medium: 1, low: 1 });
    // estimates: 3+2+5 = 10 total; completed estimate = 5 (the done item with estimate)
    expect(s.totalEstimate).toBe(10);
    expect(s.completedEstimate).toBe(5);
    // throughput: 1 completed within 7 days (2 days ago), 2 within 30 days (2 & 20 days ago)
    expect(s.throughput7).toBe(1);
    expect(s.throughput30).toBe(2);
  });

  it('handles an empty project safely', () => {
    const s = computeDashboardStats([], STATUSES, NOW);
    expect(s.totalItems).toBe(0);
    expect(s.completionRate).toBe(0);
    expect(s.throughput7).toBe(0);
  });
});
