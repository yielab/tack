import { describe, it, expect } from 'vitest';
import type { AgentProfileSummary } from '../../shared/execution/api';
import type { TeamEntry, TeamMember, TeamStep } from '../../shared/execution/teams';
import type { Item } from '../../shared/types';
import { buildFlow, type FlowInput, type RoleProfile } from './flow';
import {
  NO_ROLE_ID,
  buildCompare,
  buildMap,
  clip,
  columnHeaderId,
  countCrossings,
  linkPath,
  linkWidth,
  navigate,
  orderRows,
  profileTokenLimit,
  routesText,
  scopeOfNode,
  type MapInput,
  type MapModel,
} from './map';

const item = (id: string, over: Partial<Item> = {}): Item =>
  ({
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
  }) as Item;

const profile = (id: string, team?: string, agent?: string, over: Partial<AgentProfileSummary> = {}): AgentProfileSummary => ({
  agent_profile_id: id,
  name: id,
  instructions: '',
  tool_policy: { ...(agent ? { agent } : {}), ...(team ? { docket: { team } } : {}) },
  limits: null,
  ...over,
});

const STEPS: TeamStep[] = [
  { name: 'write', kind: 'agent', role: 'Lead' },
  { name: 'review', kind: 'agent', role: 'Reviewer', checked_by: 'reviewer', sends_back_to: 'write', sends_back_max: 2 },
];

const MEMBERS: TeamMember[] = [
  { role: 'Lead', model: 'm1', writes_project: true, can_propose: true },
  { role: 'Reviewer', model: 'm2', writes_project: false },
];

const team = (name: string, over: Partial<TeamEntry> = {}, report: Partial<NonNullable<TeamEntry['report']>> | null = {}): TeamEntry =>
  ({
    name,
    harness: 'docket',
    reported: true,
    runner: 'run-1',
    profiles: [{ id: `${name}-p`, name: `${name}-p` }],
    report: report === null ? null : { name, profile: `${name}-p`, steps: STEPS, works_in: 'repository', decides: 'to_the_end', network: 'none', members: MEMBERS, ...report },
    ...over,
  }) as TeamEntry;

const flowOf = (over: Partial<FlowInput> & Pick<FlowInput, 'profiles' | 'teams'>) =>
  buildFlow({
    items: [],
    roles: new Map(),
    blockers: new Map(),
    runs: [],
    projectRoles: [],
    defaultProfileId: null,
    statuses: [{ name: 'To Do', category: 'todo' }, { name: 'Done', category: 'done' }],
    startStatus: 'To Do',
    autoStart: true,
    autonomy: { level: 'L3', teams: [] },
    ...over,
  });

const roleOf = (name: string, profileId: string | null): RoleProfile => ({ name, profileId });

/** The project of the examples: Writer and Reviewer have profiles on two teams, Idle has none, a single agent is the default. */
function world() {
  const profiles = [profile('writer-p', 'saas-notes'), profile('review-p', 'saas-review'), profile('solo', undefined, 'claude-code')];
  const teams = [team('saas-notes'), team('saas-review'), team('orphan', { profiles: [] }, { members: [] })];
  const projectRoles = [roleOf('Writer', 'writer-p'), roleOf('Reviewer', 'review-p'), roleOf('Idle', null)];
  const items = [item('t1'), item('t2'), item('t3'), item('t4')];
  const roles = new Map<string, RoleProfile[]>([
    ['t1', [roleOf('Writer', 'writer-p')]],
    ['t2', [roleOf('Writer', 'writer-p')]],
    ['t3', [roleOf('Reviewer', 'review-p')]],
    ['t4', []],
  ]);
  const flow = flowOf({ items, roles, profiles, teams, projectRoles, defaultProfileId: 'solo' });
  const input: MapInput = {
    flow,
    roles: projectRoles,
    profiles,
    teams,
    defaultProfileId: 'solo',
    runners: [{ runner_id: 'run-1', name: 'This computer (ox)' }],
  };
  return { input, model: buildMap(input), flow, profiles, teams };
}

const ids = (m: MapModel, column: string) => m.nodes.filter((n) => n.column === column).map((n) => n.id);
const node = (m: MapModel, id: string) => m.nodes.find((n) => n.id === id)!;
const link = (m: MapModel, id: string) => m.links.find((l) => l.id === id);

describe('buildMap: the four columns', () => {
  const { model } = world();

  it('puts_roles_profiles_teams_and_members_in_their_own_column', () => {
    expect(ids(model, 'roles').sort()).toEqual(['role-none', 'role:Idle', 'role:Reviewer', 'role:Writer']);
    expect(ids(model, 'profiles').sort()).toEqual(['profile:review-p', 'profile:solo', 'profile:writer-p']);
    expect(ids(model, 'teams').sort()).toEqual(['agent:claude-code', 'team:orphan', 'team:saas-notes', 'team:saas-review']);
    expect(ids(model, 'members').sort()).toEqual([
      'member:team:saas-notes:Lead',
      'member:team:saas-notes:Reviewer',
      'member:team:saas-review:Lead',
      'member:team:saas-review:Reviewer',
    ]);
    expect(model.columns.map((c) => c.key)).toEqual(['roles', 'profiles', 'teams', 'members']);
  });

  it('places_every_node_in_the_x_of_its_column_and_gives_each_column_distinct_rows', () => {
    for (const key of ['roles', 'profiles', 'teams', 'members']) {
      const column = model.nodes.filter((n) => n.column === key);
      expect(new Set(column.map((n) => n.x)).size).toBe(1);
      expect(new Set(column.map((n) => n.row)).size).toBe(column.length);
    }
    const xs = ['roles', 'profiles', 'teams', 'members'].map((k) => model.nodes.find((n) => n.column === k)!.x);
    expect(xs).toEqual([...xs].sort((a, b) => a - b));
    expect(model.width).toBeGreaterThan(xs[3]);
  });

  it('says_where_each_team_runs_and_names_a_team_no_computer_reports', () => {
    expect(node(model, 'team:saas-notes').sub).toBe('Runs on this computer');
    const { input } = world();
    const other = buildMap({ ...input, runners: [{ runner_id: 'run-1', name: 'build-box' }] });
    expect(node(other, 'team:saas-notes').sub).toBe('Runs on build-box');
    const away = buildMap({ ...input, runners: [{ runner_id: 'other', name: 'x' }] });
    expect(node(away, 'team:saas-notes').sub).toBe('Runs on another computer');
    const unreported = buildMap({ ...input, teams: [team('saas-notes', { reported: false, report: null, runner: null }), ...input.teams.slice(1)] });
    expect(node(unreported, 'team:saas-notes').sub).toBe('No computer reports it');
  });

  it('draws_a_member_with_its_model_and_what_it_can_do', () => {
    expect(node(model, 'member:team:saas-notes:Lead').sub).toBe('m1 · Changes the project · Can propose items');
    expect(node(model, 'member:team:saas-notes:Reviewer').sub).toBe('m2 · Reads only');
  });
});

describe('buildMap: links are as wide as the open tasks routed along them', () => {
  const { model } = world();

  it('counts_the_open_tasks_on_each_link', () => {
    expect(link(model, 'role:Writer>profile:writer-p')!.tasks).toBe(2);
    expect(link(model, 'role:Reviewer>profile:review-p')!.tasks).toBe(1);
    expect(link(model, 'role-none>profile:solo')!.tasks).toBe(1);
    expect(link(model, 'profile:writer-p>team:saas-notes')!.tasks).toBe(2);
    expect(link(model, 'profile:solo>agent:claude-code')!.tasks).toBe(1);
    expect(link(model, 'team:saas-notes>member:team:saas-notes:Lead')!.tasks).toBe(2);
  });

  it('is_wider_with_more_tasks_and_a_hairline_with_none', () => {
    expect(linkWidth(0)).toBe(1);
    expect(linkWidth(1)).toBeGreaterThan(linkWidth(0));
    expect(linkWidth(2)).toBeGreaterThan(linkWidth(1));
    expect(linkWidth(500)).toBeLessThanOrEqual(22);
    expect(link(model, 'role:Writer>profile:writer-p')!.width).toBe(linkWidth(2));
  });

  it('keeps_a_link_with_no_task_because_the_route_exists', () => {
    expect(link(model, 'profile:review-p>team:saas-review')!.tasks).toBe(1);
    const { input } = world();
    const idle = buildMap({ ...input, flow: flowOf({ profiles: input.profiles, teams: input.teams, projectRoles: input.roles, defaultProfileId: 'solo' }) });
    expect(link(idle, 'role:Writer>profile:writer-p')).toMatchObject({ tasks: 0, width: 1 });
  });

  it('does_not_count_a_task_that_ended', () => {
    const { input } = world();
    const finished = buildFlow({
      ...{
        items: [item('t1', { status: 'Done' })],
        roles: new Map([['t1', [roleOf('Writer', 'writer-p')]]]),
        blockers: new Map(),
        runs: [{ request_id: 'r', item_id: 't1', state: 'succeeded', cancellation_requested_at: null, created_at: '2026-01-02T00:00:00Z', agent_profile_id: 'writer-p' }],
        profiles: input.profiles,
        teams: input.teams,
        projectRoles: input.roles,
        defaultProfileId: null,
        statuses: [{ name: 'To Do', category: 'todo' }, { name: 'Done', category: 'done' }],
        startStatus: 'To Do',
        autoStart: true,
        autonomy: { level: 'L3', teams: [] },
      },
    });
    const m = buildMap({ ...input, flow: finished });
    expect(link(m, 'role:Writer>profile:writer-p')!.tasks).toBe(0);
  });

  it('draws_a_link_from_the_right_edge_of_one_node_to_the_left_edge_of_the_next', () => {
    const l = link(model, 'role:Writer>profile:writer-p')!;
    const d = linkPath(model, l);
    const a = node(model, l.from);
    const b = node(model, l.to);
    expect(d.startsWith(`M${a.x + 184},`)).toBe(true);
    expect(d.endsWith(`${b.x},${b.y + 31}`)).toBe(true);
  });
});

describe('orderRows: rows by the barycentre of their neighbours', () => {
  it('uncrosses_a_known_example', () => {
    // A joins Q and B joins P, but P is listed first: the two lines cross until the rows swap.
    const columns = [['A', 'B'], ['P', 'Q']];
    const links = [{ from: 'A', to: 'Q' }, { from: 'B', to: 'P' }];
    expect(countCrossings(columns, links)).toBe(1);
    const ordered = orderRows(columns, links);
    expect(ordered[1]).toEqual(['Q', 'P']);
    expect(countCrossings(ordered, links)).toBe(0);
  });

  it('never_moves_a_column_or_drops_a_node', () => {
    const columns = [['A', 'B', 'C'], ['P', 'Q'], ['X']];
    const out = orderRows(columns, [{ from: 'C', to: 'P' }, { from: 'A', to: 'Q' }, { from: 'P', to: 'X' }]);
    expect(out.map((c) => [...c].sort())).toEqual([['A', 'B', 'C'], ['P', 'Q'], ['X']]);
  });

  it('keeps_the_order_of_nodes_with_no_neighbour', () => {
    expect(orderRows([['A', 'B'], ['P', 'Q']], [])).toEqual([['A', 'B'], ['P', 'Q']]);
  });

  it('orders_the_built_map_so_that_no_two_lines_cross_where_the_data_allow_it', () => {
    // Teams are listed by name (alpha, zeta) but the roles reach them in the other order.
    const profiles = [profile('zp', 'zeta'), profile('ap', 'alpha')];
    const teams = [team('alpha'), team('zeta')];
    const roles = [roleOf('Zed', 'zp'), roleOf('Ann', 'ap')];
    const flow = flowOf({ profiles, teams, projectRoles: roles });
    const m = buildMap({ flow, roles, profiles, teams, defaultProfileId: null, runners: null });
    const columns = ['roles', 'profiles', 'teams', 'members'].map((k) => m.columns.find((c) => c.key === k)!.ids);
    expect(countCrossings(columns, m.links)).toBe(0);
    expect(columns[2].slice(0, 2)).toEqual(['team:zeta', 'team:alpha']);
  });
});

describe('buildMap: what is dashed, marked or unrouted', () => {
  const { model } = world();

  it('draws_a_role_with_no_profile_dashed_with_the_fix_in_the_Teams_settings', () => {
    const idle = node(model, 'role:Idle');
    expect(idle.dashed).toBe(true);
    expect(idle.sub).toBe('No profile');
    expect(idle.fix).toEqual({ to: 'automation-teams', text: 'Settings → Automation → Teams' });
    expect(link(model, 'role:Idle>profile:solo')).toBeUndefined();
    expect(model.links.some((l) => l.from === 'role:Idle')).toBe(false);
  });

  it('draws_a_team_no_profile_names_dashed_with_the_fix_on_the_Agents_page', () => {
    const orphan = node(model, 'team:orphan');
    expect(orphan.dashed).toBe(true);
    expect(orphan.fix).toEqual({ to: 'agents-profiles', text: 'Agents → profiles' });
    expect(node(model, 'team:saas-notes').dashed).toBe(false);
  });

  it('marks_the_default_profile_and_only_it', () => {
    expect(model.nodes.filter((n) => n.isDefault).map((n) => n.id)).toEqual(['profile:solo']);
    expect(node(model, 'profile:solo').sub).toContain('Default');
  });

  it('draws_a_role_whose_profile_was_deleted_dashed_too', () => {
    const { input } = world();
    const m = buildMap({ ...input, roles: [...input.roles, roleOf('Ghost', 'gone')] });
    expect(node(m, 'role:Ghost')).toMatchObject({ dashed: true, sub: 'Its profile no longer exists' });
  });

  it('counts_the_tasks_nobody_will_take_on_the_role_that_causes_it', () => {
    const profiles = [profile('writer-p', 'saas-notes')];
    const teams = [team('saas-notes')];
    const roles = [roleOf('Writer', 'writer-p'), roleOf('Idle', null)];
    const flow = flowOf({
      profiles,
      teams,
      projectRoles: roles,
      items: [item('x'), item('y')],
      roles: new Map([['x', [roleOf('Idle', null)]], ['y', []]]),
    });
    const m = buildMap({ flow, roles, profiles, teams, defaultProfileId: null, runners: null });
    expect(node(m, 'role:Idle').stranded).toBe(1);
    expect(node(m, 'role:Idle').scope).toMatchObject({ nobody: ['x'], lanes: [] });
    // The task with no role and no default to fall back on is counted on the "No role" box.
    expect(node(m, NO_ROLE_ID)).toMatchObject({ stranded: 1, sub: 'Chosen on the task' });
    expect(node(m, NO_ROLE_ID).scope.nobody).toEqual(['y']);
  });
});

describe('buildMap: what Flow narrows to', () => {
  const { model } = world();

  it('names_the_lanes_that_pass_through_a_node', () => {
    expect(node(model, 'role:Writer').scope).toMatchObject({ kind: 'role', lanes: ['team:saas-notes'] });
    expect(node(model, 'profile:solo').scope.lanes).toEqual(['agent:claude-code']);
    expect(node(model, 'team:saas-review').scope).toMatchObject({ kind: 'team', lanes: ['team:saas-review'] });
    expect(node(model, 'member:team:saas-review:Lead').scope).toMatchObject({ kind: 'member', lanes: ['team:saas-review'] });
  });

  it('gives_a_team_with_no_lane_a_scope_that_matches_nothing', () => {
    expect(node(model, 'team:orphan').scope.lanes).toEqual([]);
  });

  it('gives_a_column_header_every_lane_of_its_nodes', () => {
    const teams = model.columns.find((c) => c.key === 'teams')!;
    expect(teams.scope.kind).toBe('column');
    expect([...teams.scope.lanes].sort()).toEqual(['agent:claude-code', 'team:saas-notes', 'team:saas-review']);
    expect(scopeOfNode(model, columnHeaderId('teams'))).toBe(teams.scope);
    expect(scopeOfNode(model, 'role:Writer')).toBe(node(model, 'role:Writer').scope);
    expect(scopeOfNode(model, 'nope')).toBeNull();
  });
});

describe('navigate: node by node', () => {
  const { model } = world();
  const col = (key: string) => model.columns.find((c) => c.key === key)!.ids;

  it('goes_down_the_column_from_its_header_and_back_up', () => {
    const roles = col('roles');
    expect(navigate(model, columnHeaderId('roles'), 'down')).toBe(roles[0]);
    expect(navigate(model, roles[0], 'down')).toBe(roles[1]);
    expect(navigate(model, roles[1], 'up')).toBe(roles[0]);
    expect(navigate(model, roles[0], 'up')).toBe(columnHeaderId('roles'));
    expect(navigate(model, columnHeaderId('roles'), 'up')).toBeNull();
    expect(navigate(model, roles[roles.length - 1], 'down')).toBeNull();
  });

  it('goes_right_to_a_linked_node_and_left_back_to_one', () => {
    expect(navigate(model, 'role:Writer', 'right')).toBe('profile:writer-p');
    expect(navigate(model, 'profile:writer-p', 'right')).toBe('team:saas-notes');
    expect(navigate(model, 'team:saas-notes', 'left')).toBe('profile:writer-p');
    expect(navigate(model, 'role:Writer', 'left')).toBeNull();
    expect(navigate(model, 'member:team:saas-notes:Lead', 'right')).toBeNull();
  });

  it('goes_to_the_nearest_row_when_nothing_is_linked_and_header_to_header', () => {
    expect(navigate(model, 'role:Idle', 'right')).not.toBeNull();
    expect(navigate(model, columnHeaderId('roles'), 'right')).toBe(columnHeaderId('profiles'));
    expect(navigate(model, 'unknown', 'down')).toBeNull();
  });
});

describe('routesText: the same routes as sentences', () => {
  const { model } = world();
  const lines = routesText(model);

  it('says_every_link_once_with_its_open_tasks', () => {
    const said = lines.filter((g) => g.key !== 'notes').flatMap((g) => g.lines.map((l) => l.id));
    expect([...said].sort()).toEqual(model.links.map((l) => l.id).sort());
    const writer = lines[0].lines.find((l) => l.id === 'role:Writer>profile:writer-p')!;
    expect(writer.text).toBe('Writer → writer-p: 2 open tasks');
    expect(lines[0].lines.find((l) => l.id === 'role-none>profile:solo')!.text).toBe('No role → solo: 1 open task');
  });

  it('adds_what_is_dashed_with_its_fix_and_what_is_default', () => {
    const notes = lines.find((g) => g.key === 'notes')!.lines;
    expect(notes.find((l) => l.id === 'dashed:role:Idle')!.fix?.to).toBe('automation-teams');
    expect(notes.find((l) => l.id === 'dashed:team:orphan')!.fix?.to).toBe('agents-profiles');
    expect(notes.find((l) => l.id === 'default:profile:solo')!.text).toContain('default profile');
  });
});

describe('clip', () => {
  it('cuts_a_long_name_with_an_ellipsis_and_leaves_a_short_one', () => {
    expect(clip('short', 10)).toBe('short');
    expect(clip('a-very-long-team-name', 10)).toBe('a-very-lo…');
  });
});

describe('buildCompare: the teams side by side', () => {
  const profiles = [
    profile('notes-p', 'saas-notes', undefined, { delivers: 'a change on a branch', limits: { budgets: { tokens: 1_600_000 } } }),
    profile('bare-p', 'bare'),
  ];
  const teams = [
    team('saas-notes', { profiles: [{ id: 'notes-p', name: 'notes-p' }] }),
    team('bare', { profiles: [{ id: 'bare-p', name: 'bare-p' }] }, { works_in: undefined, decides: undefined, network: undefined, members: undefined, steps: undefined }),
    team('gone', { reported: false, report: null, runner: null, profiles: [] }, null),
  ];
  const flow = flowOf({
    profiles,
    teams,
    projectRoles: [roleOf('Writer', 'notes-p')],
    runs: [{ request_id: 'r', item_id: 'i', state: 'running', cancellation_requested_at: null, created_at: '2026-01-02T00:00:00Z', agent_profile_id: 'notes-p', limit: { used: 12_000 } } as never],
    items: [item('i', { status: 'Doing' })],
  });
  const compare = buildCompare({ flow, teams, profiles, autonomy: { level: 'L3', teams: [], provisionalTokensPerRun: 2_000_000 } });
  const row = (key: string) => compare.rows.find((r) => r.key === key)!;
  const at = (key: string, name: string) => row(key).cells[compare.teams.findIndex((t) => t.name === name)];

  it('has_the_teams_as_columns_by_name_and_the_asked_rows_in_order', () => {
    expect(compare.teams.map((t) => t.name)).toEqual(['bare', 'gone', 'saas-notes']);
    expect(compare.rows.map((r) => r.label)).toEqual([
      'Works in',
      'Delivers',
      'Decides',
      'Network',
      'Members',
      'Change the project',
      'Steps and their checks',
      'Can propose',
      'Merge rung',
      'Token limit per run',
      'Tokens today',
    ]);
    expect(compare.rows.every((r) => r.cells.length === 3)).toBe(true);
  });

  it('reads_what_a_team_reported', () => {
    expect(at('works-in', 'saas-notes')).toEqual({ kind: 'text', text: 'The repository, on a branch' });
    expect(at('decides', 'saas-notes')).toEqual({ kind: 'text', text: 'Runs to the end' });
    expect(at('network', 'saas-notes')).toEqual({ kind: 'text', text: 'No network' });
    expect(at('members', 'saas-notes')).toEqual({ kind: 'list', items: ['Lead · m1', 'Reviewer · m2'] });
    expect(at('writes', 'saas-notes')).toEqual({ kind: 'list', items: ['Lead'] });
    expect(at('proposes', 'saas-notes')).toEqual({ kind: 'list', items: ['Lead'] });
    expect((at('steps', 'saas-notes') as { items: string[] }).items[1]).toContain('checked by a reviewer');
    expect(at('delivers', 'saas-notes')).toEqual({ kind: 'text', text: 'a change on a branch' });
    expect(at('merge', 'saas-notes')).toEqual({ kind: 'text', text: 'You merge (L3)' });
    expect(at('limit', 'saas-notes')).toEqual({ kind: 'text', text: '1 600 000 tokens' });
    expect(at('today', 'saas-notes')).toEqual({ kind: 'text', text: '12 000 tokens' });
  });

  it('says_not_reported_with_the_reason_for_a_fact_a_team_left_out', () => {
    for (const key of ['works-in', 'decides', 'network', 'members', 'steps', 'writes', 'proposes']) {
      expect(at(key, 'bare').kind, key).toBe('missing');
    }
    expect((at('works-in', 'bare') as { reason: string }).reason).toBe('The team did not report where it works.');
    expect((at('delivers', 'bare') as { reason: string }).reason).toContain('No profile of this team says what it delivers');
    expect((at('works-in', 'gone') as { reason: string }).reason).toContain('No computer reports this team');
    expect(at('today', 'bare').kind).toBe('missing');
  });

  it('tells_no_members_apart_from_members_not_reported', () => {
    const none = buildCompare({ flow, teams: [team('t', {}, { members: [] })], profiles, autonomy: null });
    expect(none.rows.find((r) => r.key === 'members')!.cells[0]).toEqual({ kind: 'text', text: 'No members' });
    const noSteps = buildCompare({ flow, teams: [team('t', {}, { steps: [] })], profiles, autonomy: null });
    expect(noSteps.rows.find((r) => r.key === 'steps')!.cells[0]).toEqual({ kind: 'text', text: 'Reports no steps' });
  });

  it('says_when_no_member_changes_the_project_and_when_it_is_not_said', () => {
    const readers = buildCompare({ flow, teams: [team('t', {}, { members: [{ role: 'A', writes_project: false }, { role: 'B', writes_project: false }] })], profiles, autonomy: null });
    expect(readers.rows.find((r) => r.key === 'writes')!.cells[0]).toMatchObject({ kind: 'text' });
    const unsaid = buildCompare({ flow, teams: [team('t', {}, { members: [{ role: 'A' }] })], profiles, autonomy: null });
    expect(unsaid.rows.find((r) => r.key === 'writes')!.cells[0].kind).toBe('missing');
  });

  it('falls_back_to_the_provisional_limit_and_says_so', () => {
    const noLimit = [profile('notes-p', 'saas-notes')];
    const c = buildCompare({ flow, teams: [teams[0]], profiles: noLimit, autonomy: { level: 'L3', teams: [], provisionalTokensPerRun: 2_000_000 } });
    expect(c.rows.find((r) => r.key === 'limit')!.cells[0]).toEqual({ kind: 'text', text: "2 000 000 tokens, the project's provisional limit" });
    const none = buildCompare({ flow, teams: [teams[0]], profiles: noLimit, autonomy: { level: 'L3', teams: [], provisionalTokensPerRun: null } });
    expect(none.rows.find((r) => r.key === 'limit')!.cells[0]).toEqual({ kind: 'text', text: 'No limit' });
  });

  it('says_the_merge_rung_is_unknown_without_the_automation_settings', () => {
    const c = buildCompare({ flow, teams: [teams[0]], profiles, autonomy: null });
    expect(c.rows.find((r) => r.key === 'merge')!.cells[0].kind).toBe('missing');
  });

  it('reads_a_profiles_own_token_limit', () => {
    expect(profileTokenLimit({ limits: { budgets: { tokens: 5 } } })).toBe(5);
    expect(profileTokenLimit({ limits: null })).toBeNull();
    expect(profileTokenLimit({ limits: { budgets: { tokens: 'x' } } })).toBeNull();
  });
});
