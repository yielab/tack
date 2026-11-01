import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { Resource } from 'solid-js';
import { MemoryRouter, Route, createMemoryHistory } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import ChangeSetsPage from './ChangeSetsPage';
import { flush } from '../../test/flush';

const disposers: Array<() => void> = [];
const settle = async () => {
  for (let i = 0; i < 6; i += 1) await flush();
};

const cs = (id: string, status: string, over: Record<string, unknown> = {}) => ({
  id,
  project: 'p1',
  created_at: new Date(Date.now() - 60_000).toISOString(),
  origin: 'run',
  origin_ref: 'req-1',
  principal: 'agent:c1',
  principal_name: 'Planner credential',
  reason: `Reason ${id}`,
  status,
  ...(status === 'applied' ? { decided_at: new Date().toISOString() } : {}),
  ops: [{ operation: { op: 'priority', item: 'i1', priority: 'high' }, state: status === 'applied' ? 'applied' : 'waiting' }],
  ...over,
});

function mount(level: string, answer: (status: string | null) => unknown[], search = '') {
  const urls: string[] = [];
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
    const url = String(input);
    urls.push(url);
    const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } });
    if (url.includes('/projects/p1/change-sets')) return json(answer(new URL(url, 'http://x').searchParams.get('status')));
    if (url.includes('/items/')) return json({ item: { id: 'i1', title: 'Payments epic', project_id: 'p1' }, roles: [], dependencies: [] });
    if (url.endsWith('/projects/p1')) return json({ id: 'p1', vocabulary: {}, workflow: { statuses: [] } });
    return json({});
  }) as typeof fetch);
  const project = { id: 'p1', name: 'My SaaS', agents_on_board: { level, creates_per_change_set: 0, creates_per_day: 0 } };
  const value = {
    projectId: () => 'p1',
    project: (() => project) as unknown as Resource<typeof project>,
    workflow: () => undefined,
    vocabulary: () => ({}),
    refetch: () => {},
  } as unknown as ProjectContextValue;
  const history = createMemoryHistory();
  history.set({ value: `/projects/p1/change-sets${search}` });
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ProjectContext.Provider value={value}>
        <MemoryRouter history={history}>
          <Route path="/projects/:id/change-sets" component={ChangeSetsPage} />
        </MemoryRouter>
      </ProjectContext.Provider>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return { container, urls };
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  vi.restoreAllMocks();
});

describe('Change sets page', () => {
  it('lists the change sets that wait, each in the panel, and reads the other statuses on demand', async () => {
    const { container: c, urls } = mount('propose', (status) =>
      status === 'applied' ? [cs('c3', 'applied')] : [cs('c1', 'waiting'), cs('c2', 'partly')],
    );
    await settle();

    expect(urls.some((u) => u.includes('/projects/p1/change-sets?status=waiting%2Cpartly'))).toBe(true);
    expect(c.querySelectorAll('[data-testid="change-set-panel"]')).toHaveLength(2);
    expect(c.querySelector('[data-testid="change-set-reason"]')!.textContent).toBe('Reason c1');
    expect(c.querySelector('[aria-pressed="true"]')!.textContent).toBe('Waiting for you');

    [...c.querySelectorAll('button')].find((b) => b.textContent === 'Applied')!.click();
    await settle();
    expect(urls.some((u) => u.includes('status=applied'))).toBe(true);
    // A decided change set folds into one line; its panel is inside.
    const folded = c.querySelector('details')!;
    expect(folded.querySelector('summary')!.textContent).toContain('Applied');
    expect(folded.querySelector('summary')!.textContent).toContain('Reason c3');
    expect(folded.querySelector('[data-testid="change-set-panel"]')).not.toBeNull();
  });

  it('says why when agents may only read and there is nothing to list, and says the list is empty', async () => {
    const { container: c } = mount('read', () => []);
    await settle();
    expect(c.querySelector('[data-testid="change-sets-off"]')!.textContent).toContain('Agents may only read in this project');
    expect(c.querySelector('[data-testid="change-sets-empty"]')!.textContent).toBe('No change set waits for you.');
  });

  it('opens on all when the inbox names a change set, and keeps that one unfolded', async () => {
    const { container: c, urls } = mount('propose', () => [cs('c3', 'applied')], '?open=c3');
    await settle();
    expect(urls.some((u) => u.includes('/change-sets') && !u.includes('status='))).toBe(true);
    expect(c.querySelector('details')).toBeNull();
    expect(c.querySelector('[data-testid="change-set-panel"]')).not.toBeNull();
  });

  it('says a failed load as one, with a way to try again', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((async () =>
      new Response(JSON.stringify({ error: { message: 'down' } }), { status: 500, headers: { 'Content-Type': 'application/json' } })) as typeof fetch);
    const value = { projectId: () => 'p1', project: (() => null) as unknown as Resource<null>, workflow: () => undefined, vocabulary: () => ({}), refetch: () => {} } as unknown as ProjectContextValue;
    const history = createMemoryHistory();
    history.set({ value: '/projects/p1/change-sets' });
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(
      () => (
        <ProjectContext.Provider value={value}>
          <MemoryRouter history={history}>
            <Route path="/projects/:id/change-sets" component={ChangeSetsPage} />
          </MemoryRouter>
        </ProjectContext.Provider>
      ),
      container,
    );
    disposers.push(() => { dispose(); container.remove(); });
    await settle();
    expect(container.querySelector('[data-testid="load-error"]')!.textContent).toContain('Could not load the change sets');
    expect(container.querySelector('[data-testid="change-sets-empty"]')).toBeNull();
  });
});
