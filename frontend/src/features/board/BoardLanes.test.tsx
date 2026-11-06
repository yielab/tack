import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { Resource } from 'solid-js';
import { MemoryRouter, Route, createMemoryHistory } from '@solidjs/router';
import { ExecutionStoreProvider } from '../../shared/state/executionContext';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import { ProjectItemsProvider } from '../../shared/state/projectItemsContext';
import ToastContainer from '../../shared/ui/ToastContainer';
import type { Item, ItemTypeDef, Progress } from '../../shared/types';
import { flush } from '../../test/flush';
import Board from './Board';

const CATALOG: ItemTypeDef[] = [
  { key: 'epic', name: 'Epic', color: 'accent2', level: 'group', default_work: false, order: 0 },
  { key: 'task', name: 'Task', color: 'accent', level: 'work', default_work: true, order: 1 },
];
const STATUSES = [
  { name: 'To Do', order: 0, category: 'todo' },
  { name: 'Doing', order: 1, category: 'in_progress' },
  { name: 'Done', order: 2, category: 'done' },
];

const PROJECT = {
  projectId: () => 'p1',
  project: (() => ({
    id: 'p1', name: 'Proj', item_types: CATALOG,
    workflow: { workflow_type: 'kanban', statuses: STATUSES },
  })) as unknown as Resource<null>,
  workflow: () => undefined,
  vocabulary: () => ({}),
  refetch: () => {},
} as ProjectContextValue;

const progress = (p: Partial<Progress>): Progress => ({
  total: 2, done: 1, open_runs: 0, needs_you: 0, estimate_total: null, estimate_done: null, starts: null, ends: null, teams: [], ...p,
});

function item(p: Partial<Item> & { id: string }): Item {
  return {
    project_id: 'p1', title: p.id, item_type: 'task', node: 'work', status: 'To Do', priority: 'medium',
    estimate_unit: 'story_points', tags: [], sort_order: 0, created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z', ...p,
  };
}

// Two epics (one done, so its lane starts folded), an empty epic, and work with no parent.
const GROUPS = [
  item({ id: 'g-checkout', title: 'Checkout', item_type: 'epic', node: 'group', status: 'Doing', sort_order: 1, progress: progress({ needs_you: 1 }) }),
  item({ id: 'g-billing', title: 'Billing', item_type: 'epic', node: 'group', status: 'Done', sort_order: 0, progress: progress({ total: 1 }) }),
  item({ id: 'g-empty', title: 'Reports', item_type: 'epic', node: 'group', status: 'To Do', sort_order: 2 }),
];
const WORK = [
  item({ id: 'w-form', title: 'Card form', parent_id: 'g-checkout', status: 'To Do', priority: 'high' }),
  item({ id: 'w-pay', title: 'Payments page', parent_id: 'g-checkout', status: 'Doing' }),
  item({ id: 'w-invoice', title: 'Invoice', parent_id: 'g-billing', status: 'Done', assignee: 'ana' }),
  item({ id: 'w-loose', title: 'Loose task', status: 'To Do' }),
];

const disposers: Array<() => void> = [];
beforeEach(() => localStorage.clear());
afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

interface Served {
  items: Item[];
  /** What a PATCH answers: undefined succeeds, a sentence is a 422 with it. */
  refuse?: string;
}

async function mountBoard(served: Served) {
  const patches: Array<{ id: string; body: Record<string, unknown> }> = [];
  vi.stubGlobal('WebSocket', class { close() {} addEventListener() {} });
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const json = (body: unknown, status = 200) =>
      new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json', ETag: '"1"' } });
    const one = /\/api\/items\/([^/?]+)$/.exec(url);
    if (init?.method === 'PATCH' && one) {
      patches.push({ id: one[1], body: JSON.parse(String(init.body)) });
      if (served.refuse) return json({ error: { status: 422, message: served.refuse, code: 'invalid_parent' } }, 422);
      return json(served.items.find((i) => i.id === one[1]));
    }
    if (one) return json({ item: served.items.find((i) => i.id === one[1]), roles: [], dependencies: [] });
    if (url.includes('/items?')) return json({ data: served.items, total: served.items.length, page: 1, per_page: 200 });
    if (url.includes('/parents')) return json([{ id: 'g-billing', title: 'Billing', type: 'epic' }, { id: 'g-checkout', title: 'Checkout', type: 'epic' }]);
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
            <Route path="/projects/:id/board" component={() => <ProjectItemsProvider><Board /></ProjectItemsProvider>} />
          </MemoryRouter>
          <ToastContainer />
        </ProjectContext.Provider>
      </ExecutionStoreProvider>
    ),
    container,
  );
  disposers.push(() => { dispose(); container.remove(); });
  for (let i = 0; i < 4; i++) await flush();
  return { container, patches };
}

const laneIds = (c: Element) => [...c.querySelectorAll('[data-testid="lane"]')].map((l) => l.getAttribute('data-lane'));
const headers = (c: Element) => [...c.querySelectorAll('[data-testid="group-header"]')].map((h) => h.getAttribute('aria-label'));
const cardTitles = (root: Element) => [...root.querySelectorAll('[data-item-id] h4')].map((h) => h.textContent);
const cell = (c: Element, label: string) => c.querySelector<HTMLElement>(`[role="group"][aria-label="${label}"]`)!;

function drop(target: Element, itemId: string) {
  const event = new Event('drop', { bubbles: true, cancelable: true });
  Object.defineProperty(event, 'dataTransfer', { value: { getData: () => itemId, dropEffect: 'move' } });
  target.dispatchEvent(event);
}

describe('Board lanes', () => {
  it('splits_the_work_into_a_lane_per_group_needs_you_first_and_no_parent_last_and_never_draws_a_group_as_a_card', async () => {
    const { container } = await mountBoard({ items: [...GROUPS, ...WORK] });
    expect(laneIds(container)).toEqual(['g-checkout', 'g-billing', 'g-empty', '__no-parent__']);
    expect(headers(container)).toEqual(['Checkout', 'Billing', 'Reports']);
    expect(container.querySelector('[data-lane="__no-parent__"] h2')!.textContent).toContain('No parent');
    // Cards are work: the groups are only lane headers.
    expect(cardTitles(container).sort()).toEqual(['Card form', 'Invoice', 'Loose task', 'Payments page']);
    expect(cardTitles(cell(container, 'To Do, Checkout'))).toEqual(['Card form']);
    expect(cardTitles(cell(container, 'Doing, Checkout'))).toEqual(['Payments page']);
    // A card in a lane does not repeat the path its lane already says.
    expect(container.querySelector('[data-item-id] [data-testid="item-path"]')).toBeNull();
  });

  it('counts_only_work_in_the_toolbar_and_the_column_heads', async () => {
    const { container } = await mountBoard({ items: [...GROUPS, ...WORK] });
    expect(container.textContent).toContain('4 items');
    const heads = [...container.querySelectorAll('[data-testid="lane-columns"] h3')].map((h) => h.parentElement!.textContent);
    expect(heads).toEqual(['To Do2Add item', 'Doing1Add item', 'Done1Add item']);
  });

  it('starts_a_lane_folded_when_its_group_is_in_a_done_status_and_remembers_what_the_person_chose', async () => {
    const { container } = await mountBoard({ items: [...GROUPS, ...WORK] });
    const toggle = (name: string) => container.querySelector<HTMLButtonElement>(`[aria-label="Work under ${name}"]`)!;
    expect(toggle('Checkout').getAttribute('aria-expanded')).toBe('true');
    expect(toggle('Billing').getAttribute('aria-expanded')).toBe('false');
    toggle('Billing').click();
    await flush();
    expect(toggle('Billing').getAttribute('aria-expanded')).toBe('true');
  });

  it('shows_an_empty_group_as_one_row_with_add_item_and_plan_with_agent', async () => {
    const { container } = await mountBoard({ items: [...GROUPS, ...WORK] });
    const empty = container.querySelector('[data-group-id="g-empty"]')!;
    expect(empty.querySelector('[data-testid="lane-empty"]')!.textContent).toContain('Nothing under Reports yet');
    expect(empty.querySelectorAll('[role="group"]').length).toBe(0); // no columns, just the row
    expect(empty.querySelector('[aria-label="Add item to Reports"]')).not.toBeNull();
    expect([...empty.querySelectorAll('button')].some((b) => b.textContent === 'Plan with agent')).toBe(true);
  });

  it('a_project_without_a_group_has_no_lanes_no_header_and_no_choice_of_lanes', async () => {
    const { container } = await mountBoard({ items: WORK.map((w) => ({ ...w, parent_id: null })) });
    expect(container.querySelector('[data-testid="group-header"]')).toBeNull();
    expect(container.querySelector('[data-testid="lane"]')).toBeNull();
    expect(container.querySelector('[data-testid="board-lanes"]')).toBeNull();
    expect(container.querySelector('[aria-label="Board columns"]')).not.toBeNull();
    expect(container.querySelectorAll('[role="region"]').length).toBe(3);
  });

  it('remembers_the_choice_of_lanes_per_project_and_none_puts_the_path_on_each_card', async () => {
    const { container } = await mountBoard({ items: [...GROUPS, ...WORK] });
    const select = container.querySelector<HTMLSelectElement>('[data-testid="board-lanes"]')!;
    expect([...select.options].map((o) => o.textContent)).toEqual(['By parent', 'None', 'Role (unavailable)', 'Priority', 'Assignee']);
    expect([...select.options].find((o) => o.value === 'role')!.disabled).toBe(true);
    expect(select.value).toBe('parent');
    select.value = 'none';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    expect(localStorage.getItem('tack_board_lanes:p1')).toBe('none');
    expect(container.querySelector('[data-testid="group-header"]')).toBeNull();
    expect(cardTitles(container).sort()).toEqual(['Card form', 'Invoice', 'Loose task', 'Payments page']);
    const form = container.querySelector('[data-item-id="w-form"]')!;
    expect(form.querySelector('[data-testid="item-path"]')!.textContent).toBe('Checkout');
    expect(container.querySelector('[data-item-id="w-loose"] [data-testid="item-path"]')).toBeNull();

    // A later visit finds the choice made.
    while (disposers.length) disposers.pop()!();
    const again = await mountBoard({ items: [...GROUPS, ...WORK] });
    expect(again.container.querySelector<HTMLSelectElement>('[data-testid="board-lanes"]')!.value).toBe('none');
    expect(again.container.querySelector('[data-testid="group-header"]')).toBeNull();
  });

  it('splits_by_priority_or_assignee_when_asked', async () => {
    const { container } = await mountBoard({ items: [...GROUPS, ...WORK] });
    const select = container.querySelector<HTMLSelectElement>('[data-testid="board-lanes"]')!;
    select.value = 'priority';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    expect([...container.querySelectorAll('[data-testid="lane"] h2')].map((h) => h.firstChild!.textContent)).toEqual(['High', 'Medium']);
    select.value = 'assignee';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    expect([...container.querySelectorAll('[data-testid="lane"] h2')].map((h) => h.firstChild!.textContent)).toEqual(['ana', 'Unassigned']);
  });

  it('a_drag_into_another_lane_sends_the_new_parent_and_keeps_the_status', async () => {
    const { container, patches } = await mountBoard({ items: [...GROUPS, ...WORK] });
    drop(cell(container, 'To Do, Billing'), 'w-form');
    await flush();
    await flush();
    expect(patches).toEqual([{ id: 'w-form', body: { parent_id: 'g-billing' } }]);
    expect(document.querySelector('[aria-live="polite"]')?.textContent).toContain('Item moved under Billing');
  });

  it('a_drag_into_no_parent_sends_a_null_parent_and_a_drag_across_a_column_sends_the_status_too', async () => {
    const { container, patches } = await mountBoard({ items: [...GROUPS, ...WORK] });
    drop(cell(container, 'To Do, No parent'), 'w-form');
    await flush();
    await flush();
    drop(cell(container, 'Doing, Billing'), 'w-loose');
    await flush();
    await flush();
    expect(patches).toEqual([
      { id: 'w-form', body: { parent_id: null } },
      { id: 'w-loose', body: { status: 'Doing', parent_id: 'g-billing' } },
    ]);
  });

  it('a_refused_move_shows_the_servers_sentence_on_the_lane_and_the_card_returns', async () => {
    const sentence = 'Five levels is the most.';
    const { container, patches } = await mountBoard({ items: [...GROUPS, ...WORK], refuse: sentence });
    drop(cell(container, 'To Do, Billing'), 'w-form');
    await flush();
    await flush();
    await flush();
    expect(patches).toHaveLength(1);
    expect(container.querySelector('[data-group-id="g-billing"] [role="alert"]')!.textContent).toBe(sentence);
    expect(cardTitles(cell(container, 'To Do, Checkout'))).toEqual(['Card form']);
    expect(cardTitles(cell(container, 'To Do, Billing'))).toEqual([]);
  });

  it('move_under_in_the_card_menu_opens_the_picker_and_sends_the_chosen_parent', async () => {
    const { container, patches } = await mountBoard({ items: [...GROUPS, ...WORK] });
    const button = container.querySelector<HTMLButtonElement>('[data-item-id="w-loose"] [aria-label="Move to…"]')!;
    button.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }));
    await flush();
    const entries = [...document.querySelectorAll('[role="menuitem"]')];
    expect(entries.map((e) => e.textContent)).toEqual(['Doing', 'Done', 'Move under…']);
    (entries[2] as HTMLElement).click();
    await flush();
    await flush();
    const picker = document.querySelector('[data-testid="parent-picker"]')!;
    expect([...picker.querySelectorAll('[role="option"]')].map((o) => o.textContent!.replace(/\s+/g, ' ').trim())).toEqual(['No parentCurrent', 'EpicBilling', 'EpicCheckout']);
    (picker.querySelectorAll('[role="option"]')[1] as HTMLElement).click();
    await flush();
    await flush();
    expect(patches).toEqual([{ id: 'w-loose', body: { parent_id: 'g-billing' } }]);
    expect(document.querySelector('[data-testid="parent-picker"]')).toBeNull();
  });

  it('a_lane_can_be_moved_under_another_group_from_its_header', async () => {
    const { container, patches } = await mountBoard({ items: [...GROUPS, ...WORK] });
    container.querySelector<HTMLButtonElement>('[aria-label="Move under…, Reports"]')!.click();
    await flush();
    await flush();
    expect(document.querySelector('[role="dialog"]')!.textContent).toContain('Move under…: Reports');
    (document.querySelector('[data-testid="parent-picker"] [role="option"]:nth-child(3)') as HTMLElement).click();
    await flush();
    await flush();
    expect(patches).toEqual([{ id: 'g-empty', body: { parent_id: 'g-checkout' } }]);
  });
});
