import { type Component, type JSX, createMemo, createResource, For, Show } from 'solid-js';
import { api } from '../../../shared/api';
import type { FolderCheck } from '../../../shared/api/projects';
import { toast } from '../../../shared/ui/toast';
import { Field, FieldShell, Select } from '../../../shared/ui';
import { useProject } from '../../../shared/state/projectContext';
import { agentProfilesApi, runnersApi } from '../../../shared/execution';
import type { UpdateProject } from '../../../shared/types';

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

  const save = async (patch: UpdateProject) => {
    const id = projectId();
    if (!id) return;
    try {
      await api.projects.update(id, patch);
      await refetch();
    } catch (err) {
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
    (runners() ?? []).map((r) => r.capability_snapshot as { harnesses?: { harness_kind: string }[]; push_configured?: boolean } | null),
  );
  const harnesses = createMemo(() => [
    ...new Set(snapshots().flatMap((s) => (s?.harnesses ?? []).map((h) => h.harness_kind))),
  ]);
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
            value={p()?.repository ?? ''}
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
        <Heading>How the agent works on it</Heading>
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
      </div>

      <div class={SECTION}>
        <Heading>Agent</Heading>
        <FieldShell label="Harness">
          <div class="flex flex-wrap gap-2">
            <For each={harnesses()} fallback={<Note>No runner reports a harness yet.</Note>}>
              {(h) => (
                <Pill active={p()?.default_harness === h} onClick={() => void save({ default_harness: p()?.default_harness === h ? null : h })}>
                  {h === 'claude-code' ? 'Claude Code' : h === 'codex' ? 'Codex' : h}
                </Pill>
              )}
            </For>
          </div>
        </FieldShell>
        <FieldShell label="Model">
          <div class="flex flex-col gap-1.5">
            <Radio name="model-mode" checked onChange={() => {}}>The agent's default (recommended)</Radio>
            <label class="flex items-center gap-2 text-sm opacity-60" style={{ color: 'var(--color-text-primary)' }}>
              <input type="radio" name="model-mode" disabled />
              Specific model
            </label>
            <Note>Choosing a specific model here is not available yet.</Note>
          </div>
        </FieldShell>
        <FieldShell label="Default profile">
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
    </div>
  );
};

export default AutomationPanel;
