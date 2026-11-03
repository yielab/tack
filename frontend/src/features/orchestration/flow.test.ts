import { describe, it, expect } from 'vitest';
import type { AgentProfileSummary, ExecutionSummary, LatestHop } from '../../shared/execution/api';
import type { TeamEntry, TeamStep } from '../../shared/execution/teams';
import { cardRun } from '../../shared/runWithAgent/cardRun';
import type { Dependency, Item } from '../../shared/types';
import {
  ageText,
  endedTally,
  candidateIds,
  laneQueue,
  blockersOf,
  buildFlow,
  capacityText,
  mergeRung,
  moveTarget,
  movesBetween,
  nobodyText,
  placements,
  routeItem,
  scopedNobody,
  sentBackOf,
  showsNobody,
  startOrder,
  stationFor,
  stepIndexOf,
  tokenPasses,
  visibleLanes,
  withProfile,
  type FlowFilter,
  type FlowInput,
  type FlowScope,
  type RoutingContext,
} from './flow';

const item = (p: Partial<Item> & { id: string }): Item =>
  ({
    project_id: 'p1',
    title: p.id,
    item_type: 'task',
    status: 'To Do',
    priority: 'medium',
    estimate_unit: 'story_points',
    tags: [],
    sort_order: 0,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...p,
  }) as Item;

const profile = (id: string, team?: string, agent?: string): AgentProfileSummary => ({
  agent_profile_id: id,
  name: id,
  instructions: '',
  tool_policy: { ...(agent ? { agent } : {}), ...(team ? { docket: { team } } : {}) },
  limits: null,
});

const STEPS: TeamStep[] = [
  { name: 'write', kind: 'agent', role: 'Lead' },
  { name: 'build', kind: 'agent', role: 'Implementer', checked_by: 'command' },
  { name: 'review', kind: 'agent', role: 'Reviewer', checked_by: 'reviewer', sends_back_to: 'build', sends_back_max: 2 },
];

const team = (name: string, over: Partial<TeamEntry> = {}, steps: TeamStep[] | null = STEPS): TeamEntry =>
  ({
    name,
    harness: 'docket',
    reported: true,
    profiles: [{ id: `${name}-profile`, name: `${name}-profile` }],
    report: { name, profile: `${name}-profile`, steps, works_in: 'repository' },
    ...over,
  }) as TeamEntry;

const run = (over: Partial<ExecutionSummary> & { item_id: string }): ExecutionSummary => ({
  request_id: `r-${over.item_id}`,
  state: 'running',
  cancellation_requested_at: null,
  created_at: '2026-01-02T00:00:00Z',
  ...over,
});

const hop = (role: string, over: Partial<LatestHop> = {}): LatestHop => ({ role, kind: 'started', verdict: null, cycle: null, ...over });

const ctx = (over: Partial<RoutingContext> = {}): RoutingContext => ({
  profiles: [profile('notes', 'saas-notes'), profile('solo', undefined, 'claude-code')],
  teams: [team('saas-notes')],
  defaultProfileId: null,
  ...over,
});

const input = (over: Partial<FlowInput> = {}): FlowInput => ({
  items: [],
  roles: new Map(),
  blockers: new Map(),
  runs: [],
  profiles: [profile('notes', 'saas-notes'), profile('solo', undefined, 'claude-code')],
  teams: [team('saas-notes')],
  projectRoles: [],
  defaultProfileId: null,
  statuses: [{ name: 'To Do', category: 'todo' }, { name: 'Doing', category: 'in_progress' }, { name: 'Done', category: 'done' }],
  startStatus: 'To Do',
  autoStart: true,
  autonomy: { level: 'L3', teams: [] },
  ...over,
});

const own = (id: string) => ({ agent_profile_id: id });

describe('routeItem: the profile a task goes to', () => {
  it('takes_the_profile_chosen_on_the_task_before_its_role_and_the_project_default', () => {
    const c = ctx({ defaultProfileId: 'solo' });
    const route = routeItem(item({ id: 'a', run_settings: own('notes') }), [{ name: 'Writer', profileId: 'solo' }], c);
    expect(route).toMatchObject({ kind: 'lane', lane: 'team:saas-notes', source: 'task' });
  });

  it('takes_the_first_role_that_has_a_profile_before_the_project_default', () => {
    const c = ctx({ defaultProfileId: 'solo' });
    const route = routeItem(item({ id: 'a' }), [{ name: 'Empty', profileId: null }, { name: 'Writer', profileId: 'notes' }], c);
    expect(route).toMatchObject({ kind: 'lane', lane: 'team:saas-notes', source: 'role', role: 'Writer' });
  });

  it('falls_back_to_the_project_default', () => {
    expect(routeItem(item({ id: 'a' }), [], ctx({ defaultProfileId: 'solo' }))).toMatchObject({
      kind: 'lane',
      lane: 'agent:claude-code',
      source: 'project',
    });
  });

  it('puts_a_profile_that_names_no_team_in_the_lane_of_its_agent', () => {
    const route = routeItem(item({ id: 'a', run_settings: own('solo') }), [], ctx());
    expect(route).toMatchObject({ kind: 'lane', lane: 'agent:claude-code' });
  });

  it('is_unknown_while_the_roles_are_not_loaded_unless_the_task_names_its_own_profile', () => {
    expect(routeItem(item({ id: 'a' }), undefined, ctx())).toBeNull();
    expect(routeItem(item({ id: 'a', run_settings: own('notes') }), undefined, ctx())).toMatchObject({ kind: 'lane' });
  });

  it('says_nobody_when_there_is_no_role_and_no_default', () => {
    const route = routeItem(item({ id: 'a' }), [], ctx());
    expect(route).toEqual({ kind: 'nobody', reason: 'no_role', detail: null });
    expect(nobodyText(route as never)).toContain('no role');
  });

  it('says_nobody_naming_the_role_when_it_has_no_profile', () => {
    const route = routeItem(item({ id: 'a' }), [{ name: 'Content', profileId: null }], ctx());
    expect(route).toEqual({ kind: 'nobody', reason: 'role_without_profile', detail: 'Content' });
    expect(nobodyText(route as never)).toBe('Its role Content has no profile, and the project has no default profile.');
  });

  it('says_nobody_when_the_profile_names_a_team_no_computer_reports', () => {
    const c = ctx({ teams: [team('saas-notes', { reported: false })] });
    const route = routeItem(item({ id: 'a', run_settings: own('notes') }), [], c);
    expect(route).toEqual({ kind: 'nobody', reason: 'team_not_reported', detail: 'saas-notes' });
    expect(nobodyText(route as never)).toBe('No computer reports the team saas-notes that its profile names.');
  });

  it('says_nobody_when_the_chosen_profile_was_deleted', () => {
    const route = routeItem(item({ id: 'a', run_settings: own('gone') }), [], ctx());
    expect(route).toEqual({ kind: 'nobody', reason: 'profile_missing', detail: 'task' });
    expect(nobodyText(route as never)).toContain('chosen on this task');
  });
});

describe('withProfile and blockersOf', () => {
  it('sets_the_profile_and_keeps_the_other_settings', () => {
    expect(withProfile({ branch: 'x' }, 'notes')).toEqual({ branch: 'x', agent_profile_id: 'notes' });
  });

  it('clears_the_profile_and_is_null_when_nothing_is_left', () => {
    expect(withProfile({ agent_profile_id: 'notes' }, null)).toBeNull();
    expect(withProfile({ agent_profile_id: 'notes', branch: 'x' }, null)).toEqual({ branch: 'x' });
  });

  it('reads_a_blocker_from_either_end_of_the_edge', () => {
    const edge = (source: string, target: string, dependency_type: Dependency['dependency_type']): Dependency =>
      ({ id: `${source}${target}`, source_item_id: source, target_item_id: target, dependency_type, created_at: '' }) as Dependency;
    expect(blockersOf('b', [edge('a', 'b', 'blocks'), edge('b', 'c', 'is_blocked_by'), edge('b', 'd', 'blocks'), edge('x', 'b', 'relates_to')])).toEqual(['a', 'c']);
  });
});

describe('stationFor: where a task stands in a team', () => {
  const running = cardRun({ request_id: 'r', state: 'running', cancellation_requested_at: null }, false);

  it('puts_a_task_with_no_run_and_a_queued_run_at_Up_next', () => {
    expect(stationFor(undefined, null, STEPS)).toBe('up-next');
    expect(stationFor(cardRun({ request_id: 'r', state: 'queued', cancellation_requested_at: null }, false), null, STEPS)).toBe('up-next');
  });

  it('puts_a_running_run_at_the_step_its_latest_hop_names_without_regard_to_case', () => {
    expect(stationFor(running, hop('Implementer'), STEPS)).toBe('step-1');
    expect(stationFor(running, hop('REVIEWER', { kind: 'finished', verdict: 'changes', cycle: 1 }), STEPS)).toBe('step-2');
  });

  it('puts_a_running_run_with_no_hop_in_a_team_that_reports_no_steps_at_Working', () => {
    expect(stationFor(running, null, [])).toBe('working');
    expect(stationFor(running, hop('Lead'), [])).toBe('working');
  });

  it('puts_a_running_run_whose_hop_no_step_has_at_Working_too', () => {
    expect(stationFor(running, hop('Janitor'), STEPS)).toBe('working');
    expect(stationFor(running, null, STEPS)).toBe('working');
  });

  it('puts_a_run_that_asks_at_Needs_you_and_one_to_review_at_Ready_for_review', () => {
    expect(stationFor(cardRun({ request_id: 'r', state: 'waiting_decision', cancellation_requested_at: null }, false), hop('Lead'), STEPS)).toBe('needs-you');
    expect(stationFor(cardRun({ request_id: 'r', state: 'succeeded', cancellation_requested_at: null }, true), null, STEPS)).toBe('review');
  });

  it('counts_what_ended_by_how_it_ended_and_leaves_out_a_state_nobody_ended_in', () => {
    const ended = (state: 'done' | 'failed' | 'cancelled') => ({ run: { state } });
    const tally = endedTally([ended('done'), ended('failed'), ended('done'), ended('cancelled'), { run: undefined }] as never);
    expect(tally).toEqual([
      { state: 'done', count: 2 },
      { state: 'cancelled', count: 1 },
      { state: 'failed', count: 1 },
    ]);
    expect(endedTally([ended('failed')] as never)).toEqual([{ state: 'failed', count: 1 }]);
    expect(endedTally([])).toEqual([]);
  });

  it('puts_ended_runs_in_Done', () => {
    for (const state of ['succeeded', 'failed', 'cancelled']) {
      expect(stationFor(cardRun({ request_id: 'r', state, cancellation_requested_at: null }, false), null, STEPS)).toBe('done');
    }
  });

  it('finds_a_step_by_role_and_nothing_for_a_missing_hop', () => {
    expect(stepIndexOf(STEPS, hop('lead'))).toBe(0);
    expect(stepIndexOf(STEPS, null)).toBeNull();
    expect(stepIndexOf(STEPS, hop('x'))).toBeNull();
  });
});

describe('sentBackOf', () => {
  it('counts_the_cycles_out_of_the_most_the_sending_step_allows_and_names_the_sender', () => {
    expect(sentBackOf(STEPS, hop('Implementer', { cycle: 1 }), 1)).toEqual({ n: 1, max: 2, from: 'review' });
  });

  it('is_null_on_the_first_pass', () => {
    expect(sentBackOf(STEPS, hop('Implementer'), 1)).toBeNull();
    expect(sentBackOf(STEPS, hop('Implementer', { cycle: 0 }), 1)).toBeNull();
  });
});

describe('buildFlow: lanes, stations and tokens', () => {
  const flowOf = (over: Partial<FlowInput>) => buildFlow(input(over));

  it('places_a_token_at_the_station_its_latest_hop_names', () => {
    const flow = flowOf({
      items: [item({ id: 'a', run_settings: own('notes') })],
      runs: [run({ item_id: 'a', agent_profile_id: 'notes', latest_hop: hop('Implementer') })],
    });
    const lane = flow.lanes[0];
    expect(lane.name).toBe('saas-notes');
    const at = lane.stations.find((s) => s.tokens.length)!;
    expect(at.key).toBe('step-1');
    expect(at.label).toBe('build');
    expect(at.check).toBe('command');
  });

  it('puts_a_running_task_of_a_team_that_reports_no_steps_at_the_single_Working_station', () => {
    const flow = flowOf({
      teams: [team('saas-notes', {}, [])],
      items: [item({ id: 'a', run_settings: own('notes') })],
      runs: [run({ item_id: 'a', agent_profile_id: 'notes' })],
    });
    const lane = flow.lanes[0];
    expect(lane.reportsSteps).toBe(false);
    expect(lane.stations.map((s) => s.key)).toEqual(['up-next', 'working', 'needs-you', 'done']);
    expect(lane.stations.find((s) => s.key === 'working')!.tokens.map((t) => t.id)).toEqual(['a']);
  });

  it('has_a_station_for_every_step_and_a_Working_one_only_when_a_run_cannot_be_placed', () => {
    const quiet = flowOf({ items: [item({ id: 'a', run_settings: own('notes') })] }).lanes[0];
    expect(quiet.stations.map((s) => s.key)).toEqual(['up-next', 'step-0', 'step-1', 'step-2', 'needs-you', 'done']);
    const lost = flowOf({
      items: [item({ id: 'a', run_settings: own('notes') })],
      runs: [run({ item_id: 'a', agent_profile_id: 'notes', latest_hop: null })],
    }).lanes[0];
    expect(lost.stations.map((s) => s.key)).toContain('working');
  });

  it('puts_a_task_that_was_sent_back_at_the_step_and_marks_the_loop', () => {
    const flow = flowOf({
      items: [item({ id: 'a', run_settings: own('notes') })],
      runs: [run({ item_id: 'a', agent_profile_id: 'notes', latest_hop: hop('Implementer', { cycle: 1 }) })],
    });
    const token = flow.lanes[0].tokens[0];
    expect(token.station).toBe('step-1');
    expect(token.sentBack).toEqual({ n: 1, max: 2, from: 'review' });
  });

  it('puts_a_task_by_the_profile_its_run_used_not_the_one_it_is_routed_to_now', () => {
    const flow = flowOf({
      items: [item({ id: 'a', run_settings: own('solo') })],
      runs: [run({ item_id: 'a', agent_profile_id: 'notes', latest_hop: hop('Lead') })],
    });
    expect(flow.lanes.find((l) => l.key === 'team:saas-notes')!.tokens.map((t) => t.id)).toEqual(['a']);
  });

  it('draws_one_lane_per_team_with_a_profile_in_the_project_and_one_per_single_agent_used', () => {
    const flow = flowOf({
      teams: [team('saas-notes'), team('other')],
      profiles: [profile('notes', 'saas-notes'), profile('solo', undefined, 'claude-code'), profile('x', 'other')],
      items: [item({ id: 'a', run_settings: own('solo') })],
      projectRoles: [{ name: 'Writer', profileId: 'notes' }],
    });
    expect(flow.lanes.map((l) => l.key).sort()).toEqual(['agent:claude-code', 'team:saas-notes']);
    expect(flow.lanes.find((l) => l.key === 'agent:claude-code')!.name).toBe('Claude Code');
    expect(flow.unused.map((u) => u.name)).toEqual(['other']);
  });

  it('orders_lanes_needs_you_first_then_running_then_idle', () => {
    const flow = flowOf({
      teams: [team('aaa'), team('bbb'), team('ccc')],
      profiles: [profile('pa', 'aaa'), profile('pb', 'bbb'), profile('pc', 'ccc')],
      projectRoles: [{ name: 'r', profileId: 'pa' }],
      items: [
        item({ id: 'i-b', run_settings: own('pb') }),
        item({ id: 'i-c', run_settings: own('pc') }),
      ],
      runs: [
        run({ item_id: 'i-b', agent_profile_id: 'pb', state: 'running' }),
        run({ item_id: 'i-c', agent_profile_id: 'pc', state: 'waiting_decision' }),
      ],
    });
    expect(flow.lanes.map((l) => [l.name, l.urgency])).toEqual([
      ['ccc', 'needs'],
      ['bbb', 'running'],
      ['aaa', 'idle'],
    ]);
  });

  it('lists_Up_next_in_the_order_auto_start_takes_and_says_how_each_starts', () => {
    const flow = flowOf({
      items: [
        item({ id: 'low', priority: 'low', run_settings: own('notes') }),
        item({ id: 'urgent', priority: 'critical', sort_order: 5, run_settings: own('notes') }),
        item({ id: 'later', priority: 'medium', status: 'Doing', run_settings: own('notes') }),
      ],
    });
    const lane = flow.lanes[0];
    const next = lane.stations[0].tokens;
    expect(next.map((t) => t.id)).toEqual(['urgent', 'low']);
    expect(next[0].upNext).toMatchObject({ mode: 'itself', text: 'starts by itself at To Do' });
  });

  it('says_by_hand_when_the_project_does_not_start_tasks_or_only_the_default_profile_would_do_it', () => {
    const off = flowOf({ autoStart: false, items: [item({ id: 'a', run_settings: own('notes') })] }).lanes[0].tokens[0];
    expect(off.upNext).toMatchObject({ mode: 'hand', text: 'by hand: the project does not start tasks by itself' });

    const byDefault = flowOf({ defaultProfileId: 'notes', roles: new Map([['a', []]]), items: [item({ id: 'a' })] }).lanes[0].tokens[0];
    expect(byDefault.upNext).toMatchObject({ mode: 'hand', text: 'by hand: only the default profile would do it' });
  });

  it('says_by_hand_while_a_task_waits_for_another', () => {
    const flow = flowOf({
      items: [item({ id: 'a', run_settings: own('notes') }), item({ id: 'b', title: 'First', run_settings: own('notes'), status: 'Doing' })],
      blockers: new Map([['a', ['b']]]),
    });
    const a = flow.lanes[0].tokens.find((t) => t.id === 'a')!;
    expect(a.upNext?.text).toBe('by hand until the task it waits for is done');
    expect(a.blockers).toEqual([{ id: 'b', title: 'First', lane: null }]);
  });

  it('gives_a_queued_task_the_reason_it_has_not_started', () => {
    const flow = flowOf({
      items: [item({ id: 'a', run_settings: own('notes') })],
      runs: [run({ item_id: 'a', agent_profile_id: 'notes', state: 'queued', waiting_reason: 'Waiting behind Fix login' })],
    });
    const token = flow.lanes[0].tokens[0];
    expect(token.station).toBe('up-next');
    expect(token.upNext?.queuedReason).toBe('Waiting behind Fix login');
    expect(flow.counts.queued).toBe(1);
  });

  it('puts_a_blocker_in_the_lane_it_stands_in', () => {
    const flow = flowOf({
      items: [item({ id: 'a', run_settings: own('notes') }), item({ id: 'b', title: 'Blocker', run_settings: own('solo'), status: 'Doing' })],
      runs: [run({ item_id: 'b', agent_profile_id: 'solo' })],
      blockers: new Map([['a', ['b']]]),
    });
    const a = flow.lanes.flatMap((l) => l.tokens).find((t) => t.id === 'a')!;
    expect(a.blockers).toEqual([{ id: 'b', title: 'Blocker', lane: 'agent:claude-code' }]);
  });

  it('carries_the_lineage_of_proposals_both_ways', () => {
    const flow = flowOf({
      items: [
        item({ id: 'research', title: 'Research', status: 'Doing', run_settings: own('notes') }),
        item({ id: 'child', title: 'Child', parent_id: 'research', source: 'plan', run_settings: own('solo') }),
      ],
      runs: [run({ item_id: 'research', agent_profile_id: 'notes' })],
    });
    const tokens = flow.lanes.flatMap((l) => l.tokens);
    expect(tokens.find((t) => t.id === 'research')!.proposed).toBe(1);
    expect(tokens.find((t) => t.id === 'child')!.fromTitle).toBe('Research');
    expect(tokens.find((t) => t.id === 'child')!.parentTitle).toBe('Research');
  });

  it('leaves_out_done_tasks_with_no_recent_run_and_tasks_that_only_hold_others', () => {
    const flow = flowOf({
      items: [
        item({ id: 'old', status: 'Done', run_settings: own('notes') }),
        item({ id: 'epic', item_type: 'epic', run_settings: own('notes') }),
        item({ id: 'part', parent_id: 'epic', run_settings: own('notes') }),
      ],
    });
    expect(flow.lanes[0].tokens.map((t) => t.id)).toEqual(['part']);
  });

  it('shows_a_task_that_ended_today_in_Done_and_one_flagged_for_review_at_Ready_for_review', () => {
    const flow = flowOf({
      items: [
        item({ id: 'ended', status: 'Done', run_settings: own('notes') }),
        item({ id: 'look', status: 'Doing', needs_review: true, run_settings: own('notes') }),
      ],
      runs: [run({ item_id: 'ended', agent_profile_id: 'notes', state: 'succeeded' })],
    });
    const stations = Object.fromEntries(flow.lanes[0].tokens.map((t) => [t.id, t.station]));
    expect(stations).toEqual({ ended: 'done', look: 'review' });
    expect(flow.counts.needs).toBe(0);
    expect(flow.counts.review).toBe(1);
  });

  it('counts_the_tokens_each_lane_has_spent', () => {
    const flow = flowOf({
      items: [item({ id: 'a', run_settings: own('notes') })],
      runs: [
        run({ item_id: 'a', agent_profile_id: 'notes', limit: { used: 1200 } as never, created_at: '2026-01-02T01:00:00Z' }),
        run({ item_id: 'a', request_id: 'earlier', agent_profile_id: 'notes', state: 'failed', limit: { used: 800 } as never }),
      ],
    });
    expect(flow.lanes[0].tokensToday).toBe(2000);
  });

  it('puts_tasks_nobody_will_take_in_the_Nobody_lane_with_each_reason', () => {
    const flow = flowOf({
      teams: [team('saas-notes'), team('lost', { reported: false })],
      profiles: [profile('notes', 'saas-notes'), profile('lost-profile', 'lost')],
      items: [
        item({ id: 'norole' }),
        item({ id: 'bare' }),
        item({ id: 'lostteam', run_settings: own('lost-profile') }),
      ],
      roles: new Map([
        ['norole', []],
        ['bare', [{ name: 'Content', profileId: null }]],
      ]),
    });
    expect(flow.nobody.map((n) => [n.id, n.route.reason])).toEqual([
      ['norole', 'no_role'],
      ['bare', 'role_without_profile'],
      ['lostteam', 'team_not_reported'],
    ]);
    expect(flow.counts.nobody).toBe(3);
  });

  it('waits_to_judge_a_task_until_its_roles_are_known', () => {
    const flow = flowOf({ items: [item({ id: 'a' })] });
    expect(flow.nobody).toEqual([]);
    expect(flow.lanes).toEqual([]);
  });

  it('lists_teams_not_used_with_the_reason_a_profile_cannot_be_made', () => {
    const flow = flowOf({
      teams: [team('saas-notes'), team('named', { profiles: [{ id: 'x', name: 'Named profile' }] }), team('bare', { profiles: [] }), team('ghost', { reported: false, profiles: [] })],
      profiles: [profile('notes', 'saas-notes')],
      projectRoles: [{ name: 'r', profileId: 'notes' }],
    });
    const byName = Object.fromEntries(flow.unused.map((u) => [u.name, u]));
    expect(Object.keys(byName)).toEqual(['bare', 'ghost', 'named']);
    expect(byName.bare.reason).toBe('Profiles are made on the Agents page, under Teams.');
    expect(byName.named.reason).toContain('Named profile already names it');
    expect(byName.ghost.reason).toContain('No computer reports');
  });
});

describe('the summary row', () => {
  const flow = buildFlow(
    input({
      teams: [team('saas-notes'), team('other')],
      profiles: [profile('notes', 'saas-notes'), profile('o', 'other')],
      items: [
        item({ id: 'ask', run_settings: own('notes') }),
        item({ id: 'go', run_settings: own('o') }),
        item({ id: 'wait', run_settings: own('o') }),
        item({ id: 'orphan' }),
      ],
      roles: new Map([['orphan', []]]),
      runs: [
        run({ item_id: 'ask', agent_profile_id: 'notes', state: 'waiting_decision' }),
        run({ item_id: 'go', agent_profile_id: 'o', state: 'running' }),
        run({ item_id: 'wait', agent_profile_id: 'o', state: 'queued' }),
      ],
    }),
  );
  const set = (...f: FlowFilter[]) => new Set<FlowFilter>(f);

  it('counts_needs_you_running_queued_and_nobody', () => {
    expect(flow.counts).toEqual({ needs: 1, review: 0, running: 1, queued: 1, nobody: 1 });
    expect(flow.open).toBe(3);
  });

  it('shows_every_lane_with_no_filter', () => {
    expect(visibleLanes(flow, set()).length).toBe(2);
    expect(showsNobody(set())).toBe(true);
  });

  it('narrows_the_lanes_to_those_with_a_task_in_the_state', () => {
    expect(visibleLanes(flow, set('waiting')).map((l) => l.name)).toEqual(['saas-notes']);
    expect(visibleLanes(flow, set('running')).map((l) => l.name)).toEqual(['other']);
    expect(visibleLanes(flow, set('running', 'waiting')).map((l) => l.name).sort()).toEqual(['other', 'saas-notes']);
    expect(showsNobody(set('running'))).toBe(false);
  });

  it('shows_only_the_Nobody_lane_for_that_counter', () => {
    expect(visibleLanes(flow, set('nobody'))).toEqual([]);
    expect(showsNobody(set('nobody'))).toBe(true);
  });

  it('lets_a_token_through_when_a_filter_takes_its_state', () => {
    const token = flow.lanes.flatMap((l) => l.tokens).find((t) => t.id === 'wait')!;
    expect(tokenPasses(token, set('queued'))).toBe(true);
    expect(tokenPasses(token, set('running'))).toBe(false);
    expect(tokenPasses(token, set())).toBe(true);
  });

  it('says_the_capacity_of_the_project_and_of_the_computers', () => {
    expect(capacityText({ open: 2, maxOpen: 3, autoStart: true, computers: { count: 1, capacity: 1 } })).toBe(
      '2 of 3 runs open in this project · this computer runs 1 at a time',
    );
    expect(capacityText({ open: 1, maxOpen: 3, autoStart: false, computers: { count: 2, capacity: 4 } })).toBe(
      '1 run open in this project · these 2 computers run 4 at a time',
    );
    expect(capacityText({ open: 0, maxOpen: 1, autoStart: true, computers: { count: 0, capacity: 0 } })).toContain('no computer is connected');
    expect(capacityText({ open: 0, maxOpen: 1, autoStart: true, computers: null })).toBe('0 of 1 runs open in this project');
  });
});

describe('moveTarget: which lane can take a task', () => {
  const lane = { key: 'team:saas-notes', profiles: [profile('notes', 'saas-notes')] };

  it('lets_a_task_that_has_not_started_move_to_another_lane_and_names_the_profile', () => {
    const result = moveTarget({ lane: 'agent:claude-code', inFlight: false, station: 'up-next' }, lane);
    expect(result).toMatchObject({ ok: true });
    expect((result as { profile: AgentProfileSummary }).profile.agent_profile_id).toBe('notes');
  });

  it('lets_a_task_nobody_would_take_move_to_a_lane', () => {
    expect(moveTarget({ lane: null, inFlight: false, station: 'up-next' }, lane)).toMatchObject({ ok: true });
  });

  it('refuses_a_task_in_flight', () => {
    expect(moveTarget({ lane: 'agent:claude-code', inFlight: true, station: 'up-next' }, lane)).toMatchObject({ ok: false });
    expect(moveTarget({ lane: 'agent:claude-code', inFlight: true, station: 'step-1' }, lane)).toMatchObject({ ok: false });
  });

  it('refuses_its_own_lane_and_a_lane_with_no_profile_to_give', () => {
    expect(moveTarget({ lane: 'team:saas-notes', inFlight: false, station: 'up-next' }, lane)).toMatchObject({ ok: false, reason: 'It is already in this lane' });
    expect(moveTarget({ lane: null, inFlight: false, station: 'up-next' }, { key: 'x', profiles: [] })).toMatchObject({ ok: false });
  });

  it('refuses_a_task_whose_work_has_ended', () => {
    expect(moveTarget({ lane: 'agent:claude-code', inFlight: false, station: 'done' }, lane)).toMatchObject({ ok: false });
  });
});

describe('the words around a move', () => {
  it('says_where_a_task_is_in_the_live_regions_words', () => {
    const flow = buildFlow(
      input({
        items: [item({ id: 'a', title: 'Collect the notes', run_settings: own('notes') })],
        runs: [run({ item_id: 'a', agent_profile_id: 'notes', latest_hop: hop('Reviewer') })],
      }),
    );
    expect(placements(flow).get('a')).toBe('Collect the notes: review, saas-notes');
  });

  it('reports_only_what_moved_or_appeared', () => {
    const before = new Map([['a', 'A: build, t'], ['b', 'B: up next, t']]);
    const after = new Map([['a', 'A: review, t'], ['b', 'B: up next, t'], ['c', 'C: up next, t']]);
    expect(movesBetween(before, after)).toEqual(['A: review, t', 'C: up next, t']);
    expect(movesBetween(after, after)).toEqual([]);
  });

  it('shortens_an_age', () => {
    const now = Date.parse('2026-01-02T12:00:00Z');
    expect(ageText('2026-01-02T11:59:40Z', now)).toBe('now');
    expect(ageText('2026-01-02T11:55:00Z', now)).toBe('5 min');
    expect(ageText('2026-01-02T09:00:00Z', now)).toBe('3 h');
    expect(ageText('2025-12-30T12:00:00Z', now)).toBe('3 d');
    expect(ageText(null, now)).toBeNull();
  });

  it('orders_by_priority_then_the_boards_order', () => {
    expect(startOrder({ priority: 'high', sort_order: 9 }, { priority: 'low', sort_order: 0 })).toBeLessThan(0);
    expect(startOrder({ priority: 'low', sort_order: 1 }, { priority: 'low', sort_order: 2 })).toBeLessThan(0);
  });
});

describe('mergeRung', () => {
  it('is_attended_at_L3_and_where_the_team_keeps_its_tasks_there', () => {
    expect(mergeRung('L3', undefined, 'repository')).toMatchObject({ attended: true, text: 'You merge (L3)' });
    expect(mergeRung('L5', { caps_at: 'L3' }, 'repository')).toMatchObject({ attended: true, level: 'L3' });
  });

  it('is_lights_out_from_L4_and_has_nothing_to_merge_in_a_folder', () => {
    expect(mergeRung('L4', undefined, 'repository')).toMatchObject({ attended: false, text: 'Lights-out inside the domains (L4)' });
    expect(mergeRung('L5', undefined, undefined)).toMatchObject({ text: 'Lights-out (L5)' });
    expect(mergeRung('L5', undefined, 'folder')).toMatchObject({ level: 'none' });
    expect(mergeRung(undefined, undefined, 'repository')).toBeNull();
  });
});

describe('laneQueue and candidateIds', () => {
  it('uses_the_teams_own_queue_when_it_reported_one_and_the_boards_count_otherwise', () => {
    const counts = { needs: 1, review: 0, running: 2, queued: 3 };
    expect(laneQueue({ report: { queue: { running: 4, waiting: 5 } } as never, counts })).toEqual({ running: 4, waiting: 5, reported: true });
    expect(laneQueue({ report: null, counts })).toEqual({ running: 2, waiting: 4, reported: false });
  });

  it('asks_for_the_details_of_tasks_with_a_run_flagged_for_review_or_ready_to_start_only', () => {
    const items = [
      item({ id: 'ready' }),
      item({ id: 'moved', status: 'Doing' }),
      item({ id: 'ran', status: 'Doing' }),
      item({ id: 'epic' }),
      item({ id: 'child', parent_id: 'epic', status: 'Doing', needs_review: true }),
    ];
    const statuses = [{ name: 'To Do', category: 'todo' }, { name: 'Doing', category: 'in_progress' }];
    expect(candidateIds(items, [run({ item_id: 'ran' })], statuses, 'To Do')).toEqual(['ready', 'ran', 'child']);
  });
});

describe('a scope chosen on the Map narrows Flow', () => {
  const flow = buildFlow(
    input({
      teams: [team('saas-notes'), team('other')],
      profiles: [profile('notes', 'saas-notes'), profile('o', 'other')],
      items: [item({ id: 'a', run_settings: own('notes') }), item({ id: 'b', run_settings: own('o') }), item({ id: 'lost' })],
      roles: new Map([['lost', []]]),
    }),
  );
  const scope = (lanes: string[], nobody: string[] = []): FlowScope => ({ kind: 'team', label: 'the team saas-notes', lanes, nobody });

  it('keeps_every_lane_and_task_without_a_scope', () => {
    expect(visibleLanes(flow, new Set()).length).toBe(2);
    expect(scopedNobody(flow, null).map((e) => e.id)).toEqual(['lost']);
  });

  it('keeps_only_the_lanes_the_scope_names', () => {
    expect(visibleLanes(flow, new Set(), scope(['team:saas-notes'])).map((l) => l.name)).toEqual(['saas-notes']);
    expect(visibleLanes(flow, new Set(), scope([])).length).toBe(0);
  });

  it('keeps_only_the_tasks_nobody_will_take_that_the_scope_names', () => {
    expect(scopedNobody(flow, scope([], ['lost'])).map((e) => e.id)).toEqual(['lost']);
    expect(scopedNobody(flow, scope(['team:saas-notes']))).toEqual([]);
  });

  it('applies_the_state_filters_inside_the_scope', () => {
    const both = scope(['team:saas-notes', 'team:other']);
    expect(visibleLanes(flow, new Set<FlowFilter>(['queued']), both).length).toBe(0);
    expect(visibleLanes(flow, new Set(), both).length).toBe(2);
  });
});
