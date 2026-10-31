import { type Component, Show, createResource } from 'solid-js';
import { api } from '../../shared/api';
import { request } from '../../shared/api/client';
import { agentProfilesApi } from '../../shared/execution';
import ProfilePicker from '../../shared/runWithAgent/ProfilePicker';
import { toast } from '../../shared/ui/toast';
import { HelpHint, LoadError } from '../../shared/ui';
import { HELP } from '../../shared/help/texts';
import type { Item, Role } from '../../shared/types';
import type { components } from '../../shared/api/schema.gen';

type RunSettings = components['schemas']['RunSettings'];

/** The sentence the Automation row shows, from the project's settings and the task's roles. */
export function automationSentence(input: {
  startsAt: string | null;
  starts: boolean;
  team: string | null;
  merge: string | null;
  roleWithoutProfile: string | null;
  hasProfile: boolean;
  /** Who does it and where that choice came from: "Content writer (chosen on this task)". */
  who?: string | null;
}): string {
  if (!input.hasProfile && input.roleWithoutProfile) return `By hand: the role ${input.roleWithoutProfile} has no profile`;
  if (!input.starts) return "By hand: this project doesn't start tasks by itself";
  return [
    `Starts by itself when it reaches ${input.startsAt}`,
    input.who ? `Goes to ${input.who}` : null,
    input.team ? `Team ${input.team}` : null,
    input.merge ? `Merge ${input.merge}` : null,
  ].filter(Boolean).join(' · ');
}

/** Where the profile that does a task came from, in words. */
export function choiceSource(input: { onTask: boolean; role: string | null }): string | null {
  if (input.onTask) return 'chosen on this task';
  return input.role ? `from the role ${input.role}` : null;
}

/** The task's run settings with the chosen profile set, or cleared; null when nothing else is left. */
export function withProfile(settings: RunSettings | null | undefined, profileId: string | null): RunSettings | null {
  const { agent_profile_id: _dropped, ...rest } = settings ?? {};
  const next: RunSettings = profileId ? { ...rest, agent_profile_id: profileId } : rest;
  return Object.keys(next).length > 0 ? next : null;
}

/** The drawer's Automation row: what happens to this task without you. */
const AutomationRow: Component<{ item: Item }> = (props) => {
  const [resource, { refetch }] = createResource(
    () => props.item.id,
    async (id) => {
      const [project, detail, profiles, autonomy] = await Promise.all([
        api.projects.get(props.item.project_id),
        request<{ roles?: Role[]; item?: { run_settings?: RunSettings | null } }>(`/items/${id}`),
        agentProfilesApi.list().then((r) => r.data.data ?? []),
        api.autonomy.get(props.item.project_id),
      ]);
      return { project, roles: detail.roles ?? [], runSettings: detail.item?.run_settings ?? null, profiles, autonomy };
    },
  );
  // The choice made on the task comes first, then the role's, then the project's.
  const chosen = () => {
    const d = resource.error ? undefined : resource();
    if (!d?.project?.id) return null;
    const onTask = d.runSettings?.agent_profile_id || null;
    const withRoleProfile = d.roles.find((r) => r.agent_profile_id);
    const profileId = onTask ?? withRoleProfile?.agent_profile_id ?? d.project.default_profile_id ?? null;
    const profile = d.profiles.find((p) => p.agent_profile_id === profileId);
    const from = onTask ? choiceSource({ onTask: true, role: null }) : choiceSource({ onTask: false, role: withRoleProfile?.name ?? null });
    return { d, profileId, profile, onTask, from };
  };
  const sentence = () => {
    const c = chosen();
    if (!c) return null;
    const { d, profileId, profile, from } = c;
    const team = (profile?.tool_policy as { docket?: { team?: string } } | undefined)?.docket?.team ?? null;
    return automationSentence({
      startsAt: d.project.auto_start_status ?? 'its first status',
      starts: !!d.project.auto_start_enabled,
      team,
      merge: d.autonomy?.level ?? null,
      roleWithoutProfile: d.roles.find((r) => !r.agent_profile_id)?.name ?? null,
      hasProfile: !!profileId,
      who: profile && from ? `${profile.name} (${from})` : null,
    });
  };
  const choose = async (profileId: string | null) => {
    const c = chosen();
    if (!c) return;
    try {
      await api.items.update(props.item.id, { run_settings: withProfile(c.d.runSettings, profileId) });
      await refetch();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Could not save who does this task.');
    }
  };
  return (
    <>
      <Show when={resource.error}>
        {(error) => <LoadError what="what happens to this task without you" error={error()} onRetry={() => void refetch()} />}
      </Show>
      <Show when={sentence()}>
        {(text) => (
          <div data-testid="automation-row" class="flex flex-col gap-1.5 px-1 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            <div class="flex items-center gap-2">
              <span class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>Automation</span>
              <span>{text()}</span>
              <HelpHint label="Automation" help={HELP.automationRow} />
            </div>
            <Show when={(chosen()?.d.profiles.length ?? 0) > 0}>
              <details data-testid="automation-who-does-it">
                <summary class="cursor-pointer text-small font-semibold">Who does it</summary>
                <div class="mt-2">
                  <ProfilePicker
                    label="Who does it"
                    value={chosen()?.onTask ?? null}
                    profiles={chosen()?.d.profiles ?? []}
                    onChange={(p) => void choose(p ? p.agent_profile_id : null)}
                  />
                </div>
              </details>
            </Show>
          </div>
        )}
      </Show>
    </>
  );
};

export default AutomationRow;
