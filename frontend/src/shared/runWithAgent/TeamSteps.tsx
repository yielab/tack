import { type Component, For, Show, createEffect, createResource, onCleanup } from 'solid-js';
import { agentName } from '../execution/agents';
import { agentProfilesApi, attemptsApi, teamOf, type AgentProfileSummary, type ExecutionSummary } from '../execution';
import { teamsApi, type TeamEntry } from '../execution/teams';
import type { TeamStepReport } from '../execution/types';
import { matchSteps } from './teamSteps';

type EventLike = { kind: string; payload: unknown };

export interface TeamStepsProps {
  /** The profile the run was made with; its team is looked up by name. */
  profileId?: string | null;
  /** The agent the run asked for (`docket`, `codex`, …); names the agent in the sentence. */
  agentKind?: string | null;
  /** The run's events so far, oldest first. */
  events: readonly EventLike[];
  /** The run has ended: a step still running stopped there. */
  ended?: boolean;
}

const KIND_WORDS: Record<TeamStepReport['kind'], string | null> = {
  agent: null,
  command: 'runs a command',
  ask: null,
  wait: 'waits',
  parallel: 'runs side by side',
};

const capitalised = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

/** The run's profile and agent, as the run's summary carries them. */
export function runWho(summary: ExecutionSummary | undefined): { profileId: string | null; agentKind: string | null } {
  return { profileId: summary?.agent_profile_id ?? null, agentKind: summary?.requested_harness_kind ?? null };
}

/** The team a profile names and the agent's report of it, or nothing. */
function reportedSteps(profiles: AgentProfileSummary[], teams: TeamEntry[], profileId: string, agentKind: string | null) {
  const name = teamOf(profiles.find((p) => p.agent_profile_id === profileId));
  if (!name) return { steps: null, harness: null };
  const entries = teams.filter((t) => t.name === name && t.report);
  const entry = entries.find((t) => agentKind && t.harness === agentKind) ?? entries.find((t) => !agentKind || !t.harness);
  const steps = entry?.report?.steps;
  return { steps: steps && steps.length > 0 ? steps : null, harness: entry?.harness ?? null };
}

const LABEL = { done: 'Done', running: 'Running', waiting: 'Waiting' } as const;
const MARK_COLOR = {
  done: 'var(--color-success-600)',
  running: 'var(--color-primary-600)',
  waiting: 'var(--color-border-light)',
} as const;

/**
 * The steps the run's team reports, in order, each done, running or waiting and with what checks it;
 * or, when the team (or the agent) reports none, a sentence that says so. A step nobody with a role
 * does carries no mark, since no event says where it stands.
 */
const TeamStepsLine: Component<TeamStepsProps> = (props) => {
  const [who] = createResource(
    () => props.profileId ?? undefined,
    async () => {
      const [profiles, teams] = await Promise.all([
        agentProfilesApi.list().then((r) => r.data.data ?? []).catch(() => [] as AgentProfileSummary[]),
        teamsApi.list().catch(() => [] as TeamEntry[]),
      ]);
      return { profiles, teams };
    },
  );
  const settled = () => !props.profileId || who.state === 'ready' || who.state === 'errored';
  const found = () => {
    const w = who.state === 'ready' ? who() : undefined;
    return w && props.profileId ? reportedSteps(w.profiles, w.teams, props.profileId, props.agentKind ?? null) : { steps: null, harness: null };
  };
  const agent = () => props.agentKind ?? found().harness;
  const shown = () => {
    const steps = found().steps;
    return steps ? matchSteps(steps, props.events) : null;
  };

  return (
    <Show when={settled()}>
      <Show
        when={shown()}
        fallback={
          <Show when={agent()}>
            {(kind) => (
              <p data-testid="team-steps-unreported" class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                {agentName(kind())} does not report its steps in advance
              </p>
            )}
          </Show>
        }
      >
        {(steps) => (
          <ol data-testid="team-steps" aria-label="The team's steps" class="flex flex-wrap gap-2">
            <For each={steps()}>
              {(step) => {
                const words = () => [step.role ? capitalised(step.role) : KIND_WORDS[step.kind], step.check].filter((w): w is string => !!w);
                const stopped = () => step.state === 'running' && props.ended;
                return (
                  <li
                    data-step={step.name}
                    data-state={step.state ?? undefined}
                    class="min-w-0 max-w-full rounded-item px-3 py-2 text-xs break-words"
                    style={{ 'background-color': 'var(--color-bg-subtle)', color: 'var(--color-text-secondary)' }}
                  >
                    <div class="flex flex-wrap items-center gap-1.5">
                      <Show when={step.state}>
                        {(state) => <span aria-hidden="true" class="h-2 w-2 flex-none rounded-full" style={{ 'background-color': MARK_COLOR[state()] }} />}
                      </Show>
                      <span class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>{step.name}</span>
                      <Show when={step.state}>
                        {(state) => <span style={{ color: 'var(--color-text-tertiary)' }}>{stopped() ? 'Stopped here' : LABEL[state()]}</span>}
                      </Show>
                    </div>
                    <Show when={words().length > 0}>
                      <p class="mt-0.5">{words().join(' · ')}</p>
                    </Show>
                  </li>
                );
              }}
            </For>
          </ol>
        )}
      </Show>
    </Show>
  );
};

export default TeamStepsLine;

const LIVE_RELOAD_MS = 3000;

/** The same line for one attempt, reading the attempt's events itself (every few seconds while it runs). */
export const AttemptTeamSteps: Component<{
  requestId: string;
  attemptNumber: number;
  live: boolean;
  profileId?: string | null;
  agentKind?: string | null;
}> = (props) => {
  const [events, { refetch }] = createResource(
    () => `${props.requestId}:${props.attemptNumber}:${props.live}`,
    () => attemptsApi.events(props.requestId, props.attemptNumber).then((r) => r.data.data ?? []),
  );
  createEffect(() => {
    if (!props.live) return;
    const timer = setInterval(() => void refetch(), LIVE_RELOAD_MS);
    onCleanup(() => clearInterval(timer));
  });
  return (
    <TeamStepsLine
      profileId={props.profileId}
      agentKind={props.agentKind}
      events={events.state === 'errored' ? [] : (events.latest ?? [])}
      ended={!props.live}
    />
  );
};
