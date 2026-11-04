import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { Resource } from 'solid-js';
import { MemoryRouter, Route, createMemoryHistory } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../state/projectContext';
import Sidebar from './Sidebar';
import { applyNotificationMessage, closeInbox, inboxOpen } from '../state/notifications';
import { flush } from '../../test/flush';

const settle = async () => {
  for (let i = 0; i < 8; i += 1) await flush();
};

const projectValue: ProjectContextValue = {
  projectId: () => undefined,
  project: (() => null) as unknown as Resource<null>,
  workflow: () => undefined,
  vocabulary: () => ({}),
  refetch: () => {},
} as unknown as ProjectContextValue;

const disposers: Array<() => void> = [];

const run = (id: string, item: string, state: string, created: string) => ({
  request_id: id, item_id: item, state, cancellation_requested_at: null, created_at: created,
});

function mount(executions: unknown[]) {
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
    const url = String(input);
    const body = url.includes('/executions') ? { protocol_version: 1, data: executions } : url.includes('/projects') ? [] : {};
    return new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } });
  }) as typeof fetch);
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ProjectContext.Provider value={projectValue}>
        <MemoryRouter>
          <Route path="/" component={Sidebar} />
        </MemoryRouter>
      </ProjectContext.Provider>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  vi.restoreAllMocks();
});

describe('Sidebar: Waiting for you', () => {
  it('counts the runs that wait for an answer and opens a row for each, on its Execution tab', async () => {
    const c = mount([
      run('req-2', 'item-b', 'waiting_decision', '2026-10-31T11:00:00Z'),
      run('req-1', 'item-a', 'waiting_decision', '2026-10-31T10:00:00Z'),
      run('req-0', 'item-c', 'running', '2026-10-31T09:00:00Z'),
    ]);
    await settle();
    expect(c.querySelector('[data-testid="waiting-count"]')!.textContent).toBe('2');
    c.querySelector<HTMLButtonElement>('#waiting-toggle')!.click();
    await flush();
    const links = [...c.querySelectorAll('[data-testid="waiting-group-ask"] a')];
    expect(links.map((a) => new URL(a.getAttribute('href')!, 'http://x').searchParams.get('run'))).toEqual(['req-2', 'req-1']);
    const href = new URL(links[0].getAttribute('href')!, 'http://x');
    expect(href.searchParams.get('item')).toBe('item-b');
    expect(href.searchParams.get('tab')).toBe('execution');
  });

  it('a run that has a newer run after it no longer counts', async () => {
    const c = mount([
      run('req-2', 'item-a', 'succeeded', '2026-10-31T11:00:00Z'),
      run('req-1', 'item-a', 'waiting_decision', '2026-10-31T10:00:00Z'),
    ]);
    await settle();
    expect(c.querySelector('[data-testid="waiting-count"]')!.textContent).toBe('0');
  });

  it('stays in the sidebar with a 0 when no run waits', async () => {
    const c = mount([run('req-1', 'item-a', 'running', '2026-10-31T10:00:00Z')]);
    await settle();
    expect(c.textContent).toContain('Waiting for you');
    expect(c.querySelector('[data-testid="waiting-count"]')!.textContent).toBe('0');
    expect(c.querySelector<HTMLButtonElement>('#waiting-toggle')!.title).toContain('Nothing is waiting for you');
  });
});

/** The sidebar inside project p1, whose rung is `level`, with `waiting` change sets waiting in it. */
function mountInProject(level: string, waiting: number) {
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
    const url = String(input);
    const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } });
    if (url.includes('/executions')) return json({ protocol_version: 1, data: [] });
    if (url.includes('/change-sets')) {
      return json(
        Array.from({ length: waiting }, (_, i) => ({
          id: `cs${i}`, project: 'p1', created_at: '2026-10-31T10:00:00Z', origin: 'mcp', principal: 'agent:c1', reason: 'r', status: 'waiting',
          ops: [{ operation: { op: 'priority', item: 'x', priority: 'high' }, state: 'waiting' }],
        })),
      );
    }
    if (url.endsWith('/projects')) {
      return json([{ id: 'p1', name: 'My SaaS', agents_on_board: { level, creates_per_change_set: 0, creates_per_day: 0 } }]);
    }
    return json({});
  }) as typeof fetch);
  const history = createMemoryHistory();
  history.set({ value: '/projects/p1/board' });
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ProjectContext.Provider value={projectValue}>
        <MemoryRouter history={history}>
          <Route path="/projects/:id/board" component={Sidebar} />
        </MemoryRouter>
      </ProjectContext.Provider>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

describe('Sidebar: Change sets', () => {
  it('links to the page when the project lets agents propose', async () => {
    const c = mountInProject('propose', 0);
    await settle();
    const link = [...c.querySelectorAll('a')].find((a) => a.textContent === 'Change sets')!;
    expect(link.getAttribute('href')).toBe('/projects/p1/change-sets');
    expect(c.querySelector('[data-testid="nav-change-sets-off"]')).toBeNull();
  });

  it('shows the entry off, with its reason, when agents may only read and nothing waits', async () => {
    const c = mountInProject('read', 0);
    await settle();
    const off = c.querySelector('[data-testid="nav-change-sets-off"]')!;
    expect(off.getAttribute('aria-disabled')).toBe('true');
    expect(off.textContent).toContain('Change sets');
    expect(off.getAttribute('title')).toContain('Agents may only read in this project');
    expect([...c.querySelectorAll('a')].some((a) => a.textContent === 'Change sets')).toBe(false);
  });

  it('turns the entry on at Read when a change set waits', async () => {
    const c = mountInProject('read', 1);
    await settle();
    expect([...c.querySelectorAll('a')].some((a) => a.textContent === 'Change sets')).toBe(true);
    expect(c.querySelector('[data-testid="waiting-count"]')!.textContent).toBe('1');
  });
});

describe('Sidebar: Inbox bell', () => {
  const notice = (id: string, open: number) =>
    applyNotificationMessage({
      type: 'opened',
      id,
      project_id: 'p1',
      kind: 'decision',
      level: 'needs_you',
      key: `decision:${id}`,
      item_id: 'item-1',
      request_id: 'req-1',
      title: 'A run is asking a question',
      cause: 'agent',
      created_at: '2026-11-03T10:00:00Z',
      open,
    });

  afterEach(() => {
    applyNotificationMessage({ type: 'read', id: 'none', open: 0 });
    closeInbox();
  });

  it('shows the open count on the bell and hides it at 0', async () => {
    const c = mount([]);
    await settle();
    const bell = c.querySelector<HTMLButtonElement>('[data-testid="inbox-bell"]')!;
    expect(c.querySelector('[data-testid="inbox-count"]')).toBeNull();
    expect(bell.getAttribute('aria-label')).toBe('Inbox');

    notice('n1', 2);
    await flush();
    expect(c.querySelector('[data-testid="inbox-count"]')!.textContent).toBe('2');
    expect(bell.getAttribute('aria-label')).toBe('Inbox, 2 open');

    applyNotificationMessage({ type: 'read', id: 'none', open: 0 });
    await flush();
    expect(c.querySelector('[data-testid="inbox-count"]')).toBeNull();
  });

  it('opens the inbox drawer', async () => {
    const c = mount([]);
    await settle();
    c.querySelector<HTMLButtonElement>('[data-testid="inbox-bell"]')!.click();
    expect(inboxOpen()).toBe(true);
  });
});
