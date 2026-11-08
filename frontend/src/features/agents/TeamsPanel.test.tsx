import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import TeamsPanel from './TeamsPanel';
import { profileEditRequest, requestProfileEdit } from './profileEditRequest';
import type { RunnerSummary } from '../../shared/execution/api';
import { flush } from '../../test/flush';
import { useToasts } from '../../shared/ui/toast';

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
    delivers: 'branch',
    methods: [
      { name: 'tdd', source: 'shipped', default: true, missing_roles: [] },
      { name: 'secure-build', source: 'shipped', default: false, missing_roles: ['security-vetter'] },
    ],
    members: [
      { role: 'Lead', model: 'opus', can_propose: true, writes_project: false },
      { role: 'Reviewer', model: 'sonnet' },
    ],
    decides: 'asks_a_person',
    violations: [{ requirement: 'A tool needs approval', fix: 'Allow it in the team settings' }],
    network: 'limited',
    queue: { waiting: 3, running: 1 },
    memory: { pending: [], accepted: [] },
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

interface Project {
  autonomy: unknown;
  roles: unknown[];
}

const PROJECT_ID = '11111111-1111-1111-1111-111111111111';

function mockApi(teams: unknown[], profiles: unknown[] = PROFILES, project?: Project, extra?: (url: string, method: string, body?: unknown) => Response | undefined) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const method = (init as RequestInit | undefined)?.method ?? 'GET';
    const answered = extra?.(url, method, (init as RequestInit | undefined)?.body);
    if (answered) return Promise.resolve(answered);
    if (project && url.endsWith('/api/projects')) return Promise.resolve(new Response(JSON.stringify([{ id: PROJECT_ID, name: 'Shop', default_profile_id: 'ap_i' }]), { status: 200 }));
    if (project && url.endsWith(`/api/projects/${PROJECT_ID}/autonomy`)) return Promise.resolve(new Response(JSON.stringify(project.autonomy), { status: 200 }));
    if (project && url.endsWith(`/api/projects/${PROJECT_ID}/roles`)) return Promise.resolve(new Response(JSON.stringify(project.roles), { status: 200 }));
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
    expect(row(team, 'members').textContent).toContain('Lead');
    expect(row(team, 'members').textContent).toContain('opus');
    expect(row(team, 'members').textContent).toContain('Reviewer');
    expect(row(team, 'members').textContent).toContain('sonnet');
    expect(row(team, 'supervision').textContent).toContain('Attended');
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

  it('calls a lights-out profile Lights-out under Supervision', async () => {
    mockApi([{ ...FULL_TEAM, report: { ...FULL_TEAM.report, profile: 'lights-out' } }]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    expect(row(card(c, 'saas-code'), 'supervision').textContent).toContain('Lights-out');
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
    const member = (role: string) => members.querySelector<HTMLElement>(`[data-testid="team-member-${role}"]`)!;
    const words = (role: string) => Array.from(member(role).querySelectorAll('[role="img"]')).map((m) => m.getAttribute('aria-label'));
    expect(words('Lead')).toEqual(['Reads only', 'Can propose items']);
    expect(words('Builder')).toEqual(['Changes the project']);
    expect(words('Reviewer')).toEqual([]);
    expect(member('Reviewer').textContent).toBe('Reviewersonnet');
    expect(members.textContent).not.toMatch(/kinds|rec__|record/i);
  });

  it('says which hosts a member reads the web from, before proposing, and the folder a folder team works in', async () => {
    const team = {
      ...FULL_TEAM,
      report: {
        ...FULL_TEAM.report,
        works_in: 'folder',
        folder: '/home/me/.docket/state/workspaces/pods/saas-code/workdir',
        members: [
          { role: 'Lead', can_propose: true, writes_project: false, reads_web: true, web_hosts: ['docs.python.org', '*.wikipedia.org'] },
          { role: 'Researcher', can_propose: false, writes_project: true, reads_web: true, web_hosts: ['*'] },
          { role: 'Writer', can_propose: false, writes_project: true, reads_web: false },
        ],
      },
    };
    mockApi([team]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const members = row(card(c, 'saas-code'), 'members');
    const words = (role: string) =>
      Array.from(members.querySelectorAll(`[data-testid="team-member-${role}"] [role="img"]`)).map((m) => m.getAttribute('aria-label'));
    expect(words('Lead')).toEqual(['Reads only', 'Reads the web: docs.python.org, *.wikipedia.org', 'Can propose items']);
    expect(words('Researcher')).toEqual(['Changes the project', 'Reads the web: any public host']);
    expect(words('Writer')).toEqual(['Changes the project']);
    expect(row(card(c, 'saas-code'), 'works-in').textContent).toContain('/home/me/.docket/state/workspaces/pods/saas-code/workdir');
  });

  it('draws each member as a row: a role chip, the model and one icon for what it reaches and one for proposing', async () => {
    const team = {
      ...FULL_TEAM,
      report: {
        ...FULL_TEAM.report,
        members: [
          { role: 'Lead', model: 'opus', can_propose: true, writes_project: false },
          { role: 'Builder', model: 'sonnet', can_propose: false, writes_project: true },
        ],
      },
    };
    mockApi([team]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const members = row(card(c, 'saas-code'), 'members');
    const lead = members.querySelector<HTMLElement>('[data-testid="team-member-Lead"]')!;
    expect(lead.querySelector('span.rounded-full')!.textContent).toBe('Lead');
    expect(lead.querySelector('.font-mono')!.textContent).toBe('opus');
    expect(lead.querySelector('[data-mark="reads"]')!.getAttribute('title')).toBe('Reads only');
    expect(lead.querySelector('[data-mark="proposes"]')!.getAttribute('aria-label')).toBe('Can propose items');
    const builder = members.querySelector<HTMLElement>('[data-testid="team-member-Builder"]')!;
    expect(builder.querySelector('[data-mark="changes"]')!.getAttribute('aria-label')).toBe('Changes the project');
    expect(builder.querySelector('[data-mark="proposes"]')).toBeNull();
    expect(builder.querySelectorAll('svg[aria-hidden="true"]').length).toBe(1);
  });

  it('draws the header with the shape as a badge and where it works as an icon with its words', async () => {
    mockApi([FULL_TEAM, { ...FULL_TEAM, name: 'folder', report: { ...FULL_TEAM.report, name: 'folder', works_in: 'folder' } }]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const team = card(c, 'saas-code');
    const shape = row(team, 'shape').querySelector('span.rounded-full')!;
    expect(shape.textContent).toBe('build');
    const worksIn = row(team, 'works-in');
    expect(worksIn.querySelector('svg[aria-hidden="true"]')).not.toBeNull();
    expect(worksIn.textContent).toBe('Works in this repository');
    expect(row(card(c, 'folder'), 'works-in').textContent).toBe('Works in a folder');
  });

  it('draws the steps as a pipeline of nodes, with a step that waits as a node of its own', async () => {
    const team = {
      ...FULL_TEAM,
      report: {
        ...FULL_TEAM.report,
        steps: [
          { name: 'write', kind: 'agent', role: 'Lead', checked_by: 'reviewer', sends_back_to: 'write', sends_back_max: 2 },
          { name: 'hold', kind: 'wait' },
          { name: 'ship', kind: 'command', checked_by: 'command' },
        ],
      },
    };
    mockApi([team]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const pipeline = row(card(c, 'saas-code'), 'steps').querySelector('ol')!;
    expect(pipeline.getAttribute('aria-label')).toBe("The team's steps");
    const nodes = Array.from(pipeline.querySelectorAll<HTMLElement>(':scope > li'));
    expect(nodes.map((n) => n.dataset.kind)).toEqual(['agent', 'wait', 'command']);
    expect(nodes[0].textContent).toContain('write');
    expect(nodes[0].textContent).toContain('sent back to write up to 2 times');
    const hold = nodes[1];
    expect(hold.querySelector('.font-semibold')!.textContent).toBe('hold');
    expect(hold.textContent).toBe('holdwaits');
    expect((hold.firstElementChild as HTMLElement).style.border).toContain('dashed');
    expect((nodes[0].firstElementChild as HTMLElement).style.border).toContain('solid');
    // An arrow joins each node to the next, and none follows the last.
    expect(nodes.map((n) => n.querySelectorAll('svg').length)).toEqual([1, 1, 0]);
  });

  it('shows work now as two counters with the sentence for a screen reader, and each record set with its row count', async () => {
    const team = {
      ...FULL_TEAM,
      report: { ...FULL_TEAM.report, records: [{ name: 'leads', rows: 2, description: 'People to write to' }, { name: 'listings', rows: 1 }] },
    };
    mockApi([team]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const queue = row(card(c, 'saas-code'), 'queue');
    expect(queue.querySelector('.sr-only')!.textContent).toBe('3 waiting, 1 running');
    const counters = Array.from(queue.querySelectorAll('[aria-hidden="true"] > span')).map((n) => n.textContent);
    expect(counters).toEqual(['3waiting', '1running']);
    const sets = row(card(c, 'saas-code'), 'records').querySelectorAll('li');
    expect(sets).toHaveLength(2);
    expect(sets[0].querySelector('span.rounded-full')!.textContent).toBe('2 rows');
    expect(sets[0].textContent).toContain('leads');
    expect(sets[0].textContent).toContain('People to write to');
    expect(sets[1].querySelector('span.rounded-full')!.textContent).toBe('1 row');
  });

  it('lists the profiles that use a team as links that open each one for editing', async () => {
    const team = { ...FULL_TEAM, profiles: [{ id: 'ap_i', name: 'Implementer' }, { id: 'ap_r', name: 'Reviewer' }] };
    mockApi([team]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const links = Array.from(card(c, 'saas-code').querySelectorAll<HTMLButtonElement>('[data-testid="team-profiles"] button'));
    expect(links.map((l) => l.textContent)).toEqual(['Implementer', 'Reviewer']);
    links[1].click();
    expect(profileEditRequest()).toBe('ap_r');
  });

  it('says which profile is refused on a team, what it lacks and the change in docket that ends it', async () => {
    const team = {
      ...FULL_TEAM,
      profiles: [
        { id: 'ap_p', name: 'Planner', misfits: ['no_web', 'no_proposals'] },
        { id: 'ap_i', name: 'Implementer', misfits: [] },
      ],
    };
    mockApi([team, { ...FULL_TEAM, name: 'fits' }]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const lines = Array.from(card(c, 'saas-code').querySelectorAll('[data-testid="team-misfit"]')).map((l) => l.textContent);
    expect(lines).toHaveLength(2);
    expect(lines[0]).toContain('Planner is refused here:');
    expect(lines[0]).toContain('docket pod set network open --widen --pod saas-code');
    expect(lines[1]).toContain('no member of this team can propose');
    expect(card(c, 'fits').querySelector('[data-testid="team-misfit"]')).toBeNull();
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

  it('says a team is paused, why, and how to resume it, only for a team that reports it', async () => {
    const budget = { ...FULL_TEAM, report: { ...FULL_TEAM.report, paused: 'budget' } };
    const other = { ...FULL_TEAM, name: 'other', report: { ...FULL_TEAM.report, name: 'other', paused: 'other' } };
    const running = { ...FULL_TEAM, name: 'running', report: { ...FULL_TEAM.report, name: 'running' } };
    mockApi([budget, other, running]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const note = card(c, 'saas-code').querySelector('[data-testid="team-paused"]')!;
    expect(note.textContent).toContain('Paused: budget spent');
    expect(note.textContent).toContain('Resume it with `docket run --pod saas-code --resume`');
    const unnamed = card(c, 'other').querySelector('[data-testid="team-paused"]')!;
    expect(unnamed.textContent).toContain('Paused');
    expect(unnamed.textContent).not.toContain('budget');
    expect(unnamed.textContent).toContain('docket run --pod other --resume');
    expect(card(c, 'running').querySelector('[data-testid="team-paused"]')).toBeNull();
    expect(card(c, 'running').querySelector('[data-testid="team-actions"]')!.textContent).not.toContain('Resume team');
  });

  describe('Resume team', () => {
    // A new day: the window restarted and counts nothing yet, but docket keeps the team paused until it is resumed.
    const BUDGET = { tokens: 1000, used: 0, window: 'day', window_start: '2026-11-04T00:00:00Z' };
    // The captured team over its day budget: 2500 counted of 1000.
    const SPENT = { tokens: 1000, used: 2500, window: 'day', window_start: '2026-11-04T00:00:00Z' };
    const paused = (budget?: unknown) => ({ ...FULL_TEAM, report: { ...FULL_TEAM.report, paused: 'budget', budget } });
    const resumeButton = (c: HTMLElement) => card(c, 'saas-code').querySelector<HTMLButtonElement>('button[aria-label="Resume team saas-code"]')!;

    it('is disabled with its reason when the agent has no way to resume the team, and the note keeps the command', async () => {
      mockApi([paused()]);
      const c = mount([runner([DOCKET])]);
      await flush();
      await flush();
      expect(resumeButton(c).disabled).toBe(true);
      expect(card(c, 'saas-code').querySelector('[data-testid="team-actions"]')!.textContent).toContain('Docket does not offer this yet');
      expect(card(c, 'saas-code').querySelector('[data-testid="team-paused"]')!.textContent).toContain('docket run --pod saas-code --resume');
    });

    it('resumes through the board, points the note at the button, and shows the report the answer carries', async () => {
      const calls: string[] = [];
      mockApi([paused(BUDGET)], PROFILES, undefined, (url, method) => {
        if (!url.endsWith('/api/teams/docket/saas-code/resume')) return undefined;
        calls.push(method);
        return new Response(JSON.stringify({ team: 'saas-code', harness: 'docket', runner: 'This computer', report: { ...FULL_TEAM.report, budget: BUDGET } }), { status: 200 });
      });
      const c = mount([runner([DOCKET])]);
      await flush();
      await flush();
      expect(card(c, 'saas-code').querySelector('[data-testid="team-paused"]')!.textContent).toContain('Press Resume team');
      expect(resumeButton(c).disabled).toBe(false);
      resumeButton(c).click();
      await flush();
      await flush();
      expect(calls).toEqual(['POST']);
      expect(card(c, 'saas-code').querySelector('[data-testid="team-paused"]')).toBeNull();
      expect(card(c, 'saas-code').querySelector('button[aria-label="Resume team saas-code"]')).toBeNull();
    });

    it('says a team whose budget is still spent pauses again, before and after it is resumed', async () => {
      mockApi([paused(SPENT)], PROFILES, undefined, (url) =>
        url.endsWith('/api/teams/docket/saas-code/resume')
          ? new Response(JSON.stringify({ team: 'saas-code', harness: 'docket', runner: 'This computer', report: { ...FULL_TEAM.report, budget: SPENT } }), { status: 200 })
          : undefined,
      );
      const c = mount([runner([DOCKET])]);
      await flush();
      await flush();
      const note = card(c, 'saas-code').querySelector('[data-testid="team-paused"]')!.textContent!;
      expect(note).toContain('Its budget is still spent, so resuming now pauses it again');
      expect(note).toContain('docket pod set budgetTokens <tokens> --pod saas-code');
      expect(note).not.toContain('Press Resume team');
      resumeButton(c).click();
      await flush();
      await flush();
      expect(useToasts()().map((t) => [t.type, t.message])).toContainEqual([
        'warning',
        expect.stringContaining('saas-code was resumed. Its budget is still spent'),
      ]);
    });

    it('is busy while the call is out and shows the sentence the agent refused with', async () => {
      let answer: (r: Response) => void = () => {};
      let calls = 0;
      const fetchSpy = mockApi([paused(BUDGET)]);
      const base = fetchSpy.getMockImplementation()!;
      fetchSpy.mockImplementation((input, init) =>
        String(input).endsWith('/resume') ? (calls++, new Promise<Response>((resolve) => { answer = resolve; })) : base(input, init));
      const c = mount([runner([DOCKET])]);
      await flush();
      await flush();
      resumeButton(c).click();
      resumeButton(c).click();
      await flush();
      expect(resumeButton(c).getAttribute('aria-busy')).toBe('true');
      expect(calls).toBe(1);
      answer(new Response(JSON.stringify({ error: { code: 'conflict', message: 'docket has not listed podResume. Resume it with `docket run --pod saas-code --resume`.' } }), { status: 409 }));
      await flush();
      await flush();
      expect(card(c, 'saas-code').querySelector('[data-testid="team-resume-refused"]')!.textContent).toContain('docket has not listed podResume');
      expect(resumeButton(c).getAttribute('aria-busy')).toBeNull();
    });
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

  it('says why a team cannot propose when the agent reports the reason, reading the members as Compare does', async () => {
    const quiet = { ...FULL_TEAM, report: { ...FULL_TEAM.report, members: FULL_TEAM.report.members.map((m) => ({ ...m, can_propose: false })) } };
    mockApi([quiet]);
    const c = mount([runner([{ ...DOCKET, features: { proposals: { support: 'unsupported', reason: 'this team keeps no proposals record' } } }])]);
    await flush();
    await flush();
    const line = row(card(c, 'saas-code'), 'proposals');
    expect(line.textContent).toContain('No member proposes items');
    expect(line.textContent).toContain('this team keeps no proposals record');
  });

  it('names the members that propose, strips backticks from a reason and says an empty record list keeps none', async () => {
    const withReason = { ...FULL_TEAM, report: { ...FULL_TEAM.report, records: [] } };
    mockApi([withReason]);
    const c = mount([runner([{ ...DOCKET, features: { proposals: { support: 'supported', reason: 'through `tack_proposals` records' } } }])]);
    await flush();
    await flush();
    const line = row(card(c, 'saas-code'), 'proposals');
    expect(line.textContent).toContain('Can propose items: Lead');
    expect(line.textContent).toContain('through tack_proposals records');
    expect(line.textContent).not.toContain('`');
    expect(row(card(c, 'saas-code'), 'records').textContent).toContain('Keeps no record sets');
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

describe('TeamsPanel sheet', () => {
  const AUTONOMY = {
    level: 'L4',
    ceiling: 'L3',
    teams: [{ name: 'saas-code', profile: 'Implementer', caps_at: 'L3', fix: 'Set the team to lights-out' }],
  };
  const SOLO = {
    name: 'claude-code',
    harness: 'claude-code',
    reported: true,
    runner: 'This computer',
    reported_at: '2026-10-31T10:00:00Z',
    profiles: [{ id: 'ap_r', name: 'Reviewer' }],
    report: {
      name: 'claude-code',
      delivers: 'branch',
      members: [{ role: 'agent', model: 'opus' }],
      methods: [{ name: 'agent', source: 'harness', default: true, missing_roles: [] }],
      needs: [{ kind: 'sign_in', name: 'signed in', fix: 'Run `claude login` on the runner\'s computer.', satisfied: false }],
    },
  };

  const headings = (el: HTMLElement) => Array.from(el.querySelectorAll('h4')).map((h) => h.textContent);

  it('answers the six questions of the guide in order, from the report', async () => {
    mockApi([FULL_TEAM], PROFILES, { autonomy: AUTONOMY, roles: [{ id: 'r1', name: 'Backend', agent_profile_id: 'ap_i' }, { id: 'r2', name: 'Docs', agent_profile_id: null }] });
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const team = card(c, 'saas-code');
    expect(headings(team)).toEqual([
      'What is it for?',
      'Who is in it?',
      'What happens to a card?',
      'When does it ask?',
      'How far does it go?',
      'How do I send it work?',
    ]);
    // 1 purpose, 2 members, 3 method, 4 asks, 5 far, 6 send
    expect(team.querySelector('[data-testid="team-q-purpose"]')!.textContent).toContain('Hands back a branch of the repository to review and merge');
    expect(team.querySelector('[data-testid="team-q-members"]')!.textContent).toContain('opus');
    const method = team.querySelector('[data-testid="team-q-method"]')!;
    expect(method.querySelector('[data-testid="team-method-tdd"]')!.textContent).toContain('Default');
    expect(method.querySelector('[data-testid="team-method-secure-build"]')!.textContent).toContain('no member plays security-vetter');
    expect(method.textContent).toContain('Steps of tdd');
    expect(team.querySelector('[data-testid="team-q-asks"]')!.textContent).toContain('Stops to ask a person');
    // The project's ceiling and its roles answer the last two.
    expect(team.querySelector('[data-testid="team-ceiling"]')!.textContent).toContain('In Shop, its tasks stay at L3: a person merges.');
    expect(team.querySelector('[data-testid="team-ceiling"]')!.textContent).toContain('Shop can reach L3 at most.');
    const roles = Array.from(team.querySelectorAll('[data-testid="team-roles"] li')).map((li) => li.textContent);
    expect(roles).toEqual(['Backend through the profile Implementer', 'A task with no role through the profile Implementer']);
    // A footer holds the actions, for buttons that act on the team.
    expect(team.querySelector('[data-testid="team-actions"]')).not.toBeNull();
  });

  it('says what is not known of a project while none is read, with the line disabled', async () => {
    mockApi([FULL_TEAM]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const team = card(c, 'saas-code');
    expect(team.querySelector('[data-testid="team-ceiling-none"]')!.getAttribute('aria-disabled')).toBe('true');
    expect(team.querySelector('[data-testid="team-roles-none"]')!.textContent).toContain('Choose a project above');
  });

  it('names the method each profile gives to a kind of card', async () => {
    const profiles = [{ ...PROFILES[0], tool_policy: { docket: { team: 'saas-code', pipeline_by_type: { bug: 'tdd', feature: 'secure-build' } } } }, PROFILES[1]];
    mockApi([FULL_TEAM], profiles);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const rows = Array.from(card(c, 'saas-code').querySelectorAll('[data-testid="team-by-type"] li')).map((li) => li.textContent);
    expect(rows).toEqual(['A bug goes through tdd (Implementer)', 'A feature goes through secure-build (Implementer)']);
  });

  it('reads a team of one as one agent, with its sign-in under when it asks', async () => {
    mockApi([SOLO]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const team = card(c, 'claude-code');
    expect(team.textContent).toContain('A team of one');
    expect(row(team, 'members').textContent).toBe('One agent, claude-code · model opus');
    // No pod-only line is drawn for it, so nothing says "Not reported" about a setting it has none of;
    // whether the agent proposes is the one thing still asked of the runner.
    expect(Array.from(team.querySelectorAll('[data-reported="no"]')).map((l) => l.closest('[data-testid]')!.getAttribute('data-testid'))).toEqual(['team-proposals']);
    expect(team.querySelector('[data-testid="team-supervision"]')).toBeNull();
    const need = team.querySelector('[data-testid="team-needs"] li')!;
    expect(need.textContent).toContain('The operator still has to: signed in.');
    expect(need.textContent).toContain('Run `claude login`');
    expect(team.querySelector('[data-testid="team-method-agent"]')!.textContent).toContain("The agent's own loop");
    // The profiles of that agent are listed by the sheet, not a second time under "no team".
    expect(c.querySelector('[data-testid="no-team-claude-code"]')).toBeNull();
  });

  it('says how the team reads the web, and the fix when it reads none', async () => {
    const closed = { mode: 'closed', reason: 'a folder team reads no web without the jail', fix: 'docket setup sandbox on' };
    mockApi([{ ...FULL_TEAM, report: { ...FULL_TEAM.report, web_read: closed } }]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const web = card(c, 'saas-code').querySelector('[data-testid="team-web"]')!;
    expect(web.textContent).toContain('None: a folder team reads no web without the jail');
    expect(web.textContent).toContain('To fix: docket setup sandbox on');
    const open = { mode: 'open', reason: 'a folder team under the jail reads any public host', fix: '' };
    mockApi([{ ...FULL_TEAM, report: { ...FULL_TEAM.report, web_read: open } }]);
    const d = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const read = card(d, 'saas-code').querySelector('[data-testid="team-web"]')!;
    expect(read.textContent).toContain('Any public host: a folder team under the jail reads any public host');
    expect(read.textContent).not.toContain('To fix');
  });

  it('says what web search the team has, the fix when it is not ready, and nothing when docket says nothing', async () => {
    const options = [
      { provider: 'searxng', command: 'docket setup search searxng --pod saas-code', secret: '' },
      { provider: 'brave', command: 'docket setup search brave --pod saas-code', secret: 'brave-search' },
    ];
    const none = { provider: 'none', ready: false, reason: 'no search service', fix: options[0].command, options };
    mockApi([{ ...FULL_TEAM, report: { ...FULL_TEAM.report, search: none } }]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const search = card(c, 'saas-code').querySelector('[data-testid="team-search"]')!;
    expect(search.textContent).toContain('None: no search service');
    expect(search.textContent).toContain('To fix: docket setup search searxng --pod saas-code');
    expect(search.textContent).toContain('Or: docket setup search brave --pod saas-code');
    expect(search.querySelector('button[aria-label="Help: Search"]')).not.toBeNull();
    const ready = { provider: 'brave', ready: true, reason: 'Web search goes through Brave.', fix: '', options };
    mockApi([{ ...FULL_TEAM, report: { ...FULL_TEAM.report, search: ready } }]);
    const d = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const brave = card(d, 'saas-code').querySelector('[data-testid="team-search"]')!;
    expect(brave.textContent).toContain('Brave Search: ready');
    expect(brave.textContent).not.toContain('To fix');
    expect(brave.textContent).not.toContain('Or:');
    // A docket that does not say draws no line, and no "Not reported" for it.
    mockApi([{ ...FULL_TEAM }]);
    const e = mount([runner([DOCKET])]);
    await flush();
    await flush();
    expect(card(e, 'saas-code').querySelector('[data-testid="team-search"]')).toBeNull();
  });

  it('says a need that only degrades the work is not a blocker', async () => {
    const needs = [{ kind: 'service', name: 'PDFs are read', fix: 'docket setup sandbox on', satisfied: false, blocking: false }];
    mockApi([{ ...SOLO, report: { ...SOLO.report, needs } }]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const need = card(c, 'claude-code').querySelector('[data-testid="team-needs"] li')!;
    expect(need.getAttribute('data-blocking')).toBe('no');
    expect(need.textContent).toContain('Runs without it, with less: PDFs are read.');
    expect(need.textContent).toContain('docket setup sandbox on');
    expect(need.textContent).not.toContain('The operator still has to');
  });

  it('lists the methods a task can name first, and says why the others cannot run', async () => {
    const broken = { name: 'capture-broken', source: 'pod:capture-broken', default: false, missing_roles: [], error: 'YAML parse error: while parsing a flow sequence\n  in "<unicode string>"' };
    const team = { ...FULL_TEAM, report: { ...FULL_TEAM.report, methods: [broken, ...FULL_TEAM.report.methods] } };
    mockApi([team]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const method = card(c, 'saas-code').querySelector('[data-testid="team-q-method"]')!;
    const names = Array.from(method.querySelectorAll('[data-testid^="team-method-"]'))
      .map((li) => li.getAttribute('data-testid'))
      .filter((id) => id !== 'team-method-missing' && id !== 'team-method-broken');
    expect(names).toEqual(['team-method-tdd', 'team-method-secure-build', 'team-method-capture-broken']);
    expect(method.querySelector('[data-testid="team-method-capture-broken"]')!.textContent).toContain('Does not load: YAML parse error: while parsing a flow sequence');
    expect(method.querySelector('[data-testid="team-method-capture-broken"]')!.textContent).not.toContain('<unicode string>');
    expect(method.querySelector('[data-testid="team-method-secure-build"]')!.textContent).toContain('Cannot run: no member plays security-vetter');
  });

  it('says in a person\'s words why each new kind of misfit refuses a profile', async () => {
    const team = {
      ...FULL_TEAM,
      profiles: [
        { id: 'ap_a', name: 'Fixer', misfits: ['no_repository', { unknown_method: { name: 'lint' } }, { steps_skipped: { method: 'secure-build', roles: ['security-vetter'] } }] },
        { id: 'ap_b', name: 'Planner', misfits: [{ needs_unmet: { name: 'signed in' } }] },
      ],
      report: { ...FULL_TEAM.report, delivers: 'report', needs: [{ kind: 'sign_in', name: 'signed in', fix: 'Run `docket login`.', satisfied: false }] },
    };
    mockApi([team]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const lines = Array.from(card(c, 'saas-code').querySelectorAll('[data-testid="team-misfit"]')).map((l) => l.textContent);
    expect(lines).toHaveLength(4);
    expect(lines[0]).toContain("changes the project's repository, and saas-code hands back a report, not a branch");
    expect(lines[1]).toContain('names the method lint, which saas-code does not have. It can run tdd');
    expect(lines[2]).toContain('which saas-code cannot run: no member plays security-vetter');
    expect(lines[3]).toContain('not signed in on the runner. Run `docket login`.');
    expect(lines.join(' ')).not.toContain('delivers only proposed items');
  });

  it('says what the sandbox reads and where to learn to grant a toolchain', async () => {
    mockApi([{ ...FULL_TEAM, report: { ...FULL_TEAM.report, read_only: [] } }, { ...FULL_TEAM, name: 'reads', report: { ...FULL_TEAM.report, name: 'reads', read_only: ['/home/x/venv'] } }]);
    const c = mount([runner([DOCKET])]);
    await flush();
    await flush();
    const none = card(c, 'saas-code').querySelector('[data-testid="team-reads"]')!;
    expect(none.textContent).toContain('reads only its own work');
    expect(none.querySelector('a')!.textContent).toContain('Administrator Guide');
    const some = card(c, 'reads').querySelector('[data-testid="team-reads"]')!;
    expect(some.textContent).toBe('Its sandbox also reads /home/x/venv, and cannot change them.');
  });

  describe('What a team has learnt', () => {
    const entry = (id: string, subject: string, extra: Record<string, unknown> = {}) => ({
      id, kind: 'generated', subject, note: `About ${subject}`, reason: 'path present at the task commit', stale: false,
      provenance: { task: 'task-1' }, ...extra,
    });
    const WITH_MEMORY = {
      ...FULL_TEAM,
      report: {
        ...FULL_TEAM.report,
        memory: {
          accepted: [entry('mem-a', 'README.md'), entry('mem-b', 'notes/old.md', { stale: true })],
          pending: [
            entry('mem-p', 'docs/', { kind: 'convention', reason: 'awaits operator approval' }),
            entry('mem-q', 'api/schema.proto', { kind: 'pitfall', reason: 'awaits operator approval' }),
          ],
        },
      },
    };
    const memory = (c: HTMLElement) => row(card(c, 'saas-code'), 'memory');

    it('lists what it has learnt newest first and marks a stale entry', async () => {
      mockApi([WITH_MEMORY]);
      const c = mount([runner([DOCKET])]);
      await flush();
      await flush();
      const learnt = memory(c).querySelector<HTMLElement>('[data-testid="team-memory-learnt"]')!;
      expect(learnt.querySelector('h5')!.textContent).toBe('What it has learnt');
      const items = Array.from(learnt.querySelectorAll('li'));
      expect(items.map((li) => li.getAttribute('data-testid'))).toEqual(['team-memory-entry-mem-b', 'team-memory-entry-mem-a']);
      expect(items[0].getAttribute('data-stale')).toBe('yes');
      expect(items[0].textContent).toContain('Stale');
      expect(items[1].textContent).not.toContain('Stale');
      expect(items[1].textContent).toContain('About README.md');
    });

    it('lists what it waits to learn with docket\'s two commands to copy and the buttons disabled with their reason', async () => {
      mockApi([WITH_MEMORY]);
      const c = mount([runner([DOCKET])]);
      await flush();
      await flush();
      const waiting = memory(c).querySelector<HTMLElement>('[data-testid="team-memory-waiting"]')!;
      expect(waiting.querySelector('h5')!.textContent).toBe('What it waits to learn');
      const first = waiting.querySelector<HTMLElement>('[data-testid="team-memory-entry-mem-p"]')!;
      expect(first.textContent).toContain('docs/');
      expect(first.textContent).toContain('Held back: awaits operator approval');
      const commands = Array.from(first.querySelectorAll('[data-testid="copy-fix-text"]')).map((code) => code.textContent);
      expect(commands).toEqual([
        'docket pod memory approve mem-p --pod saas-code',
        'docket pod memory refuse mem-p --pod saas-code --reason "<why>"',
      ]);
      const approve = first.querySelector<HTMLButtonElement>('button[aria-label="Approve docs/"]')!;
      const refuse = first.querySelector<HTMLButtonElement>('button[aria-label="Refuse docs/"]')!;
      expect(approve.disabled).toBe(true);
      expect(refuse.disabled).toBe(true);
      const reason = first.querySelector('[data-testid="team-memory-decide-reason"]')!;
      expect(reason.textContent).toBe('docket decides this with its own command for now');
      expect(approve.getAttribute('aria-describedby')).toBe(reason.id);
      expect(refuse.getAttribute('aria-describedby')).toBe(reason.id);
      expect(waiting.querySelectorAll('li[data-testid^="team-memory-entry-"]')).toHaveLength(2);
    });

    describe('deciding from the board', () => {
      const DECIDABLE = {
        ...WITH_MEMORY,
        report: { ...WITH_MEMORY.report, memory: { ...WITH_MEMORY.report.memory, can_decide: true } },
      };
      const entryBox = (c: HTMLElement, id: string) => memory(c).querySelector<HTMLElement>(`[data-testid="team-memory-entry-${id}"]`)!;
      const button = (c: HTMLElement, id: string, label: string) => entryBox(c, id).querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
      /** What the team reports once `mem-p` is decided: it is in neither list of the waiting ones any more. */
      const after = (status: 'accepted' | 'refused') => ({
        ...DECIDABLE.report,
        memory: {
          ...DECIDABLE.report.memory,
          pending: DECIDABLE.report.memory.pending.filter((e) => e.id !== 'mem-p'),
          accepted: status === 'accepted' ? [...DECIDABLE.report.memory.accepted, entry('mem-p', 'docs/')] : DECIDABLE.report.memory.accepted,
        },
      });

      it('enables Approve and Refuse when the team can be decided, and drops the reason line', async () => {
        mockApi([DECIDABLE]);
        const c = mount([runner([DOCKET])]);
        await flush();
        await flush();
        expect(button(c, 'mem-p', 'Approve docs/').disabled).toBe(false);
        expect(button(c, 'mem-p', 'Refuse docs/').disabled).toBe(false);
        expect(entryBox(c, 'mem-p').querySelector('[data-testid="team-memory-decide-reason"]')).toBeNull();
      });

      it('keeps them disabled with their reason when the team cannot be decided', async () => {
        mockApi([WITH_MEMORY]);
        const c = mount([runner([DOCKET])]);
        await flush();
        await flush();
        expect(button(c, 'mem-p', 'Approve docs/').disabled).toBe(true);
        expect(button(c, 'mem-p', 'Refuse docs/').disabled).toBe(true);
        expect(entryBox(c, 'mem-p').querySelector('[data-testid="team-memory-decide-reason"]')!.textContent).toBe('docket decides this with its own command for now');
      });

      it('approves through the board and the entry leaves what it waits to learn', async () => {
        const posted: unknown[] = [];
        mockApi([DECIDABLE], PROFILES, undefined, (url, method, body) => {
          if (!url.endsWith('/api/teams/docket/saas-code/memory/mem-p')) return undefined;
          expect(method).toBe('POST');
          posted.push(JSON.parse(String(body)));
          return new Response(JSON.stringify({ team: 'saas-code', harness: 'docket', runner: 'This computer', report: after('accepted') }), { status: 200 });
        });
        const c = mount([runner([DOCKET])]);
        await flush();
        await flush();
        button(c, 'mem-p', 'Approve docs/').click();
        await flush();
        await flush();
        expect(posted).toEqual([{ decision: 'approve' }]);
        const waiting = memory(c).querySelector('[data-testid="team-memory-waiting"]')!;
        expect(waiting.querySelector('[data-testid="team-memory-entry-mem-p"]')).toBeNull();
        expect(waiting.querySelectorAll('li[data-testid^="team-memory-entry-"]')).toHaveLength(1);
        expect(memory(c).querySelector('[data-testid="team-memory-learnt"] [data-testid="team-memory-entry-mem-p"]')).not.toBeNull();
      });

      it('asks for a reason before refusing, and sends it', async () => {
        const posted: unknown[] = [];
        mockApi([DECIDABLE], PROFILES, undefined, (url, _method, body) => {
          if (!url.endsWith('/api/teams/docket/saas-code/memory/mem-p')) return undefined;
          posted.push(JSON.parse(String(body)));
          return new Response(JSON.stringify({ team: 'saas-code', harness: 'docket', runner: 'This computer', report: after('refused') }), { status: 200 });
        });
        const c = mount([runner([DOCKET])]);
        await flush();
        await flush();
        button(c, 'mem-p', 'Refuse docs/').click();
        await flush();
        const form = entryBox(c, 'mem-p').querySelector<HTMLFormElement>('[data-testid="team-memory-refuse-form"]')!;
        const confirm = form.querySelector<HTMLButtonElement>('button[type="submit"]')!;
        const input = form.querySelector<HTMLInputElement>('input')!;
        expect(confirm.disabled).toBe(true);
        expect(posted).toEqual([]);
        input.value = '   ';
        input.dispatchEvent(new Event('input', { bubbles: true }));
        await flush();
        expect(confirm.disabled).toBe(true);
        input.value = 'CI rebuilds it';
        input.dispatchEvent(new Event('input', { bubbles: true }));
        await flush();
        expect(confirm.disabled).toBe(false);
        confirm.click();
        await flush();
        await flush();
        expect(posted).toEqual([{ decision: 'refuse', reason: 'CI rebuilds it' }]);
        expect(memory(c).querySelector('[data-testid="team-memory-entry-mem-p"]')).toBeNull();
      });

      it('shows what the agent said when it refuses to decide, and leaves the entry waiting', async () => {
        mockApi([DECIDABLE], PROFILES, undefined, (url) =>
          url.endsWith('/api/teams/docket/saas-code/memory/mem-p')
            ? new Response(JSON.stringify({ error: { code: 'conflict', message: 'That entry is no longer waiting.' } }), { status: 409 })
            : undefined,
        );
        const c = mount([runner([DOCKET])]);
        await flush();
        await flush();
        button(c, 'mem-p', 'Approve docs/').click();
        await flush();
        await flush();
        expect(entryBox(c, 'mem-p').querySelector('[data-testid="team-memory-refused"]')!.textContent).toContain('no longer waiting');
        expect(button(c, 'mem-p', 'Approve docs/').disabled).toBe(false);
      });
    });

    it('says plainly when nothing was learnt and nothing waits', async () => {
      mockApi([FULL_TEAM]);
      const c = mount([runner([DOCKET])]);
      await flush();
      await flush();
      expect(memory(c).querySelector('[data-testid="team-memory-learnt"]')!.textContent).toContain('Nothing yet');
      expect(memory(c).querySelector('[data-testid="team-memory-waiting"]')!.textContent).toContain('Nothing waits for you');
    });

    it('is one disabled line for a team whose agent did not report a memory', async () => {
      const { memory: _unreported, ...report } = FULL_TEAM.report;
      mockApi([{ ...FULL_TEAM, report }]);
      const c = mount([runner([DOCKET])]);
      await flush();
      await flush();
      expect(memory(c).querySelector('[data-testid="team-memory-waiting"]')).toBeNull();
      expect(memory(c).textContent).toContain('Not reported by Docket');
    });
  });
});
