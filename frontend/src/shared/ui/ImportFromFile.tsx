import { type Component, type JSX, For, Show, createSignal, useContext } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { api } from '../api';
import { type ImportFileKind, type OutlineResult, importFileKind } from '../api/data';
import { BOOK } from '../help/routes';
import { toast } from './toast';
import Button from './Button';
import Modal from './Modal';
import { ProjectContext } from '../state/projectContext';

/** How many items the preview names before summing up the rest. */
const PREVIEW_ITEMS = 8;

const sectionTitle = (section: string) => section.charAt(0).toUpperCase() + section.slice(1);

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** Where a file goes: a new project, or the items of an existing one. */
export type ImportTarget = { kind: 'new' } | { kind: 'project'; id: string };

/** A file read, checked with a dry run, and waiting for the person to confirm. */
interface PendingImport {
  text: string;
  fileKind: ImportFileKind;
  /** The new project's name, for a CSV (an outline names its own). */
  name?: string;
  preview?: OutlineResult;
  error?: string;
}

/** Every file import is checked first with a dry run; the dialog shows what
 *  would be created, or every reason it would not, before anything is written. */
const ImportFromFile: Component<{ target: ImportTarget; onDone?: () => void; children: JSX.Element }> = (props) => {
  const navigate = useNavigate();
  const projectContext = useContext(ProjectContext);
  const [busy, setBusy] = createSignal(false);
  const [pending, setPending] = createSignal<PendingImport | null>(null);
  let input: HTMLInputElement | undefined;

  const send = (job: PendingImport, dryRun: boolean) => {
    const target = props.target;
    if (target.kind === 'new') {
      return api.data.createProjectFromFile(job.text, job.fileKind, { name: job.name, dryRun });
    }
    return api.data.importItems(target.id, job.text, job.fileKind, dryRun);
  };

  const previewFile = async (file: File) => {
    setBusy(true);
    const fileKind = importFileKind(file.name);
    const job: PendingImport = {
      fileKind,
      text: await file.text(),
      name: props.target.kind === 'new' && fileKind === 'csv' ? file.name.replace(/\.csv$/i, '') : undefined,
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
      props.onDone?.();
    } catch (err) {
      setPending({ ...job, preview: undefined, error: err instanceof Error ? err.message : 'Import failed' });
    } finally {
      setBusy(false);
    }
  };

  const dialogTitle = () => {
    if (pending()?.error) return 'This file cannot be imported';
    if (props.target.kind === 'new') return 'Create a project from this file?';
    return `Add these items to ${projectContext?.project()?.name ?? 'this project'}?`;
  };

  return (
    <>
      <Button type="button" variant="secondary" onClick={() => input?.click()} disabled={busy()}>
        {props.children}
      </Button>
      <input
        ref={input}
        type="file"
        accept="text/csv,.csv,application/json,.json,application/x-yaml,.yaml,.yml"
        class="hidden"
        onChange={(e) => {
          const f = e.currentTarget.files?.[0];
          e.currentTarget.value = '';
          if (f) void previewFile(f);
        }}
      />

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
                  <Show when={props.target.kind === 'new'}>A new project. </Show>
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
                {props.target.kind === 'new' ? 'Create project' : 'Add items'}
              </Button>
            </Show>
          </div>
        </div>
      </Modal>
    </>
  );
};

export default ImportFromFile;
