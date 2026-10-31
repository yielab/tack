import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import AgentProfilesPanel from './AgentProfilesPanel';
import { flush } from '../../../test/flush';

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <AgentProfilesPanel />, container);
  return { container, dispose };
}

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('AgentProfilesPanel', () => {
  it('shows an empty state when no profiles exist', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }),
    );
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('No agent profiles yet');
  });

  it('lists a profile with its instructions', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          protocol_version: 1,
          data: [
            {
              agent_profile_id: 'ap_1',
              name: 'reviewer',
              instructions: 'Review the diff.',
              tool_policy: {},
              limits: {},
            },
          ],
        }),
        { status: 200 },
      ),
    );
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('reviewer');
    expect(container.textContent).toContain('Review the diff.');
  });

  it('the_form_builds_the_tool_policy', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const url = String(input);
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (url.endsWith('/api/agent-profiles') && method === 'GET') {
        return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }));
      }
      if (url.endsWith('/api/agent-profiles') && method === 'POST') {
        return Promise.resolve(
          new Response(JSON.stringify({ protocol_version: 1, agent_profile_id: 'ap_new', name: 'builder' }), {
            status: 200,
          }),
        );
      }
      return Promise.resolve(new Response('{}', { status: 200 }));
    });

    const { container } = mount();
    await flush();
    Array.from(container.querySelectorAll('button'))
      .find((b) => b.textContent?.includes('Create agent profile'))!
      .click();
    await flush();

    const type = (el: HTMLInputElement | HTMLTextAreaElement, v: string) => {
      el.value = v;
      el.dispatchEvent(new Event('input', { bubbles: true }));
    };
    type(container.querySelector<HTMLInputElement>('input[placeholder="reviewer"]')!, 'builder');
    type(
      container.querySelector<HTMLTextAreaElement>('textarea[placeholder="Review the diff for correctness and style."]')!,
      'Build the feature.',
    );
    // No raw JSON input anywhere.
    expect(container.querySelector('input[placeholder="{}"]')).toBeNull();
    expect(container.textContent).toContain("Shown as the profile's tooltip");

    // claude-code: untick "All tools", tick Read and Grep, add a free-text name.
    const all = container.querySelector<HTMLInputElement>('input[aria-label="All tools (claude-code)"]')!;
    all.click();
    await flush();
    container.querySelector<HTMLInputElement>('input[aria-label="Read (claude-code)"]')!.click();
    container.querySelector<HTMLInputElement>('input[aria-label="Grep (claude-code)"]')!.click();
    type(container.querySelector<HTMLInputElement>('input[aria-label="Other tools (claude-code)"]')!, 'mcp__x__y');
    await flush();

    Array.from(container.querySelectorAll('button[type="submit"]'))
      .find((b) => b.textContent?.includes('Create'))!
      .click();
    await flush();

    const postCall = fetchMock.mock.calls.find(
      (c) => String(c[0]).endsWith('/api/agent-profiles') && (c[1] as RequestInit)?.method === 'POST',
    );
    const body = JSON.parse((postCall![1] as RequestInit).body as string);
    expect(body).toEqual({
      name: 'builder',
      instructions: 'Build the feature.',
      tool_policy: {
        tools: {
          'claude-code': ['Read', 'Grep', 'mcp__x__y'],
          codex: ['*'],
          opencode: ['*'],
          docket: ['*'],
        },
      },
    });
  });

  it('the_network_box_reads_and_writes_tool_policy_network_and_leaves_the_other_keys', async () => {
    const profile = { agent_profile_id: 'ap_r', name: 'researcher', instructions: 'Read sources.', tool_policy: { agent: 'docket', network: true }, limits: {} };
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      if (method === 'GET') return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [profile] }), { status: 200 }));
      return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, agent_profile_id: 'ap_new', name: 'x' }), { status: 200 }));
    });
    const { container } = mount();
    await flush();
    const box = () => container.querySelector<HTMLInputElement>('input[type="checkbox"]:not([aria-label])')!;
    const send = (label: string) => Array.from(container.querySelectorAll('button')).find((b) => b.textContent?.trim() === label)!.click();
    const body = (method: string) => JSON.parse((fetchMock.mock.calls.find((c) => (c[1] as RequestInit)?.method === method)![1] as RequestInit).body as string);
    send('Edit');
    await flush();
    expect(container.textContent).toContain('This role needs the network (web search and reading sources)');
    expect(box().checked).toBe(true);
    box().click();
    await flush();
    container.querySelector<HTMLButtonElement>('button[type="submit"]')!.click();
    await flush();
    // Unticked: the key is gone (absent is off); the agent the profile names stays.
    expect(body('PATCH').tool_policy.network).toBeUndefined();
    expect(body('PATCH').tool_policy.agent).toBe('docket');
    // A new profile starts unticked, and ticking it writes true.
    send('+ Create agent profile');
    await flush();
    expect(box().checked).toBe(false);
    box().click();
    const type = (el: HTMLInputElement | HTMLTextAreaElement, v: string) => { el.value = v; el.dispatchEvent(new Event('input', { bubbles: true })); };
    type(container.querySelector<HTMLInputElement>('input[placeholder="reviewer"]')!, 'scout');
    type(container.querySelector<HTMLTextAreaElement>('textarea')!, 'Find sources.');
    await flush();
    container.querySelector<HTMLButtonElement>('button[type="submit"]')!.click();
    await flush();
    expect(body('POST').tool_policy.network).toBe(true);
  });

  it('shows what a profile delivers and can do, and the form sends the delivers line and the team', async () => {
    const profiles = [
      { agent_profile_id: 'ap_a', name: 'writer', instructions: 'Write.', tool_policy: { agent: 'claude-code', docket: { team: 'saas-content', denied_tools: ['x'] } }, limits: {}, delivers: 'a document in a folder', changes_files: true, runs_commands: false, uses_network: false },
      { agent_profile_id: 'ap_b', name: 'scout', instructions: 'Look.', tool_policy: {}, limits: {} },
    ];
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((_input, init) => {
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      const body = method === 'GET' ? { protocol_version: 1, data: profiles } : {};
      return Promise.resolve(new Response(JSON.stringify(body), { status: 200 }));
    });
    const { container } = mount();
    await flush();
    const rows = container.querySelectorAll('li');
    expect(rows[0].querySelector('[data-testid="profile-delivers"]')!.textContent).toBe('a document in a folder · on claude-code');
    expect(rows[0].querySelector('[data-testid="cap-files"]')!.getAttribute('data-can')).toBe('yes');
    // A capability the profile lacks is shown, dimmed, never hidden.
    expect(rows[0].querySelector('[data-testid="cap-commands"]')!.textContent).toBe('Runs no commands');
    expect(rows[1].querySelector('[data-testid="profile-delivers"]')!.textContent).toContain('on any agent');

    Array.from(rows[1].querySelectorAll('button')).find((b) => b.textContent === 'Edit')!.click();
    await flush();
    const type = (el: HTMLInputElement, v: string) => { el.value = v; el.dispatchEvent(new Event('input', { bubbles: true })); };
    type(container.querySelector<HTMLInputElement>('input[placeholder="a change on a branch"]')!, 'an answer');
    const select = container.querySelector<HTMLSelectElement>('select')!;
    expect(Array.from(select.options).map((o) => o.textContent)).toEqual(['No team', 'saas-content']);
    select.value = 'saas-content';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    container.querySelector<HTMLButtonElement>('button[type="submit"]')!.click();
    await flush();
    const patch = fetchMock.mock.calls.find((c) => (c[1] as RequestInit)?.method === 'PATCH')!;
    const sent = JSON.parse((patch[1] as RequestInit).body as string);
    expect(sent.delivers).toBe('an answer');
    expect(sent.tool_policy.docket).toEqual({ team: 'saas-content' });
  });

  it('shows the Built-in chip and no Delete for a built-in, Delete for a custom profile', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          protocol_version: 1,
          data: [
            { agent_profile_id: 'ap_b', name: 'Implementer', instructions: 'x', tool_policy: {}, limits: {}, kind: 'implementer', builtin: true, summary: 's' },
            { agent_profile_id: 'ap_c', name: 'mine', instructions: 'y', tool_policy: {}, limits: {}, kind: 'custom', builtin: false, summary: '' },
          ],
        }),
        { status: 200 },
      ),
    );
    const { container } = mount();
    await flush();
    const items = container.querySelectorAll('li');
    expect(items[0].textContent).toContain('Built-in');
    expect(items[0].textContent).not.toContain('Delete');
    expect(items[1].textContent).not.toContain('Built-in');
    expect(items[1].textContent).toContain('Delete');
  });
});
