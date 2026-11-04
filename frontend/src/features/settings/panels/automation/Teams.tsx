import { type Accessor, type Component, For, Show, createResource } from 'solid-js';
import { api } from '../../../../shared/api';
import type { AutonomySettings } from '../../../../shared/api/autonomy';
import { SINGLE_AGENT_TEXT, fleetsApi, teamChoices, teamOf, teamsApi, type AgentProfileSummary } from '../../../../shared/execution';
import { Button, HelpHint, LoadError } from '../../../../shared/ui';
import { toast } from '../../../../shared/ui/toast';
import { HELP } from '../../../../shared/help/texts';
import { BOOK } from '../../../../shared/help/routes';
import type { Project, Role } from '../../../../shared/types';
import { ShapeSelect, createTeamApply } from '../../../../shared/teamApply/teamApply';
import { Block, Heading, Note, inputStyle } from './parts';

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

/** Which profile each role works with, and the team and machine that profile names. */
const Teams: Component<Props> = (props) => {
  const [fleetList, { refetch: refetchFleets }] = createResource(() => fleetsApi.list().then((r) => r.data.data ?? []));
  const fleets = () => (fleetList.error ? [] : fleetList());
  const [teamList, { refetch: refetchTeams }] = createResource(() => teamsApi.list());
  /** The profiles under the team each names: teams an agent reports first, then names only profiles use. */
  const groups = () => {
    const named = props.profiles().filter((p) => teamOf(p));
    if (named.length === 0) return null;
    const { reported, notReported } = teamChoices(teamList.error ? [] : (teamList() ?? []), named.map((p) => teamOf(p)));
    const under = (team: string) => named.filter((p) => teamOf(p) === team);
    return [
      ...reported.map((team) => ({ label: team, profiles: under(team) })),
      ...notReported.map((team) => ({ label: `${team} (not reported by any computer)`, profiles: under(team) })),
      { label: SINGLE_AGENT_TEXT, profiles: props.profiles().filter((p) => !teamOf(p)) },
    ].filter((g) => g.profiles.length > 0);
  };
  /** What the server says about a team: lights-out, needs a person, or nothing while no computer reports it. */
  const stateOf = (team: string) => {
    const line = props.settings()?.teams?.find((t) => t.name === team);
    return !line || line.profile === 'unknown' ? null : line.caps_at ? 'needs a person' : 'lights-out';
  };
  /** The teams this project's roles work with that no computer reports: the ones the repository's folder can make. */
  const missingTeams = () => {
    const ids = [...props.roles().map((r) => r.agent_profile_id), props.project()?.default_profile_id];
    const named = ids.map((id) => teamOf(props.profiles().find((p) => p.agent_profile_id === id)));
    return teamChoices(teamList.error ? [] : (teamList() ?? []), named).notReported.filter((team) => named.includes(team));
  };
  const teams = createTeamApply({
    projectId: () => props.project()?.id,
    active: () => missingTeams().length > 0,
    onApplied: () => void refetchTeams(),
  });
  const applyReason = () => teams.gate().reason ?? (missingTeams().length === 0 ? 'Every team the roles use is already reported by a computer.' : undefined);
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

  /** The team's sheet on the Agents page: where it works, what it keeps and what its sandbox reads are there. */
  const SheetLink = (p: { id: string | null | undefined }) => (
    <Show when={teamOf(profile(p.id))}>
      {(team) => (
        <a href={`/agents#team-${encodeURIComponent(team())}`} class="w-fit text-xs underline" style={{ color: 'var(--color-text-secondary)' }}>
          Read the sheet of {team()}
        </a>
      )}
    </Show>
  );

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
      <Show
        when={groups()}
        fallback={
          <For each={props.profiles()}>
            {(pr) => <option value={pr.agent_profile_id} selected={pr.agent_profile_id === p.value}>{pr.name}</option>}
          </For>
        }
      >
        {(list) => (
          <For each={list()}>
            {(group) => (
              <optgroup label={group.label}>
                <For each={group.profiles}>
                  {(pr) => <option value={pr.agent_profile_id} selected={pr.agent_profile_id === p.value}>{pr.name}</option>}
                </For>
              </optgroup>
            )}
          </For>
        )}
      </Show>
    </select>
  );

  return (
    <Block id="automation-teams" testid="rung-teams">
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
            <SheetLink id={role.agent_profile_id} />
          </div>
        )}
      </For>
      <div class="flex flex-col gap-1" data-testid="team-row-default">
        <div class="flex flex-wrap items-center gap-3 text-sm" style={{ color: 'var(--color-text-primary)' }}>
          <span class="min-w-24 font-semibold">No role</span>
          <ProfileSelect label="Profile for a task with no role" value={props.project()?.default_profile_id} onChange={(v) => props.onDefaultProfile(v || null)} />
        </div>
        <Show when={line(props.project()?.default_profile_id)}>{(text) => <Note>{text()}</Note>}</Show>
        <SheetLink id={props.project()?.default_profile_id} />
      </div>
      <div class="flex flex-wrap items-start gap-3">
        <a href="/agents#profiles" class="py-1 text-sm underline" style={{ color: 'var(--color-text-secondary)' }}>Manage profiles →</a>
        <Button
          type="button"
          variant="secondary"
          size="sm"
          loading={teams.making().length > 0}
          disabled={!teams.gate().enabled || missingTeams().length === 0 || teams.making().length > 0}
          reason={applyReason()}
          onClick={() => void teams.apply(missingTeams().map((name) => ({ name, shape: teams.shapeOf(name) })))}
        >
          Apply the repository's teams
        </Button>
        <a href={BOOK.teamsAdmin} target="_blank" rel="noreferrer" class="py-1 text-xs underline" style={{ color: 'var(--color-text-secondary)' }}>
          How to set up a team there
        </a>
      </div>
      <Show when={teams.gate().enabled && missingTeams().length > 0}>
        <ul data-testid="missing-teams" aria-label="Teams the repository can make" class="flex flex-col gap-1">
          <For each={missingTeams()}>
            {(name) => (
              <li class="flex flex-wrap items-center gap-3 text-sm" style={{ color: 'var(--color-text-primary)' }}>
                <span class="min-w-24 font-semibold">{name}</span>
                <ShapeSelect name={name} teams={teams} />
              </li>
            )}
          </For>
        </ul>
      </Show>
      <teams.Host />
    </Block>
  );
};

export default Teams;
