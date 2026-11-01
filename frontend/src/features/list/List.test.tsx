import { describe, it, expect, afterEach, beforeEach, vi } from 'vitest';
import List from './List';
import { flush } from '../../test/flush';
import { makeItem, mountProjectView, unmountProjectViews } from '../../test/mountProjectView';

afterEach(unmountProjectViews);

describe('List', () => {
  it('shows_each_root_item_with_the_count_and_keeps_a_child_under_its_collapsed_parent', async () => {
    const container = await mountProjectView(() => <List />, 'list', {
      items: [
        makeItem({ id: 'epic', title: 'Billing epic', item_type: 'epic', created_at: '2026-01-01T00:00:00Z' }),
        makeItem({ id: 'child', title: 'Invoice PDF', parent_id: 'epic', created_at: '2026-01-02T00:00:00Z' }),
        makeItem({ id: 'solo', title: 'Fix typo', created_at: '2026-01-03T00:00:00Z' }),
      ],
    });

    expect(container.querySelector('h1')?.textContent).toBe('Proj');
    expect(container.textContent).toContain('2 items'); // the child is hidden until its parent opens
    expect(container.textContent).toContain('Billing epic');
    expect(container.textContent).toContain('Fix typo');
    expect(container.textContent).not.toContain('Invoice PDF');
    expect(container.textContent).not.toContain('No items yet');

    [...container.querySelectorAll<HTMLButtonElement>('button')]
      .find((b) => b.title === 'Expand all')!
      .click();
    await flush();
    expect(container.textContent).toContain('3 items');
    expect(container.textContent).toContain('Invoice PDF');
  });

  it('says_so_when_the_project_has_no_items', async () => {
    const container = await mountProjectView(() => <List />, 'list', { items: [] });
    expect(container.textContent).toContain('0 items');
    expect(container.textContent).toContain('No items yet');
    expect(container.textContent).toContain('Create your first item');
  });
});

// ── Sorting, filtering, editing and the remembered choices ───────────────────

const rowIds = (c: HTMLElement) => [...c.querySelectorAll<HTMLElement>('[data-testid="list-row"]')].map((r) => r.dataset.itemId);
const header = (c: HTMLElement, name: string) =>
  [...c.querySelectorAll<HTMLElement>('[role="columnheader"]')].find((h) => h.textContent?.trim().startsWith(name))!;
const pressHeader = async (c: HTMLElement, name: string) => {
  header(c, name).querySelector('button')!.click();
  await flush();
};
const expandAll = async (c: HTMLElement) => {
  [...c.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.title === 'Expand all')!.click();
  await flush();
};
const typeInto = async (el: HTMLInputElement, value: string) => {
  el.value = value;
  el.dispatchEvent(new Event('input', { bubbles: true }));
  await flush();
};
const choose = async (el: HTMLSelectElement, value: string) => {
  el.value = value;
  el.dispatchEvent(new Event('change', { bubbles: true }));
  await flush();
};
const patches = () =>
  vi.mocked(fetch).mock.calls
    .filter(([, init]) => (init as RequestInit | undefined)?.method === 'PATCH')
    .map(([url, init]) => ({ url: String(url), body: JSON.parse(String((init as RequestInit).body)) }));

const FAMILY = [
  makeItem({ id: 'epic', title: 'Epic', item_type: 'epic', priority: 'low', status: 'To Do', created_at: '2026-01-01T00:00:00Z' }),
  makeItem({ id: 'design', title: 'Zeta design', parent_id: 'epic', priority: 'critical', assignee: 'Kim', created_at: '2026-01-02T00:00:00Z' }),
  makeItem({ id: 'build', title: 'Alpha build', parent_id: 'epic', priority: 'none', status: 'Done', created_at: '2026-01-03T00:00:00Z' }),
  makeItem({ id: 'loose', title: 'Loose end', priority: 'critical', assignee: 'Sam', due_date: '2026-03-01T00:00:00Z', created_at: '2026-01-04T00:00:00Z' }),
];

describe('List columns', () => {
  beforeEach(() => localStorage.clear());

  it('has_a_header_row_whose_columns_are_buttons_that_say_which_way_they_sort', async () => {
    const c = await mountProjectView(() => <List />, 'list', { items: FAMILY });
    const sortable = ['Title', 'Status', 'Priority', 'Assignee', 'Due', 'Run'];
    for (const name of sortable) {
      expect(header(c, name).querySelector('button'), name).not.toBeNull();
      expect(header(c, name).getAttribute('aria-sort')).toBe('none');
    }
    expect(header(c, 'Details').querySelector('button')).toBeNull();

    await pressHeader(c, 'Priority');
    expect(header(c, 'Priority').getAttribute('aria-sort')).toBe('ascending');
    expect(header(c, 'Title').getAttribute('aria-sort')).toBe('none');
    await pressHeader(c, 'Priority');
    expect(header(c, 'Priority').getAttribute('aria-sort')).toBe('descending');
    await pressHeader(c, 'Priority');
    expect(header(c, 'Priority').getAttribute('aria-sort')).toBe('none');
  });

  it('sorts_each_column_inside_each_parent_and_goes_back_to_creation_order', async () => {
    const c = await mountProjectView(() => <List />, 'list', { items: FAMILY });
    await expandAll(c);
    expect(rowIds(c)).toEqual(['epic', 'design', 'build', 'loose']);

    await pressHeader(c, 'Title');
    expect(rowIds(c)).toEqual(['epic', 'build', 'design', 'loose']); // Alpha before Zeta under Epic; Epic before Loose
    await pressHeader(c, 'Title');
    expect(rowIds(c)).toEqual(['loose', 'epic', 'design', 'build']);

    await pressHeader(c, 'Priority'); // Critical → None
    expect(rowIds(c)).toEqual(['loose', 'epic', 'design', 'build']);
    await pressHeader(c, 'Priority');
    expect(rowIds(c)).toEqual(['epic', 'build', 'design', 'loose']);

    await pressHeader(c, 'Status');
    expect(rowIds(c)).toEqual(['epic', 'build', 'design', 'loose']); // Done, To Do under Epic; Epic and Loose are both To Do, oldest first
    await pressHeader(c, 'Assignee');
    expect(rowIds(c)).toEqual(['epic', 'build', 'design', 'loose']); // none before Kim; none before Sam
    await pressHeader(c, 'Due');
    expect(rowIds(c)).toEqual(['loose', 'epic', 'design', 'build']); // the dated one first; undated siblings keep their order

    // Pressing the same header twice more returns to the order the items were created in.
    await pressHeader(c, 'Due');
    await pressHeader(c, 'Due');
    expect(rowIds(c)).toEqual(['epic', 'design', 'build', 'loose']);
  });

  it('sorts_by_run_with_what_needs_a_person_first', async () => {
    const items = [
      makeItem({ id: 'a', title: 'Quiet', created_at: '2026-01-01T00:00:00Z' }),
      makeItem({ id: 'b', title: 'Going', created_at: '2026-01-02T00:00:00Z' }),
      makeItem({ id: 'c', title: 'Asking', created_at: '2026-01-03T00:00:00Z' }),
    ];
    const c = await mountProjectView(() => <List />, 'list', {
      items,
      executions: [
        { request_id: 'r1', item_id: 'b', state: 'running' },
        { request_id: 'r2', item_id: 'c', state: 'waiting_decision' },
      ],
    });
    await pressHeader(c, 'Run');
    expect(rowIds(c)).toEqual(['c', 'b', 'a']);
    expect(c.querySelector('[data-item-id="c"]')!.textContent).toContain('Needs you');
    expect(c.querySelector('[data-item-id="b"]')!.textContent).toContain('Running');
  });

  it('hides_a_column_from_Columns_and_remembers_it_but_never_lets_the_title_go', async () => {
    const c = await mountProjectView(() => <List />, 'list', { items: FAMILY });
    expect(header(c, 'Type')).toBeUndefined(); // hidden until shown
    const columns = [...c.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.textContent?.startsWith('Columns'))!;
    expect(columns.getAttribute('aria-expanded')).toBe('false');
    columns.click();
    await flush();

    const box = (name: string) =>
      [...c.querySelectorAll<HTMLLabelElement>('[aria-label="Columns"] label')].find((l) => l.textContent?.startsWith(name))!
        .querySelector('input')!;
    expect(box('Title').disabled).toBe(true);
    expect(c.querySelector('[aria-label="Columns"]')!.textContent).toContain('Rows are named and nested by their title');

    box('Type').click();
    box('Due').click();
    await flush();
    expect(header(c, 'Type')).toBeDefined();
    expect(header(c, 'Due')).toBeUndefined();
    expect(JSON.parse(localStorage.getItem('tack_list_cols')!)).toEqual(['due_date']);
  });

  it('starts_from_the_columns_this_browser_remembers_and_moves_the_Tables_choice_over', async () => {
    localStorage.setItem('tack_table_cols', JSON.stringify(['assignee', 'needs_review']));
    localStorage.setItem('tack_table_density', 'compact');
    const c = await mountProjectView(() => <List />, 'list', { items: FAMILY });
    expect(header(c, 'Assignee')).toBeUndefined();
    expect(header(c, 'Run')).toBeUndefined();
    expect(header(c, 'Details')).toBeUndefined();
    expect(header(c, 'Type')).toBeDefined();
    expect(localStorage.getItem('tack_table_cols')).toBeNull();
    expect(localStorage.getItem('tack_table_density')).toBeNull();
    const density = [...c.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.title === 'Toggle row density')!;
    expect(density.textContent).toBe('Comfortable'); // compact now; the button offers the other one
  });

  it('switches_between_compact_and_comfortable_and_remembers_it', async () => {
    const c = await mountProjectView(() => <List />, 'list', { items: FAMILY });
    const density = () => [...c.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.title === 'Toggle row density')!;
    const cellBlock = () => c.querySelector<HTMLElement>('[data-item-id="epic"] > div')!.style.paddingBlock;
    expect(density().textContent).toBe('Compact');
    const comfortable = cellBlock();
    density().click();
    await flush();
    expect(density().textContent).toBe('Comfortable');
    expect(density().getAttribute('aria-pressed')).toBe('true');
    expect(cellBlock()).not.toBe(comfortable);
    expect(localStorage.getItem('tack_list_density')).toBe('compact');
  });
});

describe('List filter', () => {
  beforeEach(() => localStorage.clear());

  const filterBox = (c: HTMLElement) => c.querySelector<HTMLInputElement>('input[aria-label="Filter items"]')!;
  const runBox = (c: HTMLElement) => c.querySelector<HTMLSelectElement>('select[aria-label="Filter by run state"]')!;

  it('matches_title_assignee_and_status_and_keeps_the_parents_of_what_matches', async () => {
    const c = await mountProjectView(() => <List />, 'list', { items: FAMILY });
    await typeInto(filterBox(c), 'zeta');
    expect(rowIds(c)).toEqual(['epic', 'design']); // a match is never shown without its parent
    expect(c.textContent).toContain('1 of 4 items match');

    await typeInto(filterBox(c), 'sam');
    expect(rowIds(c)).toEqual(['loose']);
    await typeInto(filterBox(c), 'done');
    expect(rowIds(c)).toEqual(['epic', 'build']);
    await typeInto(filterBox(c), 'zzz');
    expect(rowIds(c)).toEqual([]);
    expect(c.textContent).toContain('No items match your filter');

    [...c.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.textContent === 'Clear filter')!.click();
    await flush();
    expect(filterBox(c).value).toBe('');
    expect(rowIds(c)).toEqual(['epic', 'loose']);
  });

  it('filters_by_run_state_alone_and_together_with_the_text', async () => {
    const items = [
      makeItem({ id: 'a', title: 'Alpha', created_at: '2026-01-01T00:00:00Z' }),
      makeItem({ id: 'b', title: 'Beta', created_at: '2026-01-02T00:00:00Z' }),
      makeItem({ id: 'c', title: 'Gamma', created_at: '2026-01-03T00:00:00Z' }),
      makeItem({ id: 'd', title: 'Delta', created_at: '2026-01-04T00:00:00Z' }),
    ];
    const c = await mountProjectView(() => <List />, 'list', {
      items,
      executions: [
        { request_id: 'r1', item_id: 'a', state: 'failed' },
        { request_id: 'r2', item_id: 'b', state: 'running' },
        { request_id: 'r3', item_id: 'c', state: 'waiting_decision' },
      ],
    });
    expect([...runBox(c).options].map((o) => o.textContent)).toEqual([
      'Any run state', 'Waiting for you', 'Queued', 'Running', 'Needs you', 'Ready for review', 'Failed',
    ]);

    await choose(runBox(c), 'failed');
    expect(rowIds(c)).toEqual(['a']);
    await choose(runBox(c), 'running');
    expect(rowIds(c)).toEqual(['b']);
    await choose(runBox(c), 'waiting');
    expect(rowIds(c)).toEqual(['c']);

    await choose(runBox(c), 'failed');
    await typeInto(filterBox(c), 'beta');
    expect(rowIds(c)).toEqual([]); // Beta is running, not failed
    await choose(runBox(c), '');
    expect(rowIds(c)).toEqual(['b']);
  });
});

describe('List edits in place', () => {
  beforeEach(() => localStorage.clear());

  const cell = (c: HTMLElement, id: string, key: string) => c.querySelector<HTMLButtonElement>(`[data-cell="${id}:${key}"]`)!;
  const editor = (c: HTMLElement, name: string) => c.querySelector<HTMLInputElement & HTMLSelectElement>(`[aria-label="${name}"]`)!;

  const mountEditable = () =>
    mountProjectView(() => <List />, 'list', {
      items: [makeItem({ id: 'one', title: 'Fix login', assignee: 'Kim', due_date: '2026-03-01T00:00:00Z' })],
    });

  it('names_each_editable_cell_by_its_column_row_and_value', async () => {
    const c = await mountEditable();
    expect(cell(c, 'one', 'status').getAttribute('aria-label')).toBe('Status of Fix login: To Do. Edit');
    expect(cell(c, 'one', 'priority').getAttribute('aria-label')).toBe('Priority of Fix login: Medium. Edit');
    expect(cell(c, 'one', 'assignee').getAttribute('aria-label')).toBe('Assignee of Fix login: Kim. Edit');
    expect(cell(c, 'one', 'due_date').getAttribute('aria-label')).toBe('Due of Fix login: 2026-03-01. Edit');
    expect(cell(c, 'one', 'title').getAttribute('aria-label')).toBe('Edit title of Fix login');
  });

  it('edits_the_title_from_a_button_of_its_own_and_returns_the_keyboard_to_it', async () => {
    const c = await mountEditable();
    cell(c, 'one', 'title').click();
    await flush();
    const input = editor(c, 'Title of Fix login');
    expect(document.activeElement).toBe(input);
    input.value = 'Fix logout';
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await flush();
    expect(patches()).toEqual([{ url: expect.stringContaining('/items/one'), body: { title: 'Fix logout' } }]);
    expect(c.querySelector('[aria-label="Title of Fix login"]')).toBeNull(); // the editor is gone
    expect(document.activeElement).toBe(cell(c, 'one', 'title')); // and the keyboard is back on the cell
  });

  it('edits_the_status', async () => {
    const c = await mountEditable();
    cell(c, 'one', 'status').click();
    await flush();
    const select = editor(c, 'Status of Fix login');
    expect([...select.options].map((o) => o.value)).toEqual(['To Do', 'Done']);
    await choose(select, 'Done');
    expect(patches().map((p) => p.body)).toEqual([{ status: 'Done' }]);
  });

  it('edits_the_priority_with_none_among_the_choices', async () => {
    const c = await mountEditable();
    cell(c, 'one', 'priority').click();
    await flush();
    const select = editor(c, 'Priority of Fix login');
    expect([...select.options].map((o) => o.value)).toEqual(['critical', 'high', 'medium', 'low', 'none']);
    await choose(select, 'critical');
    expect(patches().map((p) => p.body)).toEqual([{ priority: 'critical' }]);
  });

  it('edits_the_assignee_and_clears_it_when_emptied', async () => {
    const c = await mountEditable();
    cell(c, 'one', 'assignee').click();
    await flush();
    const input = editor(c, 'Assignee of Fix login');
    input.value = '';
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    await flush();
    expect(patches().map((p) => p.body)).toEqual([{ assignee: null }]);
  });

  it('edits_the_due_date_when_the_field_loses_focus', async () => {
    const c = await mountEditable();
    cell(c, 'one', 'due_date').click();
    await flush();
    const input = editor(c, 'Due of Fix login');
    input.value = '2026-04-02';
    input.dispatchEvent(new FocusEvent('blur'));
    await flush();
    expect(patches().map((p) => p.body)).toEqual([{ due_date: '2026-04-02T00:00:00Z' }]);
  });

  it('saves_nothing_when_Escape_leaves_the_cell_as_it_was_or_the_value_is_unchanged', async () => {
    const c = await mountEditable();
    cell(c, 'one', 'assignee').click();
    await flush();
    const input = editor(c, 'Assignee of Fix login');
    input.value = 'Someone else';
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await flush();
    expect(c.querySelector('[aria-label="Assignee of Fix login"]')).toBeNull();

    cell(c, 'one', 'status').click();
    await flush();
    await choose(editor(c, 'Status of Fix login'), 'To Do'); // the status it already has
    expect(patches()).toEqual([]);
  });
});
