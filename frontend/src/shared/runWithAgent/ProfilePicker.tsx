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
  type AgentProfileSummary,
  type HarnessCapability,
  type TeamEntry,
} from '../execution';
import { madeOf } from './WhoDoesIt';

/** Said once under the choice, in every place the choice is made. */
export const PER_TASK_CHOICE_TEXT = 'Each task can use a different agent; this choice is for this task.';

export const NOBODY_YET = 'Nobody yet';
export const MAKE_A_PROFILE = 'Make a profile for it on the Agents page';

/** The agent a profile runs on: its team's agent, else the one its tool policy names, else none. */
function agentKindOf(profile: AgentProfileSummary, teams: readonly TeamEntry[]): string {
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
  /** Whether "Nobody yet" is offered (default true). */
  withNobody?: boolean;
  disabled?: boolean;
}

/** What the agent said about itself, as one sentence and a muted line for each thing it cannot or did not say. */
const AgentLines: Component<{ kind: string; reports: readonly HarnessCapability[] }> = (props) => {
  const lines = () => agentCapabilityLines(props.kind, pickHarness(props.reports, props.kind));
  const unsaid = () => lines().filter((l) => l.state !== 'can');
  const nothingReported = () => lines().every((l) => l.state === 'not_reported');
  return (
    <div class="flex flex-col gap-0.5" data-testid={`who-agent-${props.kind}`}>
      <Show
        when={!nothingReported()}
        fallback={
          <span aria-disabled="true" class="text-small" style={{ color: 'var(--color-text-tertiary)' }}>
            Not reported by {agentName(props.kind)}
          </span>
        }
      >
        <span class="text-small" style={{ color: 'var(--color-text-secondary)' }}>{agentCanSentence(props.kind, lines())}</span>
        <For each={unsaid()}>
          {(line) => (
            <span aria-disabled="true" class="text-small" style={{ color: 'var(--color-text-tertiary)' }}>
              {line.text}
              <Show when={line.reason}>{' · '}{line.reason}</Show>
            </span>
          )}
        </For>
      </Show>
    </div>
  );
};

/**
 * Who does a task: every profile, grouped by the team it names (with what the team is made of) or, when
 * it names none, by the agent that runs it; each agent says what it can do from what it reported. The
 * one picker of the new-item dialog, the item's Automation row and the Run with agent dialog.
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

  const alone = createMemo(() => props.profiles.filter((p) => !teamOf(p)));
  const agentKinds = createMemo(() => {
    const kinds = [...new Set(alone().map((p) => agentKindOf(p, teams())))];
    const rank = (k: string) => (k === '' ? 99 : AGENT_KINDS.includes(k) ? AGENT_KINDS.indexOf(k) : 50);
    return kinds.sort((a, b) => rank(a) - rank(b));
  });

  const Option: Component<{ profile: AgentProfileSummary }> = (o) => {
    const id = `${uid}-${o.profile.agent_profile_id}`;
    const note = () => o.profile.summary || o.profile.delivers || null;
    return (
      <div class="flex items-start gap-2">
        <input
          id={id}
          type="radio"
          name={uid}
          value={o.profile.agent_profile_id}
          checked={props.value === o.profile.agent_profile_id}
          disabled={props.disabled}
          aria-describedby={note() ? `${id}-note` : undefined}
          class="mt-1"
          onClick={() => props.onChange(o.profile)}
        />
        <div class="min-w-0">
          <label for={id} class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>{o.profile.name}</label>
          <Show when={note()}>
            {(text) => <p id={`${id}-note`} class="text-small" style={{ color: 'var(--color-text-secondary)' }}>{text()}</p>}
          </Show>
        </div>
      </div>
    );
  };

  const Group: Component<{ testid: string; title: string; children: JSX.Element }> = (g) => (
    <section class="flex flex-col gap-1.5 rounded-xl bg-app px-3.5 py-3" data-testid={g.testid}>
      <h4 class="text-sm font-bold" style={{ color: 'var(--color-text-primary)' }}>{g.title}</h4>
      {g.children}
    </section>
  );

  return (
    <div role="radiogroup" aria-label={props.label} data-testid="who-does-it" class="flex flex-col gap-2.5">
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

      <For each={teamNames()}>
        {(name) => {
          const mine = () => profilesOfTeam(name);
          const info = () => entry(name);
          const kind = () => info()?.harness ?? mine().map((p) => agentKindOf(p, teams()))[0] ?? 'docket';
          return (
            <Group testid={`who-team-${name}`} title={`Team ${name}`}>
              <Show when={madeOf(info())}>
                {(made) => <span class="text-small" style={{ color: 'var(--color-text-secondary)' }}>Made of {made()}</span>}
              </Show>
              <Show when={info() && !info()!.reported}>
                <span aria-disabled="true" class="text-small" style={{ color: 'var(--color-text-tertiary)' }}>No computer reports this team</span>
              </Show>
              <AgentLines kind={kind()} reports={reports()} />
              <For each={mine()} fallback={
                <div class="flex items-start gap-2" aria-disabled="true">
                  <input type="radio" disabled class="mt-1" aria-label={`Team ${name}: ${MAKE_A_PROFILE}`} />
                  <span class="text-small" style={{ color: 'var(--color-text-tertiary)' }}>{MAKE_A_PROFILE}</span>
                </div>
              }>
                {(p) => <Option profile={p} />}
              </For>
            </Group>
          );
        }}
      </For>

      <For each={agentKinds()}>
        {(kind) => (
          <Group testid={`who-alone-${kind || 'any'}`} title={kind ? agentName(kind) : profileAgentText({})}>
            <span class="text-small" style={{ color: 'var(--color-text-secondary)' }}>{SINGLE_AGENT_TEXT}</span>
            <Show when={kind}><AgentLines kind={kind} reports={reports()} /></Show>
            <For each={alone().filter((p) => agentKindOf(p, teams()) === kind)}>{(p) => <Option profile={p} />}</For>
          </Group>
        )}
      </For>

      <p class="text-small" style={{ color: 'var(--color-text-tertiary)' }}>{PER_TASK_CHOICE_TEXT}</p>
    </div>
  );
};

export default ProfilePicker;
