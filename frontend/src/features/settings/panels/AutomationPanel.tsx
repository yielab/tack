import { type Component, type JSX, createMemo, createResource, createSignal, For, Show } from 'solid-js';
import { api } from '../../../shared/api';
import { ApiError } from '../../../shared/api/client';
import type { AgentModelsReport } from '../../../shared/runWithAgent/models';
import ModelField from '../../../shared/runWithAgent/ModelField';
import { HARNESS_KINDS } from '../../../shared/runWithAgent/shared';
import { installLine } from '../../../shared/runWithAgent/install';
import type { FolderCheck } from '../../../shared/api/projects';
import { toast } from '../../../shared/ui/toast';
import { Field, FieldShell, HelpHint, Select } from '../../../shared/ui';
import { HELP, agentSafety, type AgentSafetyReport } from '../../../shared/help/texts';
import { useProject } from '../../../shared/state/projectContext';
import { agentProfilesApi, runnersApi } from '../../../shared/execution';
import type { UpdateProject } from '../../../shared/types';
import { Heading, Note, SECTION } from './automation/parts';
import Ladder from './automation/Ladder';


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

/** A settings section folded into one line until opened: the heading, then what it is set to. */
const Fold = (props: { id?: string; title: string; summary: string; help?: JSX.Element; children: JSX.Element }) => (
  <details id={props.id} class={SECTION}>
    <summary class="flex cursor-pointer flex-wrap items-center gap-2">
      <Heading>{props.title}</Heading>
      {props.help}
      <span class="text-[0.78125rem]" style={{ color: 'var(--color-text-secondary)' }}>{props.summary}</span>
    </summary>
    <div class="mt-2.5 flex flex-col gap-2.5">{props.children}</div>
  </details>
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
  const whereSummary = () => (origin() === 'none' ? 'No code yet' : p()?.repository || 'No folder chosen');
  const howSummary = () =>
    origin() === 'none'
      ? 'No code yet'
      : mode() === 'in_place' ? 'In the folder, directly' : mode() === 'clone' ? 'A fresh clone per run' : 'On a new branch of your repo';

  return (
    <div class="flex max-w-xl flex-col gap-4">
      <Fold id="automation-where" title="Where the code is" summary={whereSummary()}>
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
          <label class="flex items-center gap-2 text-sm" style={{ color: 'var(--color-text-primary)' }}>
            <input
              type="checkbox"
              role="switch"
              checked={p()?.prepare_command !== ''}
              onChange={(e) => void save({ prepare_command: e.currentTarget.checked ? null : '' })}
            />
            Install dependencies before each run
          </label>
          <Show when={p()?.prepare_command !== ''}>
            <Field
              label="Install command"
              aria-label="Install command"
              value={p()?.prepare_command ?? ''}
              placeholder="Taken from the lockfile"
              onChange={(e) => void save({ prepare_command: e.currentTarget.value.trim() || null })}
            />
          </Show>
          <Note>{installLine(p()?.prepare_command, hasFolder() ? check() : null)}</Note>
        </Show>
      </Fold>

      <Fold title="How the agent works on it" summary={howSummary()} help={<HelpHint label="How the agent works" help={HELP.howTheAgentWorks} />}>
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
      </Fold>

      <Fold title="When a run finishes" summary={p()?.on_finish_status ? `Moves the item to ${p()!.on_finish_status}` : 'Does not move the item'}>
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
      </Fold>

      <Fold title="Agent" summary={p()?.default_harness ? agentLabel(p()!.default_harness!) : 'No default agent'}>
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
      </Fold>

      <Show when={projectId()}>
        {(id) => (
          <Ladder
            projectId={id()}
            project={() => p() ?? undefined}
            statuses={statuses}
            profiles={() => profiles() ?? []}
            saveProject={save}
          />
        )}
      </Show>
    </div>
  );
};

export default AutomationPanel;
