import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route, createMemoryHistory } from '@solidjs/router';
import ThreadsPage from './ThreadsPage';
import { flush } from '../../test/flush';

const disposers: Array<() => void> = [];
const settle = async () => {
  for (let i = 0; i < 8; i += 1) await flush();
};

function mount(threads: unknown[], profiles: unknown[], search = '') {
  const calls: Array<{ url: string; method: string; body?: string }> = [];
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? 'GET';
    calls.push({ url, method, body: init?.body as string | undefined });
    const json = (body: unknown) => new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } });
    if (method === 'POST' && url.endsWith('/threads')) return json({ id: 'new1', title: 'T', profile: 'prof1', scope: { kind: 'project', project: 'p1' }, status: 'open', turns: 0, created_at: 'x', last_message_at: 'x' });
    if (url.includes('/threads/')) return json({ thread: threads[0], messages: [], turns: [] });
    if (url.includes('/threads')) return json(threads);
    if (url.includes('/agent-profiles')) return json({ protocol_version: 1, data: profiles });
    return json({});
  }) as typeof fetch);
  const history = createMemoryHistory();
  history.set({ value: `/projects/p1/threads${search}` });
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <MemoryRouter history={history}>
        <Route path="/projects/:id/threads" component={ThreadsPage} />
      </MemoryRouter>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return { c: container, calls };
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  vi.restoreAllMocks();
});

const q = (c: HTMLElement, id: string) => c.querySelector<HTMLElement>(`[data-testid="${id}"]`);
const profile = { agent_profile_id: 'prof1', name: 'Planner', instructions: '', tool_policy: {}, limits: {} };
const aThread = { id: 'th1', title: 'Weekly check', profile: 'prof1', scope: { kind: 'project', project: 'p1' }, status: 'open', turns: 2, created_at: 'x', last_message_at: new Date().toISOString() };

describe('ThreadsPage', () => {
  it('says there are no threads and offers to start one', async () => {
    const { c } = mount([], [profile]);
    await settle();
    expect(q(c, 'threads-empty')?.textContent).toContain('no threads in this project yet');
    expect(q(c, 'new-thread-open')).not.toBeNull();
  });

  it('lists the threads with the profile that answers and its state', async () => {
    const { c } = mount([aThread], [profile]);
    await settle();
    const item = q(c, 'thread-item')!;
    expect(item.textContent).toContain('Weekly check');
    expect(item.textContent).toContain('Planner');
    expect(item.textContent).toContain('Open');
    expect(q(c, 'thread-none-open')).not.toBeNull();
  });

  it('opens the thread named in the address', async () => {
    const { c } = mount([aThread], [profile], '?open=th1');
    await settle();
    expect(q(c, 'thread-panel')).not.toBeNull();
    expect(q(c, 'thread-title')?.textContent).toBe('Weekly check');
  });

  it('starts a thread in the project with the chosen profile', async () => {
    const { c, calls } = mount([], [profile]);
    await settle();
    q(c, 'new-thread-open')!.click();
    await settle();
    const start = q(c, 'new-thread-start') as HTMLButtonElement;
    expect(start.disabled).toBe(true);
    const title = c.querySelector<HTMLInputElement>('[data-testid="new-thread"] input')!;
    title.value = 'Weekly check';
    title.dispatchEvent(new InputEvent('input', { bubbles: true }));
    await settle();
    expect(start.disabled).toBe(false);
    start.click();
    await settle();
    const post = calls.find((x) => x.method === 'POST')!;
    expect(JSON.parse(post.body!)).toEqual({ project_id: 'p1', profile: 'prof1', title: 'Weekly check' });
  });

  it('keeps the form disabled, saying why, when no profile exists', async () => {
    const { c } = mount([], []);
    await settle();
    q(c, 'new-thread-open')!.click();
    await settle();
    expect((q(c, 'new-thread-start') as HTMLButtonElement).disabled).toBe(true);
    expect(q(c, 'new-thread')?.textContent).toContain('There is no agent profile yet');
  });
});
