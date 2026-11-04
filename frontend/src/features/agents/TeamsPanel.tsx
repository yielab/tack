import { type Component, For, Show, createResource, createSignal } from 'solid-js';
import { api } from '../../shared/api';
import { HelpHint, LoadError, Skeleton } from '../../shared/ui';
import { HELP } from '../../shared/help/texts';
import { toast } from '../../shared/ui/toast';
import {
  SINGLE_AGENT_TEXT,
  agentCapabilityLines,
  agentName,
  agentProfilesApi,
  isTeamOfOne,
  pickHarness,
  profileAgentText,
  teamOf,
  teamsApi,
  type AgentProfileSummary,
  type TeamEntry,
} from '../../shared/execution';
import type { RunnerSummary } from '../../shared/execution/api';
import { harnessesOf } from './runnerObservations';
import { requestProfileEdit } from './profileEditRequest';
import TeamCard, { type TeamSheetContext } from './TeamCard';

/**
 * Teams: one sheet per team any agent reports (`GET /api/teams`), a local agent being a team of one, each
 * line drawn only from a field the agent reported; a field it left out is one disabled line, never hidden.
 * The two answers that depend on a project (how far a team goes in it, which roles send it work) are read
 * for the project chosen at the top. Under the teams, the profiles that run no agent of their own.
 */
const TeamsPanel: Component<{ runners?: readonly RunnerSummary[] }> = (props) => {
  const [teams, { refetch: refetchTeams }] = createResource(() => teamsApi.list());
  const [profiles, { refetch: refetchProfiles }] = createResource(() => agentProfilesApi.list());
  const [busy, setBusy] = createSignal<string | null>(null);
  const [projects] = createResource(() => api.projects.list().catch(() => []));
  const projectList = () => {
    const list = projects();
    return Array.isArray(list) ? list : [];
  };
  const [chosen, setChosen] = createSignal('');
  const projectId = () => chosen() || projectList()[0]?.id || '';
  const [autonomy] = createResource(projectId, (id) => (id ? api.autonomy.get(id).catch(() => null) : null));
  const [roles] = createResource(projectId, (id) => (id ? api.roles.list(id).catch(() => []) : []));
  /** What the chosen project says of a team; null while no project is chosen or its settings did not load. */
  const contextOf = (entry: TeamEntry): TeamSheetContext | null => {
    const project = projectList().find((p) => p.id === projectId());
    const settings = autonomy();
    if (!project || !settings) return null;
    return {
      project: project.name,
      ceiling: settings.ceiling ?? null,
      line: settings.teams?.find((t) => t.name === entry.name) ?? null,
      roles: (Array.isArray(roles()) ? roles()! : []).map((r) => ({ name: r.name, profileId: r.agent_profile_id ?? null })),
      defaultProfileId: project.default_profile_id ?? null,
    };
  };

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

  /** Profiles that name no team and no agent a sheet above already lists them under, grouped by the agent that
   *  runs them (none named: any agent). */
  const noTeam = () => {
    const sheets = new Set(entries().filter(isTeamOfOne).map((e) => e.name));
    const groups = new Map<string, AgentProfileSummary[]>();
    for (const p of rows()) {
      if (teamOf(p)) continue;
      const agent = (p.tool_policy as { agent?: unknown } | null)?.agent;
      if (typeof agent === 'string' && sheets.has(agent)) continue;
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
          A team is whoever does a task: several agents working it together, or one agent on its own, which counts
          as a team of one. Each sheet answers six questions from what the agent that runs the team reported; a
          line it did not report is greyed out.
        </p>
        <Show when={projectList().length > 0}>
          <label class="mt-2 flex flex-wrap items-center gap-2 text-small" style={{ color: 'var(--color-text-secondary)' }}>
            Read the sheets for the project
            <select
              aria-label="Project the sheets are read for"
              data-testid="teams-project"
              class="rounded-full border px-3 py-1 text-sm"
              style={{ 'background-color': 'var(--color-bg-base)', color: 'var(--color-text-primary)', 'border-color': 'var(--color-border-medium)' }}
              onChange={(e) => setChosen(e.currentTarget.value)}
            >
              <For each={projectList()}>{(p) => <option value={p.id} selected={p.id === projectId()}>{p.name}</option>}</For>
            </select>
          </label>
        </Show>
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
            return (
              <TeamCard
                entry={entry}
                proposes={proposes(entry)}
                busy={busy() === entry.name}
                anyBusy={busy() !== null}
                onMakeProfile={() => void makeProfile(entry)}
                profiles={rows()}
                context={contextOf(entry)}
              />
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
