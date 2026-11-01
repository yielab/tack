import { type Component, type JSX, For, Show, createMemo, createResource, createUniqueId } from 'solid-js';
import {
  AGENT_KINDS,
  SINGLE_AGENT_TEXT,
  agentCanSentence,
  agentCapabilityLines,
  agentName,
  pickHarness,
  profileAgentText,
  runnersApi,
  teamOf,
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
 * Who does a task, as one compact list: every profile a radio, grouped by the team it names or, when
 * it names none, by the agent that runs it. A team shows its members as chips, where it works as an
 * icon and what its agent can do as five icons; teams no profile names collapse into one disabled row.
 * The one picker of the new-item dialog, the item's Automation row and the Run with agent dialog.
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

  /** Reported teams first by name, then the names only a profile uses. */
  const teamNames = createMemo(() => {
    const names = new Set<string>(teams().map((t) => t.name));
    for (const p of props.profiles) {
      const team = teamOf(p);
      if (team) names.add(team);
    }
    return [...names].sort();
  });
  const entry = (name: string) => teams().find((t) => t.name === name);
  const profilesOfTeam = (name: string) => props.profiles.filter((p) => teamOf(p) === name);
  const namedTeams = createMemo(() => teamNames().filter((n) => profilesOfTeam(n).length > 0));
  const bareTeams = createMemo(() => teamNames().filter((n) => profilesOfTeam(n).length === 0));
  const kindOfTeam = (name: string) => entry(name)?.harness ?? profilesOfTeam(name).map((p) => agentKindOf(p, teams()))[0] ?? 'docket';
  /** The agents the listed teams run on, so their shared sentence is said once for each. */
  const teamKinds = createMemo(() => [...new Set(namedTeams().map(kindOfTeam))]);

  const alone = createMemo(() => props.profiles.filter((p) => !teamOf(p)));
  const agentKinds = createMemo(() => {
    const kinds = [...new Set(alone().map((p) => agentKindOf(p, teams())))];
    const rank = (k: string) => (k === '' ? 99 : AGENT_KINDS.includes(k) ? AGENT_KINDS.indexOf(k) : 50);
    return kinds.sort((a, b) => rank(a) - rank(b));
  });
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

  const worksIn = (name: string) => entry(name)?.report?.works_in ?? null;
  const roles = (name: string) => [...new Set((entry(name)?.report?.members ?? []).map((m) => m.role))];

  return (
    <div role="radiogroup" aria-label={props.label} data-testid="who-does-it" class="flex min-w-0 flex-col gap-2.5">
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

      <Show when={teamKinds().length > 0}>
        <div class="flex flex-col gap-0.5" data-testid="who-can">
          <For each={teamKinds()}>
            {(kind) => <span class="text-small" style={{ color: 'var(--color-text-secondary)' }}>{canText(kind, linesOf(kind), true)}</span>}
          </For>
          <Muted>A struck-through icon is something it cannot do or did not report; hover it for why.</Muted>
        </div>
      </Show>

      <For each={namedTeams()}>
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
                  <span class="ml-auto"><Capabilities kind={kindOfTeam(name)} reports={reports()} /></span>
                  <Show when={info() && !info()!.reported}>
                    <Muted disabled>No computer reports this team</Muted>
                  </Show>
                </>
              }
            >
              <For each={profilesOfTeam(name)}>{(p) => <Option profile={p} />}</For>
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

      <Show when={agentKinds().length > 0}>
        <p class="pt-1 text-sm font-bold" style={{ color: 'var(--color-text-primary)' }}>
          Single agents <span class="text-small font-normal" style={{ color: 'var(--color-text-tertiary)' }}>{SINGLE_AGENT_TEXT}</span>
        </p>
      </Show>
      <For each={agentKinds()}>
        {(kind) => (
          <Group
            testid={`who-alone-${kind || 'any'}`}
            head={
              <>
                <h4 class="text-sm font-bold" style={{ color: 'var(--color-text-primary)' }}>{kind ? agentName(kind) : profileAgentText({})}</h4>
                <Show when={kind}>
                  <span class="text-small" style={{ color: 'var(--color-text-secondary)' }}>{canText(kind, linesOf(kind))}</span>
                  <span class="ml-auto"><Capabilities kind={kind} reports={reports()} /></span>
                </Show>
              </>
            }
          >
            <For each={alone().filter((p) => agentKindOf(p, teams()) === kind)}>{(p) => <Option profile={p} />}</For>
          </Group>
        )}
      </For>

      <p class="text-small" style={{ color: 'var(--color-text-tertiary)' }}>{PER_TASK_CHOICE_TEXT}</p>
    </div>
  );
};

export default ProfilePicker;
