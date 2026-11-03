// What the Orchestration view's Map and Compare draw, worked out from the Flow, the project's roles,
// the profiles and the teams any agent reports. Pure: no fetching, no rendering. The Map has four
// columns, Roles -> Profiles -> Teams -> Members, whose rows are ordered so that links cross as
// little as they can; a link is as wide as the open tasks routed along it. Compare lays the teams
// side by side. Everything is stated in the terms of `TeamReport`, profiles, roles and runs; no
// agent is named beyond the names the data carries.

import type { AgentProfileSummary } from '../../shared/execution/api';
import { teamOf } from '../../shared/execution/api';
import { agentName } from '../../shared/execution/agents';
import {
  memberReachText,
  networkText,
  stepText,
  teamProposes,
  type TeamEntry,
  type TeamMember,
  type TeamReport,
} from '../../shared/execution/teams';
import { spaced } from '../../shared/runWithAgent/runLimit';
import { agentKindOf } from '../../shared/runWithAgent/ProfilePicker';
import type { components } from '../../shared/api/schema.gen';
import { agentLaneKey, mergeRung, teamLaneKey, type Flow, type FlowScope, type FlowToken, type RoleProfile, type Route } from './flow';

type TeamLine = components['schemas']['TeamLine'];

// ── Geometry ─────────────────────────────────────────────────────────────────────────────────────

export const NODE_W = 200;
export const NODE_H = 62;
export const COL_GAP = 80;
export const ROW_GAP = 14;
export const HEADER_H = 40;
export const PAD = 12;

// ── Model ────────────────────────────────────────────────────────────────────────────────────────

export type MapColumn = 'roles' | 'profiles' | 'teams' | 'members';

export const MAP_COLUMNS: ReadonlyArray<{ key: MapColumn; title: string }> = [
  { key: 'roles', title: 'Roles' },
  { key: 'profiles', title: 'Profiles' },
  { key: 'teams', title: 'Teams' },
  { key: 'members', title: 'Members' },
];

/** Where a drawn-dashed node is mended: the project's Teams settings, or the Agents page's profiles. */
export interface MapFix {
  to: 'automation-teams' | 'agents-profiles';
  text: string;
}

export interface MapNode {
  id: string;
  column: MapColumn;
  kind: 'role' | 'profile' | 'team' | 'agent' | 'member';
  label: string;
  /** The second line: what it is or where it runs. */
  sub: string | null;
  /** Dashed: a role with no profile, a team no profile names. */
  dashed: boolean;
  /** Why it is dashed, in a sentence. */
  reason: string | null;
  fix: MapFix | null;
  /** The team a member belongs to, so a member is read under its team; null for every other kind. */
  group: string | null;
  /** The profile the project uses when a task says nothing else. */
  isDefault: boolean;
  /** Open tasks that pass through it. */
  tasks: number;
  /** Tasks nobody will take that name it as the cause. */
  stranded: number;
  /** What Flow narrows to when it is chosen. */
  scope: FlowScope;
  row: number;
  x: number;
  y: number;
}

export interface MapLink {
  id: string;
  from: string;
  to: string;
  /** Open tasks routed this way. */
  tasks: number;
  /** The stroke width, in drawing units. */
  width: number;
}

export interface MapColumnInfo {
  key: MapColumn;
  title: string;
  ids: string[];
  scope: FlowScope;
  /** The header's place, for the drawing and the keyboard. */
  x: number;
  y: number;
}

export interface MapModel {
  nodes: MapNode[];
  links: MapLink[];
  columns: MapColumnInfo[];
  width: number;
  height: number;
}

export interface MapInput {
  flow: Flow;
  /** Every role of the project, with the profile it uses. */
  roles: readonly RoleProfile[];
  profiles: readonly AgentProfileSummary[];
  teams: readonly TeamEntry[];
  defaultProfileId: string | null;
  /** The computers, to say where a team runs; null when they could not be read. */
  runners: ReadonlyArray<{ runner_id: string; name: string }> | null;
}

export const NO_ROLE_ID = 'role-none';
const roleId = (name: string) => `role:${name}`;
const profileNodeId = (id: string) => `profile:${id}`;
const memberId = (laneKey: string, role: string) => `member:${laneKey}:${role}`;

const tasksText = (n: number) => `${n} open ${n === 1 ? 'task' : 'tasks'}`;

/** A link's width: a hairline when nothing is routed along it, then wider with every task, up to a cap. */
export function linkWidth(tasks: number): number {
  if (tasks <= 0) return 1;
  return Math.min(22, 3 + tasks * 2.5);
}

/** Whether a computer's name says it is the one the board runs on. */
export const isThisComputer = (name: string): boolean => name.startsWith('This computer') || name.startsWith('local-');

function runsOn(entry: TeamEntry, runners: MapInput['runners']): string {
  if (!entry.reported) return 'No computer reports it';
  if (!entry.runner) return 'Computer not reported';
  if (!runners) return 'Computer unknown';
  const runner = runners.find((r) => r.runner_id === entry.runner);
  if (!runner) return 'Runs on another computer';
  return isThisComputer(runner.name) ? 'Runs on this computer' : `Runs on ${runner.name}`;
}

const memberSub = (member: TeamMember): string | null => {
  const words = [member.model, ...memberReachText(member)].filter(Boolean).join(' · ');
  return words || null;
};

// ── Ordering: rows that cross as little as they can ──────────────────────────────────────────────

interface Edge {
  from: string;
  to: string;
}

/**
 * Orders every column's rows by the barycentre of the rows its neighbours stand in, sweeping from the
 * left and back from the right a few times. A node with no neighbour in the column it is ordered
 * against keeps its place. The columns themselves never move.
 */
export function orderRows(columns: readonly (readonly string[])[], links: readonly Edge[]): string[][] {
  const cols = columns.map((c) => [...c]);
  const sortAgainst = (c: number, other: number) => {
    const rowOf = new Map(cols[other].map((id, i) => [id, i]));
    const keyed = cols[c].map((id, i) => {
      const at = links.flatMap((l) => {
        if (l.from === id && rowOf.has(l.to)) return [rowOf.get(l.to)!];
        if (l.to === id && rowOf.has(l.from)) return [rowOf.get(l.from)!];
        return [];
      });
      return { id, i, bary: at.length > 0 ? at.reduce((a, b) => a + b, 0) / at.length : i };
    });
    keyed.sort((a, b) => a.bary - b.bary || a.i - b.i);
    cols[c] = keyed.map((k) => k.id);
  };
  for (let pass = 0; pass < 4; pass += 1) {
    for (let c = 1; c < cols.length; c += 1) sortAgainst(c, c - 1);
    for (let c = cols.length - 2; c >= 0; c -= 1) sortAgainst(c, c + 1);
  }
  return cols;
}

/** How many pairs of links between neighbouring columns cross, for rows in the order given. */
export function countCrossings(columns: readonly (readonly string[])[], links: readonly Edge[]): number {
  const place = new Map<string, { col: number; row: number }>();
  columns.forEach((ids, col) => ids.forEach((id, row) => place.set(id, { col, row })));
  const between = links.flatMap((l) => {
    const a = place.get(l.from);
    const b = place.get(l.to);
    return a && b && b.col === a.col + 1 ? [{ col: a.col, from: a.row, to: b.row }] : [];
  });
  let crossings = 0;
  for (let i = 0; i < between.length; i += 1) {
    for (let j = i + 1; j < between.length; j += 1) {
      const a = between[i];
      const b = between[j];
      if (a.col === b.col && (a.from - b.from) * (a.to - b.to) < 0) crossings += 1;
    }
  }
  return crossings;
}

// ── Building the map ─────────────────────────────────────────────────────────────────────────────

const scopeOf = (kind: FlowScope['kind'], label: string, lanes: readonly string[], nobody: readonly string[]): FlowScope => ({
  kind,
  label,
  lanes: [...new Set(lanes)],
  nobody: [...new Set(nobody)],
});

type LaneRoute = Extract<Route, { kind: 'lane' }>;
type RoutedToken = FlowToken & { route: LaneRoute };
const isRouted = (t: FlowToken): t is RoutedToken => t.route?.kind === 'lane';

export function buildMap(input: MapInput): MapModel {
  const { flow, profiles, teams } = input;
  const laneKeys = new Set(flow.lanes.map((l) => l.key));
  const routed = flow.lanes.flatMap((l) => l.tokens).filter(isRouted);
  const open = routed.filter((t) => t.station !== 'done');
  const profileById = new Map(profiles.map((p) => [p.agent_profile_id, p]));
  const nobodyIds = (match: (e: Flow['nobody'][number]) => boolean) => flow.nobody.filter(match).map((e) => e.id);

  const nodes: MapNode[] = [];
  const links = new Map<string, MapLink>();
  const addNode = (n: Omit<MapNode, 'row' | 'x' | 'y' | 'group'> & { group?: string | null }) => nodes.push({ ...n, group: n.group ?? null, row: 0, x: 0, y: 0 });
  const addLink = (from: string, to: string, tasks: number) => {
    const id = `${from}>${to}`;
    const prior = links.get(id);
    const total = (prior?.tasks ?? 0) + tasks;
    links.set(id, { id, from, to, tasks: total, width: linkWidth(total) });
  };

  // The lane a profile feeds: its team's, or the single agent's.
  const targetOf = (profile: AgentProfileSummary): { key: string; team: string | null; kind: string } => {
    const team = teamOf(profile);
    const kind = agentKindOf(profile, teams);
    return { key: team ? teamLaneKey(team) : agentLaneKey(kind), team, kind };
  };

  // Profiles on the map: every one the install has, as the run dialog offers them; those a role, the
  // project's default or a routed task uses come first.
  const profileOrder: AgentProfileSummary[] = [];
  const reach = (id: string | null | undefined) => {
    const p = id ? profileById.get(id) : undefined;
    if (p && !profileOrder.includes(p)) profileOrder.push(p);
  };
  for (const r of input.roles) reach(r.profileId);
  reach(input.defaultProfileId);
  for (const t of routed) reach(t.route.profile.agent_profile_id);
  for (const p of profiles) reach(p.agent_profile_id);

  // ── Roles
  const roleNames = [...new Set(input.roles.map((r) => r.name))];
  for (const name of roleNames) {
    const role = input.roles.find((r) => r.name === name)!;
    const profile = role.profileId ? profileById.get(role.profileId) : undefined;
    const noProfile = !role.profileId;
    const gone = !!role.profileId && !profile;
    const stranded = nobodyIds((e) => e.route.reason === 'role_without_profile' && e.route.detail === name);
    const lane = profile ? targetOf(profile).key : null;
    const mine = open.filter((t) => t.route.source === 'role' && t.route.role === name);
    addNode({
      id: roleId(name),
      column: 'roles',
      kind: 'role',
      label: name,
      sub: noProfile ? 'No profile' : gone ? 'Its profile no longer exists' : null,
      dashed: noProfile || gone,
      reason: noProfile
        ? `The role ${name} has no profile, so a task with only this role is taken by the project's default profile, or by nobody.`
        : gone
          ? `The profile of the role ${name} no longer exists.`
          : null,
      fix: noProfile || gone ? { to: 'automation-teams', text: 'Settings → Automation → Teams' } : null,
      isDefault: false,
      tasks: mine.length,
      stranded: stranded.length,
      scope: scopeOf('role', `the role ${name}`, lane && laneKeys.has(lane) ? [lane] : [], stranded),
    });
  }
  const withoutRole = open.filter((t) => t.route.source !== 'role');
  const defaultProfile = input.defaultProfileId ? profileById.get(input.defaultProfileId) : undefined;
  const noRoleStranded = nobodyIds((e) => e.route.reason === 'no_role');
  if (withoutRole.length > 0 || defaultProfile || noRoleStranded.length > 0) {
    const lanes = [...withoutRole.map((t) => t.route.lane), ...(defaultProfile ? [targetOf(defaultProfile).key] : [])].filter((k) => laneKeys.has(k));
    addNode({
      id: NO_ROLE_ID,
      column: 'roles',
      kind: 'role',
      label: 'No role',
      sub: defaultProfile ? 'Chosen on the task, or the default' : 'Chosen on the task',
      dashed: false,
      reason: null,
      fix: null,
      isDefault: false,
      tasks: withoutRole.length,
      stranded: noRoleStranded.length,
      scope: scopeOf('role', 'tasks without a role', lanes, noRoleStranded),
    });
  }

  // ── Profiles
  for (const profile of profileOrder) {
    const target = targetOf(profile);
    const isDefault = profile.agent_profile_id === input.defaultProfileId;
    const mine = open.filter((t) => t.route.profile.agent_profile_id === profile.agent_profile_id);
    const stranded = target.team ? nobodyIds((e) => e.route.reason === 'team_not_reported' && e.route.detail === target.team) : [];
    addNode({
      id: profileNodeId(profile.agent_profile_id),
      column: 'profiles',
      kind: 'profile',
      label: profile.name,
      sub: isDefault ? `Default · ${target.team ? `team ${target.team}` : 'one agent'}` : target.team ? `Team ${target.team}` : 'One agent',
      dashed: false,
      reason: null,
      fix: null,
      isDefault,
      tasks: mine.length,
      stranded: stranded.length,
      scope: scopeOf('profile', `the profile ${profile.name}`, laneKeys.has(target.key) ? [target.key] : [], stranded),
    });
  }

  // ── Teams: every team any agent reports or a profile names, then the single agents in use
  const teamNames = new Set<string>(teams.map((t) => t.name));
  for (const p of profileOrder) {
    const team = teamOf(p);
    if (team) teamNames.add(team);
  }
  for (const name of [...teamNames].sort((a, b) => a.localeCompare(b))) {
    const entry = teams.find((t) => t.name === name) ?? ({ name, profiles: [], reported: false } as TeamEntry);
    const key = teamLaneKey(name);
    // Dashed when no profile names the team: a name the team reports for a profile that the list of
    // profiles does not have does not make it reachable.
    const noProfile = !profileOrder.some((p) => teamOf(p) === name);
    const namedElsewhere = entry.profiles.map((p) => p.name);
    const stranded = nobodyIds((e) => e.route.reason === 'team_not_reported' && e.route.detail === name);
    addNode({
      id: key,
      column: 'teams',
      kind: 'team',
      label: name,
      sub: runsOn(entry, input.runners),
      dashed: noProfile,
      reason: !noProfile
        ? null
        : namedElsewhere.length > 0
          ? `${namedElsewhere.join(', ')} ${namedElsewhere.length === 1 ? 'names' : 'name'} the team ${name}, but ${namedElsewhere.length === 1 ? 'it is' : 'they are'} not among the profiles, so no task can be routed to it.`
          : `No profile names the team ${name}, so no task can be routed to it.`,
      fix: !noProfile
        ? null
        : namedElsewhere.length > 0
          ? { to: 'automation-teams', text: 'Settings → Automation → Teams' }
          : { to: 'agents-profiles', text: 'Agents → profiles' },
      isDefault: false,
      tasks: open.filter((t) => t.route.lane === key).length,
      stranded: stranded.length,
      scope: scopeOf('team', `the team ${name}`, laneKeys.has(key) ? [key] : [], stranded),
    });
  }
  const agents = new Map<string, string>();
  for (const profile of profileOrder) {
    const target = targetOf(profile);
    if (!target.team && !agents.has(target.key)) agents.set(target.key, target.kind ? agentName(target.kind) : 'Agent');
  }
  for (const [key, name] of agents) {
    addNode({
      id: key,
      column: 'teams',
      kind: 'agent',
      label: name,
      sub: 'One agent',
      dashed: false,
      reason: null,
      fix: null,
      isDefault: false,
      tasks: open.filter((t) => t.route.lane === key).length,
      stranded: 0,
      scope: scopeOf('team', `the agent ${name}`, laneKeys.has(key) ? [key] : [], []),
    });
  }

  // ── Members, as each team reported them
  for (const node of nodes.filter((n) => n.kind === 'team')) {
    const members = teams.find((t) => teamLaneKey(t.name) === node.id)?.report?.members ?? [];
    for (const role of [...new Set(members.map((m) => m.role))]) {
      const member = members.find((m) => m.role === role)!;
      addNode({
        id: memberId(node.id, role),
        column: 'members',
        kind: 'member',
        label: role,
        group: node.label,
        sub: memberSub(member),
        dashed: false,
        reason: null,
        fix: null,
        isDefault: false,
        tasks: node.tasks,
        stranded: 0,
        scope: scopeOf('member', `the member ${role} of ${node.label}`, node.scope.lanes, []),
      });
      addLink(node.id, memberId(node.id, role), node.tasks);
    }
  }

  // ── Links: role -> profile -> team, with the open tasks routed along each
  for (const role of input.roles) {
    if (role.profileId && profileById.has(role.profileId) && !links.has(`${roleId(role.name)}>${profileNodeId(role.profileId)}`)) {
      const n = open.filter((t) => t.route.source === 'role' && t.route.role === role.name && t.route.profile.agent_profile_id === role.profileId).length;
      addLink(roleId(role.name), profileNodeId(role.profileId), n);
    }
  }
  if (nodes.some((n) => n.id === NO_ROLE_ID)) {
    const targets = new Set<string>(defaultProfile ? [defaultProfile.agent_profile_id] : []);
    for (const t of withoutRole) targets.add(t.route.profile.agent_profile_id);
    for (const id of targets) addLink(NO_ROLE_ID, profileNodeId(id), withoutRole.filter((t) => t.route.profile.agent_profile_id === id).length);
  }
  for (const profile of profileOrder) {
    const target = targetOf(profile);
    addLink(profileNodeId(profile.agent_profile_id), target.key, open.filter((t) => t.route.profile.agent_profile_id === profile.agent_profile_id).length);
  }

  // ── Order the rows, then place every node
  const byColumn = MAP_COLUMNS.map((c) => nodes.filter((n) => n.column === c.key).map((n) => n.id));
  const all = [...links.values()];
  const ordered = orderRows(byColumn, all);
  const byId = new Map(nodes.map((n) => [n.id, n]));
  ordered.forEach((ids, col) =>
    ids.forEach((id, row) => {
      const n = byId.get(id)!;
      n.row = row;
      n.x = PAD + col * (NODE_W + COL_GAP);
      n.y = PAD + HEADER_H + row * (NODE_H + ROW_GAP);
    }),
  );
  const columns: MapColumnInfo[] = MAP_COLUMNS.map((c, col) => {
    const inColumn = nodes.filter((n) => n.column === c.key);
    return {
      key: c.key,
      title: c.title,
      ids: ordered[col],
      scope: scopeOf(
        'column',
        c.key === 'roles' ? 'every role' : c.key === 'profiles' ? 'every profile' : c.key === 'teams' ? 'every team' : 'every member',
        inColumn.flatMap((n) => n.scope.lanes),
        inColumn.flatMap((n) => n.scope.nobody),
      ),
      x: PAD + col * (NODE_W + COL_GAP),
      y: PAD,
    };
  });
  const rows = Math.max(1, ...ordered.map((ids) => ids.length));
  return {
    nodes: ordered.flatMap((ids) => ids.map((id) => byId.get(id)!)),
    links: all,
    columns,
    width: PAD * 2 + MAP_COLUMNS.length * NODE_W + (MAP_COLUMNS.length - 1) * COL_GAP,
    height: PAD * 2 + HEADER_H + rows * NODE_H + (rows - 1) * ROW_GAP,
  };
}

/** What Flow narrows to for a node or a column header, read from the map as it stands now. */
export function scopeOfNode(model: Pick<MapModel, 'nodes' | 'columns'>, id: string): FlowScope | null {
  return model.nodes.find((n) => n.id === id)?.scope ?? model.columns.find((c) => columnHeaderId(c.key) === id)?.scope ?? null;
}

/** A link as an S-shaped curve from the right edge of one node to the left edge of the next. */
export function linkPath(model: Pick<MapModel, 'nodes'>, link: Pick<MapLink, 'from' | 'to'>): string {
  const a = model.nodes.find((n) => n.id === link.from);
  const b = model.nodes.find((n) => n.id === link.to);
  if (!a || !b) return '';
  const x1 = a.x + NODE_W;
  const y1 = a.y + NODE_H / 2;
  const x2 = b.x;
  const y2 = b.y + NODE_H / 2;
  const mid = (x1 + x2) / 2;
  return `M${x1},${y1} C${mid},${y1} ${mid},${y2} ${x2},${y2}`;
}

/** A name that fits a node: cut with an ellipsis; the whole name stays in the node's label and tooltip. */
export function clip(text: string, max: number): string {
  return text.length <= max ? text : `${text.slice(0, Math.max(1, max - 1))}…`;
}

/**
 * A text broken at spaces into at most `maxLines` lines of at most `width` characters, the last one cut
 * with an ellipsis when the rest does not fit; a word longer than a line is broken where the line ends.
 * The whole text stays in the node's tooltip.
 */
export function wrapLines(text: string, width: number, maxLines: number): string[] {
  const lines: string[] = [];
  let rest = text.trim();
  while (rest.length > 0 && lines.length < maxLines) {
    if (lines.length === maxLines - 1) {
      lines.push(clip(rest, width));
      break;
    }
    if (rest.length <= width) {
      lines.push(rest);
      break;
    }
    const at = rest.lastIndexOf(' ', width);
    const cut = at > 0 ? at : width;
    lines.push(rest.slice(0, cut).trimEnd());
    rest = rest.slice(cut).trimStart();
  }
  return lines;
}

// ── Keyboard: node by node ───────────────────────────────────────────────────────────────────────

export type Direction = 'up' | 'down' | 'left' | 'right';

export const columnHeaderId = (key: MapColumn): string => `column:${key}`;

/** The ids in the order a keyboard visits them in each column: the header, then the rows. */
function stops(model: MapModel): string[][] {
  return model.columns.map((c) => [columnHeaderId(c.key), ...c.ids]);
}

/**
 * Where an arrow key goes from `id`: up and down along the column (the header is its first stop);
 * left and right to the neighbouring column, to the first node linked to this one when there is one,
 * else to the row nearest. Null at an edge.
 */
export function navigate(model: MapModel, id: string, direction: Direction): string | null {
  const cols = stops(model);
  const col = cols.findIndex((ids) => ids.includes(id));
  if (col < 0) return null;
  const at = cols[col].indexOf(id);
  if (direction === 'up') return at > 0 ? cols[col][at - 1] : null;
  if (direction === 'down') return at < cols[col].length - 1 ? cols[col][at + 1] : null;
  const next = col + (direction === 'right' ? 1 : -1);
  if (next < 0 || next >= cols.length) return null;
  if (at === 0) return cols[next][0];
  const linked = cols[next].find((other) => model.links.some((l) => (l.from === id && l.to === other) || (l.to === id && l.from === other)));
  return linked ?? cols[next][Math.min(at, cols[next].length - 1)];
}

// ── The routes as text ───────────────────────────────────────────────────────────────────────────

export interface RouteLine {
  /** The link it says, or the node it describes. */
  id: string;
  text: string;
  fix: MapFix | null;
}

export interface RouteGroup {
  key: string;
  title: string;
  lines: RouteLine[];
}

/** What the drawing says, in sentences: each link with its open tasks, then what is dashed and the default. */
export function routesText(model: MapModel): RouteGroup[] {
  const byId = new Map(model.nodes.map((n) => [n.id, n]));
  const group = (key: string, title: string, from: MapColumn): RouteGroup => ({
    key,
    title,
    lines: model.links
      .filter((l) => byId.get(l.from)?.column === from)
      .map((l) => ({ id: l.id, text: `${byId.get(l.from)!.label} → ${byId.get(l.to)!.label}: ${tasksText(l.tasks)}`, fix: null })),
  });
  const notes: RouteLine[] = model.nodes.flatMap((n) => {
    const lines: RouteLine[] = [];
    if (n.isDefault) lines.push({ id: `default:${n.id}`, text: `${n.label} is the project's default profile: a task with no role and no profile of its own goes to it.`, fix: null });
    if (n.dashed && n.reason) lines.push({ id: `dashed:${n.id}`, text: n.reason, fix: n.fix });
    if (n.stranded > 0) lines.push({ id: `stranded:${n.id}`, text: `${n.stranded} ${n.stranded === 1 ? 'task' : 'tasks'} nobody will take because of ${n.label}.`, fix: null });
    return lines;
  });
  return [
    group('roles', 'Roles to profiles', 'roles'),
    group('profiles', 'Profiles to teams', 'profiles'),
    group('teams', 'Teams to members', 'teams'),
    { key: 'notes', title: 'What needs attention', lines: notes },
  ];
}

// ── Compare: the teams side by side ──────────────────────────────────────────────────────────────

/** One cell of the table: a sentence, a list, or the fact that the team did not say, with why. */
export type CompareCell =
  | { kind: 'text'; text: string }
  | { kind: 'list'; items: string[] }
  | { kind: 'missing'; reason: string };

export interface CompareRow {
  key: string;
  label: string;
  cells: CompareCell[];
}

export interface CompareModel {
  teams: Array<{ name: string; reported: boolean }>;
  rows: CompareRow[];
}

export interface CompareInput {
  flow: Flow;
  teams: readonly TeamEntry[];
  profiles: readonly AgentProfileSummary[];
  autonomy: { level: string; teams: readonly TeamLine[]; provisionalTokensPerRun: number | null } | null;
}

const text = (t: string): CompareCell => ({ kind: 'text', text: t });
const list = (items: string[]): CompareCell => ({ kind: 'list', items });
const missing = (reason: string): CompareCell => ({ kind: 'missing', reason });

const tokensText = (n: number) => `${spaced(n)} tokens`;

/** The tokens a profile's own limits allow one run, or null when it sets none. */
export function profileTokenLimit(profile: Pick<AgentProfileSummary, 'limits'>): number | null {
  const tokens = (profile.limits as { budgets?: { tokens?: unknown } } | null | undefined)?.budgets?.tokens;
  return typeof tokens === 'number' ? tokens : null;
}

/** "Lead, Reviewer" for the members for which `pick` holds; none when every one says no; unsaid otherwise. */
function memberFact(report: TeamReport | null | undefined, fact: 'writes_project', none: string): CompareCell {
  const members = report?.members;
  if (!members) return missing('The team did not report its members.');
  if (members.length === 0) return text('No members');
  const yes = members.filter((m) => m[fact] === true).map((m) => m.role);
  if (yes.length > 0) return list(yes);
  if (members.every((m) => m[fact] === false)) return text(none);
  return missing(`The team did not say this of its members.`);
}

export function buildCompare(input: CompareInput): CompareModel {
  const entries = [...input.teams].sort((a, b) => a.name.localeCompare(b.name));
  const lanes = new Map(input.flow.lanes.map((l) => [l.key, l]));
  const noReport = 'No computer reports this team, so nothing is known of it.';

  const cell = (entry: TeamEntry, say: (report: TeamReport) => CompareCell | null, why: string): CompareCell => {
    const report = entry.report;
    if (!report) return missing(noReport);
    return say(report) ?? missing(why);
  };
  const rows: Array<[string, string, (e: TeamEntry) => CompareCell]> = [
    [
      'works-in',
      'Works in',
      (e) => cell(e, (r) => (r.works_in ? text(r.works_in === 'repository' ? 'The repository, on a branch' : 'A folder') : null), 'The team did not report where it works.'),
    ],
    [
      'delivers',
      'Delivers',
      (e) => {
        const says = e.profiles
          .map((p) => ({ name: p.name, delivers: input.profiles.find((x) => x.agent_profile_id === p.id)?.delivers ?? null }))
          .filter((p): p is { name: string; delivers: string } => !!p.delivers);
        if (says.length === 0) return missing('No profile of this team says what it delivers. A profile gives it on the Agents page.');
        const same = new Set(says.map((s) => s.delivers)).size === 1;
        return same ? text(says[0].delivers) : list(says.map((s) => `${s.name}: ${s.delivers}`));
      },
    ],
    [
      'decides',
      'Decides',
      (e) => cell(e, (r) => (r.decides ? text(r.decides === 'asks_a_person' ? 'Stops for a person' : 'Runs to the end') : null), 'The team did not report whether it stops for a person.'),
    ],
    [
      'network',
      'Network',
      (e) => cell(e, (r) => (r.network ? text(networkText(r.network)) : null), 'The team did not report its network.'),
    ],
    [
      'members',
      'Members',
      (e) =>
        cell(
          e,
          (r) => (r.members ? (r.members.length > 0 ? list(r.members.map((m) => (m.model ? `${m.role} · ${m.model}` : m.role))) : text('No members')) : null),
          'The team did not report its members.',
        ),
    ],
    ['writes', 'Change the project', (e) => (e.report ? memberFact(e.report, 'writes_project', 'None: every member only reads') : missing(noReport))],
    [
      'steps',
      'Steps and their checks',
      (e) =>
        cell(e, (r) => (r.steps ? (r.steps.length > 0 ? list(r.steps.map(stepText)) : text('Reports no steps')) : null), 'The team did not report its steps.'),
    ],
    [
      'proposes',
      'Can propose',
      (e) => {
        if (!e.report) return missing(noReport);
        // The same reading the team's card on the Agents page makes: each member's own `can_propose`.
        if (e.report.members?.length === 0) return text('No members');
        const said = teamProposes(e.report);
        if (said.state === 'can') return list(said.roles);
        if (said.state === 'cannot') return text('No member proposes items');
        return missing(e.report.members ? 'The team did not say this of its members.' : 'The team did not report its members.');
      },
    ],
    [
      'merge',
      'Merge rung',
      (e) => {
        const a = input.autonomy;
        if (!a) return missing("The project's automation settings did not load.");
        const rung = mergeRung(a.level, a.teams.find((l) => l.name === e.name), e.report?.works_in);
        return rung ? text(rung.text) : missing("The project's automation settings did not load.");
      },
    ],
    [
      'limit',
      'Token limit per run',
      (e) => {
        const named = e.profiles.map((p) => ({ name: p.name, limit: profileTokenLimit(input.profiles.find((x) => x.agent_profile_id === p.id) ?? { limits: null }) }));
        if (named.length === 0) return missing('No profile names this team, so no limit is set for it.');
        const provisional = input.autonomy?.provisionalTokensPerRun ?? null;
        const words = (limit: number | null) => (limit !== null ? tokensText(limit) : provisional !== null ? `${tokensText(provisional)}, the project's provisional limit` : 'No limit');
        if (new Set(named.map((n) => n.limit)).size === 1) return text(words(named[0].limit));
        return list(named.map((n) => `${n.name}: ${words(n.limit)}`));
      },
    ],
    [
      'today',
      'Tokens today',
      (e) => {
        const lane = lanes.get(teamLaneKey(e.name));
        return lane ? text(tokensText(lane.tokensToday)) : missing('No run of this project used this team in the last 24 hours, so nothing is counted for it.');
      },
    ],
  ];
  return {
    teams: entries.map((e) => ({ name: e.name, reported: e.reported })),
    rows: rows.map(([key, label, say]) => ({ key, label, cells: entries.map(say) })),
  };
}
