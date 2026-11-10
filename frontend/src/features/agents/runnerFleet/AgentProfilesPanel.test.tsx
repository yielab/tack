import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import AgentProfilesPanel from './AgentProfilesPanel';
import { flush } from '../../../test/flush';

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <AgentProfilesPanel />, container);
  disposers.push(dispose);
  return { container, dispose };
}

// A panel left mounted would also answer a request to open a profile.
const disposers: Array<() => void> = [];

afterEach(() => {
  disposers.splice(0).forEach((d) => d());
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
    expect(rows[0].querySelector('[data-testid="profile-delivers"]')!.textContent).toBe('a document in a folder');
    expect(rows[0].querySelector('[data-testid="profile-runs"]')!.textContent).toBe('Runs with Claude Code · Team saas-content');
    expect(rows[0].querySelector('[data-testid="cap-files"]')!.getAttribute('data-can')).toBe('yes');
    // A capability the profile lacks is shown, dimmed, never hidden.
    expect(rows[0].querySelector('[data-testid="cap-commands"]')!.textContent).toBe('Runs no commands');
    expect(rows[1].querySelector('[data-testid="profile-runs"]')!.textContent).toBe('Runs with Any agent, chosen for each task · A single agent, no team');

    Array.from(rows[1].querySelectorAll('button')).find((b) => b.textContent === 'Edit')!.click();
    await flush();
    const type = (el: HTMLInputElement, v: string) => { el.value = v; el.dispatchEvent(new Event('input', { bubbles: true })); };
    type(container.querySelector<HTMLInputElement>('input[placeholder="a change on a branch"]')!, 'an answer');
    const select = container.querySelector<HTMLSelectElement>('select')!;
    expect(Array.from(select.options).map((o) => o.textContent)).toEqual(['No team', 'saas-content']);
    expect(Array.from(select.querySelectorAll('optgroup')).map((g) => g.label)).toEqual(['Not reported by any computer']);
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

  it('lets a person choose what a profile does, and a built-in keeps its own', async () => {
    const profiles = [
      { agent_profile_id: 'ap_m', name: 'scout', instructions: 'Look.', tool_policy: {}, limits: {}, kind: 'custom', builtin: false },
      { agent_profile_id: 'ap_b', name: 'Planner', instructions: 'Plan.', tool_policy: {}, limits: {}, kind: 'planner', builtin: true },
    ];
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      const body = method !== 'GET' ? {} : String(input).endsWith('/api/teams') ? { teams: [] } : { protocol_version: 1, data: profiles };
      return Promise.resolve(new Response(JSON.stringify(body), { status: 200 }));
    });
    const { container } = mount();
    await flush();
    await flush();
    const edit = (name: string) =>
      Array.from(container.querySelectorAll('li')).find((li) => li.textContent?.includes(name))!
        .querySelectorAll('button');
    Array.from(edit('scout')).find((b) => b.textContent === 'Edit')!.click();
    await flush();
    const kind = container.querySelector<HTMLSelectElement>('[data-testid="profile-kind"]')!;
    expect(kind.disabled).toBe(false);
    expect(kind.value).toBe('custom');
    expect(Array.from(kind.options).map((o) => o.value)).toEqual(['custom', 'implementer', 'reviewer', 'researcher', 'planner']);
    expect(Array.from(kind.options).find((o) => o.value === 'researcher')!.textContent).toBe('Researcher: Investigates and reports; may propose work');
    kind.value = 'researcher';
    kind.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    container.querySelector<HTMLButtonElement>('button[type="submit"]')!.click();
    await flush();
    const patch = fetchMock.mock.calls.find((c) => (c[1] as RequestInit)?.method === 'PATCH')!;
    expect(JSON.parse((patch[1] as RequestInit).body as string).kind).toBe('researcher');

    // A built-in profile shows its kind, cannot change it, and says why.
    Array.from(edit('Planner')).find((b) => b.textContent === 'Edit')!.click();
    await flush();
    const builtin = container.querySelector<HTMLSelectElement>('[data-testid="profile-kind"]')!;
    expect(builtin.disabled).toBe(true);
    expect(builtin.value).toBe('planner');
    expect(container.textContent).toContain('A built-in profile keeps what it does.');
  });

  it('has one Web access select with three one-line options and a help hint, and sends only what changed', async () => {
    const profiles = [
      { agent_profile_id: 'ap_m', name: 'scout', instructions: 'Look.', tool_policy: {}, limits: {}, kind: 'custom', builtin: false, egress_mode: 'listed' },
    ];
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
      const method = (init as RequestInit | undefined)?.method ?? 'GET';
      const body = method !== 'GET' ? {} : String(input).endsWith('/api/teams') ? { teams: [] } : { protocol_version: 1, data: profiles };
      return Promise.resolve(new Response(JSON.stringify(body), { status: 200 }));
    });
    const { container } = mount();
    await flush();
    await flush();
    Array.from(container.querySelectorAll('button')).find((b) => b.textContent === 'Edit')!.click();
    await flush();
    const selects = container.querySelectorAll('[data-testid="profile-web-access"]');
    expect(selects.length).toBe(1);
    const web = selects[0] as HTMLSelectElement;
    expect(web.value).toBe('listed');
    expect(Array.from(web.options).map((o) => [o.value, o.textContent])).toEqual([
      ['closed', 'Closed: only the model'],
      ['listed', "Listed: the model and the profile's hosts"],
      ['open', 'Open: any public site'],
    ]);
    // The help says what each mode covers and that an allowed public site can still receive data.
    const hint = container.querySelector<HTMLElement>('button[aria-label="Help: Web access"]')!;
    expect(hint.getAttribute('title')).toContain('Closed:');
    expect(hint.getAttribute('title')).toContain('Open: any public site');
    expect(hint.getAttribute('title')).toContain('can still receive what the agent sends it');

    // Saved untouched, the mode is not sent: the profile keeps its own.
    container.querySelector<HTMLButtonElement>('button[type="submit"]')!.click();
    await flush();
    const first = fetchMock.mock.calls.find((c) => (c[1] as RequestInit)?.method === 'PATCH')!;
    expect(JSON.parse((first[1] as RequestInit).body as string)).not.toHaveProperty('egress_mode');

    await flush();
    Array.from(container.querySelectorAll('button')).find((b) => b.textContent === 'Edit')!.click();
    await flush();
    const again = container.querySelector<HTMLSelectElement>('[data-testid="profile-web-access"]')!;
    again.value = 'open';
    again.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    container.querySelector<HTMLButtonElement>('button[type="submit"]')!.click();
    await flush();
    const patches = fetchMock.mock.calls.filter((c) => (c[1] as RequestInit)?.method === 'PATCH');
    expect(JSON.parse((patches[patches.length - 1][1] as RequestInit).body as string).egress_mode).toBe('open');
  });

  describe('hosts it may reach', () => {
    const profile = (egress_mode: string, tool_policy: unknown) => ({
      agent_profile_id: 'ap_h', name: 'scout', instructions: 'Look.', tool_policy, limits: {}, kind: 'custom', builtin: false, egress_mode,
    });
    const open = async (profiles: unknown[], answer: () => Response = () => new Response('{}', { status: 200 })) => {
      const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
        const method = (init as RequestInit | undefined)?.method ?? 'GET';
        if (method !== 'GET') return Promise.resolve(answer());
        const body = String(input).endsWith('/api/teams') ? { teams: [] } : { protocol_version: 1, data: profiles };
        return Promise.resolve(new Response(JSON.stringify(body), { status: 200 }));
      });
      const { container } = mount();
      await flush();
      await flush();
      Array.from(container.querySelectorAll('button')).find((b) => b.textContent === 'Edit')!.click();
      await flush();
      return { container, fetchMock };
    };
    const type = async (container: HTMLElement, text: string, key?: string) => {
      const input = container.querySelector<HTMLInputElement>('[data-testid="host-draft"]')!;
      input.value = text;
      input.dispatchEvent(new Event('input', { bubbles: true }));
      if (key) input.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
      await flush();
    };
    const sentPolicy = (fetchMock: ReturnType<typeof vi.spyOn>) => {
      const patch = fetchMock.mock.calls.filter((c) => (c[1] as RequestInit)?.method === 'PATCH').at(-1)!;
      return JSON.parse((patch[1] as RequestInit).body as string).tool_policy;
    };

    it('lists, adds and removes the hosts of a Listed profile and saves them in the tool policy', async () => {
      const { container, fetchMock } = await open([profile('listed', { allowed_hosts: ['docs.rs'], other: 1 })]);
      const rows = container.querySelector('[data-testid="host-rows"]')!;
      expect(rows.textContent).toContain('docs.rs');
      await type(container, '*.crates.io', 'Enter');
      expect(rows.textContent).toContain('*.crates.io');
      expect(container.querySelector<HTMLInputElement>('[data-testid="host-draft"]')!.value).toBe('');
      // The same host twice is one entry.
      await type(container, 'docs.rs', 'Enter');
      expect(rows.querySelectorAll('li').length).toBe(2);
      container.querySelector<HTMLButtonElement>('button[aria-label="Remove docs.rs"]')!.click();
      await flush();
      // A host typed and not added is saved too.
      await type(container, 'example.com');
      container.querySelector<HTMLButtonElement>('button[type="submit"]')!.click();
      await flush();
      const policy = sentPolicy(fetchMock);
      expect(policy.allowed_hosts).toEqual(['*.crates.io', 'example.com']);
      expect(policy.other).toBe(1);
    });

    it('leaves the key out when the list is emptied', async () => {
      const { container, fetchMock } = await open([profile('listed', { allowed_hosts: ['docs.rs'] })]);
      container.querySelector<HTMLButtonElement>('button[aria-label="Remove docs.rs"]')!.click();
      await flush();
      expect(container.querySelector('[data-testid="host-rows"]')!.textContent).toContain('reaches its model and nothing else');
      container.querySelector<HTMLButtonElement>('button[type="submit"]')!.click();
      await flush();
      expect(sentPolicy(fetchMock)).not.toHaveProperty('allowed_hosts');
    });

    it('is shown disabled, with its reason, for Closed and Open, and keeps the list', async () => {
      const { container, fetchMock } = await open([profile('listed', { allowed_hosts: ['docs.rs'] })]);
      const web = container.querySelector<HTMLSelectElement>('[data-testid="profile-web-access"]')!;
      for (const [mode, reason] of [['closed', 'reaches only its model'], ['open', 'any public site']] as const) {
        web.value = mode;
        web.dispatchEvent(new Event('change', { bubbles: true }));
        await flush();
        const note = container.querySelector('[data-testid="hosts-disabled"]')!;
        expect(note.getAttribute('aria-disabled')).toBe('true');
        expect(note.textContent).toContain(reason);
        expect(note.textContent).toContain('The 1 saved host is kept.');
        expect(container.querySelector('[data-testid="host-rows"]')).toBeNull();
      }
      container.querySelector<HTMLButtonElement>('button[type="submit"]')!.click();
      await flush();
      expect(sentPolicy(fetchMock).allowed_hosts).toEqual(['docs.rs']);
    });

    it("shows the server's refusal of a host and stays open", async () => {
      const message = 'tool_policy.allowed_hosts: "https://docs.rs" is not a host name such as example.com or *.example.com';
      const { container } = await open([profile('listed', {})], () =>
        new Response(JSON.stringify({ error: { status: 400, message, details: { field: 'tool_policy.allowed_hosts' } } }), { status: 400 }),
      );
      await type(container, 'https://docs.rs', 'Enter');
      container.querySelector<HTMLButtonElement>('button[type="submit"]')!.click();
      await flush();
      expect(container.querySelector('[data-testid="profile-refusal"]')!.textContent).toBe(`Not saved: ${message}`);
      expect(container.querySelector('[data-testid="host-rows"]')!.textContent).toContain('https://docs.rs');
    });
  });

  it('shows what a new profile of each kind starts with until the person chooses', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      const body = String(input).endsWith('/api/teams') ? { teams: [] } : { protocol_version: 1, data: [] };
      return Promise.resolve(new Response(JSON.stringify(body), { status: 200 }));
    });
    const { container } = mount();
    await flush();
    await flush();
    Array.from(container.querySelectorAll('button')).find((b) => /new profile|add profile|create/i.test(b.textContent ?? ''))!.click();
    await flush();
    const web = container.querySelector<HTMLSelectElement>('[data-testid="profile-web-access"]')!;
    const kind = container.querySelector<HTMLSelectElement>('[data-testid="profile-kind"]')!;
    expect(web.value).toBe('listed');
    for (const [k, mode] of [['researcher', 'open'], ['reviewer', 'closed'], ['planner', 'closed'], ['implementer', 'listed']] as const) {
      kind.value = k;
      kind.dispatchEvent(new Event('change', { bubbles: true }));
      await flush();
      expect(web.value, k).toBe(mode);
    }
  });

  it('says what a built-in profile is, with its description and the agent that runs it', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      const body = String(input).endsWith('/api/teams')
        ? { teams: [] }
        : { protocol_version: 1, data: [{ agent_profile_id: 'ap_b', name: 'Reviewer', instructions: 'Look.', tool_policy: {}, limits: {}, kind: 'reviewer', builtin: true, summary: 'Reads and reports; changes nothing' }] };
      return Promise.resolve(new Response(JSON.stringify(body), { status: 200 }));
    });
    const { container } = mount();
    await flush();
    await flush();
    const item = container.querySelector('li')!;
    expect(container.querySelector('[data-testid="builtin-explained"]')!.textContent).toContain('come with Tack');
    expect(item.querySelector('[data-testid="profile-summary"]')!.textContent).toBe('Reads and reports; changes nothing');
    expect(item.querySelector('[data-testid="profile-runs"]')!.textContent).toBe('Runs with Any agent, chosen for each task · A single agent, no team');
    expect(container.textContent).toContain('Each task can use a different agent');
  });

  it('offers the reported teams first, then the names only profiles use, in two groups', async () => {
    const profiles = [{ agent_profile_id: 'ap_a', name: 'scout', instructions: 'Look.', tool_policy: { docket: { team: 'old-team' } }, limits: {} }];
    const teams = [
      { name: 'old-team', harness: null, reported: false, profiles: [{ id: 'ap_a', name: 'scout' }], report: null },
      { name: 'saas-code', harness: 'docket', reported: true, profiles: [], report: { name: 'saas-code', profile: 'p' } },
    ];
    vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      const body = String(input).endsWith('/api/teams') ? { teams } : { protocol_version: 1, data: profiles };
      return Promise.resolve(new Response(JSON.stringify(body), { status: 200 }));
    });
    const { container } = mount();
    await flush();
    await flush();
    Array.from(container.querySelectorAll('button')).find((b) => b.textContent === 'Edit')!.click();
    await flush();
    const groups = Array.from(container.querySelectorAll('select optgroup'));
    expect(groups.map((g) => g.getAttribute('label'))).toEqual(['Reported by an agent', 'Not reported by any computer']);
    expect(Array.from(groups[0].querySelectorAll('option')).map((o) => o.textContent)).toEqual(['saas-code']);
    expect(Array.from(groups[1].querySelectorAll('option')).map((o) => o.textContent)).toEqual(['old-team']);
  });

  it('opens the profile a team just made for editing', async () => {
    const { requestProfileEdit } = await import('../profileEditRequest');
    const profiles = [{ agent_profile_id: 'ap_new', name: 'saas-code', instructions: 'Do it.', tool_policy: { agent: 'docket', docket: { team: 'saas-code' } }, limits: {} }];
    vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
      const body = String(input).endsWith('/api/teams') ? { teams: [] } : { protocol_version: 1, data: profiles };
      return Promise.resolve(new Response(JSON.stringify(body), { status: 200 }));
    });
    requestProfileEdit('ap_new');
    const { container } = mount();
    await flush();
    await flush();
    expect(container.querySelector<HTMLInputElement>('input[placeholder="reviewer"]')!.value).toBe('saas-code');
  });

  describe('the method for each kind of card', () => {
    const profile = { agent_profile_id: 'ap_a', name: 'builder', instructions: 'Build.', tool_policy: { agent: 'docket', docket: { team: 'saas-code', pipeline_by_type: { bug: 'tdd' } } }, limits: {} };
    const pod = (methods?: unknown) => ({
      name: 'saas-code', harness: 'docket', reported: true, profiles: [{ id: 'ap_a', name: 'builder' }],
      report: { name: 'saas-code', profile: 'attended', ...(methods ? { methods } : {}) },
    });
    const SOLO = { name: 'claude-code', harness: 'claude-code', reported: true, profiles: [], report: { name: 'claude-code', delivers: 'branch' } };

    function api(teams: unknown[], onPatch?: () => Response) {
      return vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
        const method = (init as RequestInit | undefined)?.method ?? 'GET';
        if (method === 'PATCH') return Promise.resolve(onPatch ? onPatch() : new Response('{}', { status: 200 }));
        const body = String(input).endsWith('/api/teams') ? { teams } : { protocol_version: 1, data: [profile] };
        return Promise.resolve(new Response(JSON.stringify(body), { status: 200 }));
      });
    }
    const edit = async (container: HTMLElement) => {
      await flush();
      await flush();
      Array.from(container.querySelectorAll('button')).find((b) => b.textContent === 'Edit')!.click();
      await flush();
    };
    const save = async (container: HTMLElement) => {
      container.querySelector<HTMLButtonElement>('button[type="submit"]')!.click();
      await flush();
      await flush();
    };

    it('is a select over the methods the team reports, and saves the choice', async () => {
      const fetchMock = api([pod([{ name: 'tdd', source: 'shipped', default: true, missing_roles: [] }, { name: 'research-review', source: 'shipped', default: false, missing_roles: [] }])]);
      const { container } = mount();
      await edit(container);
      const select = container.querySelector<HTMLSelectElement>('select[aria-label="Method for bug"]')!;
      expect(Array.from(select.options).map((o) => o.textContent)).toEqual(['Choose a method', 'tdd', 'research-review']);
      expect(select.value).toBe('tdd');
      select.value = 'research-review';
      select.dispatchEvent(new Event('change', { bubbles: true }));
      await flush();
      await save(container);
      const sent = JSON.parse((fetchMock.mock.calls.find((c) => (c[1] as RequestInit)?.method === 'PATCH')![1] as RequestInit).body as string);
      expect(sent.tool_policy.docket).toEqual({ team: 'saas-code', pipeline_by_type: { bug: 'research-review' } });
    });

    it('is free text while no computer reports the team\'s methods, and says so', async () => {
      api([pod()]);
      const { container } = mount();
      await edit(container);
      expect(container.querySelector('select[aria-label="Method for bug"]')).toBeNull();
      expect(container.querySelector<HTMLInputElement>('input[aria-label="Method for bug"]')!.value).toBe('tdd');
      expect(container.textContent).toContain('not checked until a run');
    });

    it('is shown disabled, with its reason, for a profile that runs a single agent', async () => {
      api([SOLO]);
      const { container } = mount();
      await edit(container);
      // A local agent is not a team a profile can name, so it is not offered as one.
      const team = container.querySelector<HTMLSelectElement>('select')!;
      expect(Array.from(team.options).map((o) => o.textContent)).toEqual(['No team', 'saas-code']);
      team.value = '';
      team.dispatchEvent(new Event('change', { bubbles: true }));
      await flush();
      const note = container.querySelector('[data-testid="methods-disabled"]')!;
      expect(note.getAttribute('aria-disabled')).toBe('true');
      expect(note.textContent).toContain('one method, the agent\'s own loop');
      expect(container.querySelector('[data-testid="method-rows"]')).toBeNull();
    });

    it('shows the refusal of a method the team does not have, and stays open', async () => {
      api([pod([{ name: 'tdd', source: 'shipped', default: true, missing_roles: [] }])], () =>
        new Response(JSON.stringify({ error: { status: 400, message: 'The team saas-code has no method named lint; it offers tdd.' } }), { status: 400 }),
      );
      const { container } = mount();
      await edit(container);
      await save(container);
      expect(container.querySelector('[data-testid="profile-refusal"]')!.textContent).toBe('Not saved: The team saas-code has no method named lint; it offers tdd.');
      expect(container.querySelector('button[type="submit"]')).not.toBeNull();
    });

    it('shows what a save deserves a second look at, and keeps the form on it', async () => {
      api([pod([{ name: 'tdd', source: 'shipped', default: true, missing_roles: ['reviewer'] }])], () =>
        new Response(JSON.stringify({ agent_profile_id: 'ap_a', warnings: ['The method tdd on the team saas-code has no member for reviewer, so a run that names it is refused until the team has the role.'] }), { status: 200 }),
      );
      const { container } = mount();
      await edit(container);
      await save(container);
      const warnings = container.querySelector('[data-testid="profile-warnings"]')!;
      expect(warnings.textContent).toContain('has no member for reviewer');
      expect(container.querySelector('[data-testid="profile-refusal"]')).toBeNull();
    });
  });
});
