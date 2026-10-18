import { describe, it, expect, vi, afterEach } from 'vitest';
import { createComponent, type Resource } from 'solid-js';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import type { Project } from '../../shared/types';
import Dashboard from './Dashboard';
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

describe('Dashboard item types', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    document.body.innerHTML = '';
  });

  it("labels a type with the project's word for it", async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ data: [mk({ item_type: 'epic' }), mk({ item_type: 'task' })], total: 2 }), { status: 200 })),
    );
    const project = { id: 'p1', name: 'P', workflow: { statuses: STATUSES }, vocabulary: { epic: 'Saga' } } as unknown as Project;
    const ctx: ProjectContextValue = {
      projectId: () => 'p1',
      project: (() => project) as unknown as Resource<Project>,
      workflow: () => project.workflow,
      vocabulary: () => ({ epic: 'Saga' }),
      refetch: () => {},
    };
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(
      () =>
        createComponent(ProjectContext.Provider, {
          value: ctx,
          get children() {
            return createComponent(MemoryRouter, {
              get children() {
                return createComponent(Route, { path: '/', component: Dashboard });
              },
            });
          },
        }),
      container,
    );
    await vi.waitFor(() => expect(container.textContent).toContain('Saga'));
    expect(container.textContent).not.toContain('epic');
    dispose();
  });
});
