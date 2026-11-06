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
    node: 'work',
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

  it('counts_the_three_tasks_of_an_epic_and_not_the_epic', () => {
    const epic = mk({ id: 'epic', item_type: 'epic', node: 'group', status: 'doing', estimate: 40, priority: 'high' });
    const tasks = [
      mk({ parent_id: 'epic', status: 'done', estimate: 2, completed_at: daysBefore(1) }),
      mk({ parent_id: 'epic', status: 'doing', estimate: 3 }),
      mk({ parent_id: 'epic', status: 'todo' }),
    ];

    const s = computeDashboardStats([epic, ...tasks], STATUSES, NOW);

    expect(s.totalItems).toBe(3);
    expect(s.doneItems).toBe(1);
    expect(s.completionRate).toBe(33);
    expect(s.byStatus.map((x) => x.count)).toEqual([1, 1, 1]);
    expect(s.byPriority).toEqual({ critical: 0, high: 0, medium: 3, low: 0 });
    expect(s.byType).toEqual([{ name: 'task', count: 3 }]);
    expect(s.totalEstimate).toBe(5);
    expect(s.completedEstimate).toBe(2);
    expect(s.recentItems).toBe(3);
    expect(s.throughput7).toBe(1);
  });

  it('lists_the_top_level_groups_those_needing_a_person_first_then_the_least_left', () => {
    const group = (id: string, title: string, total: number, done: number, needs_you = 0, over: Partial<Item> = {}) =>
      mk({ id, title, item_type: 'epic', node: 'group', progress: { total, done, needs_you, open_runs: 0, teams: [] }, ...over });
    const items = [
      group('big', 'Big', 10, 1),
      group('almost', 'Almost done', 10, 9),
      group('asks', 'Asks a person', 10, 0, 2),
      group('nested', 'Inside Big', 2, 0, 5, { parent_id: 'big' }),
      mk({ id: 'w', parent_id: 'big' }),
    ];

    expect(computeDashboardStats(items, STATUSES, NOW).groups.map((g) => g.title)).toEqual(['Asks a person', 'Almost done', 'Big']);
  });

  it('handles an empty project safely', () => {
    const s = computeDashboardStats([], STATUSES, NOW);
    expect(s.totalItems).toBe(0);
    expect(s.completionRate).toBe(0);
    expect(s.throughput7).toBe(0);
  });
});

describe('computeRunStats', () => {
  it('leaves a run turned away before it started out of the week, as it is no outcome of the work', () => {
    const a = mk({});
    const b = mk({});
    const s = computeRunStats([a, b], [{ ...run(a, 'failed', 1), never_started: true }, run(b, 'failed', 1)], NOW);
    expect(s.week).toEqual({ done: 0, review: 0, needs: 0, running: 0, failed: 1, cancelled: 0 });
    expect(s.withRuns).toBe(2);
  });

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

  const project = {
    id: 'p1',
    name: 'P',
    workflow: { statuses: STATUSES },
    vocabulary: {},
    item_types: [
      { key: 'epic', name: 'Saga', level: 'group', color: 'accent', order: 0, default_work: false },
      { key: 'task', name: 'Task', level: 'work', color: 'neutral', order: 1, default_work: true },
    ],
  } as unknown as Project;

  /** Mounts the Overview on a project whose items are `items`; returns the container and how to take it down. */
  const mountOverview = (items: Item[]) => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ data: items, total: items.length }), { status: 200 })),
    );
    const ctx: ProjectContextValue = {
      projectId: () => 'p1',
      project: (() => project) as unknown as Resource<Project>,
      workflow: () => project.workflow,
      vocabulary: () => ({}),
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
    return { container, dispose };
  };

  const saga = (over: Partial<Item>) =>
    mk({ project_id: 'p1', item_type: 'epic', node: 'group', progress: { total: 3, done: 1, needs_you: 0, open_runs: 0, teams: [] }, ...over });

  it("names a type as the project's catalog does", async () => {
    const { container, dispose } = mountOverview([saga({ title: 'Checkout' }), mk({ item_type: 'task' })]);
    await vi.waitFor(() => expect(container.textContent).toContain('Saga'));
    expect(container.textContent).not.toContain('epic');
    dispose();
  });

  it('lists_each_top_level_group_with_its_progress_and_counts_only_work_everywhere_else', async () => {
    const { container, dispose } = mountOverview([
      saga({ id: 'calm', title: 'Calm epic' }),
      saga({ id: 'asks', title: 'Asking epic', progress: { total: 4, done: 0, needs_you: 2, open_runs: 1, teams: [] } }),
      saga({ id: 'inner', title: 'Inner epic', parent_id: 'calm' }),
      saga({ id: 'empty', title: 'Empty epic', progress: { total: 0, done: 0, needs_you: 0, open_runs: 0, teams: [] } }),
      mk({ parent_id: 'calm' }),
      mk({ parent_id: 'calm', status: 'done' }),
      mk({ parent_id: 'asks' }),
    ]);
    await vi.waitFor(() => expect(container.textContent).toContain('Groups'));

    const rows = [...container.querySelectorAll('[data-testid="group-row"]')];
    // The one asking for a person, then the one with nothing left, then the one with two items left.
    expect(rows.map((row) => row.querySelector('a')?.textContent)).toEqual(['Asking epic', 'Empty epic', 'Calm epic']);
    expect(rows[0].textContent).toContain('2 need you');
    expect(rows[1].textContent).toContain('No work yet');
    expect(rows[2].textContent).toContain('1 of 3 done');
    expect(rows[2].querySelector('a')?.getAttribute('href')).toBe('/projects/p1/items/calm');
    // Three tasks, not seven items.
    const tiles = [...container.querySelectorAll('p')];
    const total = tiles.find((p) => p.textContent === 'Total Items')!.nextElementSibling!;
    expect(total.textContent).toBe('3');
    dispose();
  });

  it('has_no_groups_panel_in_a_project_without_groups', async () => {
    const { container, dispose } = mountOverview([mk({}), mk({})]);
    await vi.waitFor(() => expect(container.textContent).toContain('Status Distribution'));
    expect(container.querySelector('[data-testid="group-row"]')).toBeNull();
    expect(container.textContent).not.toContain('Groups');
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
