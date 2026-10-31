import { type Accessor, type Component, For, Show, createResource } from 'solid-js';
import { api } from '../../../../shared/api';
import type { AutonomySettings } from '../../../../shared/api/autonomy';
import { fleetsApi, type AgentProfileSummary } from '../../../../shared/execution';
import { Button, HelpHint, LoadError } from '../../../../shared/ui';
import { toast } from '../../../../shared/ui/toast';
import { HELP } from '../../../../shared/help/texts';
import { BOOK } from '../../../../shared/help/routes';
import type { Project, Role } from '../../../../shared/types';
import { Heading, Note, SECTION, inputStyle, teamOf } from './parts';

interface Props {
  roles: Accessor<Role[]>;
  profiles: Accessor<AgentProfileSummary[]>;
  /** The ladder's autonomy settings: each team's state is the server's verdict. */
  settings: Accessor<AutonomySettings | null | undefined>;
  /** Set when the roles did not load: they are not "none yet". */
  rolesError: Accessor<unknown>;
  onRetryRoles: () => void;
  project: Accessor<Project | undefined>;
  /** Re-reads the roles after one is saved. */
  onRoleSaved: () => void;
  onDefaultProfile: (profileId: string | null) => void;
}

/** Rung 1: which profile each role works with, and the team and machine that profile names. */
const Teams: Component<Props> = (props) => {
  const [fleetList, { refetch: refetchFleets }] = createResource(() => fleetsApi.list().then((r) => r.data.data ?? []));
  const fleets = () => (fleetList.error ? [] : fleetList());
  /** What the server says about a team: lights-out, needs a person, or nothing while no computer reports it. */
  const stateOf = (team: string) => {
    const line = props.settings()?.teams?.find((t) => t.name === team);
    return !line || line.profile === 'unknown' ? null : line.caps_at ? 'needs a person' : 'lights-out';
  };
  const profile = (id: string | null | undefined) => props.profiles().find((p) => p.agent_profile_id === id);

  /** "saas-content · lights-out · this computer" for the chosen profile; empty without one. */
  const line = (id: string | null | undefined) => {
    const p = profile(id);
    if (!p) return '';
    const team = teamOf(p);
    const fleetId = (p as { fleet_id?: string | null }).fleet_id;
    const machine = fleetId ? `fleet ${(fleets() ?? []).find((f) => f.fleet_id === fleetId)?.name ?? fleetId}` : 'this computer';
    return [team ?? 'no team', team ? stateOf(team) : null, machine].filter(Boolean).join(' · ');
  };

  /** What the team's sandbox reads besides its own work, in words; null while nothing reports it.
   *  `grant` is set when it reads nothing else, and a team that runs tests needs its toolchain granted. */
  const readsOf = (id: string | null | undefined) => {
    const team = teamOf(profile(id));
    const paths = team ? props.settings()?.teams?.find((t) => t.name === team)?.read_only : null;
    if (!paths) return null;
    return paths.length
      ? { text: `Its sandbox also reads ${paths.join(', ')}, and cannot change them.`, grant: false }
      : { text: 'Its sandbox reads only its own work. A team that runs tests needs its toolchain granted.', grant: true };
  };

  const ReadsNote = (p: { id: string | null | undefined }) => (
    <Show when={readsOf(p.id)}>
      {(reads) => (
        <Note testid="team-reads">
          {reads().text}
          <Show when={reads().grant}>
            {' '}
            <a href={BOOK.toolchain} target="_blank" rel="noreferrer" class="underline">The Administrator Guide shows how.</a>
          </Show>
        </Note>
      )}
    </Show>
  );

  /** Where the team works and what it keeps, in words; nothing while the computer does not say. */
  const WorkNotes = (p: { id: string | null | undefined }) => {
    const team = () => {
      const name = teamOf(profile(p.id));
      return name ? props.settings()?.teams?.find((t) => t.name === name) : undefined;
    };
    const sets = () => team()?.records?.filter((set) => set.name) ?? [];
    return (
      <>
        <Show when={team()?.works_in === 'folder'}>
          <Note testid="team-works-in">Works in a folder, not in this repository.</Note>
        </Show>
        <Show when={sets().length > 0}>
          <Note testid="team-records">
            Keeps {sets().map((set) => `${set.name} · ${set.rows} ${set.rows === 1 ? 'row' : 'rows'}`).join(', ')}.
          </Note>
        </Show>
      </>
    );
  };

  const saveRole = async (role: Role, value: string) => {
    try {
      await api.roles.update(role.id, { agent_profile_id: value || null });
      props.onRoleSaved();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to save');
    }
  };

  const ProfileSelect = (p: { label: string; value: string | null | undefined; onChange: (v: string) => void }) => (
    <select
      aria-label={p.label}
      class="rounded-full border px-3 py-1 text-sm"
      style={inputStyle}
      onChange={(e) => p.onChange(e.currentTarget.value)}
    >
      <option value="" selected={!p.value}>No profile</option>
      <For each={props.profiles()}>
        {(pr) => <option value={pr.agent_profile_id} selected={pr.agent_profile_id === p.value}>{pr.name}</option>}
      </For>
    </select>
  );

  return (
    <div class={SECTION} id="automation-teams" data-testid="rung-teams">
      <div class="flex items-center gap-2">
        <Heading>Teams</Heading>
        <HelpHint label="Teams" help={HELP.ladderTeams} />
      </div>
      <Note>Each role works with a profile. The profile names the team and the machine.</Note>
      <Show when={props.rolesError()}>
        {(error) => <LoadError what="this project's roles" error={error()} onRetry={props.onRetryRoles} />}
      </Show>
      <Show when={fleetList.error}>
        {(error) => <LoadError what="the fleets" error={error()} onRetry={() => void refetchFleets()} />}
      </Show>
      <For each={props.roles()} fallback={<Show when={!props.rolesError()}><Note>This project has no roles yet. Add them under Roles.</Note></Show>}>
        {(role) => (
          <div class="flex flex-col gap-1" data-testid="team-row">
            <div class="flex flex-wrap items-center gap-3 text-sm" style={{ color: 'var(--color-text-primary)' }}>
              <span class="min-w-24 font-semibold">{role.name}</span>
              <ProfileSelect label={`Profile for ${role.name}`} value={role.agent_profile_id} onChange={(v) => void saveRole(role, v)} />
            </div>
            <Show when={line(role.agent_profile_id)}>{(text) => <Note>{text()}</Note>}</Show>
            <WorkNotes id={role.agent_profile_id} />
            <ReadsNote id={role.agent_profile_id} />
          </div>
        )}
      </For>
      <div class="flex flex-col gap-1" data-testid="team-row-default">
        <div class="flex flex-wrap items-center gap-3 text-sm" style={{ color: 'var(--color-text-primary)' }}>
          <span class="min-w-24 font-semibold">No role</span>
          <ProfileSelect label="Profile for a task with no role" value={props.project()?.default_profile_id} onChange={(v) => props.onDefaultProfile(v || null)} />
        </div>
        <Show when={line(props.project()?.default_profile_id)}>{(text) => <Note>{text()}</Note>}</Show>
        <WorkNotes id={props.project()?.default_profile_id} />
        <ReadsNote id={props.project()?.default_profile_id} />
      </div>
      <div class="flex flex-wrap items-start gap-3">
        <a href="/agents#profiles" class="py-1 text-sm underline" style={{ color: 'var(--color-text-secondary)' }}>Manage profiles →</a>
        <Button
          type="button"
          variant="secondary"
          size="sm"
          disabled
          reason="Tack cannot run this on the team's computer from the board yet."
        >
          Apply the repository's teams
        </Button>
        <a href={BOOK.teamsAdmin} target="_blank" rel="noreferrer" class="py-1 text-xs underline" style={{ color: 'var(--color-text-secondary)' }}>
          How to set up a team there
        </a>
      </div>
    </div>
  );
};

export default Teams;
