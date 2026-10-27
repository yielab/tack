import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { JSX, Resource } from 'solid-js';
import { MemoryRouter, Route, createMemoryHistory } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import { ExecutionStoreProvider } from '../../shared/state/executionContext';
import Breadcrumb from '../../shared/ui/Breadcrumb';
import ItemPage from './ItemPage';
import { flush } from '../../test/flush';

const ITEM = {
  id: 'abcdef12-0000-0000-0000-000000000000',
  project_id: 'p1',
  title: 'My item',
  item_type: 'task',
  status: 'todo',
  priority: 'medium',
  tags: [],
  estimate_unit: 'story_points',
  sort_order: 0,
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-02T00:00:00Z',
};

const projectValue: ProjectContextValue = {
  projectId: () => 'p1',
  project: (() => ({ id: 'p1', name: 'Proj' })) as unknown as Resource<null>,
  workflow: () => ({
    workflow_type: 'kanban',
    statuses: [{ name: 'todo', category: 'todo', order: 0 }],
  }),
  vocabulary: () => ({}),
  refetch: () => {},
};

// Two edges: this item blocks one item and is blocked by another.
const DEPENDENCIES = [
  { id: 'd1', source_item_id: ITEM.id, target_item_id: 'x', dependency_type: 'blocks', created_at: '' },
  { id: 'd2', source_item_id: 'y', target_item_id: ITEM.id, dependency_type: 'blocks', created_at: '' },
];

function jsonOf(url: string): unknown {
  if (url.endsWith(`/api/items/${ITEM.id}`)) return { item: ITEM, roles: [], dependencies: DEPENDENCIES };
  if (url.endsWith('/dependencies')) return DEPENDENCIES;
  if (url.includes('/sprints')) return [];
  if (url.includes('/executions')) return { protocol_version: 1, data: [] };
  return {};
}

beforeEach(() => {
  vi.spyOn(globalThis, 'fetch').mockImplementation((input: RequestInfo | URL) =>
    Promise.resolve(
      new Response(JSON.stringify(jsonOf(String(input))), {
        status: 200,
        headers: { 'Content-Type': 'application/json', ETag: '"1"' },
      }),
    ),
  );
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('ItemPage', () => {
  it('shows the item under a breadcrumb, with its properties and a count on Dependencies', async () => {
    const history = createMemoryHistory();
    history.set({ value: `/projects/p1/items/${ITEM.id}` });
    const container = document.createElement('div');
    document.body.appendChild(container);
    const Root = (p: { children?: JSX.Element }) => (
      <>
        <Breadcrumb />
        <main>{p.children}</main>
      </>
    );
    const dispose = render(
      () => (
        <ExecutionStoreProvider>
          <ProjectContext.Provider value={projectValue}>
            <MemoryRouter history={history} root={Root}>
              <Route path="/projects/:id/items/:itemId" component={ItemPage} />
            </MemoryRouter>
          </ProjectContext.Provider>
        </ExecutionStoreProvider>
      ),
      container,
    );
    await flush();
    await flush();

    const crumb = container.querySelector('nav[aria-label="Breadcrumb"]')!;
    expect(crumb.textContent).toContain('Proj');
    expect(crumb.textContent).toContain('Items');
    expect(crumb.textContent).toContain('ABCDEF');

    expect(container.querySelector<HTMLInputElement>('input[aria-label="Item title"]')?.value).toBe('My item');
    const properties = container.querySelector('aside section[aria-label="Properties"]')!;
    expect(properties.textContent).toContain('Assignee');
    expect(properties.textContent).toContain('1 item'); // what it blocks
    expect(container.textContent).toContain('Copy link');
    expect(document.title).toBe('My item — Proj · Tack');

    const dependenciesTab = [...container.querySelectorAll('[role="tab"]')].find((t) =>
      t.textContent?.startsWith('Dependencies'),
    )!;
    expect(dependenciesTab.textContent).toContain('2');

    dispose();
  });

  it('on the Execution tab only the tab asks for the verdict, and the run has a side column', async () => {
    const run = { request_id: 'req-1', item_id: ITEM.id, state: 'succeeded', cancellation_requested_at: null, created_at: '2026-10-08T12:00:00Z' };
    const attempt = {
      attempt_id: 'att-1', request_id: 'req-1', attempt_number: 1, runner_id: 'runner-1', fencing_token: 1, state: 'succeeded',
      lease_issued_at: '2026-10-08T12:00:00Z', lease_expires_at: '2026-10-08T12:05:00Z', last_heartbeat_at: null,
      event_checkpoint: null, completion_id: null, workspace_id: null, base_revision: null, actual_execution: null,
      terminal_reason: { workspace_kept_at: '/w/1', result: 'Done.' }, usage: null, counted_tokens: null,
      started_at: '2026-10-08T12:00:05Z', ended_at: '2026-10-08T12:02:00Z', created_at: '2026-10-08T12:00:00Z',
      updated_at: '2026-10-08T12:02:00Z', model_provenance: null,
      usage_economics: {
        model_token_cost_usd_estimated: { value: null, source: 'not_measured' },
        runner_time_cost: { wall_clock_ms: null, cost_usd_estimated: { value: null, source: 'not_measured' } },
      },
      pull_request: null, review: null,
    };
    vi.spyOn(globalThis, 'fetch').mockImplementation((input: RequestInfo | URL) => {
      const url = String(input);
      let body: unknown;
      if (url.includes('/artifacts') || url.includes('/events')) body = { protocol_version: 1, data: [] };
      else if (url.includes('/attempts')) body = { protocol_version: 1, data: [attempt] };
      else if (url.includes('/executions')) body = { protocol_version: 1, data: [run] };
      else body = jsonOf(url);
      return Promise.resolve(new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json', ETag: '"1"' } }));
    });
    const history = createMemoryHistory();
    history.set({ value: `/projects/p1/items/${ITEM.id}?tab=execution` });
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(
      () => (
        <ExecutionStoreProvider>
          <ProjectContext.Provider value={projectValue}>
            <MemoryRouter history={history}>
              <Route path="/projects/:id/items/:itemId" component={ItemPage} />
            </MemoryRouter>
          </ProjectContext.Provider>
        </ExecutionStoreProvider>
      ),
      container,
    );
    for (let i = 0; i < 8; i++) await flush();

    const accepts = [...container.querySelectorAll('button')].filter((b) => b.textContent === 'Accept');
    expect(accepts).toHaveLength(1);
    expect(container.querySelector('[data-testid="review-banner"]')?.contains(accepts[0])).toBe(true);
    // The run card keeps its state and its link to the report.
    expect(container.querySelector('[data-testid="latest-run"]')?.textContent).toContain('See what it did');
    // The side column of the tab holds the ids.
    expect(container.querySelector('[data-testid="execution-side"] [data-testid="attempt-ids"]')).not.toBeNull();
    dispose();
  });
});
