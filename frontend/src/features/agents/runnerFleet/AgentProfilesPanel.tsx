import { type Component, For, Show, createResource, createSignal, createUniqueId } from 'solid-js';
import { createStore, produce } from 'solid-js/store';
import { Badge, Button, EmptyState, Field, FieldShell, Skeleton } from '../../../shared/ui';
import { toast } from '../../../shared/ui/toast';
import { IconSettings } from '../../../shared/ui/icons';
import { agentProfilesApi, type AgentProfileSummary } from '../../../shared/execution';
import { KNOWN_TOOLS } from '../../../shared/runWithAgent/models';

/**
 * Create/edit/delete UI for `agent_profiles`. The tool policy is built from a
 * per-harness checklist (`KNOWN_TOOLS`) plus a free-text line, and stored as
 * `{"tools": {"<harness>": [names] | ["*"]}}`; keys other than `tools` in an
 * existing policy are kept untouched on edit. The limits form edits `limits.budgets.tokens`
 * and `limits.timeout_seconds`; any other key in an existing `limits` is kept.
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

const AgentProfilesPanel: Component = () => {
  const [profiles, { refetch, mutate }] = createResource(() => agentProfilesApi.list());

  const [showForm, setShowForm] = createSignal(false);
  const [editing, setEditing] = createSignal<AgentProfileSummary | null>(null);
  const [name, setName] = createSignal('');
  const [summary, setSummary] = createSignal('');
  const [instructions, setInstructions] = createSignal('');
  const instructionsId = createUniqueId();
  const [tools, setTools] = createStore<ToolForm>(blankTools());
  const [tokenLimit, setTokenLimit] = createSignal('');
  const [timeLimit, setTimeLimit] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  const rows = (): AgentProfileSummary[] => profiles()?.data.data ?? [];

  const openForm = (profile: AgentProfileSummary | null) => {
    setEditing(profile);
    setName(profile?.name ?? '');
    setSummary(profile?.summary ?? '');
    setInstructions(profile?.instructions ?? '');
    setTools(toolsFromPolicy(profile?.tool_policy));
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
    const tool_policy = {
      ...base,
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
        mutate((prev) =>
          prev
            ? {
                ...prev,
                data: {
                  ...prev.data,
                  data: [...prev.data.data, { agent_profile_id: created.agent_profile_id, limits: {}, builtin: false, kind: 'custom', ...fields }],
                },
              }
            : prev,
        );
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
            <div class="rounded-[28px] border-2 border-dashed" style={{ 'border-color': 'var(--color-border-light)' }}>
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
                <li class="flex flex-col gap-1.5 rounded-[28px] bg-panel px-5 py-[18px]">
                  <div class="flex flex-wrap items-center gap-2">
                    <span class="text-[0.9375rem] font-bold" style={{ color: 'var(--color-text-primary)' }}>
                      {profile.name}
                    </span>
                    <span class="font-mono text-[0.6875rem]" style={{ color: 'var(--color-text-tertiary)' }}>
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
                  <p class="text-[0.78125rem]" style={{ color: 'var(--color-text-secondary)' }}>{profile.instructions}</p>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </Show>

      <Show when={profiles.error !== undefined}>
        <div class="text-[0.8125rem]" style={{ color: 'var(--color-danger-600)' }}>
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
        <form onSubmit={(e) => void submit(e)} class="max-w-md space-y-3 rounded-[28px] bg-panel px-5 py-[18px]">
          <Field label="Name" required placeholder="reviewer" value={name()} onInput={(e) => setName(e.currentTarget.value)} />
          <FieldShell label="Instructions" required for={instructionsId}>
            <textarea
              id={instructionsId}
              required
              rows={4}
              placeholder="Review the diff for correctness and style."
              value={instructions()}
              onInput={(e) => setInstructions(e.currentTarget.value)}
              class="w-full rounded-[20px] border px-3.5 py-2 text-sm resize-y focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1"
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
            label="Token limit per run"
            type="number"
            min="1"
            placeholder="No limit"
            hint="Empty means no limit. A run can set its own."
            value={tokenLimit()}
            onInput={(e) => setTokenLimit(e.currentTarget.value)}
          />
          <Field
            label="Time limit (seconds)"
            type="number"
            min="1"
            placeholder="Default"
            hint="Empty uses the default time limit."
            value={timeLimit()}
            onInput={(e) => setTimeLimit(e.currentTarget.value)}
          />
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
                        class="w-full rounded-[20px] border px-3.5 py-1.5 text-sm"
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
