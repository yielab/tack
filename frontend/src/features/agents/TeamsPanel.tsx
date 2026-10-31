import { type Component, type JSX, For, Show, createResource, createSignal } from 'solid-js';
import { Badge, Button, HelpHint, LoadError, Skeleton } from '../../shared/ui';
import { HELP } from '../../shared/help/texts';
import { toast } from '../../shared/ui/toast';
import {
  SINGLE_AGENT_TEXT,
  agentCapabilityLines,
  agentName,
  agentProfilesApi,
  networkText,
  pickHarness,
  profileAgentText,
  queueText,
  stepText,
  teamOf,
  teamsApi,
  type AgentProfileSummary,
  type TeamEntry,
} from '../../shared/execution';
import type { RunnerSummary } from '../../shared/execution/api';
import { harnessesOf } from './runnerObservations';
import { requestProfileEdit } from './profileEditRequest';
import DisabledLine from './DisabledLine';

/** One labelled field of a team: drawn from `value` when the agent reported it, else one disabled line
 *  saying who did not. `value` is false-y only for a field the agent left out (an empty list is a report). */
function Field<T>(props: { label: string; id: string; agent: string; value: T | null | undefined | false | ''; children: (value: NonNullable<T>) => JSX.Element }): JSX.Element {
  return (
    <div data-testid={`team-${props.id}`} class="flex flex-col gap-0.5 sm:flex-row sm:gap-3">
      <dt class="w-28 flex-none text-micro font-bold uppercase tracking-widest" style={{ color: 'var(--color-text-tertiary)' }}>
        {props.label}
      </dt>
      <dd class="min-w-0 text-small" style={{ color: 'var(--color-text-primary)' }}>
        <Show when={props.value} fallback={<DisabledLine reported={false}>Not reported by {props.agent}</DisabledLine>}>
          {(v) => props.children(v() as NonNullable<T>)}
        </Show>
      </dd>
    </div>
  );
}

/**
 * Teams: one card per team any agent reports (`GET /api/teams`), each line drawn only from a field the
 * agent reported; a field it left out is one disabled line, never hidden. Under the teams, the
 * profiles that name no team, listed under the agent that runs them.
 */
const TeamsPanel: Component<{ runners?: readonly RunnerSummary[] }> = (props) => {
  const [teams, { refetch: refetchTeams }] = createResource(() => teamsApi.list());
  const [profiles, { refetch: refetchProfiles }] = createResource(() => agentProfilesApi.list());
  const [busy, setBusy] = createSignal<string | null>(null);

  const entries = (): TeamEntry[] => (teams.error ? [] : (teams() ?? []));
  const rows = (): AgentProfileSummary[] => (profiles.error ? [] : (profiles()?.data.data ?? []));
  const reports = () => (props.runners ?? []).flatMap((r) => harnessesOf(r));

  const proposes = (entry: TeamEntry) =>
    agentCapabilityLines(entry.harness ?? '', pickHarness(reports(), entry.harness ?? '')).find((l) => l.id === 'proposals')!;

  const makeProfile = async (entry: TeamEntry) => {
    if (!entry.harness) return;
    const base = rows().find((p) => p.kind === 'implementer') ?? rows().find((p) => p.name === 'Implementer');
    const taken = new Set(rows().map((p) => p.name));
    const name = taken.has(entry.name) ? `${entry.name} team` : entry.name;
    const policy = (base?.tool_policy ?? {}) as Record<string, unknown>;
    setBusy(entry.name);
    try {
      const created = await agentProfilesApi.create({
        name,
        instructions: base?.instructions ?? 'Complete the requested change. Summarize what changed and what you could not do.',
        tool_policy: { ...policy, agent: entry.harness, docket: { team: entry.name } },
        limits: base?.limits ?? {},
        summary: `Runs ${entry.name}`,
      });
      toast.success(`Made the profile "${created.name}" for ${entry.name}`);
      await Promise.all([refetchProfiles(), refetchTeams()]);
      requestProfileEdit(created.agent_profile_id);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to make the profile');
    } finally {
      setBusy(null);
    }
  };

  /** Profiles that name no team, grouped by the agent that runs them (none named: any agent). */
  const noTeam = () => {
    const groups = new Map<string, AgentProfileSummary[]>();
    for (const p of rows()) {
      if (teamOf(p)) continue;
      const agent = (p.tool_policy as { agent?: unknown } | null)?.agent;
      const key = typeof agent === 'string' && agent ? agent : 'any';
      groups.set(key, [...(groups.get(key) ?? []), p]);
    }
    return [...groups.entries()];
  };

  const oneAgentText = (key: string) => {
    if (key === 'any') return SINGLE_AGENT_TEXT;
    const model = pickHarness(reports(), key)?.default_model;
    return model ? `One agent · ${model}` : `One agent · model chosen for each task`;
  };

  return (
    <section class="flex flex-col gap-3 rounded-card bg-panel px-5 py-5" id="teams" data-testid="teams-panel">
      <div>
        <div class="flex items-center gap-2">
          <h2 class="text-lead font-bold leading-tight" style={{ color: 'var(--color-text-primary)' }}>
            Teams
          </h2>
          <HelpHint label="Teams" help={HELP.teamsPanel} />
        </div>
        <p class="mt-1 text-small" style={{ color: 'var(--color-text-secondary)' }}>
          A team is several agents working a task together. Each card shows what the agent that runs the team
          reported about it; a line it did not report is greyed out. An agent that works alone is not a team: its
          profiles are listed after the teams.
        </p>
      </div>

      <Show when={teams.loading || profiles.loading}>
        <Skeleton height="60px" />
      </Show>
      <Show when={teams.error}>
        {(error) => <LoadError what="the teams" error={error()} onRetry={() => void refetchTeams()} />}
      </Show>
      <Show when={profiles.error}>
        {(error) => <LoadError what="the agent profiles" error={error()} onRetry={() => void refetchProfiles()} />}
      </Show>

      <Show when={!teams.loading && !teams.error && entries().length === 0}>
        <p class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
          No agent reports a team yet.
        </p>
      </Show>

      <ul class="flex flex-col gap-2.5">
        <For each={entries()}>
          {(entry) => {
            const agent = () => (entry.harness ? agentName(entry.harness) : 'any computer');
            const report = () => entry.report ?? undefined;
            return (
              <li class="flex flex-col gap-2.5 rounded-xl bg-app px-4 py-3.5" data-testid={`team-card-${entry.name}`}>
                <div class="flex flex-wrap items-center gap-2">
                  <span class="text-lead font-bold" style={{ color: 'var(--color-text-primary)' }}>
                    {entry.name}
                  </span>
                  <Show
                    when={entry.reported}
                    fallback={<Badge tone="neutral">Not reported by any computer</Badge>}
                  >
                    <Badge tone="primary">Run by {agentName(entry.harness ?? '')}</Badge>
                  </Show>
                </div>

                <Show when={entry.reported}>
                  <dl class="flex flex-col gap-1.5">
                    <Field label="Shape" id="shape" agent={agent()} value={report()?.shape}>
                      {(shape) => shape}
                    </Field>
                    <Field label="Works in" id="works-in" agent={agent()} value={report()?.works_in}>
                      {(where) => (where === 'repository' ? 'Works in this repository' : 'Works in a folder')}
                    </Field>
                    <Field label="Members" id="members" agent={agent()} value={report()?.members}>
                      {(members) => (
                        <ul>
                          <For each={members} fallback={<DisabledLine>No members reported</DisabledLine>}>
                            {(m) => <li>{m.model ? `${m.role} · ${m.model}` : m.role}</li>}
                          </For>
                        </ul>
                      )}
                    </Field>
                    <Field label="Runs" id="decides" agent={agent()} value={report()?.decides}>
                      {(decides) => (
                        <>
                          <span>{decides === 'to_the_end' ? 'Runs to the end by itself' : 'Stops to ask a person'}</span>
                          <For each={report()?.violations ?? []}>
                            {(v) => (
                              <p class="text-caption" style={{ color: 'var(--color-text-secondary)' }}>
                                {v.requirement}. To fix: {v.fix}
                              </p>
                            )}
                          </For>
                        </>
                      )}
                    </Field>
                    <Field label="Network" id="network" agent={agent()} value={report()?.network}>
                      {(network) => networkText(network)}
                    </Field>
                    <Field label="Work now" id="queue" agent={agent()} value={report()?.queue}>
                      {(queue) => queueText(queue)}
                    </Field>
                    <Field label="Keeps" id="records" agent={agent()} value={report()?.records}>
                      {(sets) => (
                        <ul>
                          <For each={sets} fallback={<DisabledLine>No record sets reported</DisabledLine>}>
                            {(set) => (
                              <li>
                                {set.name} · {set.rows} {set.rows === 1 ? 'row' : 'rows'}
                                <Show when={set.description}>
                                  <span style={{ color: 'var(--color-text-secondary)' }}> · {set.description}</span>
                                </Show>
                              </li>
                            )}
                          </For>
                        </ul>
                      )}
                    </Field>
                    <Field label="Proposes" id="proposals" agent={agent()} value={proposes(entry).state !== 'not_reported' && proposes(entry)}>
                      {(line) => (
                        <span
                          aria-disabled={line.state === 'can' ? undefined : 'true'}
                          style={{ color: line.state === 'can' ? undefined : 'var(--color-text-tertiary)' }}
                        >
                          {line.state === 'can' ? 'Can propose items' : 'Cannot propose items'}
                          <Show when={line.reason}>
                            <span style={{ color: 'var(--color-text-tertiary)' }}> · {line.reason}</span>
                          </Show>
                        </span>
                      )}
                    </Field>
                    <Field label="Steps" id="steps" agent={agent()} value={report()?.steps}>
                      {(steps) => (
                        <ol class="list-decimal pl-5">
                          <For each={steps} fallback={<DisabledLine>No steps reported</DisabledLine>}>
                            {(step) => <li>{stepText(step)}</li>}
                          </For>
                        </ol>
                      )}
                    </Field>
                  </dl>
                </Show>

                <div class="flex flex-wrap items-center gap-2 text-small" style={{ color: 'var(--color-text-secondary)' }}>
                  <Show
                    when={entry.profiles.length > 0}
                    fallback={<span>No profile uses this team yet.</span>}
                  >
                    <span>Profiles that use it: {entry.profiles.map((p) => p.name).join(', ')}</span>
                  </Show>
                  <Show when={entry.reported && entry.harness && entry.profiles.length === 0}>
                    <Button
                      type="button"
                      size="sm"
                      variant="secondary"
                      loading={busy() === entry.name}
                      disabled={busy() !== null}
                      onClick={() => void makeProfile(entry)}
                    >
                      Make a profile for this team
                    </Button>
                  </Show>
                </div>
              </li>
            );
          }}
        </For>
      </ul>

      <Show when={noTeam().length > 0}>
        <div class="flex flex-col gap-2" data-testid="no-team-profiles">
          <h3 class="text-lead font-bold leading-tight" style={{ color: 'var(--color-text-primary)' }}>
            Profiles with no team
          </h3>
          <For each={noTeam()}>
            {([key, list]) => (
              <div class="flex flex-col gap-1 rounded-xl bg-app px-4 py-3" data-testid={`no-team-${key}`}>
                <span class="text-sm font-bold" style={{ color: 'var(--color-text-primary)' }}>
                  {key === 'any' ? profileAgentText({}) : agentName(key)}
                </span>
                <ul class="flex flex-col gap-0.5 text-small" style={{ color: 'var(--color-text-secondary)' }}>
                  <For each={list}>
                    {(p) => (
                      <li>
                        <span class="font-bold" style={{ color: 'var(--color-text-primary)' }}>{p.name}</span> · {oneAgentText(key)}
                        <Show when={p.summary}>
                          <span> · {p.summary}</span>
                        </Show>
                      </li>
                    )}
                  </For>
                </ul>
              </div>
            )}
          </For>
        </div>
      </Show>
    </section>
  );
};

export default TeamsPanel;
