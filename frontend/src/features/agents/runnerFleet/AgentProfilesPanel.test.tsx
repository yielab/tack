import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import AgentProfilesPanel from './AgentProfilesPanel';

const flush = () => new Promise((r) => setTimeout(r, 0));

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
