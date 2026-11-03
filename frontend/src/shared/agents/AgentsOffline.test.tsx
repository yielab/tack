import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import AgentsOffline from './AgentsOffline';

const flush = () => new Promise((r) => setTimeout(r, 0));
const disposers: Array<() => void> = [];

const runner = (name: string, extra: Record<string, unknown>) => ({
  runner_id: `runr-${name}`, name, state: 'active', online: true, last_heartbeat_at: '2026-11-01T18:17:13Z', fleet_ids: [], ...extra,
});

async function mount(runners: unknown[] | 'failing') {
  vi.spyOn(globalThis, 'fetch').mockImplementation((async () =>
    runners === 'failing'
      ? new Response(JSON.stringify({ error: { status: 500, message: 'boom' } }), { status: 500 })
      : new Response(JSON.stringify({ protocol_version: 1, data: runners }), { status: 200 })) as typeof fetch);
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <MemoryRouter><Route path="/" component={() => <AgentsOffline />} /></MemoryRouter>, container);
  disposers.push(() => { dispose(); container.remove(); });
  await flush();
  await flush();
}

const notice = () => document.querySelector<HTMLAnchorElement>('[data-testid="agents-offline"]');

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  vi.restoreAllMocks();
});

describe('AgentsOffline', () => {
  it('says agents are offline when every computer is silent, which since when, and links to turning them on', async () => {
    await mount([runner('This computer (Pet1)', { online: false }), runner('old', { state: 'revoked', online: false })]);
    expect(notice()!.textContent).toContain('Agents offline');
    expect(notice()!.textContent).toMatch(/This computer \(Pet1\) has not answered since .+, so no run can start\./);
    expect(notice()!.textContent).not.toContain('old');
    expect(notice()!.getAttribute('href')).toBe('/agents');
    expect(notice()!.getAttribute('role')).toBe('status');
  });

  it('says nothing while one computer answers', async () => {
    await mount([runner('This computer (Pet1)', { online: false }), runner('Build box', { online: true })]);
    expect(notice()).toBeNull();
  });

  it('leaves a workspace with no computer to the first-run banner, and says nothing when the list cannot be read', async () => {
    await mount([]);
    expect(notice()).toBeNull();
    while (disposers.length) disposers.pop()!();
    vi.restoreAllMocks();
    await mount('failing');
    expect(notice()).toBeNull();
  });
});
