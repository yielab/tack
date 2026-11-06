import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { Resource } from 'solid-js';
import { MemoryRouter, Route } from '@solidjs/router';
import { ExecutionStoreProvider } from '../state/executionContext';
import { ProjectContext, type ProjectContextValue } from '../state/projectContext';
import type { Item, ItemTypeDef } from '../types';
import { flush } from '../../test/flush';
import GroupHeader, { type GroupHeaderProps } from './GroupHeader';

const CATALOG: ItemTypeDef[] = [
  { key: 'epic', name: 'Epic', color: 'accent2', level: 'group', default_work: false, order: 0 },
  { key: 'task', name: 'Task', color: 'accent', level: 'work', default_work: true, order: 1 },
];
const STATUSES = [
  { name: 'To Do', order: 0, category: 'todo' as const },
  { name: 'Done', order: 1, category: 'done' as const },
];

const item = (p: Partial<Item> & { id: string }): Item => ({
  project_id: 'p1', title: 'Untitled', item_type: 'task', node: 'work', status: 'To Do', priority: 'medium',
  estimate_unit: 'story_points', tags: [], sort_order: 0, created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z', ...p,
});
const PARENT = item({ id: 'top', title: 'Shop', item_type: 'epic', node: 'group' });
const GROUP = item({
  id: 'g', title: 'Checkout', item_type: 'epic', node: 'group', parent_id: 'top',
  progress: { total: 7, done: 3, open_runs: 1, needs_you: 2, estimate_total: null, estimate_done: null, starts: null, ends: null, teams: ['saas-build', 'claude-code'] },
});
const byId = new Map([PARENT, GROUP].map((i) => [i.id, i]));

const disposers: Array<() => void> = [];
beforeEach(() => localStorage.clear());
afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

async function mount(extra: Partial<GroupHeaderProps> = {}) {
  vi.spyOn(globalThis, 'fetch').mockImplementation((async () => new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 })) as typeof fetch);
  const ctx = { project: (() => ({ id: 'p1', item_types: CATALOG })) as unknown as Resource<null> } as ProjectContextValue;
  const container = document.createElement('div');
  document.body.appendChild(container);
  disposers.push(render(() => (
    <ExecutionStoreProvider>
      <ProjectContext.Provider value={ctx}>
        <MemoryRouter>
          <Route path="/" component={() => (
            <GroupHeader group={GROUP} byId={byId} density="lane" statuses={STATUSES} {...extra}>
              <p data-testid="work">the work</p>
            </GroupHeader>
          )} />
        </MemoryRouter>
      </ProjectContext.Provider>
    </ExecutionStoreProvider>
  ), container));
  await flush();
  return container;
}

describe('GroupHeader', () => {
  it('draws_the_path_the_tile_the_title_as_a_link_the_status_the_progress_and_the_teams', async () => {
    const c = await mount();
    expect(c.querySelector('[data-testid="item-path"]')!.textContent).toContain('Shop');
    expect(c.querySelector('[data-testid="type-tile"]')!.textContent).toBe('Epic');
    const link = c.querySelector<HTMLAnchorElement>('a[href="/projects/p1/items/g"]')!;
    expect(link.textContent).toBe('Checkout');
    expect(c.querySelector('[data-testid="group-status"]')!.textContent).toContain('To Do');
    expect(c.querySelector('[data-testid="progress-text"]')!.textContent).toBe('3 of 7 done');
    expect(c.querySelector('[data-testid="progress-needs"]')!.textContent).toBe('2 need you');
    expect(c.querySelector('[data-testid="group-teams"]')!.textContent).toBe('saas-build, claude-code');
  });

  it('always_offers_add_item_and_plan_with_agent_and_move_under_when_given', async () => {
    const onAddItem = vi.fn();
    const onMoveUnder = vi.fn();
    const c = await mount({ onAddItem, onMoveUnder });
    c.querySelector<HTMLButtonElement>('[aria-label="Add item to Checkout"]')!.click();
    c.querySelector<HTMLButtonElement>('[aria-label="Move under…, Checkout"]')!.click();
    expect(onAddItem).toHaveBeenCalledOnce();
    expect(onMoveUnder).toHaveBeenCalledOnce();
    expect([...c.querySelectorAll('button')].some((b) => b.textContent === 'Plan with agent')).toBe(true);
  });

  it('sets_the_status_from_a_chip_that_opens_the_status_menu', async () => {
    const onStatusChange = vi.fn();
    const c = await mount({ onStatusChange });
    const chip = c.querySelector<HTMLButtonElement>('[aria-haspopup="menu"]')!;
    expect(chip.getAttribute('aria-label')).toBe('Status of Checkout: To Do. Change status');
    chip.click();
    await flush();
    const entries = [...c.querySelectorAll('[role="menuitem"]')];
    expect(entries.map((e) => e.textContent)).toEqual(['To DoCurrent', 'Done']);
    (entries[1] as HTMLElement).click();
    expect(onStatusChange).toHaveBeenCalledWith('Done');
  });

  it('folds_its_work_remembers_the_choice_per_group_and_starts_shut_when_told_to', async () => {
    const c = await mount();
    const fold = c.querySelector<HTMLButtonElement>('button[aria-expanded]')!;
    expect(fold.getAttribute('aria-label')).toBe('Work under Checkout');
    expect(c.querySelector<HTMLElement>('#' + CSS.escape(fold.getAttribute('aria-controls')!))!.hidden).toBe(false);
    fold.click();
    await flush();
    expect(fold.getAttribute('aria-expanded')).toBe('false');
    expect(c.querySelector<HTMLElement>('[data-testid="work"]')!.parentElement!.hidden).toBe(true);
    expect(localStorage.getItem('tack_fold:group:g')).toBe('0');
    document.body.innerHTML = '';
    const again = await mount({ defaultOpen: true });
    expect(again.querySelector('button[aria-expanded]')!.getAttribute('aria-expanded')).toBe('false'); // what the person chose wins
    localStorage.clear();
    const shut = await mount({ defaultOpen: false });
    expect(shut.querySelector('button[aria-expanded]')!.getAttribute('aria-expanded')).toBe('false');
  });

  it('shows_the_servers_refusal_inline_as_an_alert', async () => {
    const c = await mount({ refusal: 'An epic cannot go under a task.' });
    expect(c.querySelector('[role="alert"]')!.textContent).toBe('An epic cannot go under a task.');
    expect((await mount()).querySelector('[role="alert"]')).toBeNull();
  });
});
