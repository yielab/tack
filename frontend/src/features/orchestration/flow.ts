// What the Orchestration view draws, worked out from the items, the runs, the profiles and the teams
// any agent reports. Pure: no fetching, no rendering. A lane is one team (or one single agent), a
// station is a place in it, a token is a task. Everything here is stated in the terms of
// `TeamReport`, profiles, roles, items and runs; no agent is named.

import type { AgentProfileSummary, ExecutionSummary, LatestHop } from '../../shared/execution/api';
import { teamOf } from '../../shared/execution/api';
import { agentName } from '../../shared/execution/agents';
import type { TeamEntry, TeamReport, TeamStep } from '../../shared/execution/teams';
import { agentKindOf } from '../../shared/runWithAgent/ProfilePicker';
import { cardRun, passesFilters, type CardRun, type RunFilter } from '../../shared/runWithAgent/cardRun';
import type { components } from '../../shared/api/schema.gen';
import type { Dependency, Item } from '../../shared/types';

type RunSettings = components['schemas']['RunSettings'];
type TeamLine = components['schemas']['TeamLine'];

// ── Routing: which profile, and so which lane, a task goes to ──────────────────────────────────

/** A role of the project, and the profile a task with that role runs with. */
export interface RoleProfile {
  name: string;
  profileId: string | null;
}

export interface RoutingContext {
  profiles: readonly AgentProfileSummary[];
  teams: readonly TeamEntry[];
  /** The project's default profile. */
  defaultProfileId: string | null;
}

/** Where the choice came from, in the order it is taken: the task's own, its role's, the project's. */
export type RouteSource = 'task' | 'role' | 'project';

export type NobodyReason = 'no_role' | 'role_without_profile' | 'profile_missing' | 'team_not_reported';

export type Route =
  | { kind: 'lane'; lane: string; profile: AgentProfileSummary; source: RouteSource; role: string | null }
  | { kind: 'nobody'; reason: NobodyReason; detail: string | null };

export const teamLaneKey = (team: string): string => `team:${team}`;
export const agentLaneKey = (kind: string): string => `agent:${kind}`;

/** The profile a task's own settings name, or null. */
export function ownProfileId(item: Pick<Item, 'run_settings'>): string | null {
  return item.run_settings?.agent_profile_id || null;
}

/** The lane a profile belongs to: its team, or, naming none, the single agent it runs. A team nobody
 *  reports has no lane. */
export function laneOfProfile(profile: AgentProfileSummary, ctx: Pick<RoutingContext, 'teams'>): { lane: string } | { nobody: NobodyReason; detail: string } {
  const team = teamOf(profile);
  if (team) {
    return ctx.teams.find((t) => t.name === team)?.reported
      ? { lane: teamLaneKey(team) }
      : { nobody: 'team_not_reported', detail: team };
  }
  return { lane: agentLaneKey(agentKindOf(profile, ctx.teams)) };
}

/**
 * The lane a task goes to: the profile chosen on the task, else the first role that has one, else the
 * project's default (the order the item's Automation row and the Run dialog take). Null while the
 * task's roles are not known, since they could change the answer.
 */
export function routeItem(
  item: Pick<Item, 'run_settings'>,
  roles: readonly RoleProfile[] | undefined,
  ctx: RoutingContext,
): Route | null {
  const own = ownProfileId(item);
  if (!own && !roles) return null;
  const withProfile = (roles ?? []).find((r) => r.profileId);
  let source: RouteSource;
  let id: string | null;
  if (own) {
    source = 'task';
    id = own;
  } else if (withProfile) {
    source = 'role';
    id = withProfile.profileId;
  } else {
    source = 'project';
    id = ctx.defaultProfileId;
  }
  if (!id) {
    const bare = (roles ?? [])[0];
    return bare ? { kind: 'nobody', reason: 'role_without_profile', detail: bare.name } : { kind: 'nobody', reason: 'no_role', detail: null };
  }
  const profile = ctx.profiles.find((p) => p.agent_profile_id === id);
  if (!profile) return { kind: 'nobody', reason: 'profile_missing', detail: source };
  const placed = laneOfProfile(profile, ctx);
  if ('nobody' in placed) return { kind: 'nobody', reason: placed.nobody, detail: placed.detail };
  return { kind: 'lane', lane: placed.lane, profile, source, role: source === 'role' ? withProfile!.name : null };
}

/** The sentence under a task nobody will take: why, in words. */
export function nobodyText(route: Extract<Route, { kind: 'nobody' }>): string {
  switch (route.reason) {
    case 'no_role':
      return 'It has no role, no profile chosen on it, and the project has no default profile.';
    case 'role_without_profile':
      return `Its role ${route.detail} has no profile, and the project has no default profile.`;
    case 'profile_missing':
      return `The profile ${route.detail === 'task' ? 'chosen on this task' : route.detail === 'role' ? 'of its role' : 'the project uses by default'} no longer exists.`;
    case 'team_not_reported':
      return `No computer reports the team ${route.detail} that its profile names.`;
  }
}

/** Where a choice came from, as the item's Automation row words it. */
export function sourceText(route: Extract<Route, { kind: 'lane' }>): string {
  if (route.source === 'task') return 'chosen on this task';
  return route.source === 'role' ? `from the role ${route.role}` : "the project's default";
}

/** The task's run settings with the chosen profile set, or cleared; null when nothing else is left. */
export function withProfile(settings: RunSettings | null | undefined, profileId: string | null): RunSettings | null {
  const { agent_profile_id: _dropped, ...rest } = settings ?? {};
  const next: RunSettings = profileId ? { ...rest, agent_profile_id: profileId } : rest;
  return Object.keys(next).length > 0 ? next : null;
}

/** The ids of the tasks that block `itemId`, read from its dependency edges. */
export function blockersOf(itemId: string, edges: readonly Dependency[]): string[] {
  const out: string[] = [];
  for (const d of edges) {
    if (d.target_item_id === itemId && d.dependency_type === 'blocks') out.push(d.source_item_id);
    else if (d.source_item_id === itemId && d.dependency_type === 'is_blocked_by') out.push(d.target_item_id);
  }
  return [...new Set(out)];
}

// ── Stations: where in a team's own process a run is ────────────────────────────────────────────

export type StationKey = 'up-next' | 'working' | 'needs-you' | 'done' | `step-${number}`;

export const stepStation = (index: number): StationKey => `step-${index}`;

/** The index of the step a hop belongs to: the first whose role is the hop's, without regard to case. */
export function stepIndexOf(steps: readonly TeamStep[], hop: Pick<LatestHop, 'role'> | null | undefined): number | null {
  if (!hop) return null;
  const role = hop.role.toLowerCase();
  const found = steps.findIndex((s) => s.role?.toLowerCase() === role);
  return found < 0 ? null : found;
}

/**
 * The station a task stands at. Nothing running puts it at Up next (a queued run still waits for its
 * turn); a run that asks or has finished work to look at is at Needs you; one that has ended is in
 * Done. A running run is at the step its newest hop names; with no hop, or a hop no step has, it is at
 * the single Working station, which is also all a team that reports no steps has.
 */
export function stationFor(run: CardRun | undefined, hop: LatestHop | null | undefined, steps: readonly TeamStep[]): StationKey {
  if (!run || run.state === 'queued') return 'up-next';
  switch (run.state) {
    case 'needs':
    case 'review':
      return 'needs-you';
    case 'done':
    case 'failed':
    case 'cancelled':
      return 'done';
    case 'running': {
      const at = stepIndexOf(steps, hop);
      return at === null ? 'working' : stepStation(at);
    }
  }
}

/** A task sent back by a reviewer or a check: how many times, out of how many, and by which step. */
export interface SentBack {
  n: number;
  max: number | null;
  /** The step that sent it back; null when no step says it sends back to this one. */
  from: string | null;
}

export function sentBackOf(steps: readonly TeamStep[], hop: LatestHop | null | undefined, at: number | null): SentBack | null {
  const n = hop?.cycle ?? 0;
  if (n <= 0) return null;
  const here = at === null ? undefined : steps[at];
  const sender = here ? steps.find((s) => s.sends_back_to?.toLowerCase() === here.name.toLowerCase()) : undefined;
  const max = sender?.sends_back_max ?? steps.find((s) => s.sends_back_to && s.sends_back_max != null)?.sends_back_max ?? null;
  return { n, max, from: sender?.name ?? null };
}

/** What a station is called in a sentence. */
export function stationLabel(key: StationKey, steps: readonly TeamStep[]): string {
  switch (key) {
    case 'up-next':
      return 'up next';
    case 'working':
      return 'working';
    case 'needs-you':
      return 'needs you';
    case 'done':
      return 'done today';
    default:
      return steps[Number(key.slice(5))]?.name ?? 'working';
  }
}

// ── The flow: lanes, stations, tokens ───────────────────────────────────────────────────────────

export interface StatusDef {
  name: string;
  category?: string;
}

export interface FlowInput {
  items: readonly Item[];
  /** The tasks' roles, by item id, once known. */
  roles: ReadonlyMap<string, readonly RoleProfile[]>;
  /** The ids of each task's blockers, by item id. */
  blockers: ReadonlyMap<string, readonly string[]>;
  /** The project's runs: the open ones and those that ended in the last day. */
  runs: readonly ExecutionSummary[];
  profiles: readonly AgentProfileSummary[];
  teams: readonly TeamEntry[];
  /** Every role of the project, with its profile; a role's profile gives its team a lane even before a task has it. */
  projectRoles: readonly RoleProfile[];
  defaultProfileId: string | null;
  statuses: readonly StatusDef[];
  /** The status the project starts tasks from by itself; the first to-do one when it names none. */
  startStatus: string | null;
  autoStart: boolean;
  autonomy: { level: string; teams: readonly TeamLine[] } | null;
}

/** How a task at Up next will start. */
export interface UpNext {
  mode: 'itself' | 'hand';
  /** "starts by itself at To Do", "by hand: the project does not start tasks by itself". */
  text: string;
  /** Why a queued run has not started, as the server says it. */
  queuedReason: string | null;
}

export interface FlowToken {
  id: string;
  item: Item;
  parentTitle: string | null;
  run: CardRun | undefined;
  summary: ExecutionSummary | undefined;
  /** The lane it stands in; null for a task nobody will take. */
  lane: string | null;
  station: StationKey;
  /** A run holds it: queued, running or waiting for a person. It cannot be sent to another lane. */
  inFlight: boolean;
  route: Route | null;
  sentBack: SentBack | null;
  upNext: UpNext | null;
  /** The open tasks it waits for, with the lane each stands in. */
  blockers: Array<{ id: string; title: string; lane: string | null }>;
  /** The items created from this task's proposals. */
  proposed: number;
  /** The title of the task whose proposal created this one. */
  fromTitle: string | null;
  /** The tokens the run has counted, when it reports them. */
  tokens: number | null;
}

export interface FlowStation {
  key: StationKey;
  label: string;
  tokens: FlowToken[];
  step: TeamStep | null;
  /** What checks the step before the next station: drawn as a small glyph between the two. */
  check: TeamStep['checked_by'] | null;
}

export type Urgency = 'needs' | 'running' | 'idle';

export interface FlowLane {
  key: string;
  kind: 'team' | 'agent';
  name: string;
  entry: TeamEntry | null;
  report: TeamReport | null;
  steps: readonly TeamStep[];
  /** Whether the team reports steps; one that does not has the single Working station. */
  reportsSteps: boolean;
  /** The profiles of this lane that the project can use, the ones a task can be sent to. */
  profiles: AgentProfileSummary[];
  stations: FlowStation[];
  tokens: FlowToken[];
  urgency: Urgency;
  counts: { needs: number; running: number; queued: number };
  /** The tokens the lane's runs have counted. */
  tokensToday: number;
  merge: MergeRung | null;
}

export interface NobodyEntry {
  id: string;
  item: Item;
  parentTitle: string | null;
  route: Extract<Route, { kind: 'nobody' }>;
  text: string;
}

export interface UnusedTeam {
  name: string;
  reported: boolean;
  /** The profiles that name it, none of which this project uses. */
  profiles: string[];
  /** Why "Make a profile for it" is off, as a sentence. */
  reason: string;
}

export interface Flow {
  lanes: FlowLane[];
  nobody: NobodyEntry[];
  unused: UnusedTeam[];
  counts: { needs: number; running: number; queued: number; nobody: number };
  /** The runs open now: queued, running or waiting for a person. */
  open: number;
}

const PRIORITY_RANK: Record<string, number> = { critical: 0, high: 1, medium: 2, low: 3, none: 4 };

/** The order auto-start takes tasks in: most urgent first, then the board's order. */
export function startOrder(a: Pick<Item, 'priority' | 'sort_order'>, b: Pick<Item, 'priority' | 'sort_order'>): number {
  return (PRIORITY_RANK[a.priority] ?? 4) - (PRIORITY_RANK[b.priority] ?? 4) || a.sort_order - b.sort_order;
}

/** The merge rung for a team: attended where the team keeps its tasks at L3, else the project's level. */
export interface MergeRung {
  level: string;
  text: string;
  attended: boolean;
}

export function mergeRung(
  level: string | undefined,
  line: Pick<TeamLine, 'caps_at'> | undefined,
  worksIn: string | null | undefined,
): MergeRung | null {
  if (worksIn === 'folder') return { level: 'none', text: 'Nothing to merge: it works in a folder', attended: false };
  if (!level) return null;
  if (line?.caps_at) return { level: line.caps_at, text: `You merge (${line.caps_at}): the team runs attended`, attended: true };
  if (level === 'L3') return { level, text: 'You merge (L3)', attended: true };
  return { level, text: level === 'L4' ? 'Lights-out inside the domains (L4)' : 'Lights-out (L5)', attended: false };
}

function upNextOf(item: Item, summary: ExecutionSummary | undefined, input: FlowInput, route: Route | null, blocked: number): UpNext {
  const hasRole = (input.roles.get(item.id)?.length ?? 0) > 0;
  const queuedReason = summary && summary.state === 'queued' ? (summary.waiting_reason ?? null) : null;
  const start = input.startStatus ?? 'its first status';
  if (!input.autoStart) return { mode: 'hand', text: 'by hand: the project does not start tasks by itself', queuedReason };
  // Auto-start only takes tasks that say who does them: a role or a profile chosen on the task.
  if (route?.kind === 'lane' && route.source === 'project' && !hasRole) {
    return { mode: 'hand', text: 'by hand: only the default profile would do it', queuedReason };
  }
  if (blocked > 0) return { mode: 'hand', text: `by hand until the ${blocked === 1 ? 'task it waits for is' : `${blocked} tasks it waits for are`} done`, queuedReason };
  if (item.status !== input.startStatus) return { mode: 'hand', text: `by hand: it starts by itself at ${start}`, queuedReason };
  return { mode: 'itself', text: `starts by itself at ${start}`, queuedReason };
}

/** The newest run of each item. */
function newestByItem(runs: readonly ExecutionSummary[]): Map<string, ExecutionSummary> {
  const newest = new Map<string, ExecutionSummary>();
  for (const run of runs) {
    const prior = newest.get(run.item_id);
    if (!prior || run.created_at > prior.created_at) newest.set(run.item_id, run);
  }
  return newest;
}

const rank: Record<Urgency, number> = { needs: 0, running: 1, idle: 2 };

/**
 * The whole picture. A task is a token when a run holds it or ended in the last day, when it is
 * flagged for review, or when it sits at the status the project starts tasks from and nothing has
 * run it. It stands in the lane of the profile its run used, else of the profile it is routed to;
 * a task with neither goes to the Nobody lane.
 */
export function buildFlow(input: FlowInput): Flow {
  const ctx: RoutingContext = { profiles: input.profiles, teams: input.teams, defaultProfileId: input.defaultProfileId };
  const itemsById = new Map(input.items.map((i) => [i.id, i]));
  const statusCategory = new Map(input.statuses.map((s) => [s.name, s.category]));
  const hasChildren = new Set(input.items.flatMap((i) => (i.parent_id ? [i.parent_id] : [])));
  const newest = newestByItem(input.runs);
  const proposedBy = new Map<string, number>();
  for (const i of input.items) {
    if (i.source === 'plan' && i.parent_id) proposedBy.set(i.parent_id, (proposedBy.get(i.parent_id) ?? 0) + 1);
  }

  const profileIds = new Set<string>();
  const reach = (id: string | null | undefined) => {
    if (id) profileIds.add(id);
  };
  reach(input.defaultProfileId);
  for (const r of input.projectRoles) reach(r.profileId);
  for (const roles of input.roles.values()) for (const r of roles) reach(r.profileId);
  for (const i of input.items) reach(ownProfileId(i));
  for (const r of input.runs) reach(r.agent_profile_id);
  const reachable = input.profiles.filter((p) => profileIds.has(p.agent_profile_id));

  // One lane per team a reachable profile names (and some computer reports), one per single agent.
  const lanes = new Map<string, FlowLane>();
  for (const profile of reachable) {
    const placed = laneOfProfile(profile, ctx);
    if ('nobody' in placed) continue;
    const existing = lanes.get(placed.lane);
    if (existing) {
      existing.profiles.push(profile);
      continue;
    }
    const team = teamOf(profile);
    const entry = team ? (input.teams.find((t) => t.name === team) ?? null) : null;
    const report = entry?.report ?? null;
    const steps = report?.steps ?? [];
    const kind = agentKindOf(profile, input.teams);
    lanes.set(placed.lane, {
      key: placed.lane,
      kind: team ? 'team' : 'agent',
      name: team ?? (kind ? agentName(kind) : 'Agent'),
      entry,
      report,
      steps,
      reportsSteps: steps.length > 0,
      profiles: [profile],
      stations: [],
      tokens: [],
      urgency: 'idle',
      counts: { needs: 0, running: 0, queued: 0 },
      tokensToday: 0,
      merge: mergeRung(input.autonomy?.level, team ? input.autonomy?.teams.find((l) => l.name === team) : undefined, report?.works_in),
    });
  }

  const tokens: FlowToken[] = [];
  const nobody: NobodyEntry[] = [];
  const laneOfToken = new Map<string, string | null>();

  const startable = (item: Item) => {
    const category = statusCategory.get(item.status);
    return category !== 'done' && !hasChildren.has(item.id) && item.status === input.startStatus;
  };

  // First pass: which lane each task stands in, so a blocker can say where it is.
  type Pending = { item: Item; summary: ExecutionSummary | undefined; route: Route | null; lane: string | null };
  const pending: Pending[] = [];
  for (const item of input.items) {
    const summary = newest.get(item.id);
    const run = cardRun(summary, item.needs_review === true);
    const eligible = !!run || startable(item);
    if (!eligible) continue;
    const route = routeItem(item, input.roles.get(item.id), ctx);
    let lane: string | null = null;
    const used = summary?.agent_profile_id ? input.profiles.find((p) => p.agent_profile_id === summary.agent_profile_id) : undefined;
    if (used) {
      const placed = laneOfProfile(used, ctx);
      if ('lane' in placed) lane = placed.lane;
    }
    if (!lane && route?.kind === 'lane') lane = route.lane;
    if (!lane && route === null) continue; // its roles are not known yet
    pending.push({ item, summary, route, lane });
    laneOfToken.set(item.id, lane);
  }

  for (const { item, summary, route, lane } of pending) {
    const parentTitle = item.parent_id ? (itemsById.get(item.parent_id)?.title ?? null) : null;
    if (!lane || !lanes.has(lane)) {
      if (route?.kind === 'nobody') {
        nobody.push({ id: item.id, item, parentTitle, route, text: nobodyText(route) });
      }
      continue;
    }
    const target = lanes.get(lane)!;
    const run = cardRun(summary, item.needs_review === true);
    const steps = target.steps;
    const hop = summary?.latest_hop ?? null;
    const station = stationFor(run, hop, steps);
    const at = station.startsWith('step-') ? Number(station.slice(5)) : null;
    const blockerIds = input.blockers.get(item.id) ?? [];
    const open = blockerIds.flatMap((id) => {
      const blocker = itemsById.get(id);
      return blocker && statusCategory.get(blocker.status) !== 'done' ? [{ id, title: blocker.title, lane: laneOfToken.get(id) ?? null }] : [];
    });
    tokens.push({
      id: item.id,
      item,
      parentTitle,
      run,
      summary,
      lane,
      station,
      inFlight: !!run?.live,
      route,
      sentBack: station.startsWith('step-') ? sentBackOf(steps, hop, at) : null,
      upNext: station === 'up-next' ? upNextOf(item, summary, input, route, open.length) : null,
      blockers: open,
      proposed: proposedBy.get(item.id) ?? 0,
      fromTitle: item.source === 'plan' ? parentTitle : null,
      tokens: summary?.limit?.used ?? null,
    });
  }

  tokens.sort((a, b) => startOrder(a.item, b.item));

  for (const lane of lanes.values()) {
    const mine = tokens.filter((t) => t.lane === lane.key);
    lane.tokens = mine;
    const by = (key: StationKey) => mine.filter((t) => t.station === key);
    const stations: FlowStation[] = [{ key: 'up-next', label: 'Up next', tokens: by('up-next'), step: null, check: null }];
    if (lane.reportsSteps) {
      lane.steps.forEach((step, i) =>
        stations.push({ key: stepStation(i), label: step.name, tokens: by(stepStation(i)), step, check: step.checked_by ?? null }),
      );
    }
    const loose = by('working');
    if (!lane.reportsSteps || loose.length > 0) {
      stations.push({ key: 'working', label: 'Working', tokens: loose, step: null, check: null });
    }
    stations.push({ key: 'needs-you', label: 'Needs you', tokens: by('needs-you'), step: null, check: null });
    stations.push({ key: 'done', label: 'Done today', tokens: by('done'), step: null, check: null });
    lane.stations = stations;
    lane.counts = {
      needs: mine.filter((t) => t.run?.state === 'needs' || t.run?.state === 'review').length,
      running: mine.filter((t) => t.run?.state === 'running').length,
      queued: mine.filter((t) => t.run?.state === 'queued').length,
    };
    lane.urgency = lane.counts.needs > 0 ? 'needs' : lane.counts.running > 0 || lane.counts.queued > 0 ? 'running' : 'idle';
  }

  // Tokens counted by every run in the window, not only each task's newest.
  for (const run of input.runs) {
    const used = run.limit?.used;
    const profile = run.agent_profile_id ? input.profiles.find((p) => p.agent_profile_id === run.agent_profile_id) : undefined;
    const placed = profile ? laneOfProfile(profile, ctx) : undefined;
    if (typeof used === 'number' && placed && 'lane' in placed) {
      const target = lanes.get(placed.lane);
      if (target) target.tokensToday += used;
    }
  }
  for (const lane of lanes.values()) {
    lane.profiles.sort((a, b) => Number(b.agent_profile_id === input.defaultProfileId) - Number(a.agent_profile_id === input.defaultProfileId) || a.name.localeCompare(b.name));
  }

  const ordered = [...lanes.values()].sort((a, b) => rank[a.urgency] - rank[b.urgency] || a.name.localeCompare(b.name));

  const used = new Set(ordered.map((l) => l.key));
  const reachableNames = new Set(reachable.flatMap((p) => teamOf(p) ?? []));
  const unused: UnusedTeam[] = input.teams
    .filter((t) => !used.has(teamLaneKey(t.name)) && !reachableNames.has(t.name))
    .map((t) => {
      const named = t.profiles.map((p) => p.name);
      return {
        name: t.name,
        reported: t.reported,
        profiles: named,
        reason: !t.reported
          ? 'No computer reports this team, so a profile for it would have nowhere to run.'
          : named.length > 0
            ? `${named.join(', ')} already ${named.length === 1 ? 'names' : 'name'} it, but no role of this project uses ${named.length === 1 ? 'it' : 'them'}. Choose it for a role in Settings → Automation → Teams.`
            : 'Profiles are made on the Agents page, under Teams.',
      };
    })
    .sort((a, b) => a.name.localeCompare(b.name));

  const all = tokens;
  return {
    lanes: ordered,
    nobody,
    unused,
    counts: {
      needs: all.filter((t) => t.run?.state === 'needs' || t.run?.state === 'review').length,
      running: all.filter((t) => t.run?.state === 'running').length,
      queued: all.filter((t) => t.run?.state === 'queued').length,
      nobody: nobody.length,
    },
    open: all.filter((t) => t.inFlight).length,
  };
}

// ── What a lane's header meters say ─────────────────────────────────────────────────────────────

/** Tasks being worked and tasks waiting in a lane: the team's own report when it gave one, else what the board counts. */
export function laneQueue(lane: Pick<FlowLane, 'report' | 'counts'>): { running: number; waiting: number; reported: boolean } {
  const queue = lane.report?.queue;
  if (queue) return { running: queue.running, waiting: queue.waiting, reported: true };
  return { running: lane.counts.running, waiting: lane.counts.queued + lane.counts.needs, reported: false };
}

/** The ids of the tasks whose roles and blockers the view has to read: those a run holds or ended, and those it may start. */
export function candidateIds(items: readonly Item[], runs: readonly ExecutionSummary[], statuses: readonly StatusDef[], startStatus: string | null): string[] {
  const done = new Set(statuses.filter((s) => s.category === 'done').map((s) => s.name));
  const withRun = new Set(runs.map((r) => r.item_id));
  const parents = new Set(items.flatMap((i) => (i.parent_id ? [i.parent_id] : [])));
  return items
    .filter((i) => withRun.has(i.id) || i.needs_review === true || (i.status === startStatus && !done.has(i.status) && !parents.has(i.id)))
    .map((i) => i.id);
}

// ── The summary row ─────────────────────────────────────────────────────────────────────────────

/** What a counter in the summary row filters to. Three of them are the board's own filters. */
export type FlowFilter = Extract<RunFilter, 'waiting' | 'running' | 'queued'> | 'nobody';

export function tokenPasses(token: FlowToken, filters: ReadonlySet<FlowFilter>): boolean {
  const runFilters = new Set([...filters].filter((f): f is Exclude<FlowFilter, 'nobody'> => f !== 'nobody'));
  return passesFilters(token.run, runFilters);
}

/**
 * What the Map narrows Flow to: one role, profile, team or member, or a whole column. The Map works out
 * which lanes and which unrouted tasks pass through what was chosen, since it holds the routes; Flow
 * only keeps those.
 */
export interface FlowScope {
  kind: 'role' | 'profile' | 'team' | 'member' | 'column';
  /** What was chosen, in words: "the role Reviewer", "every team". */
  label: string;
  /** The keys of the lanes that pass. */
  lanes: readonly string[];
  /** The ids of the tasks nobody will take that pass. */
  nobody: readonly string[];
}

/** The lanes to draw under the filters and the scope: with none set, all of them; otherwise those with a task that passes. */
export function visibleLanes(flow: Flow, filters: ReadonlySet<FlowFilter>, scope: FlowScope | null = null): FlowLane[] {
  const inScope = scope ? flow.lanes.filter((l) => scope.lanes.includes(l.key)) : flow.lanes;
  const runFilters = [...filters].some((f) => f !== 'nobody');
  if (filters.size === 0) return inScope;
  if (!runFilters) return [];
  return inScope.filter((l) => l.tokens.some((t) => tokenPasses(t, filters)));
}

/** The tasks nobody will take that the scope lets through: all of them without a scope. */
export function scopedNobody(flow: Flow, scope: FlowScope | null): NobodyEntry[] {
  return scope ? flow.nobody.filter((e) => scope.nobody.includes(e.id)) : flow.nobody;
}

/** Whether the Nobody lane shows: always when nothing is filtered, else only when asked for. */
export function showsNobody(filters: ReadonlySet<FlowFilter>): boolean {
  return filters.size === 0 || filters.has('nobody');
}

/** "2 of 3 runs open in this project · this computer runs 1 at a time". */
export function capacityText(input: {
  open: number;
  maxOpen: number | null;
  autoStart: boolean;
  /** The active computers' capacity, or null when it is not known. */
  computers: { count: number; capacity: number } | null;
}): string {
  const runs = input.open === 1 ? 'run' : 'runs';
  const project =
    input.autoStart && input.maxOpen != null
      ? `${input.open} of ${input.maxOpen} runs open in this project`
      : `${input.open} ${runs} open in this project`;
  const c = input.computers;
  if (!c) return project;
  if (c.count === 0) return `${project} · no computer is connected`;
  const per = `${c.capacity} at a time`;
  return `${project} · ${c.count === 1 ? 'this computer runs' : `these ${c.count} computers run`} ${per}`;
}

// ── Moving a task to another lane ───────────────────────────────────────────────────────────────

/** Whether a task may be sent to a lane, and the profile it would get; otherwise why not. */
export function moveTarget(token: { lane: string | null; inFlight: boolean; station: StationKey }, lane: Pick<FlowLane, 'key' | 'profiles'>): { ok: true; profile: AgentProfileSummary } | { ok: false; reason: string } {
  if (token.inFlight) return { ok: false, reason: 'A run holds this task, so it cannot change hands' };
  if (token.station !== 'up-next') return { ok: false, reason: 'Only a task that has not started can change hands' };
  if (token.lane === lane.key) return { ok: false, reason: 'It is already in this lane' };
  const profile = lane.profiles[0];
  if (!profile) return { ok: false, reason: 'No profile of this project can take it' };
  return { ok: true, profile };
}

// ── What the live region says ───────────────────────────────────────────────────────────────────

/** Where each token stands, in the words the live region uses: "Collect the notes: review, saas-notes". */
export function placements(flow: Flow): Map<string, string> {
  const out = new Map<string, string>();
  for (const lane of flow.lanes) {
    for (const t of lane.tokens) out.set(t.id, `${t.item.title}: ${stationLabel(t.station, lane.steps)}, ${lane.name}`);
  }
  for (const n of flow.nobody) out.set(n.id, `${n.item.title}: nobody will take it`);
  return out;
}

/** What changed between two pictures: each task that moved or appeared, as a sentence. */
export function movesBetween(before: ReadonlyMap<string, string>, after: ReadonlyMap<string, string>): string[] {
  const moves: string[] = [];
  for (const [id, sentence] of after) if (before.get(id) !== sentence) moves.push(sentence);
  return moves;
}

// ── Time ────────────────────────────────────────────────────────────────────────────────────────

/** How long ago, shortly: "now", "5 min", "2 h", "3 d". */
export function ageText(since: string | null | undefined, now: number): string | null {
  if (!since) return null;
  const then = Date.parse(since);
  if (Number.isNaN(then)) return null;
  const minutes = Math.max(0, Math.floor((now - then) / 60_000));
  if (minutes < 1) return 'now';
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  return hours < 24 ? `${hours} h` : `${Math.floor(hours / 24)} d`;
}
