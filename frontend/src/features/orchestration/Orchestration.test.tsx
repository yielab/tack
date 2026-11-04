import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { Resource } from 'solid-js';
import { MemoryRouter, Route, createMemoryHistory } from '@solidjs/router';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import { ProjectItemsProvider } from '../../shared/state/projectItemsContext';
import { ProjectHasRunProvider } from '../../shared/state/projectHasRun';
import { ITEM_UPDATED_EVENT } from '../../shared/state/itemEvents';
import WorkTabs from '../../shared/ui/WorkTabs';
import { flush } from '../../test/flush';
import { stateRank } from '../../shared/runWithAgent/cardRun';
import Orchestration from './Orchestration';

type Json = Record<string, unknown>;

const disposers: Array<() => void> = [];
let history = createMemoryHistory();
afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

const item = (id: string, over: Json = {}): Json => ({
  id,
  project_id: 'p1',
  title: id,
  item_type: 'task',
  status: 'To Do',
  priority: 'medium',
  estimate_unit: 'story_points',
  tags: [],
  sort_order: 0,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
  ...over,
});

const profile = (id: string, team?: string, agent?: string): Json => ({
  agent_profile_id: id,
  name: id,
  instructions: '',
  tool_policy: { ...(agent ? { agent } : {}), ...(team ? { docket: { team } } : {}) },
  limits: null,
});

const team = (name: string, steps: unknown[] | null, over: Json = {}): Json => ({
  name,
  harness: 'docket',
  reported: true,
  profiles: [{ id: `${name}-p`, name: `${name}-p` }],
  report: { name, profile: `${name}-p`, steps, works_in: 'repository', decides: 'to_the_end', network: 'none', members: [{ role: 'Lead', model: 'm', writes_project: true, can_propose: true }] },
  ...over,
});

const STEPS = [
  { name: 'write', kind: 'agent', role: 'Lead' },
  { name: 'review', kind: 'agent', role: 'Reviewer', checked_by: 'reviewer', sends_back_to: 'write', sends_back_max: 2 },
];

const run = (item_id: string, state: string, profile_id: string, over: Json = {}): Json => ({
  request_id: `r-${item_id}`,
  item_id,
  state,
  cancellation_requested_at: null,
  created_at: '2026-01-02T00:00:00Z',
  agent_profile_id: profile_id,
  ...over,
});

interface World {
  items: Json[];
  runs: Json[];
  teams: Json[];
  profiles: Json[];
  roles: Json[];
  /** Roles of each task, by id; absent is none. */
  taskRoles: Record<string, Json[]>;
  /** Dependency edges of each task, by id. */
  deps: Record<string, Json[]>;
  runners: Json[];
  /** What `/routing` answers when the test says it differently from the rule `routing()` stands in for. */
  routed?: Json[];
  patches: Array<{ id: string; body: Json }>;
  project: Json;
}

function world(over: Partial<World> = {}): World {
  return {
    items: [],
    runs: [],
    teams: [],
    profiles: [],
    roles: [],
    taskRoles: {},
    deps: {},
    runners: [{ runner_id: 'r1', state: 'active', online: true, total_capacity: 1, available_capacity: 1, capability_snapshot: null }],
    patches: [],
    project: {
      id: 'p1',
      name: 'Proj',
      auto_start_enabled: true,
      auto_start_status: 'To Do',
      auto_start_max_open: 3,
      default_profile_id: null,
      workflow: { workflow_type: 'kanban', statuses: [{ name: 'To Do', order: 0, category: 'todo' }, { name: 'Doing', order: 1, category: 'in_progress' }, { name: 'Done', order: 2, category: 'done' }] },
    },
    ...over,
  };
}

/** What the server's `/routing` answers for this world: the profile chosen on the task, else its first
 *  role with one, else the project's default; nobody when none is named, gone, or its team is not reported. */
function routing(w: World): Json[] {
  const reported = new Set(w.teams.filter((t) => t.reported).map((t) => t.name));
  return w.items
    .filter((i) => i.status !== 'Done')
    .map((i) => {
      const roles = w.taskRoles[i.id as string] ?? [];
      const own = (i.run_settings as { agent_profile_id?: string } | undefined)?.agent_profile_id;
      const byRole = roles.find((r) => r.agent_profile_id);
      const profileId = (own ?? byRole?.agent_profile_id ?? w.project.default_profile_id ?? null) as string | null;
      const source = own ? 'run_settings' : byRole ? `role:${byRole.name}` : 'project_default';
      const base = { item_id: i.id, profile_id: profileId, source: profileId ? source : null, role: (byRole?.name as string | undefined) ?? null, team: null, harness: null, nobody: null };
      if (!profileId) return { ...base, role: (roles[0]?.name as string | undefined) ?? null, nobody: roles.length > 0 ? 'role_without_profile' : 'no_role' };
      const found = w.profiles.find((p) => p.agent_profile_id === profileId) as { tool_policy: { agent?: string; docket?: { team?: string } } } | undefined;
      if (!found) return { ...base, nobody: 'profile_missing' };
      const named = found.tool_policy.docket?.team ?? null;
      return { ...base, team: named, harness: found.tool_policy.agent ?? null, nobody: named && !reported.has(named) ? 'team_not_reported' : null };
    });
}

async function settle() {
  for (let i = 0; i < 8; i += 1) await flush();
}

/** Mounts the work tabs and the Orchestration view the way the work layout does, with the server stubbed. */
async function mount(w: World) {
  vi.stubGlobal('WebSocket', class { close() {} addEventListener() {} });
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const json = (body: unknown, headers: Record<string, string> = {}) =>
      new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json', ETag: '"1"', ...headers } });
    const detail = /\/api\/items\/([^/?]+)$/.exec(url);
    if (detail && init?.method === 'PATCH') {
      w.patches.push({ id: detail[1], body: JSON.parse(String(init.body)) });
      return json(w.items.find((i) => i.id === detail[1]));
    }
    if (detail) {
      return json({ item: w.items.find((i) => i.id === detail[1]), roles: w.taskRoles[detail[1]] ?? [], dependencies: w.deps[detail[1]] ?? [] });
    }
    if (url.includes('/items?')) return json({ data: w.items, total: w.items.length, page: 1, per_page: 200 });
    if (url.includes('/executions?')) return json({ protocol_version: 1, data: w.runs });
    if (url.endsWith('/routing')) return json(w.routed ?? routing(w));
    if (url.endsWith('/teams')) return json({ teams: w.teams });
    if (url.includes('/agent-profiles')) return json({ protocol_version: 1, data: w.profiles });
    if (url.endsWith('/roles')) return json(w.roles);
    if (url.endsWith('/autonomy')) return json({ level: 'L3', budgets: { tokens_per_day: null }, teams: [] });
    if (url.includes('/runners')) return json({ protocol_version: 1, data: w.runners });
    return json({ protocol_version: 1, data: [] });
  }) as typeof fetch);

  const context = {
    projectId: () => 'p1',
    project: (() => w.project) as unknown as Resource<null>,
    workflow: () => undefined,
    vocabulary: () => ({}),
    refetch: () => {},
  } as ProjectContextValue;
  history = createMemoryHistory();
  history.set({ value: '/projects/p1/orchestration' });
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ProjectContext.Provider value={context}>
        <MemoryRouter history={history}>
          <Route
            path="/projects/:id/orchestration"
            component={() => (
              <ProjectItemsProvider>
                <ProjectHasRunProvider>
                  <WorkTabs />
                  <Orchestration />
                </ProjectHasRunProvider>
              </ProjectItemsProvider>
            )}
          />
        </MemoryRouter>
      </ProjectContext.Provider>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  await settle();
  return container;
}

const q = <T extends Element = HTMLElement>(c: ParentNode, sel: string) => c.querySelector<T>(sel);
const qa = <T extends Element = HTMLElement>(c: ParentNode, sel: string) => [...c.querySelectorAll<T>(sel)];
const laneNames = (c: ParentNode) => qa(c, '[data-testid="flow-lane-name"]').map((a) => a.textContent!.split(' — ')[0]);

/** Three lanes: one waiting on you, one running a team that reports no steps, one idle. */
function threeLanes(): World {
  return world({
    teams: [team('idle-team', STEPS), team('saas-code', []), team('saas-notes', STEPS)],
    profiles: [profile('idle-p', 'idle-team'), profile('code-p', 'saas-code'), profile('notes-p', 'saas-notes')],
    roles: [{ id: 'role-1', name: 'Idler', agent_profile_id: 'idle-p', project_id: 'p1' }],
    items: [
      item('i-ask', { run_settings: { agent_profile_id: 'notes-p' }, status: 'Doing' }),
      item('i-run', { run_settings: { agent_profile_id: 'code-p' }, status: 'Doing' }),
    ],
    runs: [
      run('i-ask', 'waiting_decision', 'notes-p', { latest_hop: { role: 'Lead', kind: 'started', verdict: null, cycle: null } }),
      run('i-run', 'running', 'code-p'),
    ],
  });
}

describe('Orchestration: before any run', () => {
  it('shows_the_tab_off_with_the_reason_and_the_page_links_to_Agents', async () => {
    const c = await mount(world({ items: [item('a')] }));
    const off = q(c, '[data-testid="tab-orchestration-off"]');
    expect(off).not.toBeNull();
    expect(off!.getAttribute('aria-disabled')).toBe('true');
    expect(off!.textContent).toContain('No agent has worked in this project yet');
    const empty = q(c, '[data-testid="orchestration-empty"]')!;
    expect(empty.textContent).toContain('No agent has worked in this project yet');
    expect(q<HTMLAnchorElement>(empty, 'a')!.getAttribute('href')).toBe('/agents');
    expect(q(c, '[data-testid="flow"]')).toBeNull();
  });

  it('is_a_link_once_a_run_has_started', async () => {
    const c = await mount(threeLanes());
    expect(q(c, '[data-testid="tab-orchestration-off"]')).toBeNull();
    const tab = qa<HTMLAnchorElement>(c, 'a').find((a) => a.textContent === 'Orchestration');
    expect(tab?.getAttribute('href')).toBe('/projects/p1/orchestration');
  });
});

describe('Orchestration: lanes', () => {
  it('orders_the_lanes_needs_you_then_running_then_idle_and_folds_the_idle_one', async () => {
    const c = await mount(threeLanes());
    expect(laneNames(c)).toEqual(['saas-notes', 'saas-code', 'idle-team']);
    const urgency = qa(c, '[data-testid="flow-lane"]').map((l) => l.getAttribute('data-urgency'));
    expect(urgency).toEqual(['needs', 'running', 'idle']);
    const idle = q(c, '[data-lane="team:idle-team"]')!;
    expect(q(idle, '[data-testid="flow-lane-toggle"]')!.getAttribute('aria-expanded')).toBe('false');
    expect(q(idle, '[data-testid="flow-station"]')).toBeNull();
    expect(q(c, '[data-lane="team:saas-notes"] [data-testid="flow-lane-toggle"]')!.getAttribute('aria-expanded')).toBe('true');
  });

  it('draws_a_task_in_the_lane_the_routing_endpoint_names', async () => {
    const w = threeLanes();
    // Two roles that disagree: the server falls back to the project default, not to the first role.
    w.items = [item('i-split', { status: 'To Do' }), w.items[1]];
    w.runs = [w.runs[1]];
    w.routed = [{ item_id: 'i-split', profile_id: 'notes-p', source: 'ambiguous', role: null, team: 'saas-notes', harness: 'docket', nobody: null }];
    const c = await mount(w);
    // An idle lane is folded; open it to see who stands in it.
    q<HTMLButtonElement>(c, '[data-lane="team:saas-notes"] [data-testid="flow-lane-toggle"]')!.click();
    await settle();
    expect(q(c, '[data-lane="team:saas-notes"] [data-item-id="i-split"]')).not.toBeNull();
    expect(q(c, '[data-lane="team:saas-code"] [data-item-id="i-split"]')).toBeNull();
  });

  it('puts_a_token_at_the_station_its_latest_hop_names', async () => {
    const w = threeLanes();
    w.runs = [run('i-ask', 'running', 'notes-p', { latest_hop: { role: 'Reviewer', kind: 'started', verdict: null, cycle: 1 } }), w.runs[1]];
    const c = await mount(w);
    const station = q(c, '[data-lane="team:saas-notes"] [data-station="step-1"]')!;
    expect(station.textContent).toContain('review');
    expect(q(station, '[data-item-id="i-ask"]')).not.toBeNull();
    // Sent back once out of the two the reviewer allows.
    expect(q(station, '[data-testid="flow-token-sent-back"]')!.textContent).toContain('1/2');
    expect(q(c, '[data-lane="team:saas-notes"] [data-station="step-0"] [data-item-id]')).toBeNull();
  });

  it('puts_a_run_with_no_hop_of_a_team_that_reports_no_steps_at_the_single_Working_station', async () => {
    const c = await mount(threeLanes());
    const lane = q(c, '[data-lane="team:saas-code"]')!;
    expect(qa(lane, '[data-testid="flow-station"]').map((s) => s.getAttribute('data-station'))).toEqual(['up-next', 'working', 'needs-you', 'done']);
    const working = q(lane, '[data-station="working"]')!;
    expect(q(working, '[data-item-id="i-run"]')).not.toBeNull();
    expect(working.textContent).toContain('The agent reports no steps');
  });

  it('says_how_each_task_that_ended_today_ended_instead_of_counting_them_all_as_done', async () => {
    const w = world({
      teams: [team('saas-code', [])],
      profiles: [profile('code-p', 'saas-code')],
      items: [1, 2, 3, 4].map((n) => item(`e${n}`, { run_settings: { agent_profile_id: 'code-p' } })),
      runs: [run('e1', 'succeeded', 'code-p'), run('e2', 'succeeded', 'code-p'), run('e3', 'cancelled', 'code-p'), run('e4', 'failed', 'code-p')],
    });
    const c = await mount(w);
    q<HTMLButtonElement>(c, '[data-lane="team:saas-code"] [data-testid="flow-lane-toggle"]')!.click(); // a lane with nothing going on is folded
    await flush();
    const station = q(c, '[data-lane="team:saas-code"] [data-station="done"]')!;
    expect(station.textContent).toContain('Ended today');
    expect(station.textContent).not.toContain('Done today');
    const tally = q(station, '[data-testid="flow-ended-tally"]')!;
    expect(tally.textContent).toBe('2 finished · 1 cancelled · 1 failed');
    // Each part carries its run-state icon with the state's words, so colour never says it alone.
    expect(qa(tally, '[role="img"]').map((e) => e.getAttribute('aria-label'))).toEqual(['Finished', 'Cancelled', 'Failed']);
  });

  it('draws_the_header_of_a_team_from_what_it_reported', async () => {
    const c = await mount(threeLanes());
    const lane = q(c, '[data-lane="team:saas-notes"]')!;
    expect(q(lane, '[data-mark="works-in"]')!.getAttribute('aria-label')).toContain('repository');
    expect(q(lane, '[data-mark="network"]')!.getAttribute('aria-label')).toBe('No network');
    expect(q(lane, '[data-testid="flow-merge-rung"]')!.textContent).toBe('You merge (L3)');
    const member = q(lane, '[data-testid="flow-member-Lead"]')!;
    expect(member.getAttribute('data-writes')).toBe('true');
    expect(member.getAttribute('data-proposes')).toBe('true');
    expect(q(lane, '[data-testid="flow-queue"] [role="img"]')!.getAttribute('aria-label')).toBe('1 waiting, 0 running (counted on this board)');
    expect(q(lane, '[data-testid="flow-tokens-meter"]')!.textContent).toContain('no daily limit');
    expect(q<HTMLAnchorElement>(lane, '[data-testid="flow-lane-name"]')!.getAttribute('href')).toBe('/agents#team-saas-notes');
  });

  it('draws_no_empty_queue_bar_for_a_lane_with_nothing_running_or_waiting_and_says_so_in_words', async () => {
    const c = await mount(threeLanes());
    const idle = qa(c, '[data-testid="flow-lane"]').filter((lane) => lane.getAttribute('data-urgency') === 'idle');
    expect(idle.length).toBeGreaterThan(0);
    for (const lane of idle) {
      expect(q(lane, '[data-testid="flow-queue"] [role="img"]')).toBeNull();
      expect(q(lane, '[data-testid="flow-queue"]')!.textContent).toBe('Nothing running or waiting');
    }
  });

  it('writes_the_same_word_at_every_empty_station', async () => {
    const c = await mount(threeLanes());
    const empties = qa(c, '[data-lane="team:saas-notes"] [data-testid="flow-station"] ul > li.text-caption');
    expect(empties.length).toBeGreaterThan(1);
    expect([...new Set(empties.map((li) => li.textContent))]).toEqual(['None']);
  });

  it('names_the_check_between_two_stations_in_visible_words_beside_its_glyph', async () => {
    const c = await mount(threeLanes());
    const check = q(c, '[data-lane="team:saas-notes"] [data-testid="flow-check"]')!;
    expect(q(check, '[data-testid="flow-check-word"]')!.textContent).toBe('reviewer');
  });

  it('says_the_check_between_two_stations_in_words_and_draws_it_as_a_glyph', async () => {
    const c = await mount(threeLanes());
    const lane = q(c, '[data-lane="team:saas-notes"]')!;
    expect(q(lane, '[data-testid="flow-check"]')!.getAttribute('data-check')).toBe('reviewer');
    expect(q(lane, '[data-station="step-1"]')!.textContent).toContain('Then a reviewer can send it back, up to 2 times');
  });

  it('gives_lists_and_regions_accessible_names', async () => {
    const c = await mount(threeLanes());
    expect(q(c, '[role="region"][aria-label="Stations of saas-notes"]')).not.toBeNull();
    expect(q(c, 'ul[aria-label="Needs you, saas-notes"]')).not.toBeNull();
    expect(q(c, 'ol[aria-label="Lanes"]')).not.toBeNull();
    expect(q(c, '[data-testid="flow-live"]')!.getAttribute('aria-live')).toBe('polite');
  });
});

describe('Orchestration: summary row', () => {
  it('reads_the_Work_counters_in_the_order_the_Board_and_the_List_rank_runs_with_Nobody_last', async () => {
    const c = await mount(threeLanes());
    const filters = qa(c, '[data-testid="flow-summary"] button').map((b) => b.getAttribute('data-testid')!.replace('flow-counter-', ''));
    expect(filters).toEqual(['needs', 'review', 'running', 'queued', 'nobody']);
    const ranks = filters.slice(0, 4).map((f) => stateRank(f as never));
    expect(ranks).toEqual([...ranks].sort((a, b) => a - b));
  });

  it('counts_the_tasks_and_a_counter_narrows_the_lanes', async () => {
    const c = await mount(threeLanes());
    const count = (f: string) => q(c, `[data-testid="flow-counter-${f}"]`)!.textContent!;
    expect(count('needs')).toContain('1');
    expect(count('running')).toContain('1');
    expect(q<HTMLButtonElement>(c, '[data-testid="flow-counter-queued"]')!.disabled).toBe(true);
    expect(q(c, '[data-testid="flow-capacity"]')!.textContent).toBe('2 of 3 runs open in this project · this computer runs 1 at a time');

    q<HTMLButtonElement>(c, '[data-testid="flow-counter-running"]')!.click();
    await flush();
    expect(laneNames(c)).toEqual(['saas-code']);
    expect(q(c, '[data-testid="flow-counter-running"]')!.getAttribute('aria-pressed')).toBe('true');

    q<HTMLButtonElement>(c, '[data-testid="flow-counter-running"]')!.click();
    q<HTMLButtonElement>(c, '[data-testid="flow-counter-needs"]')!.click();
    await flush();
    expect(laneNames(c)).toEqual(['saas-notes']);
  });
});

describe('Orchestration: nobody will take it', () => {
  function orphans(): World {
    return world({
      teams: [team('saas-notes', STEPS), team('ghost', STEPS, { reported: false, report: null })],
      profiles: [profile('notes-p', 'saas-notes'), profile('ghost-p', 'ghost'), profile('solo', undefined, 'claude-code')],
      items: [item('orphan'), item('bare'), item('lost', { run_settings: { agent_profile_id: 'ghost-p' } })],
      taskRoles: { bare: [{ id: 'r', name: 'Content', agent_profile_id: null, project_id: 'p1' }] },
      roles: [{ id: 'r0', name: 'Writer', agent_profile_id: 'notes-p', project_id: 'p1' }],
      runs: [run('other', 'running', 'notes-p')],
    });
  }

  it('draws_a_lane_at_the_top_with_each_reason_and_the_Who_does_it_picker_inline', async () => {
    const c = await mount(orphans());
    const lane = q(c, '[data-testid="flow-nobody"]')!;
    expect(lane.textContent).toContain('Nobody will take it');
    const reasons = Object.fromEntries(qa(lane, '[data-testid="flow-nobody-entry"]').map((e) => [e.getAttribute('data-item-id'), q(e, '[data-testid="flow-nobody-reason"]')!]));
    expect(reasons.orphan.getAttribute('data-reason')).toBe('no_role');
    expect(reasons.bare.getAttribute('data-reason')).toBe('role_without_profile');
    expect(reasons.bare.textContent).toContain('Its role Content has no profile');
    expect(reasons.lost.getAttribute('data-reason')).toBe('team_not_reported');
    expect(reasons.lost.textContent).toContain('No computer reports the team ghost');
    for (const entry of qa(lane, '[data-testid="flow-nobody-entry"]')) {
      expect(q(entry, '[data-testid="flow-nobody-picker"] [data-testid="who-does-it"]')).not.toBeNull();
    }
    // It is above the lanes.
    const position = lane.compareDocumentPosition(q(c, '[data-testid="flow-lanes"]')!);
    expect(position & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(q(c, '[data-testid="flow-counter-nobody"]')!.textContent).toContain('3');
  });

  it('saves_the_profile_chosen_in_the_picker_on_the_task', async () => {
    const w = orphans();
    const c = await mount(w);
    const entry = q(c, '[data-testid="flow-nobody-entry"][data-item-id="orphan"]')!;
    const radio = qa<HTMLInputElement>(entry, 'input[type="radio"]').find((r) => r.value === 'notes-p')!;
    radio.click();
    await settle();
    expect(w.patches).toEqual([{ id: 'orphan', body: { run_settings: { agent_profile_id: 'notes-p' } } }]);
  });

  it('is_not_drawn_when_every_task_has_a_lane', async () => {
    const c = await mount(threeLanes());
    expect(q(c, '[data-testid="flow-nobody"]')).toBeNull();
  });

  it('shows_only_that_lane_when_its_counter_is_pressed', async () => {
    const c = await mount(orphans());
    q<HTMLButtonElement>(c, '[data-testid="flow-counter-nobody"]')!.click();
    await flush();
    expect(q(c, '[data-testid="flow-nobody"]')).not.toBeNull();
    expect(qa(c, '[data-testid="flow-lane"]')).toEqual([]);
  });
});

describe('Orchestration: teams not used here', () => {
  it('lists_each_with_Make_a_profile_for_it_off_and_the_reason', async () => {
    const w = threeLanes();
    w.teams.push(team('spare', STEPS, { profiles: [] }), team('named', STEPS, { profiles: [{ id: 'n', name: 'Named profile' }] }));
    const c = await mount(w);
    const row = q(c, '[data-testid="flow-unused"]')!;
    expect(row.textContent).toContain('Teams not used here (2)');
    const spare = q(row, '[data-testid="flow-unused-spare"]')!;
    const button = q<HTMLButtonElement>(spare, 'button')!;
    expect(button.textContent).toBe('Make a profile for it');
    expect(button.disabled).toBe(true);
    expect(spare.textContent).toContain('Profiles are made on the Agents page, under Teams.');
    expect(q(row, '[data-testid="flow-unused-named"]')!.textContent).toContain('Named profile already names it');
    expect(q<HTMLAnchorElement>(spare, 'a')!.getAttribute('href')).toBe('/agents');
  });
});

describe('Orchestration: acting', () => {
  function movable(): World {
    return world({
      teams: [team('saas-notes', STEPS), team('saas-code', [])],
      profiles: [profile('notes-p', 'saas-notes'), profile('code-p', 'saas-code')],
      items: [
        item('go', { title: 'Collect the notes', run_settings: { agent_profile_id: 'notes-p', branch: 'b' } }),
        item('busy', { run_settings: { agent_profile_id: 'notes-p' }, status: 'Doing' }),
        item('there', { run_settings: { agent_profile_id: 'code-p' }, status: 'Doing' }),
      ],
      runs: [run('busy', 'queued', 'notes-p', { waiting_reason: 'Waiting behind there' }), run('there', 'running', 'code-p')],
    });
  }
  const confirm = async () => {
    const send = [...document.body.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.textContent === 'Send')!;
    send.click();
    await settle();
  };

  it('opens_the_task_on_its_Execution_tab_from_the_token_and_on_the_question_when_the_run_asks', async () => {
    const w = movable();
    w.runs.push(run('ask', 'waiting_decision', 'notes-p'));
    w.items.push(item('ask', { run_settings: { agent_profile_id: 'notes-p' }, status: 'Doing' }));
    const c = await mount(w);
    q<HTMLButtonElement>(c, '[data-item-id="there"] [data-testid="flow-token-open"]')!.click();
    await flush();
    expect(String(history.get())).toContain('item=there');
    expect(String(history.get())).toContain('tab=execution');
    expect(String(history.get())).not.toContain('run=');
    q<HTMLButtonElement>(c, '[data-item-id="ask"] [data-testid="flow-token-open"]')!.click();
    await flush();
    expect(String(history.get())).toContain('item=ask');
    expect(String(history.get())).toContain('run=r-ask');
  });

  it('sends_a_task_to_another_lane_from_the_Move_to_menu_after_a_confirmation', async () => {
    const w = movable();
    const c = await mount(w);
    const token = q(c, '[data-lane="team:saas-notes"] [data-item-id="go"]')!;
    q<HTMLButtonElement>(token, '[data-testid="move-to"]')!.click();
    await flush();
    const entries = qa(document.body, '[role="menuitem"]');
    expect(entries.map((e) => e.textContent)).toEqual(['saas-code']);
    entries[0].click();
    await flush();
    expect(w.patches).toEqual([]);
    expect(document.body.textContent).toContain('Who does it changes to code-p, in the lane saas-code, for "Collect the notes"');
    await confirm();
    expect(w.patches).toEqual([{ id: 'go', body: { run_settings: { branch: 'b', agent_profile_id: 'code-p' } } }]);
  });

  it('changes_nothing_when_the_question_is_cancelled', async () => {
    const w = movable();
    const c = await mount(w);
    q<HTMLButtonElement>(c, '[data-lane="team:saas-notes"] [data-item-id="go"] [data-testid="move-to"]')!.click();
    await flush();
    qa(document.body, '[role="menuitem"]')[0].click();
    await flush();
    [...document.body.querySelectorAll<HTMLButtonElement>('button')].find((b) => b.textContent === 'Cancel')!.click();
    await settle();
    expect(w.patches).toEqual([]);
  });

  it('moves_a_task_by_dragging_it_to_another_lanes_Up_next_after_a_confirmation', async () => {
    const w = movable();
    const c = await mount(w);
    const token = q(c, '[data-lane="team:saas-notes"] [data-item-id="go"]')!;
    expect(token.getAttribute('draggable')).toBe('true');
    token.dispatchEvent(new Event('dragstart', { bubbles: true }));
    await flush();
    const target = q(c, '[data-lane="team:saas-code"] [data-station="up-next"]')!;
    expect(target.getAttribute('data-accepts')).toBe('true');
    const over = new Event('dragover', { bubbles: true, cancelable: true });
    target.dispatchEvent(over);
    expect(over.defaultPrevented).toBe(true);
    target.dispatchEvent(new Event('drop', { bubbles: true, cancelable: true }));
    await flush();
    await confirm();
    expect(w.patches.map((p) => [p.id, (p.body.run_settings as Json).agent_profile_id])).toEqual([['go', 'code-p']]);
  });

  it('does_not_accept_a_drop_on_its_own_lane', async () => {
    const w = movable();
    const c = await mount(w);
    q(c, '[data-lane="team:saas-notes"] [data-item-id="go"]')!.dispatchEvent(new Event('dragstart', { bubbles: true }));
    await flush();
    const own = q(c, '[data-lane="team:saas-notes"] [data-station="up-next"]')!;
    expect(own.getAttribute('data-accepts')).toBe('false');
    const over = new Event('dragover', { bubbles: true, cancelable: true });
    own.dispatchEvent(over);
    expect(over.defaultPrevented).toBe(false);
    own.dispatchEvent(new Event('drop', { bubbles: true, cancelable: true }));
    await settle();
    expect(w.patches).toEqual([]);
  });

  it('does_not_move_a_token_in_flight_and_says_why', async () => {
    const w = movable();
    const c = await mount(w);
    const queued = q(c, '[data-lane="team:saas-notes"] [data-item-id="busy"]')!;
    expect(queued.getAttribute('data-station')).toBe('up-next');
    expect(queued.getAttribute('draggable')).toBe('false');
    expect(q(queued, '[data-testid="flow-token-queued-reason"]')!.textContent).toContain('Waiting behind there');
    const button = q<HTMLButtonElement>(queued, '[data-testid="move-to"]')!;
    expect(button.getAttribute('aria-disabled')).toBe('true');
    expect(button.title).toBe('A run holds this task, so it cannot change hands');
    button.click();
    await flush();
    expect(qa(document.body, '[role="menuitem"]')).toEqual([]);
    queued.dispatchEvent(new Event('dragstart', { bubbles: true }));
    q(c, '[data-lane="team:saas-code"] [data-station="up-next"]')!.dispatchEvent(new Event('drop', { bubbles: true, cancelable: true }));
    await settle();
    expect(w.patches).toEqual([]);
    // A running token has no Move to… at all.
    expect(q(c, '[data-lane="team:saas-code"] [data-item-id="there"] [data-testid="move-to"]')).toBeNull();
  });

  it('names_the_task_a_token_waits_for_and_lights_it_up_when_it_is_pointed_at', async () => {
    const w = movable();
    w.deps = { go: [{ id: 'd', source_item_id: 'there', target_item_id: 'go', dependency_type: 'blocks', created_at: '' }] };
    const c = await mount(w);
    const after = q(c, '[data-item-id="go"] [data-testid="flow-token-after"]')!;
    expect(after.textContent).toBe('after: there · saas-code');
    expect(q(c, '[data-item-id="go"] [data-testid="flow-token-start"]')!.textContent).toBe('by hand until the task it waits for is done');
    const blocker = q(c, '[data-lane="team:saas-code"] [data-item-id="there"]')!;
    expect(blocker.style.boxShadow).not.toContain('2px');
    after.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }));
    after.dispatchEvent(new Event('mouseenter'));
    await flush();
    expect(blocker.style.boxShadow).toContain('2px');
    after.dispatchEvent(new Event('mouseleave'));
    await flush();
    expect(blocker.style.boxShadow).not.toContain('2px');
  });
});

describe('Orchestration: following the stream', () => {
  it('says_in_a_polite_sentence_where_a_task_went_when_a_run_moves_to_its_next_step', async () => {
    const w = world({
      teams: [team('saas-notes', STEPS)],
      profiles: [profile('notes-p', 'saas-notes')],
      items: [item('a', { title: 'Collect the notes', run_settings: { agent_profile_id: 'notes-p' }, status: 'Doing' })],
      runs: [run('a', 'running', 'notes-p', { latest_hop: { role: 'Lead', kind: 'started', verdict: null, cycle: null } })],
    });
    const c = await mount(w);
    const live = q(c, '[data-testid="flow-live"]')!;
    expect(live.textContent).toBe('');
    w.runs = [run('a', 'running', 'notes-p', { latest_hop: { role: 'Reviewer', kind: 'started', verdict: null, cycle: null } })];
    window.dispatchEvent(new Event(ITEM_UPDATED_EVENT));
    await settle();
    expect(live.textContent).toBe('Collect the notes: review, saas-notes');
    expect(q(c, '[data-station="step-1"] [data-item-id="a"]')).not.toBeNull();
    expect(q(c, '[data-item-id="a"]')!.className).toContain('flow-arrive');
  });
});

describe('Orchestration: Map and Compare', () => {
  afterEach(() => {
    try {
      localStorage.clear();
    } catch {
      /* nothing to clear */
    }
  });

  /** Three lanes as before, a role with no profile and a team no profile names. */
  function mapWorld(): World {
    const w = threeLanes();
    w.roles = [...w.roles, { id: 'role-2', name: 'Scribe', agent_profile_id: 'notes-p', project_id: 'p1' }, { id: 'role-3', name: 'Orphan', agent_profile_id: null, project_id: 'p1' }];
    w.teams = [...w.teams, team('lonely', null, { profiles: [], reported: false, report: null })];
    w.runners = [{ runner_id: 'r1', name: 'This computer (ox)', state: 'active', online: true, total_capacity: 1, available_capacity: 1, capability_snapshot: null }];
    w.teams = w.teams.map((t) => ({ ...t, runner: t.reported ? 'r1' : null }));
    return w;
  }

  const modeButton = (c: ParentNode, mode: 'flow' | 'map') => q<HTMLButtonElement>(c, `[data-testid="orchestration-mode-${mode}"]`)!;
  const node = (c: ParentNode, id: string) => qa<SVGGElement>(c, '[data-testid="map-node"], [data-testid="map-column"]').find((n) => n.getAttribute('data-node') === id)!;
  const press = async (el: Element, key: string) => {
    el.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
    await flush();
  };

  it('opens_in_Flow_and_switches_to_Map_with_the_diagram_and_the_table_under_it', async () => {
    const c = await mount(mapWorld());
    expect(modeButton(c, 'flow').getAttribute('aria-pressed')).toBe('true');
    expect(q(c, '[data-testid="map"]')).toBeNull();
    modeButton(c, 'map').click();
    await settle();
    expect(modeButton(c, 'map').getAttribute('aria-pressed')).toBe('true');
    expect(q(c, '[data-testid="flow"]')).toBeNull();
    const map = q(c, '[data-testid="map"]')!;
    const compare = q(c, '[data-testid="compare"]')!;
    expect(map).not.toBeNull();
    // The table sits under the diagram.
    expect(map.compareDocumentPosition(compare) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    modeButton(c, 'flow').click();
    await settle();
    expect(q(c, '[data-testid="flow"]')).not.toBeNull();
  });

  it('remembers_the_mode_in_this_browser_and_falls_back_to_Flow_when_storage_is_closed', async () => {
    const first = await mount(mapWorld());
    modeButton(first, 'map').click();
    await settle();
    expect(localStorage.getItem('tack_orchestration_mode')).toBe('map');
    while (disposers.length) disposers.pop()!();
    document.body.innerHTML = '';
    const again = await mount(mapWorld());
    expect(modeButton(again, 'map').getAttribute('aria-pressed')).toBe('true');
    expect(q(again, '[data-testid="map"]')).not.toBeNull();

    while (disposers.length) disposers.pop()!();
    document.body.innerHTML = '';
    const read = Storage.prototype.getItem;
    const write = Storage.prototype.setItem;
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(function (this: Storage, key: string) {
      if (key === 'tack_orchestration_mode') throw new Error('closed');
      return read.call(this, key);
    });
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(function (this: Storage, key: string, value: string) {
      if (key === 'tack_orchestration_mode') throw new Error('closed');
      write.call(this, key, value);
    });
    const closed = await mount(mapWorld());
    expect(q(closed, '[data-testid="flow"]')).not.toBeNull();
    modeButton(closed, 'map').click();
    await settle();
    expect(q(closed, '[data-testid="map"]')).not.toBeNull();
  });

  it('draws_the_four_columns_with_a_title_and_a_description_and_every_node_focusable_with_a_name', async () => {
    const c = await mount(mapWorld());
    modeButton(c, 'map').click();
    await settle();
    const svg = q<SVGSVGElement>(c, '[data-testid="map"] svg')!;
    expect(q(svg, 'title')!.textContent).toContain('Roles, profiles, teams and members');
    expect(q(svg, 'desc')).not.toBeNull();
    expect(qa(c, '[data-testid="map-column"]').map((n) => n.getAttribute('aria-label')!.split('.')[0])).toEqual(['Roles', 'Profiles', 'Teams', 'Members']);
    const nodes = qa(c, '[data-testid="map-node"]');
    expect(nodes.length).toBeGreaterThan(6);
    for (const n of nodes) {
      expect(n.getAttribute('role')).toBe('button');
      expect(n.hasAttribute('tabindex')).toBe(true);
      expect(n.getAttribute('aria-label')).toMatch(/Press Enter to show it in Flow/);
    }
    // Only one stop is in the tab order.
    expect([...nodes, ...qa(c, '[data-testid="map-column"]')].filter((n) => n.getAttribute('tabindex') === '0').length).toBe(1);
  });

  it('draws_a_role_with_no_profile_and_a_team_no_profile_names_dashed_with_the_fix_as_a_link', async () => {
    const c = await mount(mapWorld());
    modeButton(c, 'map').click();
    await settle();
    const orphan = node(c, 'role:Orphan');
    expect(orphan.getAttribute('data-dashed')).toBe('true');
    expect(q(orphan, 'rect')!.getAttribute('stroke-dasharray')).toBeTruthy();
    const lonely = node(c, 'team:lonely');
    expect(lonely.getAttribute('data-dashed')).toBe('true');
    const fixes = qa<SVGAElement>(c, '[data-testid="map-fix"]').map((a) => a.getAttribute('href'));
    expect(fixes).toContain('/projects/p1/settings?tab=automation');
    expect(fixes).toContain('/agents');
    expect(node(c, 'team:saas-notes').getAttribute('data-dashed')).toBeNull();
  });

  it('moves_between_nodes_with_the_arrow_keys', async () => {
    const c = await mount(mapWorld());
    modeButton(c, 'map').click();
    await settle();
    const first = node(c, 'column:roles');
    first.focus();
    expect(document.activeElement).toBe(first);
    await press(first, 'ArrowDown');
    // The first role down the column is the one drawn highest.
    const top = qa(c, '[data-testid="map-node"]')
      .filter((n) => n.getAttribute('data-kind') === 'role')
      .sort((a, b) => Number(q(a, 'rect')!.getAttribute('y')) - Number(q(b, 'rect')!.getAttribute('y')))[0];
    expect(document.activeElement).toBe(top);
    await press(document.activeElement!, 'ArrowRight');
    expect(document.activeElement!.getAttribute('data-kind')).toBe('profile');
    await press(document.activeElement!, 'ArrowLeft');
    expect(document.activeElement!.getAttribute('data-kind')).toBe('role');
    // The stop that has the focus is the one in the tab order.
    expect(document.activeElement!.getAttribute('tabindex')).toBe('0');
    await press(node(c, 'role:Idler'), 'ArrowRight');
    expect(document.activeElement!.getAttribute('data-node')).toBe('profile:idle-p');
  });

  it('opens_Flow_narrowed_to_the_lanes_a_node_passes_through_and_clears_back', async () => {
    const c = await mount(mapWorld());
    modeButton(c, 'map').click();
    await settle();
    node(c, 'role:Idler').dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await settle();
    expect(q(c, '[data-testid="map"]')).toBeNull();
    expect(laneNames(c)).toEqual(['idle-team']);
    expect(q(c, '[data-testid="flow-scope"]')!.textContent).toContain('the role Idler');
    // The visit does not become the mode left on: Map stays what was saved.
    expect(localStorage.getItem('tack_orchestration_mode')).toBe('map');
    q<HTMLButtonElement>(c, '[data-testid="flow-scope-clear"]')!.click();
    await settle();
    expect(laneNames(c).length).toBe(3);
    expect(q(c, '[data-testid="flow-scope"]')).toBeNull();
  });

  it('opens_Flow_from_the_keyboard_with_Enter_and_from_a_column_header', async () => {
    const c = await mount(mapWorld());
    modeButton(c, 'map').click();
    await settle();
    await press(node(c, 'team:saas-code'), 'Enter');
    expect(laneNames(c)).toEqual(['saas-code']);
    modeButton(c, 'map').click();
    await settle();
    expect(q(c, '[data-testid="flow-scope"]')).toBeNull();
    node(c, 'column:teams').dispatchEvent(new MouseEvent('click', { bubbles: true }));
    await settle();
    expect(q(c, '[data-testid="flow-scope"]')!.textContent).toContain('every team');
    expect(laneNames(c).sort()).toEqual(['idle-team', 'saas-code', 'saas-notes']);
  });

  it('draws_every_link_and_a_link_with_no_open_task_as_a_hairline', async () => {
    const c = await mount(mapWorld());
    modeButton(c, 'map').click();
    await settle();
    const links = qa(c, '[data-testid="map-link"]');
    const idle = links.filter((l) => Number(l.getAttribute('data-tasks')) === 0);
    expect(idle.length).toBeGreaterThan(0);
    expect(idle.every((l) => l.getAttribute('stroke-width') === '1')).toBe(true);
    expect(q(c, '[data-testid="map-summary"]')!.textContent).not.toContain('appears when');
  });

  it('writes_the_same_routes_as_text_one_line_for_each_link', async () => {
    const c = await mount(mapWorld());
    modeButton(c, 'map').click();
    await settle();
    const links = qa(c, '[data-testid="map-link"]');
    const lines = qa(c, '[data-route]').filter((li) => links.some((l) => l.getAttribute('data-link') === li.getAttribute('data-route')));
    expect(links.length).toBeGreaterThan(0);
    expect(lines.length).toBe(links.length);
    for (const l of links) {
      const line = lines.find((li) => li.getAttribute('data-route') === l.getAttribute('data-link'))!;
      const tasks = Number(l.getAttribute('data-tasks'));
      expect(line.textContent).toContain(`${tasks} open ${tasks === 1 ? 'task' : 'tasks'}`);
    }
    // The text is in the page whether or not the fold is open, and says what is dashed with its fix.
    const fold = q<HTMLDetailsElement>(c, '[data-testid="map-routes"]')!;
    expect(fold.open).toBe(false);
    expect(fold.textContent).toContain('Routes as text');
    const notes = q(c, '[data-testid="map-routes-notes"]')!;
    expect(notes.textContent).toContain('Orphan');
    expect(qa<HTMLAnchorElement>(notes, 'a').map((a) => a.getAttribute('href'))).toContain('/projects/p1/settings?tab=automation');
  });

  it('compares_the_teams_in_a_table_with_row_and_column_headers', async () => {
    const c = await mount(mapWorld());
    modeButton(c, 'map').click();
    await settle();
    const table = q<HTMLTableElement>(c, '[data-testid="compare"] table')!;
    expect(qa(table, 'thead th[scope="col"]').length).toBe(1 + 4);
    expect(qa(table, 'tbody th[scope="row"]').map((th) => th.textContent)).toContain('Token limit per run');
    expect(qa(table, '[data-testid="compare-team"]').map((th) => th.textContent!.trim().split('No computer')[0].trim())).toEqual(['idle-team', 'lonely', 'saas-code', 'saas-notes']);
    const cellFor = (rowKey: string, teamName: string) => {
      const col = qa(table, '[data-testid="compare-team"]').findIndex((th) => th.textContent!.startsWith(teamName));
      return qa(q(table, `[data-row="${rowKey}"]`)!, 'td')[col];
    };
    expect(cellFor('works-in', 'saas-notes').textContent).toContain('The repository, on a branch');
    expect(cellFor('network', 'saas-notes').textContent).toBe('No network');
    expect(cellFor('members', 'saas-notes').textContent).toContain('Lead · m');
    expect(cellFor('steps', 'saas-notes').textContent).toContain('checked by a reviewer');
    expect(cellFor('merge', 'saas-notes').textContent).toBe('You merge (L3)');
  });

  it('lets_a_compare_cell_wrap_at_its_words_inside_equal_columns_and_carries_its_full_text_as_a_tooltip', async () => {
    const c = await mount(mapWorld());
    modeButton(c, 'map').click();
    await settle();
    const table = q<HTMLTableElement>(c, '[data-testid="compare"] table')!;
    // Fixed, equal columns: a long cell wraps instead of widening the table past its frame.
    expect(table.style.tableLayout).toBe('fixed');
    expect(table.style.minWidth).not.toBe('max-content');
    expect(qa(table, 'tbody th[scope="row"]').every((th) => !th.className.includes('whitespace-nowrap'))).toBe(true);
    const step = qa(table, '[data-row="steps"] li').find((li) => li.textContent!.includes('checked by a reviewer'))!;
    expect(step.getAttribute('title')).toBe(step.textContent);
    const text = qa(table, '[data-row="network"] td span')[0];
    expect(text.getAttribute('title')).toBe(text.textContent);
  });

  it('reads_not_reported_for_what_a_team_left_out_with_the_reason_on_hover', async () => {
    const c = await mount(mapWorld());
    modeButton(c, 'map').click();
    await settle();
    const lonelyCol = qa(c, '[data-testid="compare-team"]').findIndex((th) => th.textContent!.startsWith('lonely'));
    const cell = qa(q(c, '[data-row="works-in"]')!, 'td')[lonelyCol];
    const missing = q(cell, '[data-missing="true"]')!;
    expect(missing.textContent).toContain('not reported');
    expect(missing.getAttribute('title')).toContain('No computer reports this team');
    expect(qa(c, '[data-missing="true"]').length).toBeGreaterThan(3);
  });

  it('says_why_a_fact_is_not_reported_beside_it_collapses_a_row_nobody_filled_and_hints_at_hidden_columns', async () => {
    const c = await mount(mapWorld());
    modeButton(c, 'map').click();
    await settle();
    // A reason that is not the column's own is written out beside "not reported".
    const reasons = qa(c, '[data-testid="compare-reason"]').map((e) => e.textContent);
    expect(reasons).toContain('No profile names this team, so no limit is set for it.');
    expect(reasons.some((r) => r!.startsWith('No computer reports this team'))).toBe(false); // the header says that once
    // A row no team filled is one line across the table.
    const collapsed = qa(c, '[data-collapsed="true"]');
    expect(collapsed.length).toBeGreaterThan(0);
    expect(collapsed[0].getAttribute('colspan')).toBe('4');
    // It says it is every team's, not the first column's alone.
    expect(collapsed[0].textContent).toContain('not reported by any of the 4 teams');
    // The table is wider than its frame: the hint and the edge appear.
    const frame = q<HTMLElement>(c, '[aria-label="Teams compared"]')!;
    Object.defineProperty(frame, 'clientWidth', { configurable: true, value: 100 });
    Object.defineProperty(frame, 'scrollWidth', { configurable: true, value: 400 });
    frame.dispatchEvent(new Event('scroll'));
    await settle();
    expect(q(c, '[data-testid="compare-scroll-hint"]')!.textContent).toContain('to the right');
    expect(q(c, '[data-testid="compare-edge-right"]')!.getAttribute('aria-hidden')).toBe('true');
    expect(q(c, '[data-testid="compare-edge-left"]')).toBeNull();
  });
});
