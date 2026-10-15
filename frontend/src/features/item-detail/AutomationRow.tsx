import { type Component, Show, createResource } from 'solid-js';
import { api } from '../../shared/api';
import { request } from '../../shared/api/client';
import { agentProfilesApi } from '../../shared/execution';
import { HelpHint } from '../../shared/ui';
import { HELP } from '../../shared/help/texts';
import type { Item, Role } from '../../shared/types';

/** The sentence the Automation row shows, from the project's settings and the task's roles. */
export function automationSentence(input: {
  startsAt: string | null;
  starts: boolean;
  team: string | null;
  merge: string | null;
  roleWithoutProfile: string | null;
  hasProfile: boolean;
}): string {
  if (!input.hasProfile && input.roleWithoutProfile) return `By hand: the role ${input.roleWithoutProfile} has no profile`;
  if (!input.starts) return "By hand: this project doesn't start tasks by itself";
  return [
    `Starts by itself when it reaches ${input.startsAt}`,
    input.team ? `Team ${input.team}` : null,
    input.merge ? `Merge ${input.merge}` : null,
  ].filter(Boolean).join(' · ');
}

/** The drawer's Automation row: what happens to this task without you. */
const AutomationRow: Component<{ item: Item }> = (props) => {
  const [data] = createResource(
    () => props.item.id,
    async (id) => {
      const [project, detail, profiles, autonomy] = await Promise.all([
        api.projects.get(props.item.project_id),
        request<{ roles?: Role[] }>(`/items/${id}`),
        agentProfilesApi.list().then((r) => r.data.data ?? []),
        api.autonomy.get(props.item.project_id).catch(() => null),
      ]);
      return { project, roles: detail.roles ?? [], profiles, autonomy };
    },
  );
  const sentence = () => {
    const d = data();
    if (!d?.project?.id) return null;
    const withProfile = d.roles.find((r) => r.agent_profile_id);
    const profileId = withProfile?.agent_profile_id ?? d.project.default_profile_id;
    const profile = d.profiles.find((p) => p.agent_profile_id === profileId);
    const team = (profile?.tool_policy as { docket?: { team?: string } } | undefined)?.docket?.team ?? null;
    return automationSentence({
      startsAt: d.project.auto_start_status ?? 'its first status',
      starts: !!d.project.auto_start_enabled,
      team,
      merge: d.autonomy?.level ?? null,
      roleWithoutProfile: d.roles.find((r) => !r.agent_profile_id)?.name ?? null,
      hasProfile: !!profileId,
    });
  };
  return (
    <Show when={sentence()}>
      {(text) => (
        <div data-testid="automation-row" class="flex items-center gap-2 px-1 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          <span class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>Automation</span>
          <span>{text()}</span>
          <HelpHint label="Automation" help={HELP.automationRow} />
        </div>
      )}
    </Show>
  );
};

export default AutomationRow;
