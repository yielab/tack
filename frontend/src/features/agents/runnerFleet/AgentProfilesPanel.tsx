import { type Component, For, Show, createResource, createSignal, createUniqueId } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import { Badge, Button, EmptyState, Field, FieldShell, LoadError, Skeleton } from '../../../shared/ui';
import { toast } from '../../../shared/ui/toast';
import { IconSettings } from '../../../shared/ui/icons';
import { agentProfilesApi, teamOf, type AgentProfileSummary, type ProfileHistory } from '../../../shared/execution';
import { KNOWN_TOOLS } from '../../../shared/runWithAgent/models';

/**
 * Create/edit/delete UI for `agent_profiles`. The tool policy is built from a
 * per-harness checklist (`KNOWN_TOOLS`) plus a free-text line, and stored as
 * `{"tools": {"<harness>": [names] | ["*"]}}`; keys other than `tools` in an
 * existing policy are kept untouched on edit, except `network`: the "needs the network" box sets
 * `network: true`, and unticked it leaves the key out (absent is off). The limits form edits `limits.budgets.tokens`
 * and `limits.timeout_seconds`; any other key in an existing `limits` is kept. The team select sets
 * `tool_policy.docket.team` (the other `docket` keys stay); its options are the teams the board's
 * profiles already name. "Delivers" is one line the profile carries; what the profile can do is
 * derived by the server from its tools and only shown.
 */
const HARNESSES = Object.keys(KNOWN_TOOLS);

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
  const teamId = createUniqueId();
  const [instructions, setInstructions] = createSignal('');
  const instructionsId = createUniqueId();
  const [tools, setTools] = createStore<ToolForm>(blankTools());
  const [network, setNetwork] = createSignal(false);
  const [tokenLimit, setTokenLimit] = createSignal('');
  const [timeLimit, setTimeLimit] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  const rows = (): AgentProfileSummary[] => profiles()?.data.data ?? [];
  /** The teams the board's profiles name, and the one being edited, once each. */
  const teams = (): string[] =>
    [...new Set([...rows().map((p) => teamOf(p)), team()].filter((t): t is string => !!t))].sort();

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
    setInstructions(profile?.instructions ?? '');
    setTools(toolsFromPolicy(profile?.tool_policy));
    setNetwork((profile?.tool_policy as { network?: unknown } | null)?.network === true);
    const { tokens, seconds } = limitsFrom(profile?.limits);
    setTokenLimit(tokens);
    setTimeLimit(seconds);
    setShowForm(true);
  };

  const limitsInvalid = () => limitNumber(tokenLimit()) === undefined || limitNumber(timeLimit()) === undefined;

  const submit = async (e: Event) => {
    e.preventDefault();
    if (!name().trim() || !instructions().trim() || limitsInvalid()) return;
    const existing = editing();
    const base = (existing?.tool_policy ?? {}) as Record<string, unknown>;
    const { network: _network, docket, ...rest } = base;
    const { team: _team, ...docketRest } = (docket ?? {}) as Record<string, unknown>;
    const nextDocket = team() ? { ...docketRest, team: team() } : docketRest;
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
    };
    setSaving(true);
    try {
      if (existing) {
        await agentProfilesApi.update(existing.agent_profile_id, fields);
        toast.success(`Saved agent profile "${fields.name}"`);
        void refetch();
      } else {
        const created = await agentProfilesApi.create(fields);
        toast.success(`Created agent profile "${created.name}"`);
        void refetch();
      }
      setShowForm(false);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to save agent profile');
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
                    <span class="ml-auto flex gap-1">
                      <Button variant="ghost" size="sm" onClick={() => openForm(profile)}>Edit</Button>
                      <Show when={!profile.builtin}>
                        <Button variant="ghost" size="sm" onClick={() => void remove(profile)}>Delete</Button>
                      </Show>
                    </span>
                  </div>
                  <div class="flex flex-wrap items-center gap-2 text-caption" style={{ color: 'var(--color-text-secondary)' }}>
                    <span data-testid="profile-delivers">
                      {profile.delivers || 'Delivers: not said'} · on {(profile.tool_policy as { agent?: string } | null)?.agent ?? 'any agent'}
                    </span>
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
          <FieldShell label="Team" for={teamId} hint="The docket team that runs this profile. Teams come from the board's profiles.">
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
              <For each={teams()}>{(t) => <option value={t} selected={t === team()}>{t}</option>}</For>
            </select>
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
