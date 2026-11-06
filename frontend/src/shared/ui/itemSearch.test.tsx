import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render } from 'solid-js/web';
import { createMemoryHistory, MemoryRouter, Route } from '@solidjs/router';
import SearchBar from './SearchBar';
import CommandPalette, { type Command } from './CommandPalette';
import type { Item } from '../types';
import { flush } from '../../test/flush';

const item = (id: string, over: Partial<Item> = {}): Item =>
  ({
    id,
    project_id: 'p1',
    title: id,
    item_type: 'task',
    node: 'work',
    status: 'To Do',
    priority: 'none',
    estimate_unit: 'story_points',
    tags: [],
    sort_order: 0,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...over,
  }) as Item;

// "Pay" sits under "Payments", which sits under "Checkout".
const TREE = [
  item('checkout', { title: 'Checkout', item_type: 'epic', node: 'group' }),
  item('payments', { title: 'Payments', item_type: 'epic', node: 'group', parent_id: 'checkout' }),
  item('pay', { title: 'Pay by card', parent_id: 'payments' }),
  item('top', { title: 'Pay later' }),
];
const FOUND = [TREE[2], TREE[3]];

let fetched: string[] = [];

beforeEach(() => {
  fetched = [];
  vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
    const url = String(input);
    fetched.push(url);
    const body = url.includes('/search') ? FOUND : { data: TREE, total: TREE.length, page: 1, per_page: 200 };
    return Promise.resolve(new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } }));
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

const settle = async () => {
  await new Promise((r) => setTimeout(r, 400));
  await flush();
  await flush();
};

function mount(view: () => unknown) {
  const history = createMemoryHistory();
  history.set({ value: '/projects/p1/board' });
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(
    () => (
      <MemoryRouter history={history}>
        <Route path="/projects/:id/board" component={() => view() as never} />
      </MemoryRouter>
    ),
    container,
  );
  return { container, history };
}

const typeInto = (input: HTMLInputElement, value: string) => {
  input.value = value;
  input.dispatchEvent(new Event('input', { bubbles: true }));
};

describe('Search results', () => {
  it('shows_each_result_with_its_type_tile_and_the_path_it_sits_under', async () => {
    const { container } = mount(() => <SearchBar projectId="p1" />);
    typeInto(container.querySelector('input')!, 'pay');
    await settle();
    const rows = [...document.querySelectorAll('[data-testid="search-result"]')];
    expect(rows).toHaveLength(2);
    expect(rows[0].querySelector('[data-testid="type-tile"]')!.textContent).toBe('Task');
    const path = [...rows[0].querySelectorAll('[data-testid="item-path"] a')];
    expect(path.map((a) => a.textContent)).toEqual(['Checkout', 'Payments']);
    expect(path[1].getAttribute('href')).toBe('/projects/p1/items/payments');
    // An item at the top has no path, and the tree was read once for the project.
    expect(rows[1].querySelector('[data-testid="item-path"]')).toBeNull();
    expect(fetched.filter((u) => u.includes('/items?'))).toHaveLength(1);
  });

  it('opens_the_item_drawer_on_the_board_when_a_result_is_chosen', async () => {
    const { container, history } = mount(() => <SearchBar projectId="p1" />);
    const input = container.querySelector('input')!;
    typeInto(input, 'pay');
    await settle();
    (document.querySelectorAll('[data-testid="search-result"]')[1] as HTMLElement).click();
    await flush();
    expect(history.get()).toBe('/projects/p1/board?item=top');
    expect(document.querySelector('[data-testid="search-result"]')).toBeNull();
  });

  it('closes_the_results_when_a_path_link_is_followed', async () => {
    const { container } = mount(() => <SearchBar projectId="p1" />);
    typeInto(container.querySelector('input')!, 'pay');
    await settle();
    document.querySelector<HTMLAnchorElement>('[data-testid="item-path"] a')!.click();
    await flush();
    expect(document.querySelector('[data-testid="search-result"]')).toBeNull();
  });
});

describe('Command palette results', () => {
  const commands: Command[] = [{ id: 'go-pay', label: 'Pay settings', action: () => {} }];

  it('lists_found_items_after_the_commands_with_their_type_tile_and_path', async () => {
    mount(() => <CommandPalette isOpen onClose={() => {}} commands={commands} />);
    await flush();
    const input = document.querySelector<HTMLInputElement>('input[role="combobox"]')!;
    typeInto(input, 'pay');
    await settle();
    const options = [...document.querySelectorAll('[role="option"]')];
    expect(options.map((o) => o.getAttribute('id'))).toEqual(['cmdk-option-0', 'cmdk-option-1', 'cmdk-option-2']);
    expect(options[0].textContent).toContain('Pay settings');
    const found = options[1];
    expect(found.querySelector('[data-testid="type-tile"]')!.textContent).toBe('Task');
    expect([...found.querySelectorAll('[data-testid="item-path"] a')].map((a) => a.textContent)).toEqual(['Checkout', 'Payments']);
    expect(document.querySelector('#cmdk-results')!.textContent).toContain('Items');
  });

  it('opens_the_chosen_item_and_closes', async () => {
    const onClose = vi.fn();
    const { history } = mount(() => <CommandPalette isOpen onClose={onClose} commands={commands} />);
    await flush();
    typeInto(document.querySelector<HTMLInputElement>('input[role="combobox"]')!, 'pay');
    await settle();
    (document.querySelectorAll('[role="option"]')[2] as HTMLElement).click();
    await flush();
    expect(history.get()).toBe('/projects/p1/board?item=top');
    expect(onClose).toHaveBeenCalled();
  });
});
