import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { Resource } from 'solid-js';
import { MemoryRouter, Route, createMemoryHistory } from '@solidjs/router';
import { ExecutionStoreProvider } from '../../shared/state/executionContext';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import { ProjectItemsProvider } from '../../shared/state/projectItemsContext';
import ToastContainer from '../../shared/ui/ToastContainer';
import Board, { ItemCard } from './Board';
import { cardRun, type CardRun } from '../../shared/runWithAgent/cardRun';
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

type CardProps = { run?: CardRun; notStarted?: boolean; alwaysPriority?: boolean; waiting?: { code: string; text: string } | null };

async function mount(item: Item, onMove: (id: string, status: string) => void = () => {}, extra: CardProps = {}) {
  vi.spyOn(globalThis, 'fetch').mockImplementation(
    (async () => new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 })) as typeof fetch,
  );
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ExecutionStoreProvider>
        <MemoryRouter>
          <Route path="/" component={() => <ItemCard item={item} typeLabel="Task" statuses={STATUSES} onEdit={() => {}} onMove={onMove} {...extra} />} />
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

const run = (state: string, needsReview = false) =>
  cardRun({ request_id: 'req-1', state, cancellation_requested_at: null }, needsReview);

describe('Board ItemCard', () => {
  it('a_waiting_card_says_why_auto_start_has_not_started_it_and_a_free_card_says_nothing', async () => {
    const text = '3 runs are already open, the most this project allows';
    const held = await mount(makeItem(), undefined, { waiting: { code: 'max_open', text } });
    const line = held.querySelector('[data-testid="card-waiting"]')!;
    expect(line.textContent).toBe(`Waiting to start: ${text}`);
    expect(line.getAttribute('title')).toBe(`Waiting to start: ${text}`);
    const free = await mount(makeItem(), undefined, { waiting: null });
    expect(free.querySelector('[data-testid="card-waiting"]')).toBeNull();
    const unknown = await mount(makeItem());
    expect(unknown.querySelector('[data-testid="card-waiting"]')).toBeNull();
  });

  it('a_card_auto_start_holds_or_started_names_its_parent_and_any_other_card_does_not', async () => {
    const text = '3 runs are already open, the most this project allows';
    const held = await mount(makeItem({ parent_id: 'p' }), undefined, { waiting: { code: 'max_open', text }, parentTitle: 'Billing epic' });
    expect(held.querySelector('[data-testid="card-parent"]')!.textContent).toBe('Part of: Billing epic');
    const started = await mount(makeItem({ parent_id: 'p', started_by: 'auto_start' }), undefined, { parentTitle: 'Billing epic' });
    expect(started.querySelector('[data-testid="card-parent"]')).not.toBeNull();
    const plain = await mount(makeItem({ parent_id: 'p' }), undefined, { parentTitle: 'Billing epic' });
    expect(plain.querySelector('[data-testid="card-parent"]')).toBeNull();
  });

  it('a_parent_card_says_how_many_of_its_children_are_done_and_which_teams_have_them', async () => {
    const parent = await mount(makeItem({ progress: { total: 2, done: 1, open_runs: 0, needs_you: 0, estimate_total: null, estimate_done: null, starts: null, ends: null, teams: ['claude-code', 'saas-build'] } }));
    expect(parent.querySelector('[data-testid="card-children"]')!.textContent).toBe('1 of 2 children done · claude-code, saas-build');
    const lone = await mount(makeItem());
    expect(lone.querySelector('[data-testid="card-children"]')).toBeNull();
  });

  it('a_card_shows_ready_for_review_when_its_run_left_something_to_review', async () => {
    const flagged = await mount(makeItem({ needs_review: true }), undefined, { run: cardRun(undefined, true) });
    expect(flagged.querySelector('[data-testid="card-meta"]')?.textContent).toContain('Ready for review');
    const plain = await mount(makeItem({ needs_review: false }), undefined, { run: cardRun(undefined, false) });
    expect(plain.textContent).not.toContain('Ready for review');
  });

  it('a_card_shows_repeats_only_for_a_recurring_task', async () => {
    const repeating = await mount(makeItem({ recurrence: 'weekly' }));
    expect(repeating.textContent).toContain('Repeats');
    const plain = await mount(makeItem());
    expect(plain.textContent).not.toContain('Repeats');
  });

  it('the_title_comes_first_and_the_meta_line_reads_state_team_priority_type', async () => {
    const c = await mount(makeItem({ priority: 'high', team: 'saas-code' }), undefined, { run: run('running') });
    const card = c.querySelector('[data-item-id]')!;
    expect(card.firstElementChild?.nextElementSibling).toBe(c.querySelector('[data-testid="card-meta"]'));
    expect(card.querySelector('h4')?.textContent).toBe('Fix login bug');
    const meta = c.querySelector('[data-testid="card-meta"]')!;
    const order = [...meta.children].map((el) => el.textContent ?? '');
    expect(order[0]).toContain('Running');
    expect(order[1]).toContain('saas-code');
    expect(order[2]).toContain('High');
    expect(order[3]).toContain('Task');
    expect(c.querySelector('[data-testid="badge-team"]')?.getAttribute('title')).toBe('Team: saas-code');
  });

  it('the_state_chip_still_opens_the_execution_tab_under_its_old_name', async () => {
    const c = await mount(makeItem(), undefined, { run: run('running') });
    const chip = c.querySelector('[data-testid="card-meta"] button')!;
    expect(chip.textContent).toContain('open the Execution tab for Fix login bug');
  });

  it('priority_is_hidden_at_the_default_and_shown_otherwise_or_when_asked_for', async () => {
    const dflt = await mount(makeItem({ priority: 'medium' }));
    expect(dflt.querySelector('[data-testid="card-meta"]')?.textContent).not.toContain('Medium');
    const high = await mount(makeItem({ priority: 'high' }));
    expect(high.querySelector('[data-testid="card-meta"]')?.textContent).toContain('High');
    const always = await mount(makeItem({ priority: 'medium' }), undefined, { alwaysPriority: true });
    expect(always.querySelector('[data-testid="card-meta"]')?.textContent).toContain('Medium');
    const none = await mount(makeItem({ priority: 'none' }), undefined, { alwaysPriority: true });
    expect(none.querySelector('[data-testid="card-meta"]')?.textContent).not.toContain('None');
  });

  it('a_card_that_needs_you_or_failed_has_a_rail_in_that_states_ink', async () => {
    for (const state of ['waiting_decision', 'failed']) {
      const c = await mount(makeItem(), undefined, { run: run(state) });
      const rail = c.querySelector<HTMLElement>('[data-testid="card-rail"]');
      expect(rail, state).toBeTruthy();
      expect(rail!.style.background).toContain(state === 'failed' ? '--run-failed-ink' : '--run-needs-ink');
      expect(rail!.getAttribute('aria-hidden')).toBe('true');
    }
    for (const state of ['running', 'succeeded', 'cancelled']) {
      const c = await mount(makeItem(), undefined, { run: run(state) });
      expect(c.querySelector('[data-testid="card-rail"]'), state).toBeNull();
    }
    expect((await mount(makeItem())).querySelector('[data-testid="card-rail"]')).toBeNull();
  });

  it('a_live_run_in_a_column_nothing_has_started_in_marks_the_card_too', async () => {
    const c = await mount(makeItem(), undefined, { run: run('running'), notStarted: true });
    expect(c.querySelector('[data-testid="card-rail"]')?.getAttribute('data-state')).toBe('running');
  });

  it('the_run_button_is_hidden_on_a_live_run_and_offered_otherwise', async () => {
    const trigger = '[aria-label="Run with agent: Fix login bug"]';
    for (const state of ['queued', 'running', 'waiting_decision']) {
      expect((await mount(makeItem(), undefined, { run: run(state) })).querySelector(trigger), state).toBeNull();
    }
    for (const state of ['failed', 'succeeded']) {
      expect((await mount(makeItem(), undefined, { run: run(state) })).querySelector(trigger), state).toBeTruthy();
    }
    expect((await mount(makeItem())).querySelector(`[data-testid="card-run"]${trigger}`)).toBeTruthy();
    expect((await mount(makeItem())).querySelector(trigger)).toBeTruthy();
  });

  it('a_card_ready_for_review_offers_to_review_not_to_run', async () => {
    const c = await mount(makeItem({ needs_review: true }), undefined, { run: cardRun(undefined, true) });
    expect(c.querySelector('[aria-label="Run with agent: Fix login bug"]')).toBeNull();
    const review = c.querySelector<HTMLButtonElement>('[data-testid="card-review"]');
    expect(review?.getAttribute('aria-label')).toBe('Review the run: Fix login bug');
    expect(review?.title).toBe('Review and merge');
  });

  it('the_short_id_is_on_the_cards_hover_and_in_its_menu', async () => {
    const c = await mount(makeItem({ id: 'abcdef12-0000' }));
    const card = c.querySelector('[data-item-id]')!;
    expect(card.getAttribute('title')).toBe('Item ABCDEF');
    expect(card.querySelector('h4')?.parentElement?.textContent).not.toContain('ABCDEF');
    tick(c.querySelector('[aria-label="Move to…"]')!, 'ArrowDown');
    await flush();
    expect(document.querySelector('[data-testid="card-short-id"]')?.textContent).toBe('ABCDEF');
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

const summary = (id: string, itemId: string, state: string) => ({
  request_id: id, item_id: itemId, state, cancellation_requested_at: null, created_at: '2026-01-01T00:00:00Z',
});

async function mountBoard(items: Item[], executions: ReturnType<typeof summary>[]) {
  vi.stubGlobal('WebSocket', class { close() {} addEventListener() {} });
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
    const url = String(input);
    const json = (body: unknown) =>
      new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json', ETag: '"1"' } });
    if (url.includes('/items?')) return json({ data: items, total: items.length, page: 1, per_page: 200 });
    if (url.includes('/executions')) return json({ protocol_version: 1, data: executions });
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
        </ProjectContext.Provider>
      </ExecutionStoreProvider>
    ),
    container,
  );
  disposers.push(() => { dispose(); container.remove(); });
  for (let i = 0; i < 4; i++) await flush();
  return container;
}

const titles = (c: Element) => [...c.querySelectorAll('[data-item-id] h4')].map((h) => h.textContent);

describe('Board column order', () => {
  it('a_column_lists_what_needs_a_person_first_then_what_is_going_then_the_rest_keeping_its_own_order_among_equals', async () => {
    const items = [
      makeItem({ id: 'a', title: 'Alpha', status: 'To Do' }),
      makeItem({ id: 'b', title: 'Bravo', status: 'To Do' }),
      makeItem({ id: 'c', title: 'Charlie', status: 'To Do' }),
      makeItem({ id: 'd', title: 'Delta', status: 'To Do' }),
      makeItem({ id: 'e', title: 'Echo', status: 'To Do', needs_review: true }),
    ];
    const c = await mountBoard(items, [
      summary('r1', 'b', 'running'),
      summary('r2', 'c', 'waiting_decision'),
    ]);
    expect(titles(c)).toEqual(['Charlie', 'Echo', 'Bravo', 'Alpha', 'Delta']);
  });
});

describe('Board runs the server describes', () => {
  const ITEMS = [
    makeItem({ id: 'a', title: 'Alpha', status: 'To Do' }),
    makeItem({ id: 'b', title: 'Bravo', status: 'To Do' }),
    makeItem({ id: 'c', title: 'Charlie', status: 'To Do', needs_review: true }),
    makeItem({ id: 'd', title: 'Delta', status: 'To Do', needs_review: true }),
  ];
  const RUNS = [
    Object.assign(summary('r1', 'a', 'failed'), { never_started: true }),
    summary('r2', 'b', 'failed'),
    Object.assign(summary('r3', 'c', 'succeeded'), { verifier_recommendation: 'reject' }),
    Object.assign(summary('r4', 'd', 'succeeded'), { verifier_recommendation: 'merge' }),
  ];
  const chip = (c: HTMLElement, id: string) => c.querySelector(`[data-item-id="${id}"] [data-testid="card-meta"] button`)!.textContent!;

  it('a_run_turned_away_before_it_started_reads_Not_started_and_is_not_counted_as_failed', async () => {
    const c = await mountBoard(ITEMS, RUNS);
    expect(chip(c, 'a')).toContain('Not started');
    expect(chip(c, 'a')).not.toContain('Failed');
    expect(chip(c, 'b')).toContain('Failed');
    const strip = c.querySelector('[data-testid="agent-strip"]')!;
    const counts = [...strip.querySelectorAll('button')].map((b) => b.textContent!.replace(/\s+/g, ' ').trim().split('.')[0]);
    expect(counts).toContain('1 Failed');
    expect(counts).toContain('1 Not started');
    expect(counts).toContain('2 Ready for review');
    // It sorts after the runs that need a person or are going, never among the failed ones.
    expect(titles(c)).toEqual(['Charlie', 'Delta', 'Bravo', 'Alpha']);
  });

  it('a_ready_for_review_run_the_verifier_says_to_reject_says_so_in_words_and_stays_in_review', async () => {
    const c = await mountBoard(ITEMS, RUNS);
    expect(chip(c, 'c')).toContain('Ready for review · verifier says reject');
    expect(chip(c, 'd')).toContain('Ready for review');
    expect(chip(c, 'd')).not.toContain('verifier says reject');
  });
});

describe('Board agent strip and column bars', () => {
  const ITEMS = [
    makeItem({ id: 'a', title: 'Alpha', status: 'To Do' }),
    makeItem({ id: 'b', title: 'Bravo', status: 'To Do' }),
    makeItem({ id: 'c', title: 'Charlie', status: 'To Do' }),
    makeItem({ id: 'd', title: 'Delta', status: 'Doing' }),
  ];
  const RUNS = [
    summary('r1', 'a', 'running'),
    summary('r2', 'b', 'waiting_decision'),
    summary('r3', 'c', 'running'),
    summary('r4', 'd', 'failed'),
  ];

  it('the_strip_counts_runs_by_state_with_one_chip_per_state_and_no_aggregate', async () => {
    const c = await mountBoard(ITEMS, RUNS);
    const strip = c.querySelector('[data-testid="agent-strip"]')!;
    expect(strip.getAttribute('aria-label')).toBe('Work in this project');
    expect(strip.textContent).toContain('Work in this project');
    const names = [...strip.querySelectorAll('button')].map((b) => b.textContent!.replace(/\s+/g, ' ').trim());
    expect(names.join(' ')).not.toContain('Your turn');
    expect(c.querySelector('[data-testid="strip-waiting"]')).toBeNull();
    expect(names.map((n) => n.split('.')[0])).toEqual(['1 Needs you', '0 Ready for review', '1 Failed', '2 Running', '0 Queued']);
    // A counter with nothing in it says why it cannot be used.
    const queued = strip.querySelector<HTMLButtonElement>('[data-testid="strip-queued"]')!;
    expect(queued.disabled).toBe(true);
    expect(queued.title).toBe('No work under Queued');
  });

  it('a_counter_toggles_a_filter_on_the_board', async () => {
    const c = await mountBoard(ITEMS, RUNS);
    // Bravo asks for a person, so it leads its column.
    expect(titles(c)).toEqual(['Bravo', 'Alpha', 'Charlie', 'Delta']);
    const running = c.querySelector<HTMLButtonElement>('[data-testid="strip-running"]')!;
    running.click();
    await flush();
    expect(running.getAttribute('aria-pressed')).toBe('true');
    expect(titles(c)).toEqual(['Alpha', 'Charlie']);
    // The column still says how many cards it holds.
    expect(c.querySelector('section')?.textContent).toContain('2 of 3');
    c.querySelector<HTMLButtonElement>('[data-testid="strip-needs"]')!.click();
    await flush();
    expect(titles(c)).toEqual(['Bravo', 'Alpha', 'Charlie']);
    running.click();
    c.querySelector<HTMLButtonElement>('[data-testid="strip-needs"]')!.click();
    await flush();
    expect(titles(c)).toEqual(['Bravo', 'Alpha', 'Charlie', 'Delta']);
  });

  it('a_column_header_has_a_mini_bar_whose_segments_are_filters_and_are_spelled_out', async () => {
    const c = await mountBoard(ITEMS, RUNS);
    const bars = [...c.querySelectorAll('[data-testid="run-mini-bar"]')];
    expect(bars.map((b) => b.getAttribute('aria-label'))).toEqual([
      'Work in this column: 1 needs you, 2 running',
      'Work in this column: 1 failed',
    ]);
    // Under each bar a legend names every state in words, in the same order, so colour is never the only cue.
    const legends = [...c.querySelectorAll('[data-testid="run-mini-legend"]')].map((l) => [...l.querySelectorAll('li')].map((li) => li.textContent).join(' | '));
    expect(legends).toEqual(['1Needs you | 2Running', '1Failed']);
    const segment = bars[0].querySelector<HTMLButtonElement>('[aria-label="1 needs you"]')!;
    segment.click();
    await flush();
    expect(titles(c)).toEqual(['Bravo']);
    expect(c.querySelector<HTMLButtonElement>('[data-testid="strip-needs"]')!.getAttribute('aria-pressed')).toBe('true');
  });

  it('the_strip_and_the_bars_are_absent_when_the_project_has_never_run_anything', async () => {
    const c = await mountBoard(ITEMS, []);
    expect(c.querySelector('[data-testid="agent-strip"]')).toBeNull();
    expect(c.querySelector('[data-testid="run-mini-bar"]')).toBeNull();
    expect(titles(c)).toEqual(['Alpha', 'Bravo', 'Charlie', 'Delta']);
  });

  it('a_live_run_in_a_to_do_column_shows_the_rail_so_the_board_does_not_read_as_idle', async () => {
    const c = await mountBoard(ITEMS, RUNS);
    const rail = c.querySelector('[data-item-id="a"] [data-testid="card-rail"]');
    expect(rail?.getAttribute('data-state')).toBe('running');
  });
});

describe('Board always-show-priority choice', () => {
  afterEach(() => localStorage.clear());

  it('has_no_switch_of_its_own_and_follows_the_choice_made_in_settings', async () => {
    const items = [makeItem({ id: 'a', priority: 'medium' })];
    const off = await mountBoard(items, []);
    expect(off.querySelector('input[role="switch"]')).toBeNull();
    expect(off.textContent).not.toContain('Always show priority on cards');
    expect(off.querySelector('[data-testid="card-meta"]')?.textContent).not.toContain('Medium');

    localStorage.setItem('tack_board_always_priority', '1');
    const on = await mountBoard(items, []);
    expect(on.querySelector('[data-testid="card-meta"]')?.textContent).toContain('Medium');
  });

  it('still_works_when_storage_throws', async () => {
    // Only the board's own key is blocked: the socket's session storage is not under test.
    const realGet = Storage.prototype.getItem;
    const get = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(function (this: Storage, key: string) {
      if (key === 'tack_board_always_priority') throw new Error('blocked');
      return realGet.call(this, key);
    });
    const c = await mountBoard([makeItem({ id: 'a', priority: 'medium' })], []);
    expect(c.querySelector('[data-testid="card-meta"]')?.textContent).not.toContain('Medium');
    get.mockRestore();
  });
});
