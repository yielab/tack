import { describe, it, expect, vi, afterEach } from 'vitest';
import { AddManagerError, MANAGER_INSTRUCTIONS, addManager, mcpConfig, presetTriggers } from './managerSetup';

afterEach(() => vi.restoreAllMocks());

interface Call {
  url: string;
  method: string;
  body: Record<string, unknown> | null;
}

function mockApi(options: { failOn?: string } = {}) {
  const calls: Call[] = [];
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input).replace(/^.*\/api/, '');
    const method = init?.method ?? 'GET';
    const body = init?.body ? (JSON.parse(String(init.body)) as Record<string, unknown>) : null;
    calls.push({ url, method, body });
    const json = (value: unknown, status = 200) => new Response(JSON.stringify(value), { status, headers: { 'Content-Type': 'application/json' } });
    if (options.failOn && url.startsWith(options.failOn) && method !== 'GET') return json({ error: { message: 'No room' } }, 400);
    if (url === '/agent-profiles' && method === 'POST') return json({ protocol_version: 1, agent_profile_id: 'prof-new', name: body!.name });
    if (url === '/agent-profiles') return json({ protocol_version: 1, data: [{ agent_profile_id: 'prof1', name: 'Planner' }] });
    if (url === '/agent-credentials') return json({ secret: 'tack_agent_secret', credential: { id: 'cred1', name: body!.name } });
    if (url === '/threads') return json({ id: 'th1', title: body!.title });
    if (url === '/triggers') return json({ id: `tr${calls.length}`, name: body!.name });
    if (url === '/projects/p1') return json({ id: 'p1', agents_on_board: { level: 'propose', creates_per_change_set: 0, creates_per_day: 0 } });
    return json({});
  }) as typeof fetch);
  return calls;
}

const input = { projectId: 'p1', profile: { kind: 'new', name: 'Manager', agent: 'claude-code' } as const, utcOffsetMinutes: 60, apiUrl: 'http://127.0.0.1:3210' };

describe('addManager', () => {
  it('makes the profile, a credential with read, propose and converse, the thread and three triggers', async () => {
    const calls = mockApi();
    const done = await addManager(input);
    const writes = calls.filter((c) => c.method !== 'GET');
    expect(writes.map((c) => c.url)).toEqual(['/agent-profiles', '/agent-credentials', '/threads', '/triggers', '/triggers', '/triggers']);
    expect(writes[0].body).toMatchObject({ name: 'Manager', instructions: MANAGER_INSTRUCTIONS, tool_policy: { agent: 'claude-code' } });
    expect(writes[1].body).toEqual({
      name: 'Manager',
      scopes: [{ all_projects: false, projects: ['p1'], capabilities: ['read', 'propose', 'converse'] }],
    });
    expect(writes[2].body).toMatchObject({ project_id: 'p1', credential_id: 'cred1', profile: 'prof-new' });
    expect(writes.slice(3).map((c) => c.body!.name)).toEqual(['Failed runs', 'Change sets waiting', 'Weekday digest']);
    expect(writes.slice(3).every((c) => c.body!.credential_id === 'cred1' && c.body!.profile === 'prof-new')).toBe(true);
    expect(done.triggers).toHaveLength(3);
    expect(done.rung).toBe('propose');
    expect(done.mcpConfig).toContain('tack_agent_secret');
  });

  it('uses a profile that exists without making another', async () => {
    const calls = mockApi();
    const done = await addManager({ ...input, profile: { kind: 'existing', id: 'prof1' } });
    expect(calls.filter((c) => c.method === 'POST' && c.url === '/agent-profiles')).toHaveLength(0);
    expect(done.profileName).toBe('Planner');
    expect(calls.find((c) => c.url === '/agent-credentials')!.body!.name).toBe('Planner');
  });

  it('never touches the project: the rung stays where it was', async () => {
    const calls = mockApi();
    await addManager(input);
    expect(calls.some((c) => c.url === '/projects/p1' && c.method !== 'GET')).toBe(false);
  });

  it('tells what was made when a step fails, so the secret is not lost', async () => {
    mockApi({ failOn: '/threads' });
    const seen: string[] = [];
    const error = await addManager(input, (d) => seen.push(d.credential ? 'credential' : 'none')).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(AddManagerError);
    const failed = error as AddManagerError;
    expect(failed.step).toBe('thread');
    expect(failed.done.credential?.secret).toBe('tack_agent_secret');
    expect(failed.done.triggers).toHaveLength(0);
    expect(seen).toContain('credential');
  });
});

describe('the preset is made of general parts', () => {
  const text = JSON.stringify([MANAGER_INSTRUCTIONS, presetTriggers({ projectId: 'p1', credentialId: 'c', profile: 'prof', utcOffsetMinutes: 0 })]);

  it('names no project, team, person or product', () => {
    for (const word of ['My SaaS', 'docket', 'prill', 'Tack']) expect(text).not.toContain(word);
  });

  it('asks for a weekday digest, failed runs and change sets that wait', () => {
    const triggers = presetTriggers({ projectId: 'p1', credentialId: 'c', profile: 'prof', utcOffsetMinutes: 0 });
    expect(triggers.map((t) => t.source)).toEqual([
      { kind: 'event', event: 'run_failed', filter: {} },
      { kind: 'event', event: 'change_set_waiting', filter: { after_seconds: 3600 } },
      { kind: 'schedule', days: [1, 2, 3, 4, 5], minute_of_day: 540, utc_offset_minutes: 0 },
    ]);
  });

  it('writes an MCP configuration that runs `tack mcp` under the credential', () => {
    const config = JSON.parse(mcpConfig('http://x', 's'));
    expect(config.mcpServers.tack).toEqual({ command: 'tack', args: ['mcp'], env: { TACK_API_URL: 'http://x', TACK_API_TOKEN: 's' } });
  });
});
