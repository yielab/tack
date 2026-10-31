import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { Resource } from 'solid-js';
import { MemoryRouter, Route, createMemoryHistory } from '@solidjs/router';
import { ExecutionStoreProvider } from '../../shared/state/executionContext';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import { ProjectItemsProvider } from '../../shared/state/projectItemsContext';
import ToastContainer from '../../shared/ui/ToastContainer';
import Board, { ItemCard } from './Board';
import type { Item } from '../../shared/types';
import { flush } from '../../test/flush';
const disposers: Array<() => void> = [];

function makeItem(p: Partial<Item> = {}): Item {
  return {
    id: 'item-1',
    project_id: 'project-1',
    title: 'Fix login bug',
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

const STATUSES = ['To Do', 'Doing', 'Done'];

async function mount(item: Item, onMove: (id: string, status: string) => void = () => {}) {
  vi.spyOn(globalThis, 'fetch').mockImplementation(
    (async () => new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 })) as typeof fetch,
  );
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ExecutionStoreProvider>
        <MemoryRouter>
          <Route path="/" component={() => <ItemCard item={item} typeLabel="Task" statuses={STATUSES} onEdit={() => {}} onMove={onMove} />} />
        </MemoryRouter>
      </ExecutionStoreProvider>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  await flush();
  return container;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('Board ItemCard', () => {
  it('a_card_shows_needs_your_review', async () => {
    const flagged = await mount(makeItem({ needs_review: true }));
    expect(flagged.textContent).toContain('Needs your review');
    const plain = await mount(makeItem({ needs_review: false }));
    expect(plain.textContent).not.toContain('Needs your review');
  });

  it('a_card_shows_repeats_only_for_a_recurring_task', async () => {
    const repeating = await mount(makeItem({ recurrence: 'weekly' }));
    expect(repeating.textContent).toContain('Repeats');
    const plain = await mount(makeItem());
    expect(plain.textContent).not.toContain('Repeats');
  });
});

const tick = (el: Element, key: string) =>
  el.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));

describe('Board move menu', () => {
  it('a_card_moves_by_keyboard_through_its_move_to_menu', async () => {
    const onMove = vi.fn();
    const container = await mount(makeItem(), onMove);
    const button = container.querySelector<HTMLButtonElement>('[aria-label="Move to…"]')!;
    expect(button.getAttribute('aria-expanded')).toBe('false');

    // ArrowDown opens it, as Enter or Space does through the button's click.
    tick(button, 'ArrowDown');
    await flush();
    const menu = document.querySelector('[role="menu"]')!;
    const names = [...menu.querySelectorAll('[role="menuitem"]')].map((el) => el.textContent);
    expect(names).toEqual(['Doing', 'Done']); // the other columns, not its own
    expect(button.getAttribute('aria-expanded')).toBe('true');
    expect(document.activeElement?.textContent).toBe('Doing');

    tick(menu, 'ArrowDown');
    expect(document.activeElement?.textContent).toBe('Done');
    (document.activeElement as HTMLElement).click(); // what Enter does on a button

    expect(onMove).toHaveBeenCalledWith('item-1', 'Done');
    expect(document.querySelector('[role="menu"]')).toBeNull();
    expect(document.activeElement).toBe(button);
  });

  it('escape_closes_the_menu_without_moving', async () => {
    const onMove = vi.fn();
    const container = await mount(makeItem(), onMove);
    const button = container.querySelector<HTMLButtonElement>('[aria-label="Move to…"]')!;
    tick(button, 'ArrowDown');
    await flush();
    tick(document.querySelector('[role="menu"]')!, 'Escape');
    expect(document.querySelector('[role="menu"]')).toBeNull();
    expect(document.activeElement).toBe(button);
    expect(onMove).not.toHaveBeenCalled();
  });
});

const PROJECT = {
  projectId: () => 'p1',
  project: (() => ({
    id: 'p1',
    name: 'Proj',
    workflow: { workflow_type: 'kanban', statuses: STATUSES.map((name, order) => ({ name, order, category: 'todo' })) },
  })) as unknown as Resource<null>,
  workflow: () => undefined,
  vocabulary: () => ({}),
  refetch: () => {},
} as ProjectContextValue;

describe('Board columns', () => {
  it('each_column_is_a_named_region_and_its_scroll_takes_focus_and_a_move_is_announced', async () => {
    const item = makeItem();
    const patches: unknown[] = [];
    vi.stubGlobal('WebSocket', class { close() {} addEventListener() {} });
    vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const json = (body: unknown) =>
        new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json', ETag: '"1"' } });
      if (init?.method === 'PATCH') { patches.push(JSON.parse(String(init.body))); return json({ ...item, status: 'Doing' }); }
      if (url.includes('/items?')) return json({ data: [item], total: 1, page: 1, per_page: 200 });
      if (url.endsWith('/api/items/item-1')) return json({ item, roles: [], dependencies: [] });
      return json({ protocol_version: 1, data: [] });
    }) as typeof fetch);

    const history = createMemoryHistory();
    history.set({ value: '/projects/p1/board' });
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(
      () => (
        <ExecutionStoreProvider>
          <ProjectContext.Provider value={PROJECT}>
            <MemoryRouter history={history}>
              <Route
                path="/projects/:id/board"
                component={() => (
                  <ProjectItemsProvider>
                    <Board />
                  </ProjectItemsProvider>
                )}
              />
            </MemoryRouter>
            <ToastContainer />
          </ProjectContext.Provider>
        </ExecutionStoreProvider>
      ),
      container,
    );
    disposers.push(() => { dispose(); container.remove(); });
    await flush();
    await flush();

    const regions = [...container.querySelectorAll('[role="region"]')];
    expect(regions.map((r) => document.getElementById(r.getAttribute('aria-labelledby')!)?.textContent)).toEqual(STATUSES);
    for (const region of regions) {
      const body = region.querySelector('[tabindex="0"]');
      expect(body?.getAttribute('aria-labelledby')).toBe(region.getAttribute('aria-labelledby'));
    }
    expect(container.querySelector('[aria-label="Add item to To Do"]')?.textContent).toContain('Add item');

    const button = container.querySelector<HTMLButtonElement>('[aria-label="Move to…"]')!;
    tick(button, 'ArrowDown');
    await flush();
    (document.activeElement as HTMLElement).click(); // "Doing", the first other column
    await flush();
    await flush();

    expect(patches).toEqual([{ status: 'Doing' }]);
    expect(document.querySelector('[aria-live="polite"]')?.textContent).toContain('Item moved to Doing');
  });
});
