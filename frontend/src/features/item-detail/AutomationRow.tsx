import { type Component, Show, createResource } from 'solid-js';
import { api } from '../../shared/api';
import { request } from '../../shared/api/client';
import { agentProfilesApi, runnersApi, teamOf } from '../../shared/execution';
import { noPlaceToRun } from '../../shared/runWithAgent/shared';
import ProfilePicker from '../../shared/runWithAgent/ProfilePicker';
import { useAutoStart } from '../../shared/state/autoStart';
import { toast } from '../../shared/ui/toast';
import { Fold, HelpHint, LoadError } from '../../shared/ui';
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
  /** Why the board cannot pick a computer for it, when it cannot (`noPlaceToRun`). */
  noPlace?: string | null;
  /** Why auto-start has not started it yet, when it would have (`useAutoStart`). */
  waiting?: string | null;
}): string {
  if (!input.hasProfile && input.roleWithoutProfile) return `By hand: the role ${input.roleWithoutProfile} has no profile`;
  if (!input.starts) return "By hand: this project doesn't start tasks by itself";
  return [
    input.noPlace
      ? `By hand: it would start when it reaches ${input.startsAt}, but ${input.noPlace}`
      : `Starts by itself when it reaches ${input.startsAt}`,
    input.waiting && !input.noPlace ? `Waiting to start: ${input.waiting}` : null,
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

/** What the Automation row and the idle Now panel both read for a task: the project, its roles, the
 *  profiles, the merge level and the computers, loaded once and shared by whoever shows them. */
export function useAutomation(item: () => Item) {
  const [resource, { refetch }] = createResource(
    () => item().id,
    async (id) => {
      const [project, detail, profiles, autonomy, runners] = await Promise.all([
        api.projects.get(item().project_id),
        request<{ roles?: Role[]; item?: { run_settings?: RunSettings | null } }>(`/items/${id}`),
        agentProfilesApi.list().then((r) => r.data.data ?? []),
        api.autonomy.get(item().project_id),
        // Where it runs is a hint on the sentence, never a reason to hide it.
        runnersApi.list().then((r) => (Array.isArray(r.data.data) ? r.data.data : null)).catch(() => null),
      ]);
      return { project, roles: detail.roles ?? [], runSettings: detail.item?.run_settings ?? null, profiles, autonomy, runners };
    },
  );
  const autoStart = useAutoStart(() => item().project_id);
  /** Why auto-start has not started the task yet, as a sentence; null when nothing holds it back. */
  const waiting = () => autoStart.waiting(item().id)?.text ?? null;
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
  /** `whoShownAbove`: the idle Now panel already says who would do it ("Would go to …"), so the
   *  sentence leaves out who and the team and says each once. */
  const sentence = (whoShownAbove = false) => {
    const c = chosen();
    if (!c) return null;
    const { d, profileId, profile, from } = c;
    return automationSentence({
      startsAt: d.project.auto_start_status ?? 'its first status',
      starts: !!d.project.auto_start_enabled,
      team: whoShownAbove ? null : teamOf(profile),
      merge: d.autonomy?.level ?? null,
      roleWithoutProfile: d.roles.find((r) => !r.agent_profile_id)?.name ?? null,
      hasProfile: !!profileId,
      who: !whoShownAbove && profile && from ? `${profile.name} (${from})` : null,
      noPlace: profile && d.runners ? noPlaceToRun(d.runners, profile.fleet_id) : null,
      waiting: whoShownAbove ? null : waiting(),
    });
  };
  return { resource, refetch, chosen, sentence, waiting, item };
}

export type Automation = ReturnType<typeof useAutomation>;

/** Who would do a task that has not run, as the Now panel says it: the profile and its team, or why nobody is chosen. */
export function whoWouldDoIt(automation: Automation): string | null {
  const c = automation.chosen();
  if (!c) return null;
  if (!c.profile) return 'No profile is chosen yet: you pick the agent when you run it';
  const team = teamOf(c.profile);
  return team ? `${c.profile.name} · team ${team}` : c.profile.name;
}

/** Under the Run button of a task with no run: who would do it, and why auto-start has not started it yet. */
export const WhoWouldDoIt: Component<{ automation: Automation }> = (props) => (
  <>
    <Show when={whoWouldDoIt(props.automation)}>
      {(text) => (
        <p data-testid="now-who" class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
          Would go to {text()}
        </p>
      )}
    </Show>
    <Show when={props.automation.waiting()}>
      {(text) => (
        <p data-testid="now-waiting" class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
          Waiting to start: {text()}
        </p>
      )}
    </Show>
  </>
);

/** What happens to this task without you: one muted line with its help, and the choice of who does it. It sits at the foot of the Now panel, under a hairline. */
const AutomationRow: Component<{ item: Item; automation?: Automation; whoShownAbove?: () => boolean }> = (props) => {
  const automation = props.automation ?? useAutomation(() => props.item);
  const { resource, refetch, chosen, sentence } = automation;
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
  // What a person has to do before this task can go: pick a profile for a role that has none.
  const needsYou = () => {
    const c = chosen();
    const role = c && !c.profileId ? c.d.roles.find((r) => !r.agent_profile_id)?.name : null;
    return role ? `needs you: the role ${role} has no profile` : null;
  };
  return (
    <>
      <Show when={resource.error}>
        {(error) => <LoadError what="what happens to this task without you" error={error()} onRetry={() => void refetch()} />}
      </Show>
      <Show when={sentence(props.whoShownAbove?.() ?? false)}>
        {(text) => (
          <Fold
            kind="automation"
            testId="automation-row"
            size="sub"
            title="Automation"
            summary={text()}
            needs={needsYou()}
            class="border-t pt-2.5"
            style={{ color: 'var(--color-text-secondary)', 'border-color': 'var(--color-border-light)' }}
          >
            <div class="flex min-w-0 flex-col gap-1.5 text-xs">
              <div class="flex min-w-0 items-center gap-1.5">
                <span class="min-w-0" style={{ 'overflow-wrap': 'anywhere' }}>{text()}</span>
                <HelpHint label="Automation" help={HELP.automationRow} />
              </div>
              <Show when={(chosen()?.d.profiles.length ?? 0) > 0}>
                <div data-testid="automation-who-does-it">
                  <ProfilePicker
                    label="Who does it"
                    value={chosen()?.onTask ?? null}
                    profiles={chosen()?.d.profiles ?? []}
                    onChange={(p) => void choose(p ? p.agent_profile_id : null)}
                  />
                </div>
              </Show>
            </div>
          </Fold>
        )}
      </Show>
    </>
  );
};

export default AutomationRow;
