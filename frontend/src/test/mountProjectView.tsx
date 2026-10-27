import { vi } from 'vitest';
import { render } from 'solid-js/web';
import type { JSX, Resource } from 'solid-js';
import { MemoryRouter, Route, createMemoryHistory } from '@solidjs/router';
import { ExecutionStoreProvider } from '../shared/state/executionContext';
import { ProjectContext, type ProjectContextValue } from '../shared/state/projectContext';
import { ProjectItemsProvider } from '../shared/state/projectItemsContext';
import type { Dependency, Item, Sprint } from '../shared/types';
import { flush } from './flush';

export function makeItem(p: Partial<Item> & { id: string }): Item {
  return {
    project_id: 'p1',
    title: 'Untitled',
    item_type: 'task',
    status: 'To Do',
    priority: 'medium',
    estimate_unit: 'story_points',
    tags: [],
    sort_order: 0,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...p,
  };
}

export const STATUSES = [
  { name: 'To Do', order: 0, category: 'todo' },
  { name: 'Done', order: 1, category: 'done' },
];

const PROJECT = {
  projectId: () => 'p1',
  project: (() => ({
    id: 'p1',
    name: 'Proj',
    workflow: { workflow_type: 'kanban', statuses: STATUSES },
  })) as unknown as Resource<null>,
  workflow: () => undefined,
  vocabulary: () => ({}),
  refetch: () => {},
} as ProjectContextValue;

const disposers: Array<() => void> = [];

/** Takes down every view `mountProjectView` put in the document; call it from `afterEach`. */
export function unmountProjectViews() {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
}

interface Served {
  items: Item[];
  sprints?: Sprint[];
  /** Dependencies by the id of the item they are listed for. */
  dependencies?: Record<string, Dependency[]>;
}

/**
 * Mounts a project view the way the app does, at `/projects/p1/<segment>`, with the project's
 * items (and sprints, and dependencies) answered by a stubbed `fetch`. Returns its container once
 * the first loads have settled.
 */
export async function mountProjectView(view: () => JSX.Element, segment: string, served: Served) {
  vi.stubGlobal('WebSocket', class { close() {} addEventListener() {} });
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
    const url = String(input);
    const json = (body: unknown) =>
      new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json', ETag: '"1"' } });
    if (url.includes('/items?')) {
      return json({ data: served.items, total: served.items.length, page: 1, per_page: 200 });
    }
    if (url.endsWith('/sprints')) return json(served.sprints ?? []);
    const dependencies = /\/items\/([^/]+)\/dependencies$/.exec(url);
    if (dependencies) return json(served.dependencies?.[dependencies[1]] ?? []);
    return json({ protocol_version: 1, data: [] });
  }) as typeof fetch);

  const history = createMemoryHistory();
  history.set({ value: `/projects/p1/${segment}` });
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ExecutionStoreProvider>
        <ProjectContext.Provider value={PROJECT}>
          <MemoryRouter history={history}>
            <Route
              path={`/projects/:id/${segment}`}
              component={() => <ProjectItemsProvider>{view()}</ProjectItemsProvider>}
            />
          </MemoryRouter>
        </ProjectContext.Provider>
      </ExecutionStoreProvider>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  await flush();
  await flush();
  return container;
}
