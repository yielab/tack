import { describe, it, expect, afterEach, beforeEach, vi } from 'vitest';
import List from './List';
import { flush } from '../../test/flush';
import { makeItem, mountProjectView, unmountProjectViews } from '../../test/mountProjectView';

// The List moves a row by what `solid-dnd` reports when one is dropped on another. The library works
// from pointer geometry, which a test cannot produce, so it is replaced by a stand-in that hands the
// drop handler to the test.
const drag = vi.hoisted(() => ({ onDragEnd: undefined as undefined | ((e: unknown) => void) }));
vi.mock('@thisbeyond/solid-dnd', () => ({
  DragDropProvider: (props: { onDragEnd: (e: unknown) => void; children: unknown }) => {
    drag.onDragEnd = props.onDragEnd;
    return props.children;
  },
  DragDropSensors: () => null,
  SortableProvider: (props: { children: unknown }) => props.children,
  createSortable: () => Object.assign(() => undefined, { ref: () => undefined, dragActivators: {}, isActiveDraggable: false, isActiveDroppable: false }),
  closestCenter: () => null,
}));

afterEach(() => {
  unmountProjectViews();
  drag.onDragEnd = undefined;
});
beforeEach(() => localStorage.clear());

const PROGRESS = { total: 2, done: 1, needs_you: 1, open_runs: 0, teams: [] };
const ITEMS = [
  makeItem({ id: 'epic', title: 'Checkout', item_type: 'epic', node: 'group', progress: PROGRESS, created_at: '2026-01-01T00:00:00Z' }),
  makeItem({ id: 'form', title: 'Payment form', parent_id: 'epic', created_at: '2026-01-02T00:00:00Z' }),
  makeItem({ id: 'copy', title: 'Write the copy', parent_id: 'epic', status: 'Done', created_at: '2026-01-03T00:00:00Z' }),
  makeItem({ id: 'epic2', title: 'Billing', item_type: 'epic', node: 'group', progress: { ...PROGRESS, total: 0, done: 0, needs_you: 0 }, created_at: '2026-01-04T00:00:00Z' }),
  makeItem({ id: 'loose', title: 'Fix typo', created_at: '2026-01-05T00:00:00Z' }),
];

const rowIds = (c: HTMLElement) => [...c.querySelectorAll<HTMLElement>('[data-testid="list-row"]')].map((r) => r.dataset.itemId);
const row = (c: HTMLElement, id: string) => c.querySelector<HTMLElement>(`[data-testid="list-row"][data-item-id="${id}"]`)!;
const writes = () =>
  vi.mocked(fetch).mock.calls
    .filter(([, init]) => (init as RequestInit | undefined)?.method === 'PATCH')
    .map(([url, init]) => ({ url: String(url), body: JSON.parse(String((init as RequestInit).body)) }));

/** Makes the server turn every PATCH down with a sentence, and leaves every other answer as it was. */
function refuseMoves(sentence: string) {
  const answer = vi.mocked(fetch).getMockImplementation()!;
  vi.mocked(fetch).mockImplementation(async (input, init) =>
    init?.method === 'PATCH'
      ? new Response(JSON.stringify({ error: { status: 422, message: sentence, code: 'group_under_work' } }), { status: 422 })
      : answer(input, init),
  );
}

describe('List: groups and work', () => {
  it('a_group_row_is_drawn_with_its_progress_and_a_work_row_with_its_run', async () => {
    const c = await mountProjectView(() => <List />, 'list', {
      items: ITEMS,
      executions: [{ request_id: 'r1', item_id: 'loose', state: 'running' }],
    });
    const group = row(c, 'epic');
    const work = row(c, 'loose');
    expect(group.dataset.holdsWork).toBe('true');
    expect(work.dataset.holdsWork).toBe('false');
    expect(group.querySelector('[data-testid="progress-text"]')?.textContent).toBe('1 of 2 done');
    expect(group.querySelector('[data-testid="progress-needs"]')?.textContent).toBe('1 needs you');
    expect(group.querySelector('[data-testid="run-state"]')).toBeNull();
    expect(work.querySelector('[data-testid="progress"]')).toBeNull();
    expect(work.textContent).toContain('Running');
    // A group with nothing under it has no progress to draw, and says it has no run.
    expect(row(c, 'epic2').querySelector('[data-testid="progress"]')).toBeNull();
  });

  it('show_is_offered_when_there_are_groups_and_narrows_the_rows', async () => {
    const c = await mountProjectView(() => <List />, 'list', { items: ITEMS });
    const choice = (name: string) => [...c.querySelectorAll<HTMLButtonElement>('[role="group"][aria-label="Show"] button')].find((b) => b.textContent === name)!;
    expect([...c.querySelectorAll('[role="group"][aria-label="Show"] button')].map((b) => b.textContent)).toEqual(['Everything', 'Work', 'Groups']);
    expect(rowIds(c)).toEqual(['epic', 'epic2', 'loose']);

    choice('Work').click();
    await flush();
    expect(rowIds(c)).toEqual(['form', 'copy', 'loose']);
    expect(choice('Work').getAttribute('aria-pressed')).toBe('true');

    choice('Groups').click();
    await flush();
    expect(rowIds(c)).toEqual(['epic', 'epic2']);
  });

  it('a_project_with_no_group_has_no_show_choice', async () => {
    const c = await mountProjectView(() => <List />, 'list', { items: [makeItem({ id: 'a' }), makeItem({ id: 'b' })] });
    expect(c.querySelector('[role="group"][aria-label="Show"]')).toBeNull();
  });

  it('dropping_a_row_on_another_moves_it_under_that_row', async () => {
    const c = await mountProjectView(() => <List />, 'list', { items: ITEMS });
    drag.onDragEnd!({ draggable: { id: 'loose' }, droppable: { id: 'epic2' } });
    await flush();
    await flush();
    expect(writes().map((w) => w.body)).toEqual([{ parent_id: 'epic2' }]);
    expect(c.querySelector('[data-testid="list-refusal"]')).toBeNull();
  });

  it('dropping_a_row_on_the_row_it_is_already_under_sends_nothing', async () => {
    await mountProjectView(() => <List />, 'list', { items: ITEMS });
    drag.onDragEnd!({ draggable: { id: 'form' }, droppable: { id: 'epic' } });
    await flush();
    expect(writes()).toEqual([]);
  });

  it('a_refused_drop_shows_the_servers_sentence', async () => {
    const c = await mountProjectView(() => <List />, 'list', { items: ITEMS });
    refuseMoves('An epic cannot go under a task.');
    drag.onDragEnd!({ draggable: { id: 'epic2' }, droppable: { id: 'loose' } });
    await flush();
    await flush();
    expect(c.querySelector('[data-testid="list-refusal"]')?.textContent).toContain('An epic cannot go under a task.');
    expect(c.querySelector('[data-testid="list-refusal"]')!.getAttribute('role')).toBe('alert');
  });

  it('move_under_in_the_row_menu_offers_the_servers_parents_and_sends_the_one_chosen', async () => {
    const c = await mountProjectView(() => <List />, 'list', { items: ITEMS });
    const base = vi.mocked(fetch).getMockImplementation()!;
    vi.mocked(fetch).mockImplementation(async (input, init) =>
      String(input).includes('/parents?')
        ? new Response(JSON.stringify([{ id: 'epic2', title: 'Billing', type: 'epic' }]), { status: 200 })
        : base(input, init),
    );
    row(c, 'loose').querySelector<HTMLButtonElement>('button[aria-label="More actions for Fix typo"]')!.click();
    await flush();
    [...c.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')].find((b) => b.textContent === 'Move under…')!.click();
    await flush();
    await flush();
    expect(vi.mocked(fetch).mock.calls.some(([url]) => String(url).includes('/projects/p1/parents?item=loose'))).toBe(true);
    [...document.querySelectorAll<HTMLElement>('[role="option"]')].find((o) => o.textContent?.includes('Billing'))!.click();
    await flush();
    await flush();
    expect(writes().map((w) => w.body)).toEqual([{ parent_id: 'epic2' }]);
  });

  it('plus_on_a_row_opens_the_form_for_a_child_and_opens_the_row_to_show_it', async () => {
    const c = await mountProjectView(() => <List />, 'list', { items: ITEMS });
    expect(rowIds(c)).not.toContain('form');
    row(c, 'epic').querySelector<HTMLButtonElement>('button[aria-label="Add item to Checkout"]')!.click();
    await flush();
    expect(rowIds(c)).toContain('form');
    expect(c.querySelector('input[aria-label="Item title"]')).not.toBeNull();
  });
});
