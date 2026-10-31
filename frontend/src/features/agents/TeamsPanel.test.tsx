import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import TeamsPanel from './TeamsPanel';
import { profileEditRequest, requestProfileEdit } from './profileEditRequest';
import type { RunnerSummary } from '../../shared/execution/api';
import { flush } from '../../test/flush';

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
  requestProfileEdit(null);
});

const FULL_TEAM = {
  name: 'saas-code',
  harness: 'docket',
  reported: true,
  runner: 'This computer',
  reported_at: '2026-10-31T10:00:00Z',
  profiles: [{ id: 'ap_i', name: 'Implementer' }],
  report: {
    name: 'saas-code',
    profile: 'saas',
    shape: 'build',
    works_in: 'repository',
    members: [
      { role: 'Lead', model: 'opus', can_propose: true, writes_project: false },
      { role: 'Reviewer', model: 'sonnet' },
    ],
    decides: 'asks_a_person',
    violations: [{ requirement: 'A tool needs approval', fix: 'Allow it in the team settings' }],
    network: 'limited',
    queue: { waiting: 3, running: 1 },
    records: [{ name: 'leads', description: 'People to write to', rows: 2 }],
    steps: [
      { name: 'write', kind: 'agent', role: 'Lead', checked_by: 'reviewer', sends_back_to: 'write', sends_back_max: 2 },
      { name: 'ship', kind: 'command', checked_by: 'command' },
    ],
  },
};

const EMPTY_TEAM = {
  name: 'bare',
  harness: 'docket',
  reported: true,
  runner: 'This computer',
  reported_at: '2026-10-31T10:00:00Z',
  profiles: [],
  report: { name: 'bare', profile: 'bare' },
};

const PROFILES = [
  { agent_profile_id: 'ap_i', name: 'Implementer', instructions: 'Do it.', tool_policy: { tools: { 'claude-code': ['Read', 'Edit'] } }, limits: {}, kind: 'implementer', builtin: true },
  { agent_profile_id: 'ap_r', name: 'Reviewer', instructions: 'Look.', tool_policy: { agent: 'claude-code' }, limits: {}, kind: 'reviewer', builtin: true, summary: 'Reads and reports' },
];

const runner = (harnesses: unknown[]): RunnerSummary =>
  ({
    runner_id: 'r1', name: 'This computer', state: 'active', labels: null, labels_raw: '{}', total_capacity: 1, available_capacity: 1,
    capability_snapshot: { harnesses }, capability_snapshot_raw: '', protocol_version: 1, runner_version: '1', last_heartbeat_at: null,
    online: true, revoked_at: null, fleet_ids: [], created_at: '', updated_at: '',
  }) as RunnerSummary;

const DOCKET = {
  harness_kind: 'docket', installed_version: '', api_version: '6', probe_error: null, probed_at: '',
  features: { proposals: { support: 'supported', reason: 'through a record set' } },
};

function mockApi(teams: unknown[], profiles: unknown[] = PROFILES) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const method = (init as RequestInit | undefined)?.method ?? 'GET';
    if (url.endsWith('/api/teams')) return Promise.resolve(new Response(JSON.stringify({ teams }), { status: 200 }));
    if (url.endsWith('/api/agent-profiles') && method === 'POST') {
      return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, agent_profile_id: 'ap_new', name: 'saas-code' }), { status: 200 }));
    }
    if (url.endsWith('/api/agent-profiles')) return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: profiles }), { status: 200 }));
    return Promise.resolve(new Response('{}', { status: 200 }));
  });
}

function mount(runners: RunnerSummary[] = []) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(() => <TeamsPanel runners={runners} />, container);
  return container;
}

const card = (c: HTMLElement, name: string) => c.querySelector<HTMLElement>(`[data-testid="team-card-${name}"]`)!;
const row = (el: HTMLElement, id: string) => el.querySelector<HTMLElement>(`[data-testid="team-${id}"]`)!;

describe('TeamsPanel', () => {
  it('renders every field the agent reported', async () => {
    mockApi([FULL_TEAM]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const team = card(c, 'saas-code');
    expect(team.textContent).toContain('Run by Docket');
    expect(row(team, 'shape').textContent).toContain('build');
    expect(row(team, 'works-in').textContent).toContain('Works in this repository');
    expect(row(team, 'members').textContent).toContain('Lead · opus');
    expect(row(team, 'members').textContent).toContain('Reviewer · sonnet');
    expect(row(team, 'decides').textContent).toContain('Stops to ask a person');
    expect(row(team, 'decides').textContent).toContain('Allow it in the team settings');
    expect(row(team, 'network').textContent).toContain('Limited network');
    expect(row(team, 'queue').textContent).toContain('3 waiting, 1 running');
    expect(row(team, 'records').textContent).toContain('leads');
    expect(row(team, 'records').textContent).toContain('2 rows');
    expect(row(team, 'proposals').textContent).toContain('Can propose items');
    const steps = row(team, 'steps').querySelectorAll('li');
    expect(steps).toHaveLength(2);
    expect(steps[0].textContent).toContain('write');
    expect(steps[0].textContent).toContain('checked by a reviewer');
    expect(steps[0].textContent).toContain('sent back to write up to 2 times');
    expect(steps[1].textContent).toContain('checked by a command');
    expect(team.querySelectorAll('[data-reported="no"]')).toHaveLength(0);
  });

  it('says in plain words what a member can do when the agent reported it, and nothing when it did not', async () => {
    const team = {
      ...FULL_TEAM,
      report: {
        ...FULL_TEAM.report,
        members: [
          { role: 'Lead', model: 'opus', can_propose: true, writes_project: false },
          { role: 'Builder', model: 'sonnet', can_propose: false, writes_project: true },
          { role: 'Reviewer', model: 'sonnet' },
        ],
      },
    };
    mockApi([team]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const members = row(card(c, 'saas-code'), 'members');
    const line = (role: string) => members.querySelector(`[data-testid="team-member-${role}"]`)!.textContent;
    expect(line('Lead')).toBe('Lead · opus · Reads only · Can propose items');
    expect(line('Builder')).toBe('Builder · sonnet · Changes the project');
    expect(line('Reviewer')).toBe('Reviewer · sonnet');
    expect(members.textContent).not.toMatch(/kinds|rec__|record/i);
  });

  it('explains the limit of proposals kept for all tasks together, only for a team that reports it', async () => {
    const shared = { ...FULL_TEAM, report: { ...FULL_TEAM.report, proposals_shared_across_tasks: true } };
    const apart = { ...FULL_TEAM, name: 'apart', report: { ...FULL_TEAM.report, name: 'apart', proposals_shared_across_tasks: false } };
    const unsaid = { ...FULL_TEAM, name: 'unsaid', report: { ...FULL_TEAM.report, name: 'unsaid' } };
    mockApi([shared, apart, unsaid]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const note = card(c, 'saas-code').querySelector('[data-testid="team-proposals-shared"]')!;
    expect(note.textContent).toContain('a name used in one task cannot be used again in another');
    expect(note.textContent).toContain('recreated');
    expect(card(c, 'apart').querySelector('[data-testid="team-proposals-shared"]')).toBeNull();
    expect(card(c, 'unsaid').querySelector('[data-testid="team-proposals-shared"]')).toBeNull();
  });

  it('shows a disabled "Not reported by" line for each field the agent did not report', async () => {
    mockApi([EMPTY_TEAM]);
    const c = mount([runner([{ ...DOCKET, features: undefined }])]);
    await flush();
    await flush();
    const team = card(c, 'bare');
    for (const id of ['shape', 'works-in', 'members', 'decides', 'network', 'queue', 'records', 'steps', 'proposals']) {
      const line = row(team, id).querySelector('[data-reported="no"]')!;
      expect(line.textContent, id).toBe('Not reported by Docket');
      expect(line.getAttribute('aria-disabled')).toBe('true');
    }
  });

  it('says why a team cannot propose when the agent reports the reason', async () => {
    mockApi([FULL_TEAM]);
    const c = mount([runner([{ ...DOCKET, features: { proposals: { support: 'unsupported', reason: 'this team keeps no proposals record' } } }])]);
    await flush();
    await flush();
    const line = row(card(c, 'saas-code'), 'proposals');
    expect(line.textContent).toContain('Cannot propose items');
    expect(line.textContent).toContain('this team keeps no proposals record');
  });

  it('lists a profile with no team under its agent as One agent', async () => {
    mockApi([]);
    const c = mount([runner([{ harness_kind: 'claude-code', installed_version: '2.0', probe_error: null, probed_at: '', default_model: 'opus' }])]);
    await flush();
    await flush();
    const group = c.querySelector<HTMLElement>('[data-testid="no-team-claude-code"]')!;
    expect(group.textContent).toContain('Claude Code');
    expect(group.textContent).toContain('Reviewer');
    expect(group.textContent).toContain('One agent · opus');
    const any = c.querySelector<HTMLElement>('[data-testid="no-team-any"]')!;
    expect(any.textContent).toContain('Implementer');
    expect(any.textContent).toContain('A single agent, no team');
  });

  it('Make a profile for this team creates the profile with the Implementer tools and opens it for editing', async () => {
    const fetchMock = mockApi([EMPTY_TEAM]);
    const c = mount();
    await flush();
    await flush();
    const button = Array.from(card(c, 'bare').querySelectorAll('button')).find((b) => b.textContent === 'Make a profile for this team')!;
    button.click();
    await flush();
    await flush();
    const post = fetchMock.mock.calls.find((call) => (call[1] as RequestInit)?.method === 'POST')!;
    const sent = JSON.parse((post[1] as RequestInit).body as string);
    expect(sent.name).toBe('bare');
    expect(sent.instructions).toBe('Do it.');
    expect(sent.tool_policy.agent).toBe('docket');
    expect(sent.tool_policy.docket).toEqual({ team: 'bare' });
    expect(sent.tool_policy.tools).toEqual({ 'claude-code': ['Read', 'Edit'] });
    expect(profileEditRequest()).toBe('ap_new');
  });

  it('marks a team only profiles use as not reported by any computer, with no button', async () => {
    mockApi([{ name: 'ghost', harness: null, reported: false, profiles: [{ id: 'ap_r', name: 'Reviewer' }], report: null }]);
    const c = mount();
    await flush();
    await flush();
    const team = card(c, 'ghost');
    expect(team.textContent).toContain('Not reported by any computer');
    expect(team.textContent).toContain('Reviewer');
    expect(team.textContent).not.toContain('Make a profile');
  });
});
