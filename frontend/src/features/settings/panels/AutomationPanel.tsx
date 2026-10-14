import { type Component, type JSX, createMemo, createResource, createSignal, For, Show } from 'solid-js';
import { api } from '../../../shared/api';
import { ApiError } from '../../../shared/api/client';
import type { AgentModelsReport } from '../../../shared/runWithAgent/models';
import ModelField from '../../../shared/runWithAgent/ModelField';
import { HARNESS_KINDS } from '../../../shared/runWithAgent/shared';
import type { FolderCheck } from '../../../shared/api/projects';
import { toast } from '../../../shared/ui/toast';
import { Field, FieldShell, HelpHint, Select } from '../../../shared/ui';
import { HELP, agentSafety, type AgentSafetyReport } from '../../../shared/help/texts';
import { useProject } from '../../../shared/state/projectContext';
import { agentProfilesApi, runnersApi } from '../../../shared/execution';
import type { UpdateProject } from '../../../shared/types';
import type { AutonomyDomain, AutonomySettings } from '../../../shared/api/autonomy';


/** The part of a runner's reported harness this panel reads. Every field
 *  but the kind is optional: an older runner omits them. */
interface SnapshotHarness extends AgentSafetyReport, AgentModelsReport {
  harness_kind: string;
}

/** One sentence for what `check-folder` found at a path. */
function checkSentence(r: FolderCheck): string {
  if (!r.exists) return 'That path does not exist';
  if (!r.is_dir) return 'That path is a file, not a folder';
  if (!r.is_git) return 'A folder without git — the agent will work in it directly';
  const dirty = Array.isArray(r.dirty_files) ? r.dirty_files.length : Number(r.dirty_files ?? 0);
  const on = r.branch ? ` on branch ${r.branch}` : '';
  return `A git repository${on}, ${dirty === 0 ? 'no uncommitted files' : `${dirty} uncommitted file${dirty === 1 ? '' : 's'}`}`;
}

const SECTION = 'flex flex-col gap-2.5 rounded-[26px] bg-panel p-[18px]';

const Heading = (props: { children: JSX.Element }) => (
  <h3 class="text-[19px] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
    {props.children}
  </h3>
);

const Note = (props: { children: JSX.Element }) => (
  <p class="text-[12.5px]" style={{ color: 'var(--color-text-secondary)' }}>
    {props.children}
  </p>
);

const Radio = (props: { name: string; checked: boolean; onChange: () => void; children: JSX.Element }) => (
  <label class="flex cursor-pointer items-center gap-2 text-sm" style={{ color: 'var(--color-text-primary)' }}>
    <input type="radio" name={props.name} checked={props.checked} onChange={props.onChange} />
    {props.children}
  </label>
);

const Pill = (props: { active: boolean; title?: string; onClick: () => void; children: JSX.Element }) => (
  <button
    type="button"
    title={props.title}
    aria-pressed={props.active}
    onClick={props.onClick}
    class="rounded-full border px-3 py-1 text-sm"
    style={{
      'border-color': props.active ? 'var(--color-primary-600)' : 'var(--color-border-medium)',
      'background-color': props.active ? 'var(--color-accent-soft)' : 'var(--color-bg-base)',
      color: 'var(--color-text-primary)',
    }}
  >
    {props.children}
  </button>
);

const LEVELS = [
  { value: 'L3', sentence: 'You merge every change yourself.' },
  { value: 'L4', sentence: 'Tack merges changes that stay inside the domains below.' },
  { value: 'L5', sentence: 'Tack merges any change that passes its checks.' },
] as const;

/** prill's L4 example for the stages it must run. */
const PRILL_STAGES = ['criteria', 'suite', 'anti_tautology', 'static_analysis'];

const list = (text: string) => text.split(/[,\n]/).map((x) => x.trim()).filter(Boolean);

const BUDGETS = [
  { key: 'tokens_per_day', label: 'Tokens per day', integer: true },
  { key: 'usd_per_day', label: 'Dollars per day', integer: false },
  { key: 'auto_merges_per_day', label: 'Automatic merges per day', integer: true },
  { key: 'queue_items_max', label: 'Items waiting at once', integer: true },
] as const;

const inputClass = 'w-full rounded-full border px-3.5 py-2 text-sm';
const inputStyle = {
  'background-color': 'var(--color-bg-base)',
  color: 'var(--color-text-primary)',
  'border-color': 'var(--color-border-medium)',
} as const;

/**
 * Autonomy: how much Tack may merge by itself. Every field saves on change
 * (`PUT /projects/{id}/autonomy`, the whole object); a refusal is a toast.
 */
const AutonomySection: Component<{ projectId: string }> = (props) => {
  const [settings, { mutate }] = createResource(
    () => props.projectId,
    (id) => api.autonomy.get(id).catch(() => null),
  );
  const blocked = () => settings()?.automatic_merge_blocked_by ?? [];
  const blockedWhy = () => blocked().join('. ');
  const save = async (patch: Partial<AutonomySettings>) => {
    const current = settings();
    if (!current) return;
    try {
      mutate(await api.autonomy.put(props.projectId, { ...current, ...patch }));
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to save');
    }
  };
  const saveDomain = (index: number, patch: Partial<AutonomyDomain>) =>
    save({ domains: (settings()?.domains ?? []).map((d, i) => (i === index ? { ...d, ...patch } : d)) });
  const addDomain = () =>
    save({ domains: [...(settings()?.domains ?? []), { name: `domain-${(settings()?.domains?.length ?? 0) + 1}`, path_globs: [], item_types: [], level: 'L3' }] });
  const saveBudget = (key: (typeof BUDGETS)[number]['key'], raw: string, integer: boolean) => {
    const text = raw.trim();
    const n = text === '' ? null : integer ? Math.trunc(Number(text)) : Number(text);
    if (n !== null && (!Number.isFinite(n) || n < 0)) return void toast.error('A limit is a number from 0 up, or empty for none');
    return save({ budgets: { ...(settings()?.budgets ?? {}), [key]: n } });
  };

  return (
    <div class={SECTION} id="automation-autonomy">
      <div class="flex items-center gap-2">
        <Heading>Autonomy</Heading>
        <HelpHint label="Autonomy" help={HELP.autonomyLevel} />
      </div>
      <Show when={settings()} fallback={<Note>Autonomy settings are not available yet.</Note>}>
        {(s) => (
          <>
            <div class="flex flex-col gap-2">
              <For each={LEVELS}>
                {(l) => {
                  const off = () => l.value !== 'L3' && blocked().length > 0;
                  return (
                    <label class="flex items-start gap-2 text-sm" style={{ color: off() ? 'var(--color-text-tertiary)' : 'var(--color-text-primary)' }}>
                      <input
                        type="radio"
                        name="autonomy-level"
                        value={l.value}
                        checked={s().level === l.value}
                        disabled={off()}
                        onChange={() => void save({ level: l.value })}
                      />
                      <span>
                        <span class="font-semibold">{l.value}</span> — {l.sentence}
                      </span>
                    </label>
                  );
                }}
              </For>
              <Show when={blocked().length > 0}>
                <p data-testid="autonomy-blocked" class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                  L4 and L5 are off: {blockedWhy()}
                </p>
              </Show>
            </div>

            <div class="flex items-center gap-2">
              <label class="flex items-center gap-2 text-sm" style={{ color: 'var(--color-text-primary)' }}>
                <input
                  type="checkbox"
                  role="switch"
                  aria-label="Merge when moved to Done"
                  checked={s().merge_on_done}
                  onChange={(e) => void save({ merge_on_done: e.currentTarget.checked })}
                />
                Merge when moved to Done
              </label>
              <HelpHint label="Merge when moved to Done" help={HELP.mergeOnDone} />
            </div>

            <FieldShell label="Domains">
              <div class="flex flex-col gap-3">
                <For each={s().domains} fallback={<Note>No domains yet. L4 merges only inside a domain.</Note>}>
                  {(d, i) => (
                    <div data-testid="autonomy-domain" class="flex flex-col gap-1.5 rounded-[20px] border p-3" style={{ 'border-color': 'var(--color-border-medium)' }}>
                      <input
                        class={inputClass}
                        style={inputStyle}
                        aria-label="Domain name"
                        value={d.name}
                        onChange={(e) => void saveDomain(i(), { name: e.currentTarget.value.trim() })}
                      />
                      <input
                        class={inputClass}
                        style={inputStyle}
                        aria-label="Paths it covers"
                        placeholder="docs/**, README.md"
                        value={d.path_globs.join(', ')}
                        onChange={(e) => void saveDomain(i(), { path_globs: list(e.currentTarget.value) })}
                      />
                      <input
                        class={inputClass}
                        style={inputStyle}
                        aria-label="Item types it covers"
                        placeholder="Any type"
                        value={(d.item_types ?? []).join(', ')}
                        onChange={(e) => void saveDomain(i(), { item_types: list(e.currentTarget.value) })}
                      />
                      <div class="flex flex-wrap items-center gap-3">
                        <span data-testid="domain-counts" class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                          {d.merged ?? 0} merged · {d.undone ?? 0} undone
                        </span>
                        <select
                          aria-label="Domain level"
                          class="rounded-full border px-3 py-1 text-sm"
                          style={inputStyle}
                          onChange={(e) => void saveDomain(i(), { level: e.currentTarget.value })}
                        >
                          <For each={LEVELS}>
                            {(l) => (
                              <option value={l.value} selected={d.level === l.value} disabled={l.value !== 'L3' && blocked().length > 0}>
                                {l.value}
                              </option>
                            )}
                          </For>
                        </select>
                        <button
                          type="button"
                          class="text-xs underline"
                          style={{ color: 'var(--color-text-secondary)' }}
                          onClick={() => void save({ domains: s().domains.filter((_, j) => j !== i()) })}
                        >
                          Remove
                        </button>
                      </div>
                    </div>
                  )}
                </For>
                <div>
                  <button type="button" class="rounded-full border px-3 py-1 text-sm" style={inputStyle} onClick={() => void addDomain()}>
                    Add a domain
                  </button>
                </div>
              </div>
            </FieldShell>

            <FieldShell label="Stages prill must run" help={HELP.verifyRequire}>
              <div class="flex flex-col gap-1.5">
                <input
                  class={inputClass}
                  style={inputStyle}
                  aria-label="Stages prill must run"
                  value={s().verify_require.join(', ')}
                  onChange={(e) => void save({ verify_require: list(e.currentTarget.value) })}
                />
                <div>
                  <button
                    type="button"
                    class="rounded-full border px-3 py-1 text-sm"
                    style={inputStyle}
                    onClick={() => void save({ verify_require: PRILL_STAGES })}
                  >
                    Fill prill's example
                  </button>
                </div>
              </div>
            </FieldShell>

            <FieldShell label="Public key">
              <textarea
                aria-label="Public key"
                rows={3}
                class="w-full resize-none rounded-[20px] border px-3.5 py-2 text-sm"
                style={{ ...inputStyle, 'font-family': 'var(--font-mono)' }}
                value={s().attestation_public_key ?? ''}
                onChange={(e) => void save({ attestation_public_key: e.currentTarget.value.trim() || null })}
              />
              <Note>The key the signed pack is checked against, in PEM form.</Note>
            </FieldShell>

            <FieldShell label="Times a rejected change goes back to the agent" help={HELP.loopCap}>
              <input
                type="number"
                min="0"
                class={inputClass}
                style={inputStyle}
                aria-label="Times a rejected change goes back to the agent"
                value={s().verify_loop_max_attempts}
                onChange={(e) => void save({ verify_loop_max_attempts: Math.max(0, Math.trunc(Number(e.currentTarget.value) || 0)) })}
              />
            </FieldShell>

            <FieldShell label="Daily limits" help={HELP.budgets}>
              <div class="grid grid-cols-2 gap-2">
                <For each={BUDGETS}>
                  {(b) => (
                    <label class="flex flex-col gap-1 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                      {b.label}
                      <input
                        type="number"
                        min="0"
                        class={inputClass}
                        style={inputStyle}
                        aria-label={b.label}
                        placeholder="No limit"
                        value={s().budgets?.[b.key] ?? ''}
                        onChange={(e) => void saveBudget(b.key, e.currentTarget.value, b.integer)}
                      />
                    </label>
                  )}
                </For>
              </div>
            </FieldShell>
          </>
        )}
      </Show>
    </div>
  );
};

/**
 * Project automation: where the code is, how the agent works on it, what
 * happens when a run finishes, and the default agent. Every field saves on
 * change (`PATCH /projects/{id}`); a failure is a toast.
 */
const AutomationPanel: Component = () => {
  const { project, projectId, refetch, workflow } = useProject();
  const p = () => project();
  const origin = () => p()?.code_origin ?? 'none';
  const mode = () => p()?.workspace_mode ?? 'local_branch';
  const hasFolder = () => origin() === 'folder' || origin() === 'new_folder';

  const [folderError, setFolderError] = createSignal<string | undefined>();

  const save = async (patch: UpdateProject) => {
    const id = projectId();
    if (!id) return;
    setFolderError(undefined);
    try {
      await api.projects.update(id, patch);
      await refetch();
    } catch (err) {
      if (err instanceof ApiError && err.field === 'repository') return void setFolderError(err.message);
      toast.error(err instanceof Error ? err.message : 'Failed to save');
    }
  };

  const [check] = createResource(
    () => (hasFolder() ? p()?.repository || null : null),
    (path) => api.projects.checkFolder(path).catch(() => null),
  );

  const saveFolder = async (path: string) => {
    const value = path.trim();
    if (!value) return void save({ repository: null, default_branch: null });
    let branch: string | null = null;
    try {
      const r = await api.projects.checkFolder(value);
      branch = r.is_git ? r.branch : null;
    } catch {
      /* the PATCH below reports an invalid path */
    }
    await save({ repository: value, default_branch: branch });
  };

  const [runners] = createResource(() => runnersApi.list().then((r) => r.data.data).catch(() => []));
  const [profiles] = createResource(() => agentProfilesApi.list().then((r) => r.data.data).catch(() => []));
  const snapshots = createMemo(() =>
    (runners() ?? []).map((r) => r.capability_snapshot as { harnesses?: SnapshotHarness[]; push_configured?: boolean } | null),
  );
  const harnesses = createMemo(() => [
    ...new Set(snapshots().flatMap((s) => (s?.harnesses ?? []).map((h) => h.harness_kind))),
  ]);
  /** Every runner's report of the default agent, across snapshots. */
  const defaultHarnessReports = () =>
    snapshots().flatMap((s) => s?.harnesses ?? []).filter((h) => h.harness_kind === p()?.default_harness);
  const savedModel = () => p()?.default_model || null;
  const agentLabel = (kind: string) => HARNESS_KINDS.find((k) => k.value === kind)?.label ?? kind;
  const pushConfigured = () => snapshots().some((s) => s?.push_configured === true);
  const pushDisabledReason = () =>
    mode() === 'in_place'
      ? 'Not available when the agent works in the folder directly'
      : !pushConfigured()
        ? 'Push is not configured on this computer'
        : null;

  const statuses = () => [...(workflow()?.statuses ?? [])].sort((a, b) => a.order - b.order);

  return (
    <div class="flex max-w-xl flex-col gap-4">
      <div id="automation-where" class={SECTION}>
        <Heading>Where the code is</Heading>
        <div class="flex flex-col gap-2">
          <Radio name="code-origin" checked={origin() === 'folder' || origin() === 'new_folder'} onChange={() => void save({ code_origin: 'folder' })}>
            An existing folder on this computer
          </Radio>
          <Radio name="code-origin" checked={origin() === 'none'} onChange={() => void save({ code_origin: 'none' })}>
            No code yet
          </Radio>
          <details open={origin() === 'url'}>
            <summary class="cursor-pointer text-sm" style={{ color: 'var(--color-text-secondary)' }}>Advanced</summary>
            <Radio name="code-origin" checked={origin() === 'url'} onChange={() => void save({ code_origin: 'url' })}>
              A repository URL, cloned per run
            </Radio>
          </details>
        </div>
        <Show when={origin() === 'none'}>
          <Note>This project has no code. Choose a folder to let agents work on it.</Note>
        </Show>
        <Show when={origin() !== 'none'}>
          <Field
            label={hasFolder() ? 'Folder' : 'Repository URL'}
            aria-label={hasFolder() ? 'Folder' : 'Repository URL'}
            help={hasFolder() ? HELP.folder : undefined}
            value={p()?.repository ?? ''}
            error={folderError()}
            placeholder={hasFolder() ? '/home/you/code/project' : 'https://github.com/org/repo.git'}
            onChange={(e) => void saveFolder(e.currentTarget.value)}
          />
          <Field
            label="Branch"
            aria-label="Branch"
            value={p()?.default_branch ?? ''}
            onChange={(e) => void save({ default_branch: e.currentTarget.value.trim() || null })}
          />
          <Show when={hasFolder() && check()}>
            <Note>{checkSentence(check()!)}</Note>
          </Show>
        </Show>
      </div>

      <div class={SECTION}>
        <div class="flex items-center gap-2">
          <Heading>How the agent works on it</Heading>
          <HelpHint label="How the agent works" help={HELP.howTheAgentWorks} />
        </div>
        <Show
          when={origin() !== 'none'}
          fallback={
            <Note>
              This project has no code. Choose a folder to let agents work on it.{' '}
              <a href="#automation-where" onClick={(e) => { e.preventDefault(); document.getElementById('automation-where')?.scrollIntoView?.(); }} class="underline">
                Change where the code is
              </a>
            </Note>
          }
        >
          <div class="flex flex-col gap-2">
            <Radio name="workspace-mode" checked={mode() === 'local_branch'} onChange={() => void save({ workspace_mode: 'local_branch' })}>
              On a new branch of your repo (recommended)
            </Radio>
            <Radio name="workspace-mode" checked={mode() === 'in_place'} onChange={() => void save({ workspace_mode: 'in_place' })}>
              In the folder, directly
            </Radio>
            <details open={mode() === 'clone'}>
              <summary class="cursor-pointer text-sm" style={{ color: 'var(--color-text-secondary)' }}>Advanced</summary>
              <Radio name="workspace-mode" checked={mode() === 'clone'} onChange={() => void save({ workspace_mode: 'clone' })}>
                A fresh clone per run
              </Radio>
            </details>
          </div>
          <div class="flex items-center gap-2">
            <label class="flex items-center gap-2 text-sm" style={{ color: 'var(--color-text-primary)' }}>
              <input
                type="checkbox"
                role="switch"
                checked={!!p()?.push_after_run}
                disabled={pushDisabledReason() !== null}
                onChange={(e) => void save({ push_after_run: e.currentTarget.checked })}
              />
              Push the branch when a run finishes
            </label>
            <HelpHint label="Push the branch" help={HELP.pushTheBranch} />
          </div>
          <Show when={pushDisabledReason()}>
            <Note>{pushDisabledReason()}</Note>
          </Show>
        </Show>
      </div>

      <div class={SECTION}>
        <Heading>When a run finishes</Heading>
        <Select
          label="Move the item to"
          value={p()?.on_finish_status ?? ''}
          onChange={(e) => void save({ on_finish_status: e.currentTarget.value || null })}
          options={[{ value: '', label: 'Do not move' }, ...statuses().map((s) => ({ value: s.name, label: s.name }))]}
        />
        <FieldShell label="Definition of done" help={HELP.definitionOfDone}>
          <textarea
            aria-label="Definition of done"
            rows={3}
            value={p()?.definition_of_done ?? ''}
            onBlur={(e) => void save({ definition_of_done: e.currentTarget.value.trim() || null })}
            class="w-full resize-none rounded-[20px] border px-3.5 py-2 text-sm"
            style={{
              'background-color': 'var(--color-bg-base)',
              color: 'var(--color-text-primary)',
              'border-color': 'var(--color-border-medium)',
            }}
          />
        </FieldShell>
      </div>

      <div class={SECTION}>
        <Heading>Agent</Heading>
        <FieldShell label="Agent" help={HELP.sandbox}>
          <div class="flex flex-wrap gap-2">
            <For each={harnesses()} fallback={<Note>No runner reports an agent yet.</Note>}>
              {(h) => (
                <Pill active={p()?.default_harness === h} onClick={() => { void save({ default_harness: p()?.default_harness === h ? null : h, default_model: null }); }}>
                  {agentLabel(h)}
                </Pill>
              )}
            </For>
          </div>
          <Show when={p()?.default_harness ? agentSafety(defaultHarnessReports()) : null}>
            {(safety) => (
              <p
                class="mt-1.5 text-xs"
                data-testid="default-agent-safety"
                style={{ color: safety().tone === 'blocked' ? 'var(--color-danger-600)' : 'var(--color-text-secondary)' }}
              >
                {safety().text}
              </p>
            )}
          </Show>
        </FieldShell>
        <Show
          when={p()?.default_harness}
          keyed
          fallback={
            <FieldShell label="Model" help={HELP.model}>
              <Note>Choose an agent first</Note>
            </FieldShell>
          }
        >
          {(harness) => (
            <ModelField
              agentLabel={agentLabel(harness)}
              reports={defaultHarnessReports()}
              value={savedModel() ?? ''}
              onChange={(model) => void save({ default_model: model.trim() || null })}
            />
          )}
        </Show>
        <FieldShell label="Default profile" help={HELP.profile}>
          <div class="flex flex-wrap gap-2">
            <For each={profiles()} fallback={<Note>No agent profiles yet.</Note>}>
              {(pr) => (
                <Pill
                  active={p()?.default_profile_id === pr.agent_profile_id}
                  title={(pr as { summary?: string }).summary}
                  onClick={() => void save({ default_profile_id: p()?.default_profile_id === pr.agent_profile_id ? null : pr.agent_profile_id })}
                >
                  {pr.name}
                </Pill>
              )}
            </For>
          </div>
          <Note>
            <a href="/agents#profiles" class="underline">Manage profiles</a>
          </Note>
        </FieldShell>
      </div>

      <Show when={projectId()}>{(id) => <AutonomySection projectId={id()} />}</Show>
    </div>
  );
};

export default AutomationPanel;
