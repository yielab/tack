import type { AgentProfileSummary } from '../shared/execution/api';
import type { Routed } from '../shared/execution/routing';
import type { TeamEntry } from '../shared/execution/teams';
import type { Item } from '../shared/types';

/** One role of a task and the profile it names. */
interface TaskRole {
  name: string;
  profileId: string | null;
}

/**
 * What the server's `/routing` answers for a world of items, profiles and teams, by item id: the
 * profile chosen on the task, else the one its roles agree on, else the project's default; nobody when
 * none is named, it is gone, or its team is not reported. Done tasks are not open, so they are not asked.
 */
export function routingFixture(world: {
  items: readonly Item[];
  roles?: ReadonlyMap<string, readonly TaskRole[]>;
  profiles: readonly AgentProfileSummary[];
  teams: readonly TeamEntry[];
  defaultProfileId: string | null;
  statuses: ReadonlyArray<{ name: string; category?: string }>;
}): Map<string, Routed> {
  const done = new Set(world.statuses.filter((s) => s.category === 'done').map((s) => s.name));
  const out = new Map<string, Routed>();
  for (const item of world.items) {
    if (done.has(item.status)) continue;
    const roles = world.roles?.get(item.id) ?? [];
    const named = roles.filter((r) => r.profileId);
    const own = item.run_settings?.agent_profile_id || null;
    let source: string | null = null;
    let profileId: string | null = null;
    let role: string | null = null;
    if (own) {
      [source, profileId] = ['run_settings', own];
    } else if (named.length > 0 && named.every((r) => r.profileId === named[0].profileId)) {
      [source, profileId, role] = [`role:${named[0].name}`, named[0].profileId, named[0].name];
    } else if (world.defaultProfileId) {
      [source, profileId] = [named.length > 0 ? 'ambiguous' : 'project_default', world.defaultProfileId];
    }
    const routed: Routed = { item_id: item.id, profile_id: profileId, source, role, team: null, harness: null, nobody: null };
    if (!profileId) {
      routed.role = roles[0]?.name ?? null;
      routed.nobody = roles.length > 0 ? 'role_without_profile' : 'no_role';
    } else {
      const found = world.profiles.find((p) => p.agent_profile_id === profileId);
      const policy = found?.tool_policy as { agent?: string; docket?: { team?: string } } | undefined;
      routed.team = policy?.docket?.team ?? null;
      routed.harness = policy?.agent ?? null;
      if (!found) routed.nobody = 'profile_missing';
      else if (routed.team && !world.teams.find((t) => t.name === routed.team)?.reported) routed.nobody = 'team_not_reported';
    }
    out.set(item.id, routed);
  }
  return out;
}
