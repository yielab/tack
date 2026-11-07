import { type Component, For, Show, createEffect, createResource, createSignal, createUniqueId } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import { Badge, Button, EmptyState, Field, FieldShell, HelpHint, LoadError, Skeleton } from '../../../shared/ui';
import { EGRESS_MODES, HELP, defaultEgressMode } from '../../../shared/help/texts';
import { toast } from '../../../shared/ui/toast';
import { IconSettings } from '../../../shared/ui/icons';
import {
  PER_TASK_AGENT_TEXT,
  SINGLE_AGENT_TEXT,
  agentProfilesApi,
  profileAgentText,
  methodsByType,
  podsOnly,
  teamChoices,
  teamOf,
  teamsApi,
  type AgentProfileSummary,
  type EgressMode,
  type ProfileHistory,
  type ProfileKind,
} from '../../../shared/execution';
import { profileEditRequest, requestProfileEdit } from '../profileEditRequest';
import { KNOWN_TOOLS } from '../../../shared/runWithAgent/models';

/**
 * Create/edit/delete UI for `agent_profiles`. The tool policy is built from a
 * per-harness checklist (`KNOWN_TOOLS`) plus a free-text line, and stored as
 * `{"tools": {"<harness>": [names] | ["*"]}}`; keys other than `tools` in an
 * existing policy are kept untouched on edit, except `network`: the "needs the network" box sets
 * `network: true`, and unticked it leaves the key out (absent is off). The limits form edits `limits.budgets.tokens`
 * and `limits.timeout_seconds`; any other key in an existing `limits` is kept. The team select sets
 * `tool_policy.docket.team` (the other `docket` keys stay); its options are the teams an agent reports
 * (`GET /api/teams`), then the names only profiles use. "Delivers" is one line the profile carries; what the profile can do is
 * derived by the server from its tools and only shown.
 */
const HARNESSES = Object.keys(KNOWN_TOOLS);

/** What a profile can do, in the order the select lists them. A run that changes the repository needs a team that delivers one. */
const PROFILE_KINDS: ReadonlyArray<{ value: ProfileKind; label: string; says: string }> = [
  { value: 'custom', label: 'Custom', says: 'Changes the repository, as its tools allow' },
  { value: 'implementer', label: 'Implementer', says: 'Changes the repository' },
  { value: 'reviewer', label: 'Reviewer', says: 'Reads the change and reports; changes nothing' },
  { value: 'researcher', label: 'Researcher', says: 'Investigates and reports; may propose work' },
  { value: 'planner', label: 'Planner', says: 'Plans: proposes work only' },
];

const FIELD_STYLE = {
  'background-color': 'var(--color-bg-base)',
  color: 'var(--color-text-primary)',
  'border-color': 'var(--color-border-medium)',
} as const;

interface ToolState { all: boolean; checked: string[]; extra: string }
type ToolForm = Record<string, ToolState>;

const blankTools = (): ToolForm =>
  Object.fromEntries(HARNESSES.map((h) => [h, { all: true, checked: [], extra: '' }]));

const toolsFromPolicy = (policy: unknown): ToolForm => {
  const form = blankTools();
  const tools = (policy as { tools?: Record<string, unknown> } | null)?.tools;
  for (const h of HARNESSES) {
    const list = tools?.[h];
    if (!Array.isArray(list) || list.includes('*')) continue;
    const names = list.filter((n): n is string => typeof n === 'string');
    form[h] = {
      all: false,
      checked: names.filter((n) => KNOWN_TOOLS[h].includes(n)),
      extra: names.filter((n) => !KNOWN_TOOLS[h].includes(n)).join(', '),
    };
  }
  return form;
};

const namesFor = (h: string, st: ToolState): string[] =>
  st.all
    ? ['*']
    : [
        ...KNOWN_TOOLS[h].filter((n) => st.checked.includes(n)),
        ...st.extra.split(/[\s,]+/).filter((n) => n && !st.checked.includes(n)),
      ];

/** A limit field as a number: `null` for empty, `undefined` for text that is no positive whole number. */
const limitNumber = (text: string): number | null | undefined => {
  if (text.trim() === '') return null;
  const n = Number(text);
  return Number.isInteger(n) && n > 0 ? n : undefined;
};

const limitsFrom = (limits: unknown): { tokens: string; seconds: string } => {
  const l = (limits ?? {}) as { budgets?: { tokens?: unknown }; timeout_seconds?: unknown };
  return {
    tokens: typeof l.budgets?.tokens === 'number' ? String(l.budgets.tokens) : '',
    seconds: typeof l.timeout_seconds === 'number' ? String(l.timeout_seconds) : '',
  };
};

/** The three derived capabilities as chips: always shown, dimmed when the profile cannot. */
const CapabilityChips: Component<{ profile: AgentProfileSummary }> = (props) => {
  const chip = (can: boolean | undefined, yes: string, no: string, id: string) => (
    <span data-testid={`cap-${id}`} data-can={can ? 'yes' : 'no'}>
      <Badge tone={can ? 'primary' : 'neutral'}>{can ? yes : no}</Badge>
    </span>
  );
  return (
    <span class="flex flex-wrap gap-1">
      {chip(props.profile.changes_files, 'Changes files', 'Changes no files', 'files')}
      {chip(props.profile.runs_commands, 'Runs commands', 'Runs no commands', 'commands')}
      {chip(props.profile.uses_network, 'Uses the network', 'No network', 'network')}
    </span>
  );
};

const spaced = (n: number) => String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ' ');

/** What a profile's last successful runs used, and the limit proposed from them. */
export const HistoryLine: Component<{ history: ProfileHistory; onApply: (tokens: number) => void; busy?: boolean }> = (props) => (
  <Show
    when={props.history.proposal !== null && props.history.min !== null && props.history.max !== null}
    fallback={
      <p data-testid="profile-history" class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
        {props.history.n} of {props.history.runs_needed} successful runs measured before a limit is proposed
      </p>
    }
  >
    <p data-testid="profile-history" class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
      The last {props.history.n} runs used {spaced(props.history.min!)} to {spaced(props.history.max!)} tokens. Proposed limit:{' '}
      <strong>{spaced(props.history.proposal!)}</strong>{' '}
      <Button type="button" size="sm" variant="secondary" disabled={props.busy} onClick={() => props.onApply(props.history.proposal!)}>
        Apply
      </Button>
    </p>
  </Show>
);

const AgentProfilesPanel: Component = () => {
  const [profiles, { refetch }] = createResource(() => agentProfilesApi.list());

  const [showForm, setShowForm] = createSignal(false);
  const [editing, setEditing] = createSignal<AgentProfileSummary | null>(null);
  const [name, setName] = createSignal('');
  const [summary, setSummary] = createSignal('');
  const [delivers, setDelivers] = createSignal('');
  const [team, setTeam] = createSignal('');
  const [kind, setKind] = createSignal<ProfileKind>('custom');
  /** What the person chose for web access; `null` follows what the kind starts with. */
  const [chosenEgress, setChosenEgress] = createSignal<EgressMode | null>(null);
  const egressMode = (): EgressMode => chosenEgress() ?? defaultEgressMode(kind());
  const teamId = createUniqueId();
  const kindId = createUniqueId();
  const egressId = createUniqueId();
  const [instructions, setInstructions] = createSignal('');
  const instructionsId = createUniqueId();
  const [tools, setTools] = createStore<ToolForm>(blankTools());
  const [network, setNetwork] = createSignal(false);
  const [tokenLimit, setTokenLimit] = createSignal('');
  const [timeLimit, setTimeLimit] = createSignal('');
  const [saving, setSaving] = createSignal(false);
  /** The method this profile gives to each kind of card (`docket.pipeline_by_type`), as rows being edited. */
  const [methodRows, setMethodRows] = createSignal<Array<{ type: string; method: string }>>([]);
  /** What the last save was refused for, and what it was saved with a second look at. */
  const [refusal, setRefusal] = createSignal<string | null>(null);
  const [warnings, setWarnings] = createSignal<string[]>([]);

  const rows = (): AgentProfileSummary[] => profiles()?.data.data ?? [];

  const [teamList, { refetch: refetchTeams }] = createResource(() => teamsApi.list());
  /** The teams an agent reports, then the names only profiles use (and the one being edited). */
  const choices = () =>
    teamChoices(teamList.error ? [] : (teamList() ?? []), [...rows().map((p) => teamOf(p)), team()]);
  const reportedTeam = (name: string | null) => !!name && choices().reported.includes(name);
  /** The methods the chosen team reports it has, once a computer reports them; absent otherwise. */
  const catalogue = () =>
    (teamList.error ? [] : (teamList() ?? [])).find((e) => e.reported && e.name === team() && podsOnly([e]).length === 1)?.report?.methods?.map((m) => m.name) ?? null;

  // The history of the profile being edited; a new profile has none.
  const [historyResource, { refetch: refetchHistory }] = createResource(
    () => (showForm() ? editing()?.agent_profile_id : undefined),
    (id) => agentProfilesApi.history(id),
  );
  const history = () => (historyResource.error ? undefined : historyResource());

  // Writes the proposed limit to the profile now, through the same update the form's Save uses.
  const applyProposal = async (tokens: number) => {
    const existing = editing();
    if (!existing) return;
    const old = (existing.limits ?? {}) as { budgets?: Record<string, unknown> } & Record<string, unknown>;
    const limits = { ...old, budgets: { ...old.budgets, tokens } };
    setSaving(true);
    try {
      await agentProfilesApi.update(existing.agent_profile_id, { limits });
      setEditing({ ...existing, limits });
      setTokenLimit(String(tokens));
      toast.success(`Token limit of "${existing.name}" set to ${spaced(tokens)}`);
      void refetch();
      void refetchHistory();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to set the token limit');
    } finally {
      setSaving(false);
    }
  };

  const openForm = (profile: AgentProfileSummary | null) => {
    setEditing(profile);
    setName(profile?.name ?? '');
    setSummary(profile?.summary ?? '');
    setDelivers(profile?.delivers ?? '');
    setTeam(teamOf(profile ?? undefined) ?? '');
    setKind(profile?.kind ?? 'custom');
    setChosenEgress(profile?.egress_mode ?? null);
    setMethodRows(methodsByType(profile?.tool_policy));
    setRefusal(null);
    setWarnings([]);
    setInstructions(profile?.instructions ?? '');
    setTools(toolsFromPolicy(profile?.tool_policy));
    setNetwork((profile?.tool_policy as { network?: unknown } | null)?.network === true);
    const { tokens, seconds } = limitsFrom(profile?.limits);
    setTokenLimit(tokens);
    setTimeLimit(seconds);
    setShowForm(true);
  };

  // A team's "Make a profile for this team" asks for the new profile to open here.
  createEffect(() => {
    const id = profileEditRequest();
    if (!id) return;
    requestProfileEdit(null);
    void (async () => {
      const fresh = await agentProfilesApi.list().catch(() => undefined);
      void refetch();
      const wanted = fresh?.data.data.find((p) => p.agent_profile_id === id);
      if (!wanted) return;
      openForm(wanted);
      document.getElementById('profiles')?.scrollIntoView?.({ block: 'start' });
    })();
  });

  const limitsInvalid = () => limitNumber(tokenLimit()) === undefined || limitNumber(timeLimit()) === undefined;

  const submit = async (e: Event) => {
    e.preventDefault();
    if (!name().trim() || !instructions().trim() || limitsInvalid()) return;
    const existing = editing();
    const base = (existing?.tool_policy ?? {}) as Record<string, unknown>;
    const { network: _network, docket, ...rest } = base;
    const { team: _team, pipeline_by_type: _byType, ...docketRest } = (docket ?? {}) as Record<string, unknown>;
    const byType = Object.fromEntries(methodRows().filter((r) => r.type.trim() && r.method.trim()).map((r) => [r.type.trim(), r.method.trim()]));
    const nextDocket = {
      ...docketRest,
      ...(team() ? { team: team() } : {}),
      ...(Object.keys(byType).length > 0 ? { pipeline_by_type: byType } : {}),
    };
    const tool_policy = {
      ...rest,
      ...(Object.keys(nextDocket).length > 0 ? { docket: nextDocket } : {}),
      ...(network() ? { network: true } : {}),
      tools: Object.fromEntries(HARNESSES.map((h) => [h, namesFor(h, tools[h])])),
    };
    // The edited keys replace the profile's; every other key of its limits stays.
    const old = (existing?.limits ?? {}) as { budgets?: Record<string, unknown> } & Record<string, unknown>;
    const limits: Record<string, unknown> = { ...old };
    const budgets: Record<string, unknown> = { ...old.budgets };
    const tokens = limitNumber(tokenLimit());
    const seconds = limitNumber(timeLimit());
    delete budgets.tokens;
    if (tokens) budgets.tokens = tokens;
    delete limits.budgets;
    if (Object.keys(budgets).length > 0) limits.budgets = budgets;
    delete limits.timeout_seconds;
    if (seconds) limits.timeout_seconds = seconds;
    const fields = {
      name: name().trim(),
      instructions: instructions().trim(),
      tool_policy,
      ...(existing || Object.keys(limits).length > 0 ? { limits } : {}),
      ...(summary().trim() || existing ? { summary: summary().trim() } : {}),
      ...(delivers().trim() ? { delivers: delivers().trim() } : existing?.delivers ? { delivers: null } : {}),
      // A built-in profile keeps its kind, so it is never sent.
      ...(!existing?.builtin && kind() !== (existing?.kind ?? 'custom') ? { kind: kind() } : {}),
      // Left out, a new profile starts with its kind's mode and an edited one keeps its own.
      ...(chosenEgress() && chosenEgress() !== existing?.egress_mode ? { egress_mode: chosenEgress()! } : {}),
    };
    setSaving(true);
    setRefusal(null);
    setWarnings([]);
    try {
      let saidSo: string[] = [];
      let made: string | null = null;
      if (existing) {
        saidSo = (await agentProfilesApi.update(existing.agent_profile_id, fields))?.warnings ?? [];
        toast.success(`Saved agent profile "${fields.name}"`);
      } else {
        const created = await agentProfilesApi.create(fields);
        saidSo = created.warnings ?? [];
        made = created.agent_profile_id;
        toast.success(`Created agent profile "${created.name}"`);
      }
      void refetch();
      void refetchTeams();
      // A save that deserves a second look stays open on it.
      if (saidSo.length > 0) {
        // A profile just made is open for editing, so a second save updates it and makes no copy.
        if (made) {
          const fresh = await agentProfilesApi.list().catch(() => undefined);
          const wanted = fresh?.data.data.find((p) => p.agent_profile_id === made);
          if (wanted) openForm(wanted);
        }
        setWarnings(saidSo);
      } else setShowForm(false);
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Failed to save agent profile';
      setRefusal(message);
      toast.error(message);
    } finally {
      setSaving(false);
    }
  };

  const remove = async (profile: AgentProfileSummary) => {
    try {
      await agentProfilesApi.remove(profile.agent_profile_id);
      toast.success(`Deleted agent profile "${profile.name}"`);
      void refetch();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to delete agent profile');
    }
  };

  return (
    <div id="profiles" class="space-y-4">
      <div class="space-y-1" data-testid="builtin-explained">
        <p class="text-small" style={{ color: 'var(--color-text-secondary)' }}>
          <HelpHint label="Built-in profiles" help={HELP.builtInProfiles} />{' '}
          A profile says what an agent may do and how it is told to work. The built-in profiles (Implementer,
          Reviewer, Researcher and Planner) come with Tack: you can edit them, but not delete them. A profile
          that names no agent runs with whichever agent you pick for the task.
        </p>
        <p class="text-small" style={{ color: 'var(--color-text-secondary)' }}>{PER_TASK_AGENT_TEXT}</p>
      </div>
      <Show when={profiles.loading}>
        <Skeleton height="60px" />
      </Show>

      <Show when={!profiles.loading && profiles.error === undefined}>
        <Show
          when={rows().length > 0}
          fallback={
            <div class="rounded-card border-2 border-dashed" style={{ 'border-color': 'var(--color-border-light)' }}>
              <EmptyState
                icon={<IconSettings size={26} />}
                title="No agent profiles yet"
                description="An agent profile bundles the instructions and tool/limits policy an execution request snapshots at creation."
              />
            </div>
          }
        >
          <ul class="space-y-2">
            <For each={rows()}>
              {(profile) => (
                <li class="flex flex-col gap-1.5 rounded-card bg-panel px-5 py-[18px]">
                  <div class="flex flex-wrap items-center gap-2">
                    <span class="text-lead font-bold" style={{ color: 'var(--color-text-primary)' }}>
                      {profile.name}
                    </span>
                    <span class="font-mono text-micro" style={{ color: 'var(--color-text-tertiary)' }}>
                      {profile.agent_profile_id}
                    </span>
                    <Show when={profile.builtin}>
                      <Badge tone="neutral">Built-in</Badge>
                    </Show>
                    <Show when={teamOf(profile)}>
                      <Badge tone="primary">Team</Badge>
                    </Show>
                    <span class="ml-auto flex gap-1">
                      <Button variant="ghost" size="sm" onClick={() => openForm(profile)}>Edit</Button>
                      <Show when={!profile.builtin}>
                        <Button variant="ghost" size="sm" onClick={() => void remove(profile)}>Delete</Button>
                      </Show>
                    </span>
                  </div>
                  <Show when={profile.summary}>
                    <p data-testid="profile-summary" class="text-small" style={{ color: 'var(--color-text-primary)' }}>
                      {profile.summary}
                    </p>
                  </Show>
                  <p data-testid="profile-runs" class="text-caption" style={{ color: 'var(--color-text-secondary)' }}>
                    Runs with {profileAgentText(profile)} ·{' '}
                    {teamOf(profile) ? `Team ${teamOf(profile)}` : SINGLE_AGENT_TEXT}
                  </p>
                  <Show when={teamOf(profile) && !teamList.loading && !teamList.error && !reportedTeam(teamOf(profile))}>
                    <p class="text-caption" style={{ color: 'var(--color-text-tertiary)' }} aria-disabled="true">
                      No computer reports this team, so what it can do is not known.
                    </p>
                  </Show>
                  <div class="flex flex-wrap items-center gap-2 text-caption" style={{ color: 'var(--color-text-secondary)' }}>
                    <span data-testid="profile-delivers">{profile.delivers || 'Delivers: not said'}</span>
                    <CapabilityChips profile={profile} />
                  </div>
                  <p class="text-caption" style={{ color: 'var(--color-text-secondary)' }}>{profile.instructions}</p>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </Show>

      <Show when={profiles.error !== undefined}>
        <div class="text-small" style={{ color: 'var(--color-danger-600)' }}>
          Couldn't load agent profiles.{' '}
          <button type="button" class="font-bold underline" onClick={() => void refetch()}>
            Retry
          </button>
        </div>
      </Show>

      <Show
        when={showForm()}
        fallback={
          <Button variant="ghost" size="sm" onClick={() => openForm(null)}>
            + Create agent profile
          </Button>
        }
      >
        <form onSubmit={(e) => void submit(e)} class="max-w-md space-y-3 rounded-card bg-panel px-5 py-[18px]">
          <Field label="Name" required placeholder="reviewer" value={name()} onInput={(e) => setName(e.currentTarget.value)} />
          <FieldShell label="Instructions" required for={instructionsId}>
            <textarea
              id={instructionsId}
              required
              rows={4}
              placeholder="Review the diff for correctness and style."
              value={instructions()}
              onInput={(e) => setInstructions(e.currentTarget.value)}
              class="w-full rounded-item border px-3.5 py-2 text-sm resize-y focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1"
              style={{
                'background-color': 'var(--color-bg-base)',
                color: 'var(--color-text-primary)',
                'border-color': 'var(--color-border-medium)',
                '--tw-ring-color': 'var(--color-focus-ring)',
              }}
            />
          </FieldShell>
          <Field
            label="Summary"
            hint="Shown as the profile's tooltip"
            value={summary()}
            onInput={(e) => setSummary(e.currentTarget.value)}
          />
          <Field
            label="Delivers"
            placeholder="a change on a branch"
            hint="One line: what a run of this profile hands back, such as a change on a branch, a document in a folder or an answer."
            value={delivers()}
            onInput={(e) => setDelivers(e.currentTarget.value)}
          />
          <FieldShell label="Team" for={teamId} hint="The team that runs this profile. Teams an agent reports come first; a name only profiles use is marked as not reported.">
            <select
              id={teamId}
              value={team()}
              onChange={(e) => setTeam(e.currentTarget.value)}
              class="rounded-full border px-3 py-1 text-sm"
              style={{
                'background-color': 'var(--color-bg-base)',
                color: 'var(--color-text-primary)',
                'border-color': 'var(--color-border-medium)',
              }}
            >
              <option value="" selected={!team()}>No team</option>
              <Show when={choices().reported.length > 0}>
                <optgroup label="Reported by an agent">
                  <For each={choices().reported}>{(t) => <option value={t} selected={t === team()}>{t}</option>}</For>
                </optgroup>
              </Show>
              <Show when={choices().notReported.length > 0}>
                <optgroup label="Not reported by any computer">
                  <For each={choices().notReported}>{(t) => <option value={t} selected={t === team()}>{t}</option>}</For>
                </optgroup>
              </Show>
            </select>
          </FieldShell>
          <FieldShell label="Web access" for={egressId} help={HELP.egressMode}>
            <select
              id={egressId}
              data-testid="profile-web-access"
              value={egressMode()}
              onChange={(e) => setChosenEgress(e.currentTarget.value as EgressMode)}
              class="rounded-full border px-3 py-1 text-sm"
              style={FIELD_STYLE}
            >
              <For each={EGRESS_MODES}>
                {(m) => (
                  <option value={m.value} selected={m.value === egressMode()}>
                    {m.label}
                  </option>
                )}
              </For>
            </select>
          </FieldShell>
          <FieldShell
            label="What this profile does"
            for={kindId}
            hint={
              editing()?.builtin
                ? 'A built-in profile keeps what it does.'
                : `${PROFILE_KINDS.find((k) => k.value === kind())!.says}. A profile that does not change the repository can run on a team that delivers a report.`
            }
          >
            <select
              id={kindId}
              data-testid="profile-kind"
              value={kind()}
              disabled={!!editing()?.builtin}
              onChange={(e) => setKind(e.currentTarget.value as ProfileKind)}
              class="rounded-full border px-3 py-1 text-sm"
              style={FIELD_STYLE}
            >
              <For each={PROFILE_KINDS}>
                {(k) => (
                  <option value={k.value} selected={k.value === kind()}>
                    {k.label}: {k.says}
                  </option>
                )}
              </For>
            </select>
          </FieldShell>
          <FieldShell
            label="Method for each kind of card"
            hint="How a task of that kind is worked: the team offers a catalogue and the card's kind picks from it. A kind left out goes through the team's default."
          >
            <Show
              when={team()}
              fallback={
                <p data-testid="methods-disabled" aria-disabled="true" class="text-small" style={{ color: 'var(--color-text-tertiary)' }}>
                  A profile that runs a single agent has one method, the agent's own loop, so there is none to choose. Choose a team above to name one for each kind of card.
                </p>
              }
            >
              <div class="space-y-2" data-testid="method-rows">
                <For each={methodRows()}>
                  {(row, i) => (
                    <div class="flex flex-wrap items-center gap-2">
                      <input
                        type="text"
                        aria-label={`Kind of card, row ${i() + 1}`}
                        placeholder="bug"
                        value={row.type}
                        onInput={(e) => setMethodRows((rs) => rs.map((r, j) => (j === i() ? { ...r, type: e.currentTarget.value } : r)))}
                        class="w-32 rounded-item border px-3 py-1.5 text-sm"
                        style={FIELD_STYLE}
                      />
                      <Show
                        when={catalogue()}
                        fallback={
                          <input
                            type="text"
                            aria-label={`Method for ${row.type || `row ${i() + 1}`}`}
                            placeholder="tdd"
                            value={row.method}
                            onInput={(e) => setMethodRows((rs) => rs.map((r, j) => (j === i() ? { ...r, method: e.currentTarget.value } : r)))}
                            class="w-44 rounded-item border px-3 py-1.5 text-sm"
                            style={FIELD_STYLE}
                          />
                        }
                      >
                        {(names) => (
                          <select
                            aria-label={`Method for ${row.type || `row ${i() + 1}`}`}
                            value={row.method}
                            onChange={(e) => setMethodRows((rs) => rs.map((r, j) => (j === i() ? { ...r, method: e.currentTarget.value } : r)))}
                            class="rounded-full border px-3 py-1 text-sm"
                            style={FIELD_STYLE}
                          >
                            <option value="" selected={!row.method}>Choose a method</option>
                            <For each={names()}>{(name) => <option value={name} selected={name === row.method}>{name}</option>}</For>
                            <Show when={row.method && !names().includes(row.method)}>
                              <option value={row.method} selected>{row.method} (the team does not have it)</option>
                            </Show>
                          </select>
                        )}
                      </Show>
                      <Button type="button" variant="ghost" size="sm" aria-label={`Remove the method for ${row.type || `row ${i() + 1}`}`} onClick={() => setMethodRows((rs) => rs.filter((_, j) => j !== i()))}>
                        Remove
                      </Button>
                    </div>
                  )}
                </For>
                <Button type="button" variant="ghost" size="sm" onClick={() => setMethodRows((rs) => [...rs, { type: '', method: '' }])}>
                  + Name a method for a kind of card
                </Button>
                <Show when={catalogue() === null}>
                  <p class="text-caption" style={{ color: 'var(--color-text-tertiary)' }} aria-disabled="true">
                    No computer reports this team's methods, so a name is not checked until a run.
                  </p>
                </Show>
              </div>
            </Show>
          </FieldShell>
          <Field
            label="Token limit per run"
            type="number"
            min="1"
            placeholder="Provisional limit"
            hint="Empty uses the provisional limit set in Settings → Automation, if there is one. A run can set its own."
            value={tokenLimit()}
            onInput={(e) => setTokenLimit(e.currentTarget.value)}
          />
          <Show when={historyResource.error}>
            {(error) => <LoadError what="this profile's run history" error={error()} onRetry={() => void refetchHistory()} />}
          </Show>
          <Show when={history()}>
            {(h) => <HistoryLine history={h()} onApply={(tokens) => void applyProposal(tokens)} busy={saving()} />}
          </Show>
          <Field
            label="Time limit (seconds)"
            type="number"
            min="1"
            placeholder="Default"
            hint="Empty uses the default time limit."
            value={timeLimit()}
            onInput={(e) => setTimeLimit(e.currentTarget.value)}
          />
          <label class="flex items-start gap-2 text-sm" style={{ color: 'var(--color-text-primary)' }}>
            <input type="checkbox" class="mt-0.5" checked={network()} onChange={(e) => setNetwork(e.currentTarget.checked)} />
            <span>
              This role needs the network (web search and reading sources)
              <span class="block text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                Its runs start with web tools on, including the ones that start by themselves. A run can still turn them off.
              </span>
            </span>
          </label>
          <FieldShell label="Tools">
            <div class="space-y-3">
              <For each={HARNESSES}>
                {(h) => (
                  <fieldset class="space-y-1 text-sm" style={{ color: 'var(--color-text-primary)' }}>
                    <legend class="font-bold">{h}</legend>
                    <label class="flex items-center gap-2">
                      <input type="checkbox" aria-label={`All tools (${h})`} checked={tools[h].all}
                        onChange={(e) => setTools(h, 'all', e.currentTarget.checked)} />
                      All tools
                    </label>
                    <Show when={!tools[h].all}>
                      <For each={KNOWN_TOOLS[h]}>
                        {(t) => (
                          <label class="mr-3 inline-flex items-center gap-1.5">
                            <input type="checkbox" aria-label={`${t} (${h})`} checked={tools[h].checked.includes(t)}
                              onChange={(e) => setTools(produce((st) => {
                                const cur = st[h].checked.filter((n) => n !== t);
                                st[h].checked = e.currentTarget.checked ? [...cur, t] : cur;
                              }))} />
                            {t}
                          </label>
                        )}
                      </For>
                      <input
                        type="text"
                        aria-label={`Other tools (${h})`}
                        placeholder="Other tool names, comma separated"
                        value={tools[h].extra}
                        onInput={(e) => setTools(h, 'extra', e.currentTarget.value)}
                        class="w-full rounded-item border px-3.5 py-1.5 text-sm"
                        style={{ 'background-color': 'var(--color-bg-base)', 'border-color': 'var(--color-border-medium)' }}
                      />
                    </Show>
                  </fieldset>
                )}
              </For>
            </div>
          </FieldShell>
          <Show when={refusal()}>
            {(message) => (
              <p data-testid="profile-refusal" role="alert" class="text-small" style={{ color: 'var(--color-danger-600)' }}>
                Not saved: {message()}
              </p>
            )}
          </Show>
          <Show when={warnings().length > 0}>
            <div data-testid="profile-warnings" role="status" class="rounded-item px-3 py-2 text-small" style={{ 'background-color': 'var(--color-warning-100)', color: 'var(--color-warning-700)' }}>
              <p class="font-semibold">Saved, with a second look to take:</p>
              <ul class="list-disc pl-5">
                <For each={warnings()}>{(w) => <li>{w}</li>}</For>
              </ul>
            </div>
          </Show>
          <div class="flex gap-2">
            <Button type="submit" loading={saving()} disabled={saving() || !name().trim() || !instructions().trim() || limitsInvalid()}>
              {editing() ? 'Save' : 'Create'}
            </Button>
            <Button type="button" variant="ghost" onClick={() => setShowForm(false)} disabled={saving()}>
              Cancel
            </Button>
          </div>
        </form>
      </Show>
    </div>
  );
};

export default AgentProfilesPanel;
