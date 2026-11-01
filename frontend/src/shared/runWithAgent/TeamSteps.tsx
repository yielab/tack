import { type Component, For, Show, createEffect, createResource, onCleanup } from 'solid-js';
import { agentName } from '../execution/agents';
import { agentProfilesApi, attemptsApi, teamOf, type AgentProfileSummary, type ExecutionSummary } from '../execution';
import { teamsApi, type TeamEntry } from '../execution/teams';
import type { RunStep, TeamStepReport } from '../execution/types';
import { formatTokenCount } from './attemptFormat';
import { classWords, roleWords } from './AgentWork';
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
  /** The hops the run made, once it has ended: each step shows the model and verdict of its role's last hop. */
  hops?: readonly RunStep[];
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

/** What a hop shows under its step: the model, the verdict, and the size of the prompts it read. */
function hopFacts(hop: RunStep | undefined): string[] {
  if (!hop) return [];
  const promptTokens = hop.prompts.reduce((sum, p) => sum + (p.tokens ?? 0), 0);
  return [
    classWords(hop.model, hop.class),
    hop.verdict ? `Verdict: ${hop.verdict}` : null,
    promptTokens > 0 ? `${formatTokenCount(promptTokens)} prompt tokens` : null,
  ].filter((x): x is string => !!x);
}

/** One node of the track: a bar in the state's colour over the step's words, so the state is also said. */
const TrackNode: Component<{ name: string; state?: keyof typeof LABEL | null; stopped?: boolean; lines: string[]; step?: string }> = (props) => (
  <li
    data-step={props.step}
    data-state={props.state ?? undefined}
    class="min-w-0 break-words text-xs"
    style={{ flex: '1 1 9rem', 'max-width': '16rem', color: 'var(--color-text-secondary)' }}
  >
    <span
      aria-hidden="true"
      class="mb-1.5 block h-1 rounded-full"
      style={{ 'background-color': props.state ? MARK_COLOR[props.state] : 'var(--color-border-light)' }}
    />
    <div class="flex flex-wrap items-baseline gap-x-1.5">
      <span class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>{props.name}</span>
      <Show when={props.state}>
        {(state) => <span style={{ color: 'var(--color-text-tertiary)' }}>{props.stopped ? 'Stopped here' : LABEL[state()]}</span>}
      </Show>
    </div>
    <For each={props.lines}>{(line) => <p class="mt-0.5">{line}</p>}</For>
  </li>
);
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
          <Show
            when={(props.hops ?? []).length > 0}
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
            <ol data-testid="run-hops" aria-label="The steps the run made" class="flex flex-wrap gap-x-3 gap-y-3">
              <For each={props.hops}>
                {(hop, i) => <TrackNode name={roleWords(hop.role, i() + 1)} lines={hopFacts(hop)} />}
              </For>
            </ol>
          </Show>
        }
      >
        {(steps) => (
          <ol data-testid="team-steps" aria-label="The team's steps" class="flex flex-wrap gap-x-3 gap-y-3">
            <For each={steps()}>
              {(step) => {
                const words = () => [step.role ? capitalised(step.role) : KIND_WORDS[step.kind], step.check].filter((w): w is string => !!w);
                const hop = () => (step.role ? [...(props.hops ?? [])].reverse().find((h) => h.role?.toLowerCase() === step.role!.toLowerCase()) : undefined);
                return (
                  <TrackNode
                    step={step.name}
                    name={step.name}
                    state={step.state}
                    stopped={step.state === 'running' && props.ended}
                    lines={[...(words().length > 0 ? [words().join(' · ')] : []), ...hopFacts(hop())]}
                  />
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
  hops?: readonly RunStep[];
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
      hops={props.hops}
    />
  );
};
