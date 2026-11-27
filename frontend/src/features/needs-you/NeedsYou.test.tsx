import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route, createMemoryHistory } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import { applyNotificationMessage } from '../../shared/state/notifications';
import { flush } from '../../test/flush';
import NeedsYou from './NeedsYou';
import { waitedFor, wordsOf } from './words';

const settle = async () => {
  for (let i = 0; i < 8; i += 1) await flush();
};

const minutesAgo = (m: number) => new Date(Date.now() - m * 60_000).toISOString();

const changeSet = (over: Record<string, unknown> = {}) => ({
  id: 'cs1',
  project: 'p1',
  created_at: minutesAgo(30),
  origin: 'mcp',
  principal: 'agent:c1',
  principal_name: 'Planner credential',
  reason: 'Billing blocks the launch',
  status: 'waiting',
  ops: [{ operation: { op: 'priority', item: 'item-a', priority: 'high' }, state: 'waiting' }],
  ...over,
});

interface Served {
  waitingMax?: number | null;
  decided?: unknown[];
  level?: string;
}

const disposers: Array<() => void> = [];
let fetched: string[] = [];

function mount(served: Served = {}, at = '/projects/p1/needs-you') {
  fetched = [];
  vi.stubGlobal('WebSocket', class { close() {} addEventListener() {} });
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
    const url = String(input);
    fetched.push(url);
    const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } });
    if (url.includes('/projects/p1/items?')) {
      const data = [
        { id: 'item-a', project_id: 'p1', title: 'Add invoices' },
        { id: 'item-b', project_id: 'p1', title: 'Fix the login' },
      ];
      return json({ data, total: data.length, page: 1, per_page: 200 });
    }
    if (url.endsWith('/projects/p1/autonomy')) return json({ level: 'L3', domains: [], budgets: { waiting_max: served.waitingMax ?? null } });
    if (url.includes('/projects/p1/change-sets')) return json(served.decided ?? []);
    if (url.endsWith('/change-sets/cs1')) return json(changeSet());
    return json({});
  }) as typeof fetch);
  const project = {
    projectId: () => 'p1',
    project: (() => ({ id: 'p1', name: 'My SaaS', agents_on_board: { level: served.level ?? 'propose', creates_per_change_set: 0, creates_per_day: 0 } })) as never,
    workflow: () => undefined,
    vocabulary: () => ({}),
    refetch: () => {},
  } as unknown as ProjectContextValue;
  const history = createMemoryHistory();
  history.set({ value: at });
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ProjectContext.Provider value={project}>
        <MemoryRouter history={history}>
          <Route path="/projects/:id/needs-you" component={NeedsYou} />
          <Route path="/projects/:id/items/:itemId" component={() => <p>the item</p>} />
        </MemoryRouter>
      </ProjectContext.Provider>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return { container, history };
}

const IDS = ['w1', 'w2', 'w3', 'w4'];
type Kind = 'decision' | 'review' | 'run_failed' | 'change_set';
let opened = 0;
const wait = (id: string, kind: Kind, over: { project?: string; at?: string; item?: string | null } = {}) => {
  opened += 1;
  applyNotificationMessage({
    type: 'opened',
    id,
    project_id: over.project ?? 'p1',
    kind: kind as never,
    level: 'needs_you',
    key: kind === 'change_set' ? 'change_set:cs1' : `${kind}:${id}`,
    item_id: over.item === undefined ? 'item-a' : over.item,
    request_id: kind === 'change_set' ? null : `req-${id}`,
    title: 'A run is asking a question',
    cause: 'Implementer',
    created_at: over.at ?? minutesAgo(5),
    open: opened,
  });
};

beforeEach(() => {
  opened = 0;
});

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  for (const id of IDS) applyNotificationMessage({ type: 'closed', id, key: id, open: 0 });
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

const rows = (c: HTMLElement) => [...c.querySelectorAll('[data-testid="needs-you-row"]')];

describe('Needs you', () => {
  it('says nothing waits when nothing does, with no count', async () => {
    const { container } = mount();
    await settle();
    expect(container.querySelector('h1')!.textContent).toBe('Needs you');
    expect(container.querySelector('[data-testid="needs-you-empty"]')!.textContent).toBe('Nothing waits for you in this project.');
    expect(container.querySelector('[data-testid="needs-you-count"]')).toBeNull();
  });

  it('lists the waits of this project oldest first, each with its task, kind in words, time waited and one action', async () => {
    wait('w1', 'run_failed', { at: minutesAgo(5), item: 'item-b' });
    wait('w2', 'decision', { at: minutesAgo(120) });
    wait('w3', 'review', { at: minutesAgo(30), item: 'item-b' });
    wait('w4', 'decision', { project: 'p2' });
    const { container } = mount();
    await settle();
    const list = rows(container);
    expect(list.map((r) => r.getAttribute('data-kind'))).toEqual(['decision', 'review', 'run_failed']);
    expect(list.map((r) => r.querySelector('[data-testid="needs-you-item"]')!.textContent)).toEqual(['Add invoices', 'Fix the login', 'Fix the login']);
    expect(list[0].textContent).toContain('A question');
    expect(list[0].textContent).toContain('Waiting 2 h');
    expect(list[1].textContent).toContain('Ready for review');
    expect(list[1].textContent).toContain('Waiting 30 min');
    expect(list[2].textContent).toContain('Failed');
    const actions = list.map((r) => r.querySelector('button')!.textContent);
    expect(actions).toEqual(['Answer', 'Review', 'See why']);
    expect(container.querySelector('[data-testid="needs-you-count"]')!.textContent).toBe('3 waiting for you');
  });

  it('makes the first action the one solid button and the others outlined', async () => {
    wait('w1', 'decision', { at: minutesAgo(60) });
    wait('w2', 'review', { at: minutesAgo(10) });
    const { container } = mount();
    await settle();
    const [first, second] = rows(container).map((r) => r.querySelector('button')!);
    expect(first.classList.contains('border')).toBe(false);
    expect(second.classList.contains('border')).toBe(true);
  });

  it('opens the run on the task\'s Runs from a question, a review or a failure', async () => {
    wait('w1', 'decision');
    const { container, history } = mount();
    await settle();
    container.querySelector<HTMLButtonElement>('[data-testid="needs-you-row"] button')!.click();
    await settle();
    expect(history.get()).toBe('/projects/p1/items/item-a?tab=execution&run=req-w1');
  });

  it('says how many wait out of the limit the project sets', async () => {
    wait('w1', 'decision');
    wait('w2', 'review');
    const { container } = mount({ waitingMax: 5 });
    await settle();
    expect(container.querySelector('[data-testid="needs-you-count"]')!.textContent).toBe('2 of 5 waiting for you');
  });

  it('says auto-start is paused when the limit is reached', async () => {
    wait('w1', 'decision');
    wait('w2', 'review');
    const { container } = mount({ waitingMax: 2 });
    await settle();
    expect(container.querySelector('[data-testid="needs-you-count"]')!.textContent).toBe(
      '2 of 2 waiting for you: auto-start is paused until one is decided',
    );
  });

  it('shows a change set\'s panel inline when its action is pressed, and hides it again', async () => {
    wait('w1', 'change_set', { item: null });
    const { container } = mount();
    await settle();
    expect(container.querySelector('[data-testid="needs-you-change-set"]')).toBeNull();
    const button = container.querySelector<HTMLButtonElement>('[data-testid="needs-you-row"] button')!;
    expect(button.textContent).toBe('Review the changes');
    button.click();
    await settle();
    expect(fetched.some((u) => u.endsWith('/change-sets/cs1'))).toBe(true);
    expect(container.querySelector('[data-testid="needs-you-change-set"]')!.textContent).toContain('Billing blocks the launch');
    expect(button.textContent).toBe('Hide the changes');
    button.click();
    await settle();
    expect(container.querySelector('[data-testid="needs-you-change-set"]')).toBeNull();
  });

  it('opens the change set the address names', async () => {
    wait('w1', 'change_set', { item: null });
    const { container } = mount({}, '/projects/p1/needs-you?open=cs1');
    await settle();
    expect(container.querySelector('[data-testid="needs-you-change-set"]')).not.toBeNull();
  });
});

describe('Needs you: Decided', () => {
  const decided = [
    changeSet({ id: 'cs2', status: 'applied', decided_at: minutesAgo(60) }),
    changeSet({ id: 'cs3', status: 'rejected', decided_at: minutesAgo(600), reason: 'Not now' }),
  ];
  const show = async (c: HTMLElement) => {
    [...c.querySelectorAll<HTMLButtonElement>('[aria-label="Show"] button')].find((b) => b.textContent === 'Decided')!.click();
    await settle();
  };

  it('has the two filters, Waiting pressed first', async () => {
    const { container } = mount();
    await settle();
    const buttons = [...container.querySelectorAll<HTMLButtonElement>('[aria-label="Show"] button')];
    expect(buttons.map((b) => [b.textContent, b.getAttribute('aria-pressed')])).toEqual([['Waiting', 'true'], ['Decided', 'false']]);
  });

  it('lists the change sets applied or turned down, each folded into a line', async () => {
    const { container } = mount({ decided });
    await settle();
    await show(container);
    expect(fetched.some((u) => u.includes('/projects/p1/change-sets?status=applied%2Crejected'))).toBe(true);
    const lines = [...container.querySelectorAll('[data-testid="needs-you-decided"] summary')].map((s) => s.textContent);
    expect(lines[0]).toContain('Planner credential · Applied');
    expect(lines[1]).toContain('Turned down');
    expect(lines[1]).toContain('Not now');
    expect(container.querySelector('[data-testid="needs-you-empty"]')).toBeNull();
  });

  it('says none has been decided when none has', async () => {
    const { container } = mount({ decided: [] });
    await settle();
    await show(container);
    expect(container.querySelector('[data-testid="needs-you-decided-empty"]')!.textContent).toContain('No change set has been applied or turned down yet.');
  });

  it('says why agents propose nothing when they may only read', async () => {
    const { container } = mount({ decided: [], level: 'read' });
    await settle();
    await show(container);
    expect(container.querySelector('[data-testid="needs-you-off"]')!.textContent).toContain('Agents may only read in this project');
  });
});

describe('the words', () => {
  it('names each kind of wait and its action', () => {
    expect(wordsOf('decision')).toEqual({ label: 'A question', action: 'Answer' });
    expect(wordsOf('review')).toEqual({ label: 'Ready for review', action: 'Review' });
    expect(wordsOf('run_failed')).toEqual({ label: 'Failed', action: 'See why' });
    expect(wordsOf('change_set')).toEqual({ label: 'Changes to apply', action: 'Review the changes' });
    expect(wordsOf('anything_else').action).toBe('Open');
  });

  it('says how long something has waited', () => {
    const now = Date.parse('2026-11-26T12:00:00Z');
    const ago = (ms: number) => new Date(now - ms).toISOString();
    expect(waitedFor(ago(20_000), now)).toBe('Waiting less than a minute');
    expect(waitedFor(ago(14 * 60_000), now)).toBe('Waiting 14 min');
    expect(waitedFor(ago(3 * 3_600_000), now)).toBe('Waiting 3 h');
    expect(waitedFor(ago(24 * 3_600_000), now)).toBe('Waiting 1 day');
    expect(waitedFor(ago(72 * 3_600_000), now)).toBe('Waiting 3 days');
  });
});
