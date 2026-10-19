import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { JSX, Resource } from 'solid-js';
import { MemoryRouter, Route, createMemoryHistory } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import { ExecutionStoreProvider } from '../../shared/state/executionContext';
import Breadcrumb from '../../shared/ui/Breadcrumb';
import ItemPage from './ItemPage';

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

const flush = () => new Promise((r) => setTimeout(r, 0));

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

    const dependenciesTab = [...container.querySelectorAll('[role="tab"]')].find((t) =>
      t.textContent?.startsWith('Dependencies'),
    )!;
    expect(dependenciesTab.textContent).toContain('2');

    dispose();
  });
});
