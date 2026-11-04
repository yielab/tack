import { type Component, type JSX, For, Show, createMemo, createResource, createSignal, createUniqueId } from 'solid-js';
import {
  AGENT_KINDS,
  SINGLE_AGENT_TEXT,
  agentCanSentence,
  agentCapabilityLines,
  agentName,
  pickHarness,
  runnersApi,
  teamOf,
  isTeamOfOne,
  teamsApi,
  type AgentCapabilityLine,
  type AgentProfileSummary,
  type HarnessCapability,
  type TeamEntry,
} from '../execution';
import { A } from '@solidjs/router';
import { CapabilityGlyph, IconBranch, IconFolder } from './runDialogIcons';

/** Said once under the choice, in every place the choice is made. */
export const PER_TASK_CHOICE_TEXT = 'Each task can use a different agent; this choice is for this task.';

export const NOBODY_YET = 'Nobody yet';
export const MAKE_A_PROFILE = 'Make a profile for it on the Agents page';

/** "3 teams have no profile yet": the one row standing for every reported team that no profile names. */
const noProfileText = (n: number) => (n === 1 ? '1 team has no profile yet' : `${n} teams have no profile yet`);

/** The agent a profile runs on: its team's agent, else the one its tool policy names, else none. */
export function agentKindOf(profile: AgentProfileSummary, teams: readonly TeamEntry[]): string {
  const team = teamOf(profile);
  if (team) return teams.find((t) => t.name === team)?.harness ?? 'docket';
  const policy = profile.tool_policy as { agent?: unknown; tools?: Record<string, unknown> } | null | undefined;
  if (typeof policy?.agent === 'string' && policy.agent) return policy.agent;
  return Object.keys(policy?.tools ?? {})[0] ?? '';
}

/**
 * Whether a profile is offered under an agent. A team's profile is offered under its team's agent, and
 * a profile that names one agent under that one; any other under each agent its tool policy lists, or
 * every agent when it lists none. A docket run goes to a team, so docket offers a profile without a team
 * only while no profile names a team.
 */
export function offeredUnder(
  profile: AgentProfileSummary,
  kind: string,
  all: readonly AgentProfileSummary[],
  teams: readonly TeamEntry[] = [],
): boolean {
  const team = teamOf(profile);
  if (team) return (teams.find((t) => t.name === team)?.harness ?? 'docket') === kind;
  const policy = profile.tool_policy as { agent?: unknown; tools?: Record<string, unknown> } | null | undefined;
  if (typeof policy?.agent === 'string' && policy.agent) return policy.agent === kind;
  const listed = Object.keys(policy?.tools ?? {});
  const runs = listed.length === 0 || listed.includes(kind);
  return kind === 'docket' ? runs && !all.some((p) => teamOf(p)) : runs;
}

/** One agent a person can choose; a disabled one says why. */
export interface AgentChoice {
  value: string;
  label: string;
  disabled?: boolean;
  reason?: string;
}

/** Docket first, then the agents in the order the Agents page lists them, then any other by name. */
export const agentRank = (kind: string) => (kind === 'docket' ? -1 : AGENT_KINDS.includes(kind) ? AGENT_KINDS.indexOf(kind) : 50);

export interface ProfilePickerProps {
  /** The profile chosen, by id; null for nobody yet. */
  value: string | null;
  /** The profile picked, or null for "Nobody yet". */
  onChange: (profile: AgentProfileSummary | null) => void;
  /** Every profile the project's agents have. */
  profiles: readonly AgentProfileSummary[];
  /** The agents' reports, to say what each can do; read from the computers when not given. */
  reports?: readonly HarnessCapability[];
  /** The teams any agent reports (GET /api/teams); fetched when not given. */
  teams?: readonly TeamEntry[];
  /** The agents to choose from; when not given, every agent a computer reports or a profile names. */
  agents?: readonly AgentChoice[];
  /** The agent chosen, when the caller holds it (the Run dialog sends it); else the picker keeps its own. */
  agent?: string;
  /** A click on an agent. */
  onAgent?: (kind: string) => void;
  /** The accessible name of the group. */
  label: string;
  /** Where the chosen profile came from ("Chosen by the role Implementer"), said under its row. */
  choiceNote?: string | null;
  /** Whether "Nobody yet" is offered (default true). */
  withNobody?: boolean;
  disabled?: boolean;
}

/** The agent's sentence about itself, said once; `named` puts the agent first, for a line that does not sit under its name. */
function canText(kind: string, lines: readonly AgentCapabilityLine[], named = false): string {
  if (lines.every((l) => l.state === 'not_reported')) return `Not reported by ${agentName(kind)}`;
  const sentence = agentCanSentence(kind, lines);
  return named && sentence.startsWith('Can ') ? `${agentName(kind)} can ${sentence.slice(4)}` : sentence;
}

/**
 * Five icons, one per thing an agent can do. Only one it cannot do, or did not report, is struck
 * through; the tooltip and the accessible name carry the reason, and the agent's own note when it can.
 */
const Capabilities: Component<{ kind: string; reports: readonly HarnessCapability[] }> = (props) => {
  const lines = () => agentCapabilityLines(props.kind, pickHarness(props.reports, props.kind));
  const words = (line: AgentCapabilityLine) =>
    [line.text, line.reason].filter(Boolean).join(' · ');
  return (
    <span role="group" aria-label={`What ${agentName(props.kind)} can do`} class="inline-flex flex-none items-center gap-1.5" data-testid={`who-agent-${props.kind}`}>
      <For each={lines()}>
        {(line) => (
          <span
            role="img"
            aria-label={words(line)}
            title={words(line)}
            data-capability={line.id}
            data-state={line.state}
            style={{ color: line.state === 'can' ? 'var(--color-text-secondary)' : 'var(--color-text-tertiary)' }}
          >
            <CapabilityGlyph id={line.id} struck={line.state !== 'can'} />
          </span>
        )}
      </For>
    </span>
  );
};

/** A line of the group head: muted, wrapping, never wider than the dialog. */
const Muted: Component<{ children: JSX.Element; disabled?: boolean }> = (props) => (
  <span
    aria-disabled={props.disabled ? 'true' : undefined}
    class="text-small"
    style={{ color: 'var(--color-text-tertiary)', 'overflow-wrap': 'anywhere' }}
  >
    {props.children}
  </span>
);

/**
 * Who does a task, in two steps: first the agent, then only the profiles that agent runs. Under docket
 * the profiles are grouped by the team they name, each team with its members as chips and where it
 * works as an icon, and the teams no profile names collapse into one disabled row; under any other agent
 * they are one list. What the chosen agent can do is said once, as five icons.
 * The one picker of the new-item dialog, the item's Automation row, the Flow view and the Run with agent dialog.
 */
const ProfilePicker: Component<ProfilePickerProps> = (props) => {
  const uid = createUniqueId();
  const [fetched] = createResource(() => (props.teams ? null : true), () => teamsApi.list().catch(() => [] as TeamEntry[]));
  const teams = (): readonly TeamEntry[] => props.teams ?? fetched() ?? [];
  const [runners] = createResource(
    () => (props.reports ? null : true),
    () => runnersApi.list().then((r) => r.data.data ?? [], () => []),
  );
  const reports = (): readonly HarnessCapability[] =>
    props.reports ??
    (runners() ?? []).flatMap((r) => {
      const harnesses = (r.capability_snapshot as { harnesses?: unknown } | null | undefined)?.harnesses;
      return Array.isArray(harnesses) ? (harnesses as HarnessCapability[]) : [];
    });
  const offered = (p: AgentProfileSummary, kind: string) => offeredUnder(p, kind, props.profiles, teams());

  /** Every agent a computer reports or a profile names, docket first. */
  const agents = createMemo((): readonly AgentChoice[] => {
    if (props.agents) return [...props.agents].sort((a, b) => agentRank(a.value) - agentRank(b.value));
    const kinds = new Set<string>(reports().map((h) => h.harness_kind));
    for (const p of props.profiles) {
      const team = teamOf(p);
      if (team) kinds.add(teams().find((t) => t.name === team)?.harness ?? 'docket');
      else {
        const policy = p.tool_policy as { agent?: unknown; tools?: Record<string, unknown> } | null | undefined;
        if (typeof policy?.agent === 'string' && policy.agent) kinds.add(policy.agent);
        else for (const k of Object.keys(policy?.tools ?? {})) kinds.add(k);
      }
    }
    if (kinds.size === 0) for (const k of AGENT_KINDS) if (k !== 'docket') kinds.add(k);
    return [...kinds].sort((a, b) => agentRank(a) - agentRank(b)).map((value) => ({ value, label: agentName(value) }));
  });
  const enabled = () => agents().filter((a) => !a.disabled);
  const selected = () => props.profiles.find((p) => p.agent_profile_id === props.value);

  // Uncontrolled, the picker opens on the chosen profile's agent, else the first agent with a profile.
  const [own, setOwn] = createSignal<string | null>(null);
  const agent = (): string => {
    if (props.agent !== undefined) return props.agent;
    const mine = own();
    if (mine) return mine;
    const chosen = selected();
    const home = (chosen && enabled().find((a) => offered(chosen, a.value))) ?? enabled().find((a) => props.profiles.some((p) => offered(p, a.value)));
    return (home ?? enabled()[0] ?? agents()[0])?.value ?? '';
  };
  const chooseAgent = (kind: string) => {
    setOwn(kind);
    props.onAgent?.(kind);
  };

  /** The profiles offered under the chosen agent, and the chosen one when no agent here offers it. */
  const listed = createMemo(() => {
    const kind = agent();
    const rows = props.profiles.filter((p) => offered(p, kind));
    const chosen = selected();
    if (chosen && !rows.includes(chosen) && !enabled().some((a) => offered(chosen, a.value))) rows.push(chosen);
    return rows;
  });
  const withTeam = createMemo(() => [...new Set(listed().map(teamOf).filter((t): t is string => !!t))].sort());
  const alone = createMemo(() => listed().filter((p) => !teamOf(p)));
  /** The reported teams of this agent that no profile names. */
  const bareTeams = createMemo(() =>
    teams()
      .filter((t) => !isTeamOfOne(t) && (t.harness ?? 'docket') === agent() && !props.profiles.some((p) => teamOf(p) === t.name))
      .map((t) => t.name)
      .sort());

  const entry = (name: string) => teams().find((t) => t.name === name);
  const worksIn = (name: string) => entry(name)?.report?.works_in ?? null;
  const roles = (name: string) => [...new Set((entry(name)?.report?.members ?? []).map((m) => m.role))];
  const linesOf = (kind: string) => agentCapabilityLines(kind, pickHarness(reports(), kind));

  const Option: Component<{ profile: AgentProfileSummary }> = (o) => {
    const id = `${uid}-${o.profile.agent_profile_id}`;
    const note = () => o.profile.summary || o.profile.delivers || null;
    const chosen = () => props.value === o.profile.agent_profile_id;
    return (
      <div class="flex min-w-0 items-start gap-2">
        <input
          id={id}
          type="radio"
          name={uid}
          value={o.profile.agent_profile_id}
          checked={chosen()}
          disabled={props.disabled}
          aria-describedby={note() ? `${id}-note` : undefined}
          class="mt-1"
          onClick={() => props.onChange(o.profile)}
        />
        <div class="min-w-0">
          <label for={id} class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)', 'overflow-wrap': 'anywhere' }}>{o.profile.name}</label>
          <Show when={chosen() && props.choiceNote}>
            {(text) => <p data-testid="profile-source" class="text-small" style={{ color: 'var(--color-text-secondary)' }}>{text()}</p>}
          </Show>
          <Show when={note()}>
            {(text) => <p id={`${id}-note`} class="text-small" style={{ color: 'var(--color-text-secondary)', 'overflow-wrap': 'anywhere' }}>{text()}</p>}
          </Show>
        </div>
      </div>
    );
  };

  /** One group of rows under a hairline: a head line, then the profiles. */
  const Group: Component<{ testid: string; head: JSX.Element; children: JSX.Element }> = (g) => (
    <section class="flex min-w-0 flex-col gap-1.5 pt-2.5" style={{ 'border-top': '1px solid var(--color-border-light)' }} data-testid={g.testid}>
      <div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">{g.head}</div>
      {g.children}
    </section>
  );

  return (
    <div data-testid="who-does-it" class="flex min-w-0 flex-col gap-2.5">
      <div role="radiogroup" aria-label="Agent" data-testid="who-agent-choice" class="flex min-w-0 flex-wrap gap-1.5">
        <For each={agents()}>
          {(a) => {
            const on = () => a.value === agent();
            return (
              <button
                type="button"
                role="radio"
                aria-checked={on() ? 'true' : 'false'}
                data-agent={a.value}
                disabled={a.disabled || props.disabled}
                title={a.reason}
                onClick={() => chooseAgent(a.value)}
                class="rounded-full px-3.5 py-1.5 text-sm font-semibold focus:outline-none focus-visible:ring-2 disabled:cursor-not-allowed"
                style={{
                  'background-color': on() ? 'var(--color-accent-soft)' : 'var(--color-bg-app)',
                  color: a.disabled ? 'var(--color-text-tertiary)' : on() ? 'var(--color-accent-ink)' : 'var(--color-text-primary)',
                  border: `1px solid ${on() ? 'var(--color-accent-ink)' : 'var(--color-border-light)'}`,
                  '--tw-ring-color': 'var(--color-focus-ring)',
                }}
              >
                {a.label}
                <Show when={a.disabled && a.reason}><span class="sr-only">{`. ${a.reason}`}</span></Show>
              </button>
            );
          }}
        </For>
      </div>

      <Show when={agent()}>
        <div class="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1" data-testid="who-can">
          <span class="text-small" style={{ color: 'var(--color-text-secondary)' }}>{canText(agent(), linesOf(agent()), true)}</span>
          <span class="ml-auto"><Capabilities kind={agent()} reports={reports()} /></span>
          <span class="w-full"><Muted>A struck-through icon is something it cannot do or did not report; hover it for why.</Muted></span>
        </div>
      </Show>

      <div role="radiogroup" aria-label={props.label} class="flex min-w-0 flex-col gap-2.5">
        <Show when={props.withNobody !== false}>
          <div class="flex items-center gap-2">
            <input
              id={`${uid}-nobody`}
              type="radio"
              name={uid}
              value=""
              checked={!props.value}
              disabled={props.disabled}
              onClick={() => props.onChange(null)}
            />
            <label for={`${uid}-nobody`} class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>{NOBODY_YET}</label>
          </div>
        </Show>

        <For each={withTeam()}>
          {(name) => {
            const info = () => entry(name);
            return (
              <Group
                testid={`who-team-${name}`}
                head={
                  <>
                    <h4 class="text-sm font-bold" style={{ color: 'var(--color-text-primary)', 'overflow-wrap': 'anywhere' }}>{`Team ${name}`}</h4>
                    <Show when={roles(name).length > 0}>
                      <ul aria-label="Made of" class="flex min-w-0 flex-wrap gap-1" data-testid="who-members">
                        <For each={roles(name)}>
                          {(role) => (
                            <li
                              class="rounded-full px-2 text-small"
                              style={{ 'background-color': 'var(--color-bg-subtle)', color: 'var(--color-text-secondary)', 'overflow-wrap': 'anywhere' }}
                            >
                              {role}
                            </li>
                          )}
                        </For>
                      </ul>
                    </Show>
                    <Show when={worksIn(name)}>
                      {(where) => {
                        const text = () => (where() === 'folder' ? 'Works in its own folder, not in the repository' : 'Works on a branch of the repository');
                        return (
                          <span role="img" aria-label={text()} title={text()} data-works-in={where()} style={{ color: 'var(--color-text-secondary)' }}>
                            {where() === 'folder' ? <IconFolder size={16} /> : <IconBranch size={16} />}
                          </span>
                        );
                      }}
                    </Show>
                    <Show when={info() && !info()!.reported}>
                      <Muted disabled>No computer reports this team</Muted>
                    </Show>
                  </>
                }
              >
                <For each={listed().filter((p) => teamOf(p) === name)}>{(p) => <Option profile={p} />}</For>
              </Group>
            );
          }}
        </For>

        <Show when={bareTeams().length > 0}>
          <div
            class="flex min-w-0 items-start gap-2 pt-2.5"
            style={{ 'border-top': '1px solid var(--color-border-light)' }}
            aria-disabled="true"
            data-testid="who-teams-without-profile"
            title={bareTeams().join(', ')}
          >
            <input type="radio" disabled class="mt-1" aria-label={`${noProfileText(bareTeams().length)}: ${MAKE_A_PROFILE}`} />
            <div class="min-w-0">
              <span class="text-small" style={{ color: 'var(--color-text-tertiary)' }}>
                {noProfileText(bareTeams().length)}{' '}
                <A href="/agents" class="font-semibold hover:underline" style={{ color: 'var(--color-accent-ink)' }}>→ Agents</A>
              </span>
              <p class="text-small" style={{ color: 'var(--color-text-tertiary)', 'overflow-wrap': 'anywhere' }}>{bareTeams().join(', ')}</p>
            </div>
          </div>
        </Show>

        <Show when={alone().length > 0}>
          <section class="flex min-w-0 flex-col gap-1.5" data-testid={`who-alone-${agent() || 'any'}`}>
            <Show when={withTeam().length > 0}>
              <p class="pt-2.5 text-small" style={{ color: 'var(--color-text-tertiary)', 'border-top': '1px solid var(--color-border-light)' }}>{SINGLE_AGENT_TEXT}</p>
            </Show>
            <For each={alone()}>{(p) => <Option profile={p} />}</For>
          </section>
        </Show>

        <Show when={listed().length === 0 && bareTeams().length === 0}>
          <p data-testid="who-agent-empty" class="text-small" style={{ color: 'var(--color-text-tertiary)' }}>
            {`No profile runs on ${agentName(agent())} yet `}
            <A href="/agents" class="font-semibold hover:underline" style={{ color: 'var(--color-accent-ink)' }}>→ Agents</A>
          </p>
        </Show>
      </div>

      <p class="text-small" style={{ color: 'var(--color-text-tertiary)' }}>{PER_TASK_CHOICE_TEXT}</p>
    </div>
  );
};

export default ProfilePicker;
