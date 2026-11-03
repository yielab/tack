import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import type { Project } from '../types';
import { decisionTokenStore } from '../execution';
import { flush } from '../../test/flush';
import WaitingForYou, { WaitingBadge } from './WaitingForYou';

const LOOKED_KEY = 'tack_waiting_last_looked';
const disposers: Array<() => void> = [];
const minutesAgo = (m: number) => new Date(Date.now() - m * 60_000).toISOString();

const projects = [{ id: 'p1', name: 'Fabulada' }] as unknown as Project[];

interface Scenario {
  runs: Array<{ id: string; item: string; state: string; created: string; ended?: string; needsReview?: boolean; title?: string }>;
  /** The change sets the project p1 answers for `status=waiting,partly`. */
  changeSets?: Array<{ id: string; waiting: number; by?: string; reason?: string; created: string }>;
}

/** The three reads the inbox makes: the runs, an item for each, and a finished run's attempts. */
function mockApi({ runs, changeSets }: Scenario) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
    const url = String(input);
    const ok = (body: unknown) =>
      new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } });
    if (url.endsWith('/projects')) return ok(projects);
    if (url.includes('/projects/p1/change-sets')) {
      return ok(
        (changeSets ?? []).map((c) => ({
          id: c.id,
          project: 'p1',
          created_at: c.created,
          origin: 'mcp',
          principal: 'agent:c1',
          principal_name: c.by ?? null,
          reason: c.reason ?? 'Because',
          status: 'waiting',
          ops: Array.from({ length: c.waiting + 1 }, (_, i) => ({ operation: { op: 'priority', item: 'x', priority: 'high' }, state: i === 0 ? 'applied' : 'waiting' })),
        })),
      );
    }
    const attempts = /\/executions\/([^/]+)\/attempts/.exec(url);
    if (attempts) {
      const r = runs.find((x) => x.id === attempts[1]);
      return ok({ protocol_version: 1, data: r?.ended ? [{ ended_at: r.ended, updated_at: r.ended }] : [] });
    }
    if (url.endsWith('/executions')) {
      return ok({
        protocol_version: 1,
        data: runs.map((r) => ({
          request_id: r.id, item_id: r.item, state: r.state, cancellation_requested_at: null, created_at: r.created,
        })),
      });
    }
    const item = /\/items\/([^/?]+)/.exec(url);
    if (item) {
      const r = runs.find((x) => x.item === item[1])!;
      return ok({
        item: { id: r.item, title: r.title ?? `Item ${r.item}`, project_id: 'p1', needs_review: r.needsReview === true },
        roles: [],
        dependencies: [],
      });
    }
    return ok({});
  }) as typeof fetch);
}

function mount(opts: { sidebar?: boolean } = {}) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <MemoryRouter>
        <Route
          path="/"
          component={() => (
            <>
              <WaitingBadge />
              {opts.sidebar === false ? null : <WaitingForYou projects={projects} onNavigate={() => {}} />}
            </>
          )}
        />
      </MemoryRouter>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

async function settle() {
  for (let i = 0; i < 8; i += 1) await flush();
}

const toggle = (c: HTMLElement) => c.querySelector<HTMLButtonElement>('#waiting-toggle')!;
const badge = (c: HTMLElement) => c.querySelector<HTMLButtonElement>('[data-testid="waiting-badge"]')!;
const rowsOf = (c: HTMLElement, group: string) => [...c.querySelectorAll(`[data-testid="waiting-group-${group}"] [data-testid="waiting-row"]`)];

beforeEach(() => {
  localStorage.setItem(LOOKED_KEY, String(Date.now() - 3_600_000));
  decisionTokenStore.set(null);
});

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  vi.restoreAllMocks();
  localStorage.clear();
  decisionTokenStore.set(null);
});

describe('Waiting for you', () => {
  it('stays when nothing waits, reads 0, and says what it means on hover and to a screen reader', async () => {
    mockApi({ runs: [{ id: 'r1', item: 'a', state: 'running', created: minutesAgo(5) }] });
    const c = mount();
    await settle();

    expect(c.querySelector('[data-testid="waiting-count"]')!.textContent).toBe('0');
    expect(toggle(c).title).toContain('Nothing is waiting for you');
    expect(toggle(c).title).toContain('runs that failed since you last looked');
    expect(toggle(c).textContent).toContain('Nothing is waiting for you');
    toggle(c).click();
    await flush();
    expect(c.querySelector('[data-testid="waiting-empty"]')!.textContent).toBe('Nothing is waiting for you.');
    expect(badge(c).getAttribute('aria-label')).toBe('0: Nothing is waiting for you');
    // Label in Name (WCAG 2.5.3): the name starts with the count the button shows.
    expect(badge(c).getAttribute('aria-label')!.startsWith(badge(c).textContent!.trim())).toBe(true);
  });

  it('lists a question, a run to review and a failure since the last look, each with its dot, title, project, age and action', async () => {
    mockApi({
      runs: [
        { id: 'q', item: 'a', state: 'waiting_decision', created: minutesAgo(3), title: 'Pick a chart library' },
        { id: 'v', item: 'b', state: 'succeeded', created: minutesAgo(50), ended: minutesAgo(20), needsReview: true, title: 'Add the export' },
        { id: 'f', item: 'c', state: 'failed', created: minutesAgo(40), ended: minutesAgo(10), title: 'Fix the importer' },
        { id: 'old', item: 'd', state: 'failed', created: minutesAgo(300), ended: minutesAgo(200), title: 'An old failure' },
        { id: 'ok', item: 'e', state: 'succeeded', created: minutesAgo(30), ended: minutesAgo(25), needsReview: false, title: 'Reviewed already' },
      ],
    });
    const c = mount();
    await settle();
    toggle(c).click();
    await flush();

    const ask = rowsOf(c, 'ask');
    expect(ask).toHaveLength(1);
    expect(ask[0].textContent).toContain('Pick a chart library');
    expect(ask[0].textContent).toContain('Fabulada');
    expect(ask[0].textContent).toContain('3m ago');
    expect(ask[0].querySelector('[role="img"]')!.getAttribute('aria-label')).toBe('Needs you');
    const answer = ask[0].querySelector('a')!;
    expect(answer.textContent).toBe('Answer the question');
    const href = new URL(answer.getAttribute('href')!, 'http://x');
    expect(href.searchParams.get('item')).toBe('a');
    expect(href.searchParams.get('tab')).toBe('execution');
    expect(href.searchParams.get('run')).toBe('q');

    const review = rowsOf(c, 'review');
    expect(review).toHaveLength(1);
    expect(review[0].textContent).toContain('Add the export');
    expect(review[0].textContent).toContain('20m ago');
    expect(review[0].querySelector('[role="img"]')!.getAttribute('aria-label')).toBe('Ready for review');
    expect(review[0].querySelector('a')!.textContent).toBe('Review and merge');

    const failed = rowsOf(c, 'failed');
    expect(failed).toHaveLength(1);
    expect(failed[0].textContent).toContain('Fix the importer');
    expect(failed[0].textContent).toContain('10m ago');
    expect(failed[0].querySelector('[role="img"]')!.getAttribute('aria-label')).toBe('Failed');
    expect(failed[0].querySelector('a')!.textContent).toBe('See why it failed');
    expect(failed[0].querySelector('a')!.getAttribute('aria-label')).toBe('See why it failed: Fix the importer');

    expect(c.textContent).not.toContain('An old failure');
    expect(c.textContent).not.toContain('Reviewed already');
    expect(c.querySelector('[data-testid="waiting-count"]')!.textContent).toBe('3');
  });

  it('says in words, under its name, what the count is made of and that it covers every project', async () => {
    mockApi({
      runs: [
        { id: 'q', item: 'a', state: 'waiting_decision', created: minutesAgo(3), title: 'Pick a chart library' },
        { id: 'v', item: 'b', state: 'succeeded', created: minutesAgo(50), ended: minutesAgo(20), needsReview: true, title: 'Add the export' },
      ],
    });
    const c = mount();
    await settle();

    const scope = c.querySelector('[data-testid="waiting-scope"]')!;
    expect(scope.textContent).toBe('All projects: 1 needs you, 1 ready for review');
    expect(badge(c).title).toContain('across all projects');
  });

  it('shows no scope line when nothing waits', async () => {
    mockApi({ runs: [{ id: 'r1', item: 'a', state: 'running', created: minutesAgo(5) }] });
    const c = mount();
    await settle();
    expect(c.querySelector('[data-testid="waiting-scope"]')).toBeNull();
  });

  it('the top bar badge shows the same count and announces it', async () => {
    mockApi({
      runs: [
        { id: 'q1', item: 'a', state: 'waiting_decision', created: minutesAgo(3) },
        { id: 'q2', item: 'b', state: 'waiting_decision', created: minutesAgo(2) },
      ],
    });
    const c = mount();
    await settle();

    expect(badge(c).textContent).toBe('2');
    expect(badge(c).getAttribute('aria-label')).toBe('2 things are waiting for you');
    expect(badge(c).getAttribute('aria-label')!.startsWith(badge(c).textContent!.trim())).toBe(true);
    expect(c.querySelector('[data-testid="waiting-count"]')!.textContent).toBe('2');
  });

  it('pressing the badge opens the list', async () => {
    mockApi({ runs: [{ id: 'q', item: 'a', state: 'waiting_decision', created: minutesAgo(3) }] });
    const c = mount();
    await settle();
    expect(c.querySelector('#waiting-panel')).toBeNull();

    badge(c).click();
    await flush();
    expect(c.querySelector('#waiting-panel')).not.toBeNull();
    expect(toggle(c).getAttribute('aria-expanded')).toBe('true');
  });

  it('opening remembers the time, keeps the failures in view while open, and drops them once closed', async () => {
    const before = Number(localStorage.getItem(LOOKED_KEY));
    mockApi({ runs: [{ id: 'f', item: 'c', state: 'failed', created: minutesAgo(40), ended: minutesAgo(10), title: 'Fix the importer' }] });
    const c = mount();
    await settle();
    expect(Number(localStorage.getItem(LOOKED_KEY))).toBe(before);
    expect(badge(c).textContent).toBe('1');

    toggle(c).click();
    await flush();
    expect(Number(localStorage.getItem(LOOKED_KEY))).toBeGreaterThan(before);
    expect(rowsOf(c, 'failed')).toHaveLength(1);

    toggle(c).click();
    await flush();
    expect(badge(c).textContent).toBe('0');

    // The next visit starts from the time of the last look.
    disposers.pop()!();
    const again = mount();
    await settle();
    expect(badge(again).textContent).toBe('0');
  });

  it('a browser that never looked starts from now, and one whose storage throws still works', async () => {
    localStorage.clear();
    mockApi({ runs: [{ id: 'f', item: 'c', state: 'failed', created: minutesAgo(40), ended: minutesAgo(10) }] });
    const c = mount();
    await settle();
    expect(badge(c).textContent).toBe('0');
    expect(localStorage.getItem(LOOKED_KEY)).not.toBeNull();
    disposers.pop()!();

    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('blocked');
    });
    const blocked = mount();
    await settle();
    expect(badge(blocked).textContent).toBe('0');
    toggle(blocked).click();
    await flush();
    expect(blocked.querySelector('[data-testid="waiting-empty"]')).not.toBeNull();
  });

  it('asks for the decision token once: in the list while a question waits, and not again after it is given', async () => {
    mockApi({ runs: [{ id: 'q', item: 'a', state: 'waiting_decision', created: minutesAgo(3) }] });
    const setItem = vi.spyOn(Storage.prototype, 'setItem');
    const c = mount();
    await settle();
    toggle(c).click();
    await flush();

    const field = c.querySelector<HTMLInputElement>('input[type="password"]')!;
    expect(field).not.toBeNull();
    field.value = 's3cret';
    field.dispatchEvent(new InputEvent('input', { bubbles: true }));
    [...c.querySelectorAll('button')].find((b) => b.textContent === 'Save')!.click();
    await flush();

    expect(decisionTokenStore.get()).toBe('s3cret');
    expect(c.querySelector('input[type="password"]')).toBeNull();
    // Kept for the session only: nothing about the token reaches localStorage.
    expect(localStorage.length).toBe(1);
    expect(localStorage.getItem(LOOKED_KEY)).not.toBe('s3cret');
    expect(setItem.mock.calls.filter(([k]) => String(k).includes('decision'))).toHaveLength(1);
  });

  it('asks for no token when no question waits', async () => {
    mockApi({ runs: [{ id: 'v', item: 'b', state: 'succeeded', created: minutesAgo(50), ended: minutesAgo(20), needsReview: true }] });
    const c = mount();
    await settle();
    toggle(c).click();
    await flush();
    expect(c.querySelector('input[type="password"]')).toBeNull();
  });

  it('counts the change sets that wait with the runs, lists them in a fourth group and opens the change sets page', async () => {
    mockApi({
      runs: [{ id: 'q', item: 'a', state: 'waiting_decision', created: minutesAgo(3), title: 'Pick a chart library' }],
      changeSets: [
        { id: 'cs1', waiting: 3, by: 'Planner credential', reason: 'Billing goes first', created: minutesAgo(20) },
        { id: 'cs2', waiting: 1, created: minutesAgo(10) },
      ],
    });
    const c = mount();
    await settle();

    expect(c.querySelector('[data-testid="waiting-count"]')!.textContent).toBe('3');
    expect(badge(c).getAttribute('aria-label')).toBe('3 things are waiting for you');
    expect(toggle(c).title).toContain('2 change sets to apply');
    toggle(c).click();
    await flush();

    const rows = [...c.querySelectorAll('[data-testid="waiting-group-changes"] [data-testid="waiting-changeset"]')];
    expect(rows).toHaveLength(2);
    // Oldest first; an operation already decided does not count.
    expect(rows[0].textContent).toContain('Planner credential proposes 3 changes');
    expect(rows[0].textContent).toContain('Fabulada');
    expect(rows[1].textContent).toContain('An agent proposes 1 change');
    expect(rows[0].querySelector('[role="img"]')!.getAttribute('aria-label')).toBe('Needs you');
    const open = rows[0].querySelector('a')!;
    expect(open.textContent).toBe('Review the changes');
    expect(open.getAttribute('href')).toBe('/projects/p1/change-sets?open=cs1');
  });

  it('shows no group for change sets when none wait', async () => {
    mockApi({ runs: [], changeSets: [] });
    const c = mount();
    await settle();
    toggle(c).click();
    await flush();
    expect(c.querySelector('[data-testid="waiting-group-changes"]')).toBeNull();
    expect(c.querySelector('[data-testid="waiting-count"]')!.textContent).toBe('0');
  });
});
