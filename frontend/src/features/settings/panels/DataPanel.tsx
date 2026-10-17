import { type Component, For, Show, createSignal } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { api } from '../../../shared/api';
import { type ImportFileKind, type OutlineResult, importFileKind } from '../../../shared/api/data';
import { BOOK } from '../../../shared/help/routes';
import { toast } from '../../../shared/ui/toast';
import { Button, Modal } from '../../../shared/ui';
import { useProject } from '../../../shared/state/projectContext';

function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  URL.revokeObjectURL(url);
}

/** How many items the preview names before summing up the rest. */
const PREVIEW_ITEMS = 8;

const sectionTitle = (section: string) => section.charAt(0).toUpperCase() + section.slice(1);

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** A file read, checked with a dry run, and waiting for the person to confirm. */
interface PendingImport {
  target: 'this-project' | 'new-project';
  text: string;
  kind: ImportFileKind;
  /** The new project's name, for a CSV (an outline names its own). */
  name?: string;
  preview?: OutlineResult;
  error?: string;
}

/** Export this project (JSON/CSV) and import items or a full snapshot. */
const DataPanel: Component = () => {
  const { projectId, project } = useProject();
  const navigate = useNavigate();
  const [busy, setBusy] = createSignal(false);
  const [pending, setPending] = createSignal<PendingImport | null>(null);
  let importJsonInput: HTMLInputElement | undefined;
  let addItemsInput: HTMLInputElement | undefined;
  let newProjectInput: HTMLInputElement | undefined;

  const exportAs = async (format: 'json' | 'yaml' | 'csv', scope?: 'snapshot' | 'bundle') => {
    const id = projectId();
    if (!id) return;
    setBusy(true);
    try {
      const blob = await api.data.exportProject(id, format, scope);
      const base = (project()?.name ?? 'project').replace(/\s+/g, '-');
      downloadBlob(blob, `${base}-${scope === 'bundle' ? 'project' : 'export'}.${format}`);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Export failed');
    } finally {
      setBusy(false);
    }
  };

  // Accepts a JSON or YAML snapshot, routed by file extension.
  const importSnapshot = async (file: File) => {
    setBusy(true);
    try {
      const text = await file.text();
      const isYaml = /\.ya?ml$/i.test(file.name);
      const created = isYaml
        ? await api.data.importProjectYaml(text)
        : await api.data.importProject(JSON.parse(text));
      toast.success('Project imported');
      navigate(`/projects/${created.project.id}/board`);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Import failed (invalid file?)');
    } finally {
      setBusy(false);
    }
  };

  const send = (job: PendingImport, dryRun: boolean) => {
    if (job.target === 'new-project') {
      return api.data.createProjectFromFile(job.text, job.kind, { name: job.name, dryRun });
    }
    return api.data.importItems(projectId() ?? '', job.text, job.kind, dryRun);
  };

  // Every file import is checked first with a dry run; the dialog shows what
  // would be created, or every reason it would not, before anything is written.
  const previewFile = async (file: File, target: PendingImport['target']) => {
    if (target === 'this-project' && !projectId()) return;
    setBusy(true);
    const kind = importFileKind(file.name);
    const job: PendingImport = {
      target,
      kind,
      text: await file.text(),
      name: target === 'new-project' && kind === 'csv' ? file.name.replace(/\.csv$/i, '') : undefined,
    };
    try {
      setPending({ ...job, preview: await send(job, true) });
    } catch (err) {
      setPending({ ...job, error: err instanceof Error ? err.message : 'The file was refused' });
    } finally {
      setBusy(false);
    }
  };

  const confirmImport = async () => {
    const job = pending();
    if (!job?.preview) return;
    setBusy(true);
    try {
      const result = await send(job, false);
      setPending(null);
      toast.success(`Created ${plural(result.created, 'item', 'items')}`);
      if (result.project) navigate(`/projects/${result.project.id}/board`);
    } catch (err) {
      setPending({ ...job, preview: undefined, error: err instanceof Error ? err.message : 'Import failed' });
    } finally {
      setBusy(false);
    }
  };

  const dialogTitle = () => {
    const job = pending();
    if (job?.error) return 'This file cannot be imported';
    if (job?.target === 'new-project') return 'Create a project from this file?';
    return `Add these items to ${project()?.name ?? 'this project'}?`;
  };

  const fileInput = (ref: (el: HTMLInputElement) => void, onFile: (f: File) => void) => (
    <input
      ref={ref}
      type="file"
      accept="text/csv,.csv,application/json,.json,application/x-yaml,.yaml,.yml"
      class="hidden"
      onChange={(e) => {
        const f = e.currentTarget.files?.[0];
        e.currentTarget.value = '';
        if (f) onFile(f);
      }}
    />
  );

  const CARD = 'flex flex-col gap-2.5 rounded-[26px] bg-panel p-[18px]';
  const TITLE = 'text-[19px] leading-tight';
  const NOTE = 'text-[12.5px]';
  const CODE = 'font-mono';

  return (
    <div class="flex max-w-xl flex-col gap-3">
      <section class={CARD}>
        <h3 class={TITLE} style={{ color: 'var(--color-text-primary)' }}>
          Export
        </h3>
        <div class="flex flex-wrap gap-2">
          <Button variant="secondary" onClick={() => void exportAs('json')} disabled={busy()}>
            Export JSON
          </Button>
          <Button variant="secondary" onClick={() => void exportAs('yaml')} disabled={busy()}>
            Export YAML
          </Button>
          <Button variant="secondary" onClick={() => void exportAs('csv')} disabled={busy()}>
            Export CSV
          </Button>
          <Button variant="secondary" onClick={() => void exportAs('yaml', 'bundle')} disabled={busy()}>
            Export project file (YAML)
          </Button>
        </div>
        <p class={NOTE} style={{ color: 'var(--color-text-secondary)' }}>
          The whole project — fields, roles, profiles, sprints, automation and items — as one file you can edit and import again.
        </p>
      </section>

      <section class={CARD}>
        <h3 class={TITLE} style={{ color: 'var(--color-text-primary)' }}>
          Import
        </h3>

        <p class={NOTE} style={{ color: 'var(--color-text-secondary)' }}>
          Restore a full project from a previously exported JSON or YAML snapshot (creates a new project).
        </p>
        <Button class="self-start" onClick={() => importJsonInput?.click()} disabled={busy()}>
          Import from JSON / YAML…
        </Button>
        <input
          ref={importJsonInput}
          type="file"
          accept="application/json,.json,application/x-yaml,.yaml,.yml"
          class="hidden"
          onChange={(e) => {
            const f = e.currentTarget.files?.[0];
            e.currentTarget.value = '';
            if (f) void importSnapshot(f);
          }}
        />

        <p class={`${NOTE} mt-1.5`} style={{ color: 'var(--color-text-secondary)' }}>
          Create items from a CSV with a <code class={CODE}>title</code> column, or from an outline
          written as JSON or YAML — by you, a script or an AI assistant. Items can name a{' '}
          <code class={CODE}>parent</code> and what they <code class={CODE}>depends_on</code>. You see
          what will be created before anything is written.{' '}
          <a href={BOOK.importFiles} target="_blank" rel="noreferrer" class="underline">
            File format
          </a>
        </p>
        <div class="flex flex-wrap gap-2">
          <Button variant="secondary" onClick={() => addItemsInput?.click()} disabled={busy()}>
            Add items from a file…
          </Button>
          <Button variant="secondary" onClick={() => newProjectInput?.click()} disabled={busy()}>
            New project from a file…
          </Button>
        </div>
        {fileInput((el) => (addItemsInput = el), (f) => void previewFile(f, 'this-project'))}
        {fileInput((el) => (newProjectInput = el), (f) => void previewFile(f, 'new-project'))}
      </section>

      <Modal isOpen={pending() !== null} onClose={() => setPending(null)} title={dialogTitle()} help={BOOK.importFiles}>
        <div class="flex flex-col gap-3 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          <Show when={pending()?.error}>
            {(error) => <p class="whitespace-pre-wrap" role="alert">{error()}</p>}
          </Show>
          <Show when={pending()?.preview}>
            {(preview) => (
              <>
                <Show when={(preview().sections ?? []).length > 0}>
                  <ul class="flex flex-col gap-1">
                    <For each={preview().sections}>
                      {(section) => (
                        <li>
                          <span style={{ color: 'var(--color-text-primary)' }}>
                            {sectionTitle(section.section)}: {section.created} new, {section.updated} updated,{' '}
                            {section.not_mentioned} not mentioned
                          </span>
                          <Show when={section.warnings.length > 0}>
                            <ul class="list-disc pl-5">
                              <For each={section.warnings}>{(w) => <li>{w}</li>}</For>
                            </ul>
                          </Show>
                        </li>
                      )}
                    </For>
                  </ul>
                </Show>
                <p>
                  <Show when={pending()?.target === 'new-project'}>A new project. </Show>
                  {plural(preview().created, 'item', 'items')} will be added
                  <Show when={preview().dependencies > 0}>
                    {' '}with {plural(preview().dependencies, 'dependency', 'dependencies')}
                  </Show>
                  .
                </p>
                <ul class="flex flex-col gap-1">
                  <For each={preview().items.slice(0, PREVIEW_ITEMS)}>
                    {(item) => (
                      <li style={{ color: 'var(--color-text-primary)' }}>
                        {item.title} <span style={{ color: 'var(--color-text-tertiary)' }}>· {item.status}</span>
                      </li>
                    )}
                  </For>
                  <Show when={preview().items.length > PREVIEW_ITEMS}>
                    <li>and {preview().items.length - PREVIEW_ITEMS} more</li>
                  </Show>
                </ul>
                <Show when={preview().warnings.length > 0}>
                  <div>
                    <p style={{ color: 'var(--color-text-primary)' }}>Read differently than written:</p>
                    <ul class="list-disc pl-5">
                      <For each={preview().warnings}>{(w) => <li>{w}</li>}</For>
                    </ul>
                  </div>
                </Show>
              </>
            )}
          </Show>
          <div class="flex justify-end gap-2">
            <Button variant="secondary" onClick={() => setPending(null)} disabled={busy()}>
              {pending()?.error ? 'Close' : 'Cancel'}
            </Button>
            <Show when={pending()?.preview}>
              <Button onClick={() => void confirmImport()} disabled={busy()}>
                {pending()?.target === 'new-project' ? 'Create project' : 'Add items'}
              </Button>
            </Show>
          </div>
        </div>
      </Modal>
    </div>
  );
};

export default DataPanel;
