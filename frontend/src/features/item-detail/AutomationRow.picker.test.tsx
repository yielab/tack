import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import AutomationRow from './AutomationRow';
import { flush } from '../../test/flush';
import type { Item } from '../../shared/types';

const disposers: Array<() => void> = [];

const PROFILES = [
  { agent_profile_id: 'profile-1', name: 'Content writer', instructions: 'x', tool_policy: { agent: 'codex' }, limits: null },
  { agent_profile_id: 'profile-2', name: 'Fixer', instructions: 'x', tool_policy: { agent: 'claude-code' }, limits: null },
];
const PROJECT = { id: 'p1', auto_start_enabled: true, auto_start_status: 'To Do', default_profile_id: null };

function routeFetch(detail: { roles: unknown[]; run_settings: unknown }, patches: unknown[], runners: unknown[] = [{ runner_id: 'r1', state: 'active', online: true }]) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const json = (body: unknown, headers: Record<string, string> = {}) =>
      Promise.resolve(new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json', ...headers } }));
    if (init?.method === 'PATCH') {
      patches.push(JSON.parse(String(init.body)));
      return json({ id: 'item-1' }, { ETag: '"2"' });
    }
    if (url.includes('/agent-profiles')) return json({ protocol_version: 1, data: PROFILES });
    if (url.includes('/runners')) return json({ protocol_version: 1, data: runners });
    if (url.includes('/teams')) return json({ teams: [] });
    if (url.endsWith('/autonomy')) return json({ level: 'L3' });
    if (url.includes('/projects/')) return json(PROJECT);
    if (url.endsWith('/items/item-1')) return json({ item: { id: 'item-1', run_settings: detail.run_settings }, roles: detail.roles }, { ETag: '"1"' });
    return json({});
  });
}

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <AutomationRow item={{ id: 'item-1', project_id: 'p1' } as Item} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

async function settle() {
  await flush();
  await flush();
  await flush();
}

afterEach(() => {
  localStorage.clear();
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('AutomationRow: who does it', () => {
  it('says the choice was made on this task', async () => {
    routeFetch({ roles: [], run_settings: { agent_profile_id: 'profile-1' } }, []);
    const row = mount();
    await settle();
    expect(row.querySelector('[data-testid="automation-row"]')?.textContent).toContain('Goes to Content writer (chosen on this task)');
  });

  it('says the choice came from the role', async () => {
    routeFetch({ roles: [{ name: 'Content', agent_profile_id: 'profile-2' }], run_settings: null }, []);
    const row = mount();
    await settle();
    expect(row.querySelector('[data-testid="automation-row"]')?.textContent).toContain('Goes to Fixer (from the role Content)');
  });

  it('says why a task cannot start by itself when two computers answer and its profile names no group', async () => {
    routeFetch({ roles: [], run_settings: { agent_profile_id: 'profile-1' } }, [], [
      { runner_id: 'r1', state: 'active', online: true },
      { runner_id: 'r2', state: 'active', online: true },
    ]);
    const row = mount();
    await settle();
    const text = row.querySelector('[data-testid="automation-row"]')?.textContent ?? '';
    expect(text).toContain('By hand: it would start when it reaches To Do, but 2 computers could run it and its profile names no group of computers to choose from');
    expect(text).toContain('Goes to Content writer (chosen on this task)');
  });

  it('choosing a profile stores it in the run settings and keeps the other settings; Nobody yet clears it', async () => {
    const patches: unknown[] = [];
    routeFetch({ roles: [], run_settings: { timeout_seconds: 900 } }, patches);
    const row = mount();
    await settle();
    const radio = (name: string) => {
      const label = [...row.querySelectorAll<HTMLLabelElement>('[data-testid="who-does-it"] label')].find((l) => l.textContent === name)!;
      return document.getElementById(label.htmlFor) as HTMLInputElement;
    };
    radio('Fixer').click();
    await settle();
    expect(patches).toEqual([{ run_settings: { timeout_seconds: 900, agent_profile_id: 'profile-2' } }]);
  });

  it('Nobody yet removes the profile from the run settings', async () => {
    const patches: unknown[] = [];
    routeFetch({ roles: [], run_settings: { agent_profile_id: 'profile-1' } }, patches);
    const row = mount();
    await settle();
    const label = [...row.querySelectorAll<HTMLLabelElement>('[data-testid="who-does-it"] label')].find((l) => l.textContent === 'Nobody yet')!;
    (document.getElementById(label.htmlFor) as HTMLInputElement).click();
    await settle();
    expect(patches).toEqual([{ run_settings: null }]);
  });

  it('is folded when the task has a profile to go to: the header says the line, and Who does it is inside', async () => {
    routeFetch({ roles: [], run_settings: { agent_profile_id: 'profile-1' } }, []);
    const row = mount();
    await settle();
    const head = row.querySelector<HTMLButtonElement>('[data-testid="automation-row"] > button')!;
    expect(head.getAttribute('aria-expanded')).toBe('false');
    expect(head.textContent).toContain('Automation');
    expect(head.textContent).toContain('Goes to Content writer (chosen on this task)');
    expect(head.querySelector('[data-testid="fold-needs"]')).toBeNull();
    head.click();
    expect(head.getAttribute('aria-expanded')).toBe('true');
    expect(row.querySelector('[data-testid="who-does-it"]')!.closest('[hidden]')).toBeNull();
  });

  it('says in its header, and opens, when a role has no profile and a person has to pick one', async () => {
    routeFetch({ roles: [{ name: 'Content' }], run_settings: null }, []);
    const row = mount();
    await settle();
    const head = row.querySelector<HTMLButtonElement>('[data-testid="automation-row"] > button')!;
    expect(head.getAttribute('aria-expanded')).toBe('true');
    expect(row.querySelector('[data-testid="who-does-it"]')!.closest('[hidden]')).toBeNull();
    head.click();
    expect(head.textContent).toContain('needs you: the role Content has no profile');
  });
});
