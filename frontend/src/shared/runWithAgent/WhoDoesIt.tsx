import { type Component, For, Show, createMemo } from 'solid-js';
import type { AgentProfileSummary } from '../execution/api';
import { teamOf } from '../execution/api';
import type { components } from '../api/schema.gen';

export type TeamEntry = components['schemas']['TeamEntry'];

/** The agent a profile runs on: the first harness its tool policy names; "Agent" when it names none. */
export function agentOf(profile: AgentProfileSummary): string {
  const tools = (profile.tool_policy as { tools?: Record<string, unknown> } | null | undefined)?.tools;
  return tools ? (Object.keys(tools)[0] ?? 'Agent') : 'Agent';
}

/** What a team is made of, in a few words: its members' roles, or nothing when no agent reported it. */
export function madeOf(team: TeamEntry | undefined): string {
  const roles = [...new Set((team?.report?.members ?? []).map((m) => m.role))];
  return roles.join(', ');
}

const NOBODY = '';
const UNMATCHED = 'team:';
const NO_TEAM = 'profile:';

export interface WhoDoesItProps {
  /** The team the entry goes to, by name; null for nobody yet. */
  value: string | null;
  onChange: (team: string | null) => void;
  /** Every profile the project's agents have. */
  profiles: AgentProfileSummary[];
  /** What GET /api/teams reports; a team it does not list shows its name only. */
  teams?: TeamEntry[];
  /** The accessible name of the control. */
  label: string;
  /** The control's id, so a visible label can name it. */
  id?: string;
  disabled?: boolean;
}

/**
 * Who does an entry: one option per profile that names a team, grouped by that team with what the
 * team is made of; a profile that names no team is listed under its agent and cannot be chosen, since
 * a team is the only thing a proposal can send.
 */
const WhoDoesIt: Component<WhoDoesItProps> = (props) => {
  const teamNames = createMemo(() => [...new Set(props.profiles.map(teamOf).filter((t): t is string => !!t))]);
  const teamEntry = (name: string) => props.teams?.find((t) => t.name === name);
  const profilesOf = (name: string) => props.profiles.filter((p) => teamOf(p) === name);
  const noTeam = createMemo(() => props.profiles.filter((p) => !teamOf(p)));
  const agents = createMemo(() => [...new Set(noTeam().map(agentOf))]);
  const unmatched = () => (props.value && !teamNames().includes(props.value) ? props.value : null);
  const selected = () => {
    if (!props.value) return NOBODY;
    if (unmatched()) return UNMATCHED + props.value;
    return profilesOf(props.value)[0]?.agent_profile_id ?? NOBODY;
  };
  const groupLabel = (name: string) => {
    const made = madeOf(teamEntry(name));
    const count = profilesOf(name).length;
    return [`Team ${name}`, made, count > 1 ? `${count} profiles name it, so it cannot pick one` : null].filter(Boolean).join(' · ');
  };
  const choose = (value: string) => {
    if (value === NOBODY) return props.onChange(null);
    if (value.startsWith(UNMATCHED)) return;
    const profile = props.profiles.find((p) => p.agent_profile_id === value);
    if (profile) props.onChange(teamOf(profile));
  };

  return (
    <select
      id={props.id}
      aria-label={props.label}
      disabled={props.disabled}
      class="rounded px-2 py-1 text-sm"
      style={{ border: '1px solid var(--color-border-light)', color: 'var(--color-text-primary)', background: 'transparent' }}
      onChange={(e) => choose(e.currentTarget.value)}
    >
      <option value={NOBODY} selected={selected() === NOBODY}>
        Nobody yet
      </option>
      <Show when={unmatched()}>
        {(team) => (
          <option value={UNMATCHED + team()} selected={selected() === UNMATCHED + team()} disabled>
            {`Team ${team()} · no profile names it`}
          </option>
        )}
      </Show>
      <For each={teamNames()}>
        {(name) => (
          <optgroup label={groupLabel(name)}>
            <For each={profilesOf(name)}>
              {(p) => (
                <option value={p.agent_profile_id} selected={selected() === p.agent_profile_id}>
                  {p.name}
                </option>
              )}
            </For>
          </optgroup>
        )}
      </For>
      <For each={agents()}>
        {(agent) => (
          <optgroup label={`${agent} · no team`}>
            <For each={noTeam().filter((p) => agentOf(p) === agent)}>
              {(p) => (
                <option value={NO_TEAM + p.agent_profile_id} disabled>
                  {`${p.name} · names no team, so a proposal cannot send it`}
                </option>
              )}
            </For>
          </optgroup>
        )}
      </For>
    </select>
  );
};

export default WhoDoesIt;
