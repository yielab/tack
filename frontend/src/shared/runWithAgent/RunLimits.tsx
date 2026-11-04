import { type Component, For, Show, createResource } from 'solid-js';
import { Fold } from '../ui';
import { agentProfilesApi, teamOf, type AgentProfileSummary } from '../execution';
import type { RunLimit } from '../execution/api';
import { teamsApi, type TeamEntry } from '../execution/teams';
import { type RunTeam, limitRows, limitsSummary } from './runLimits';

export interface RunLimitsProps {
  limit?: RunLimit | null;
  /** The newest attempt's `terminal_reason`, which carries the hops and the bound a step ran out of. */
  terminalReason?: unknown;
  /** The profile the run was made with and the agent it asked for; the team is the one the profile names. */
  profileId?: string | null;
  agentKind?: string | null;
}

/** The team the run's profile names, as the agent reports it. */
function runTeam(profiles: AgentProfileSummary[], teams: TeamEntry[], profileId: string, agentKind: string | null): RunTeam | null {
  const name = teamOf(profiles.find((p) => p.agent_profile_id === profileId));
  if (!name) return null;
  const entries = teams.filter((t) => t.name === name && t.report);
  const entry = entries.find((t) => agentKind && t.harness === agentKind) ?? entries.find((t) => !agentKind || !t.harness);
  return entry?.report ? { name, budget: entry.report.budget, paused: entry.report.paused } : null;
}

/**
 * The run's limits, one row per level and who applies it: the run's tokens, a step's own bounds
 * and the team's budget. Rows an agent did not report are not drawn, so a run on an agent with no
 * teams shows one.
 */
const RunLimits: Component<RunLimitsProps> = (props) => {
  const [team] = createResource(
    () => props.profileId ?? undefined,
    async (profileId): Promise<RunTeam | null> => {
      const [profiles, teams] = await Promise.all([
        agentProfilesApi.list().then((r) => r.data.data ?? []).catch(() => [] as AgentProfileSummary[]),
        teamsApi.list().catch(() => [] as TeamEntry[]),
      ]);
      return runTeam(profiles, teams, profileId, props.agentKind ?? null);
    },
  );
  const rows = () =>
    limitRows({
      limit: props.limit,
      terminalReason: props.terminalReason,
      team: team.state === 'ready' ? team() : null,
    });
  return (
    <Show when={rows().length > 0}>
      <Fold kind="run-limits" title="Limits" size="sub" summary={limitsSummary(rows())} testId="run-limits">
        <ul class="space-y-2">
          <For each={rows()}>
            {(row) => (
              <li data-testid={`limit-${row.id}`} class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                <p class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>{row.title}</p>
                <For each={row.lines}>{(line) => <p>{line}</p>}</For>
                <p data-testid={`limit-${row.id}-owner`} style={{ color: 'var(--color-text-tertiary)' }}>{row.owner}</p>
              </li>
            )}
          </For>
        </ul>
      </Fold>
    </Show>
  );
};

export default RunLimits;
