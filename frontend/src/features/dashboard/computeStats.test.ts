import { describe, it, expect, vi, afterEach } from 'vitest';
import { createComponent, type Resource } from 'solid-js';
import { render } from 'solid-js/web';
import { MemoryRouter, Route, createMemoryHistory } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import { ExecutionStoreProvider } from '../../shared/state/executionContext';
import type { ExecutionSummary } from '../../shared/execution';
import type { Project } from '../../shared/types';
import Dashboard from './Dashboard';
import { computeDashboardStats, computeRunStats, TOKENS_BY_TEAM_UNAVAILABLE } from './computeStats';
import type { Item, WorkflowStatus } from '../../shared/types';

const STATUSES: WorkflowStatus[] = [
  { name: 'todo', category: 'todo', order: 0 },
  { name: 'doing', category: 'in_progress', order: 1 },
  { name: 'done', category: 'done', order: 2 },
];

const NOW = new Date('2025-06-15T00:00:00Z');
const daysBefore = (n: number, from: Date = NOW) => {
  const d = new Date(from);
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

let runSeq = 0;
/** A run of `item` that started `daysAgo` days before `from`; the Overview reads the real clock, so its render tests pass the real one. */
const run = (item: Item, state: string, daysAgo: number, from: Date = NOW): ExecutionSummary => ({
  request_id: `req-${++runSeq}`,
  item_id: item.id,
  state,
  cancellation_requested_at: null,
  created_at: daysBefore(daysAgo, from),
});

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

describe('computeRunStats', () => {
  it('sorts each item by its latest run into the last seven days, and counts what needs a person now', () => {
    const a = mk({});
    const b = mk({ needs_review: true });
    const c = mk({});
    const d = mk({});
    const e = mk({});
    const f = mk({});
    const g = mk({ needs_review: true });
    const h = mk({});
    const i = mk({ needs_review: true });
    const items = [a, b, c, d, e, f, g, h, i];
    const runs = [
      run(a, 'failed', 20), // an older run is replaced by the newer one below
      run(a, 'failed', 2),
      run(b, 'succeeded', 3), // finished and not yet reviewed: ready for review
      run(c, 'succeeded', 1),
      run(d, 'waiting_decision', 1), // asks a question
      run(e, 'running', 6),
      run(f, 'cancelled', 10), // outside the window: counted as having run, not in the bar
      run(i, 'failed', 2), // a failed run is never ready for review, whatever the item flag says
      { ...run(h, 'failed', 1), item_id: 'some-other-items-run' }, // belongs to no item here
    ];

    const s = computeRunStats(items, runs, NOW);

    expect(s.week).toEqual({ done: 1, review: 1, needs: 1, running: 1, failed: 2, cancelled: 0 });
    // g has no run loaded yet but its item needs review, so it still counts as ready for review
    expect(s.needYou).toBe(3); // b (review), d (question), g (review)
    expect(s.withRuns).toBe(8); // every item with a run; h has none, and a run for an unknown item is ignored
  });

  it('counts an item whose only run is older than the window in the row but not in the bar', () => {
    const a = mk({});
    const s = computeRunStats([a], [run(a, 'succeeded', 30)], NOW);
    expect(s.week).toEqual({ done: 0, review: 0, needs: 0, running: 0, failed: 0, cancelled: 0 });
    expect(s.withRuns).toBe(1);
  });

  it('never adds tokens up per team, since the loaded runs do not carry them', () => {
    const s = computeRunStats([mk({})], [], NOW);
    expect(s.tokensByTeam).toBeNull();
    expect(TOKENS_BY_TEAM_UNAVAILABLE).toMatch(/attempt/);
  });

  it('is empty for a project with no items and for one that has never run anything', () => {
    const empty = computeRunStats([], [], NOW);
    expect(empty.withRuns).toBe(0);
    expect(empty.needYou).toBe(0);
    expect(Object.values(empty.week).every((n) => n === 0)).toBe(true);

    const neverRan = computeRunStats([mk({}), mk({})], [], NOW);
    expect(neverRan.withRuns).toBe(0);
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
        createComponent(ExecutionStoreProvider, {
          get children() {
            return createComponent(ProjectContext.Provider, {
              value: ctx,
              get children() {
                return createComponent(MemoryRouter, {
                  get children() {
                    return createComponent(Route, { path: '/', component: Dashboard });
                  },
                });
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

describe('Dashboard runs row', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    document.body.innerHTML = '';
  });

  /** Renders the Overview for a project whose items and runs are given; the item reads and the run reads are answered from them. */
  function renderOverview(items: Item[], runs: ExecutionSummary[]) {
    vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
      const url = String(input);
      const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200 });
      if (url.includes('/executions')) return json({ protocol_version: 1, data: runs });
      return json({ data: items, total: items.length });
    }) as typeof fetch);
    const project = { id: 'p1', name: 'P', workflow: { statuses: STATUSES } } as unknown as Project;
    const ctx: ProjectContextValue = {
      projectId: () => 'p1',
      project: (() => project) as unknown as Resource<Project>,
      workflow: () => project.workflow,
      vocabulary: () => ({}),
      refetch: () => {},
    };
    const history = createMemoryHistory();
    history.set({ value: '/projects/p1/overview' });
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(
      () =>
        createComponent(ExecutionStoreProvider, {
          get children() {
            return createComponent(ProjectContext.Provider, {
              value: ctx,
              get children() {
                return createComponent(MemoryRouter, {
                  history,
                  get children() {
                    return createComponent(Route, { path: '/projects/:id/overview', component: Dashboard });
                  },
                });
              },
            });
          },
        }),
      container,
    );
    return { container, dispose };
  }

  it('shows each outcome in the bar with its count in words, tokens per team disabled with the reason, and a link to Factory metrics', async () => {
    const real = new Date();
    const asking = mk({});
    const failed = mk({});
    const { container, dispose } = renderOverview(
      [asking, failed],
      [run(asking, 'waiting_decision', 0, real), run(failed, 'failed', 2, real)],
    );
    await vi.waitFor(() =>
      expect(container.querySelector('[role="img"][aria-label^="Runs started in the last 7 days"]')?.getAttribute('aria-label')).toBe(
        'Runs started in the last 7 days: 1 needs you, 1 failed.',
      ),
    );
    expect(container.textContent).toContain(TOKENS_BY_TEAM_UNAVAILABLE);
    expect(container.querySelector('[aria-disabled="true"]')?.textContent).toContain('Tokens per team');
    expect(container.querySelector('a[href="/projects/p1/factory"]')?.textContent).toContain('Factory metrics');
    dispose();
  });

  it('is hidden for a project that has never run anything', async () => {
    const { container, dispose } = renderOverview([mk({})], []);
    await vi.waitFor(() => expect(container.textContent).toContain('Total Items'));
    expect(container.textContent).not.toContain('Tokens per team');
    expect(container.querySelector('[role="img"][aria-label^="Runs started"]')).toBeNull();
    dispose();
  });
});
