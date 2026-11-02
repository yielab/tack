import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import AddManager, { addManagerReason } from './AddManager';
import { flush } from '../../test/flush';

const disposers: Array<() => void> = [];
const settle = async () => {
  for (let i = 0; i < 10; i += 1) await flush();
};

function mount(profiles: unknown[]) {
  const writes: Array<{ url: string; body: Record<string, unknown> }> = [];
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input).replace(/^.*\/api/, '');
    const method = init?.method ?? 'GET';
    const body = init?.body ? (JSON.parse(String(init.body)) as Record<string, unknown>) : {};
    const json = (value: unknown) => new Response(JSON.stringify(value), { status: 200, headers: { 'Content-Type': 'application/json' } });
    if (method !== 'GET') writes.push({ url, body });
    if (url === '/projects') return json([{ id: 'p1', name: 'My SaaS' }]);
    if (url === '/projects/p1') return json({ id: 'p1', agents_on_board: { level: 'propose', creates_per_change_set: 0, creates_per_day: 0 } });
    if (url === '/agent-profiles' && method === 'GET') return json({ protocol_version: 1, data: profiles });
    if (url === '/agent-profiles') return json({ protocol_version: 1, agent_profile_id: 'prof-new', name: body.name });
    if (url === '/agent-credentials') return json({ secret: 'tack_agent_abc', credential: { id: 'cred1', name: body.name } });
    if (url === '/threads') return json({ id: 'th1', title: body.title });
    if (url === '/triggers') return json({ id: `tr-${writes.length}`, name: body.name, ...body, scope: { kind: 'credential', credential: 'cred1', home: 'p1' }, enabled: true });
    return json({});
  }) as typeof fetch);
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <MemoryRouter>
        <Route path="/" component={AddManager} />
      </MemoryRouter>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return { c: container, writes };
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  vi.restoreAllMocks();
});

const q = (c: HTMLElement, id: string) => c.querySelector<HTMLElement>(`[data-testid="${id}"]`);

describe('Add a manager', () => {
  it('adds the profile, the credential, the thread and three triggers from one button, and shows the secret once', async () => {
    const { c, writes } = mount([]);
    await settle();
    // With no profile yet the form offers to make one.
    expect(c.textContent).toContain('Make a new profile for it');
    (q(c, 'manager-add') as HTMLButtonElement).click();
    await settle();
    expect(writes.map((w) => w.url)).toEqual(['/agent-profiles', '/agent-credentials', '/threads', '/triggers', '/triggers', '/triggers']);
    expect(q(c, 'manager-made-profile')?.textContent).toContain('Manager');
    expect(q(c, 'manager-made-credential')?.textContent).toContain('read, propose and converse');
    expect(q(c, 'manager-made-thread')?.querySelector('a')?.getAttribute('href')).toBe('/projects/p1/threads?open=th1');
    expect(c.querySelectorAll('[data-testid="manager-made-trigger"]')).toHaveLength(3);
    expect(q(c, 'manager-rung')?.textContent).toContain('Propose');
    const mcp = q(c, 'manager-mcp')!;
    expect(mcp.textContent).toContain('"command": "tack"');
    expect(mcp.textContent).toContain('tack_agent_abc');
    // After "I have copied it" the secret is gone from the page.
    [...mcp.querySelectorAll('button')].find((b) => b.textContent === 'I have copied it')!.click();
    await settle();
    expect(c.textContent).not.toContain('tack_agent_abc');
  });

  it('uses a profile that exists instead of making one', async () => {
    const { c, writes } = mount([{ agent_profile_id: 'prof1', name: 'Planner', instructions: '', tool_policy: {}, limits: {} }]);
    await settle();
    expect(c.textContent).not.toContain('Name');
    (q(c, 'manager-add') as HTMLButtonElement).click();
    await settle();
    expect(writes.some((w) => w.url === '/agent-profiles')).toBe(false);
    expect(writes.find((w) => w.url === '/threads')!.body.profile).toBe('prof1');
  });

  it('says what stopped it and keeps what was made', async () => {
    const { c } = mount([]);
    const fetchMock = vi.mocked(globalThis.fetch);
    const original = fetchMock.getMockImplementation()!;
    fetchMock.mockImplementation(((input: RequestInfo | URL, init?: RequestInit) =>
      String(input).endsWith('/threads')
        ? Promise.resolve(new Response(JSON.stringify({ error: { message: 'No room' } }), { status: 400, headers: { 'Content-Type': 'application/json' } }))
        : original(input, init)) as typeof fetch);
    await settle();
    (q(c, 'manager-add') as HTMLButtonElement).click();
    await settle();
    expect(q(c, 'manager-failure')?.textContent).toContain('Stopped at the thread');
    expect(q(c, 'manager-mcp')?.textContent).toContain('tack_agent_abc');
  });

  it('names why it cannot add yet', () => {
    expect(addManagerReason({ project: '', profile: 'x', name: 'M' })).toContain('project');
    expect(addManagerReason({ project: 'p', profile: '__new__', name: ' ' })).toContain('name');
    expect(addManagerReason({ project: 'p', profile: 'x', name: '' })).toBeUndefined();
  });
});
