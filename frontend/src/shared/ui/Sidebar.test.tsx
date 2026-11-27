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

/** The sidebar inside project p1. */
function mountInProject() {
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
    const url = String(input);
    const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } });
    if (url.includes('/executions')) return json({ protocol_version: 1, data: [] });
    if (url.includes('/change-sets')) return json([]);
    if (url.endsWith('/projects')) return json([{ id: 'p1', name: 'My SaaS' }, { id: 'p2', name: 'Other' }]);
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

const wait = (id: string, project: string, open: number, kind: 'decision' | 'run_failed' = 'decision') =>
  applyNotificationMessage({
    type: 'opened',
    id,
    project_id: project,
    kind,
    level: 'needs_you',
    key: `${kind}:${id}`,
    item_id: 'item-1',
    request_id: 'req-1',
    title: 'A run is asking a question',
    cause: 'agent',
    created_at: '2026-11-03T10:00:00Z',
    open,
  });

describe('Sidebar: project destinations', () => {
  afterEach(() => {
    for (const id of ['w1', 'w2', 'w3']) applyNotificationMessage({ type: 'closed', id, key: `decision:${id}`, open: 0 });
  });

  const projectLinks = (c: HTMLElement) => {
    const nav = c.querySelector('nav')!;
    return [...nav.querySelectorAll('a')].filter((a) => a.getAttribute('href')?.startsWith('/projects/p1/'));
  };

  it('lists the nine destinations in order, Needs you first, at their own addresses', async () => {
    const c = mountInProject();
    await settle();
    expect(projectLinks(c).map((a) => [a.textContent?.replace(/\d+ waiting$/, ''), a.getAttribute('href')])).toEqual([
      ['Needs you', '/projects/p1/needs-you'],
      ['Board', '/projects/p1/board'],
      ['List', '/projects/p1/list'],
      ['Timeline', '/projects/p1/timeline'],
      ['Sprint', '/projects/p1/sprint'],
      ['Orchestration', '/projects/p1/orchestration'],
      ['Threads', '/projects/p1/threads'],
      ['Overview', '/projects/p1/overview'],
      ['Project settings', '/projects/p1/settings'],
    ]);
  });

  it('has no Change sets, Calendar or Factory metrics entry, and no disabled one', async () => {
    const c = mountInProject();
    await settle();
    for (const gone of ['Change sets', 'Calendar', 'Factory metrics']) {
      expect(c.textContent).not.toContain(gone);
    }
    expect(c.querySelector('[aria-disabled="true"]')).toBeNull();
  });

  it('counts on the Needs you entry what waits in this project, and not what waits in another', async () => {
    const c = mountInProject();
    await settle();
    expect(c.querySelector('[data-testid="nav-count"]')).toBeNull();
    wait('w1', 'p1', 1);
    wait('w2', 'p1', 2, 'run_failed');
    wait('w3', 'p2', 3);
    await flush();
    const entry = projectLinks(c)[0];
    expect(entry.querySelector('[data-testid="nav-count"]')!.textContent).toBe('2');
    expect(entry.textContent).toContain('2 waiting');
    expect(projectLinks(c).slice(1).every((a) => a.querySelector('[data-testid="nav-count"]') === null)).toBe(true);
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
    applyNotificationMessage({ type: 'closed', id: 'n1', key: 'decision:n1', open: 0 });
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
