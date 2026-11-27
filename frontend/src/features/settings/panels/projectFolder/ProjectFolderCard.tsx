import { type Component, For, Show, createResource, createSignal } from 'solid-js';
import { api } from '../../../../shared/api';
import type { FolderApplied, PlanSource } from '../../../../shared/api/folder';
import { BOOK } from '../../../../shared/help/routes';
import { FiFolder } from 'solid-icons/fi';
import { Button, SectionHeading } from '../../../../shared/ui';
import { toast } from '../../../../shared/ui/toast';
import { directoryPicker, readPickedFolder, writeFolder } from './folderAccess';
import PlanDialog from './PlanDialog';
import { formatWhen, originLabel, statusLine } from './planText';

const MUTED = { color: 'var(--color-text-secondary)' };
const PRIMARY = { color: 'var(--color-text-primary)' };

/** Why Export is off where the browser cannot write a folder. */
export const NO_FOLDER_WRITE =
  'This browser cannot write a folder. Run `tack folder export` in the repository, or open Tack in a browser that can (Chrome, Edge).';

/**
 * Settings → Data → Project folder: the plan kept in `.tack/` beside the code. Export writes the
 * board's plan into the folder; Import… reads the folder and shows its plan before anything is
 * written; History lists the revisions, and any one can be gone back to the same way.
 */
const ProjectFolderCard: Component<{ projectId: string }> = (props) => {
  const [status, { refetch: refetchStatus }] = createResource(() => props.projectId, (id) => api.folder.status(id));
  const [showHistory, setShowHistory] = createSignal(false);
  const [revisions, { refetch: refetchRevisions }] = createResource(
    () => (showHistory() ? props.projectId : undefined),
    (id) => api.folder.revisions(id),
  );
  const [planning, setPlanning] = createSignal<{ source: PlanSource; title: string } | null>(null);
  const [busy, setBusy] = createSignal(false);
  let folderInput: HTMLInputElement | undefined;
  const pick = directoryPicker();

  const refresh = () => {
    void refetchStatus();
    if (showHistory()) void refetchRevisions();
  };

  const exportFolder = async () => {
    if (!pick) return;
    let picked;
    try {
      picked = await pick({ mode: 'readwrite' });
    } catch {
      return; // the person closed the picker
    }
    setBusy(true);
    try {
      const { files } = await api.folder.export(props.projectId);
      const { written, removed } = await writeFolder(picked, files);
      toast.success(`Wrote ${written} files${removed > 0 ? `, removed ${removed} item files the board no longer has` : ''}`);
      refresh();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Export failed');
    } finally {
      setBusy(false);
    }
  };

  const importFolder = async (files: FileList) => {
    const read = await readPickedFolder(files);
    if (Object.keys(read).length === 0) {
      toast.error('No project folder in what you picked: choose the .tack folder, or the repository that holds it.');
      return;
    }
    setPlanning({ source: { files: read }, title: 'Apply the project folder?' });
  };

  const applied = (result: FolderApplied) => {
    setPlanning(null);
    toast.success(`Applied. Revision #${result.revision.number}`);
    refresh();
  };

  return (
    <section aria-labelledby="project-folder-title" data-testid="project-folder" class="flex flex-col gap-4">
      <div class="flex flex-col gap-1">
        <SectionHeading id="project-folder-title" icon={<FiFolder size={16} />}>
          Project folder
        </SectionHeading>
        <p class="text-sm" style={MUTED}>
          The plan kept beside your code, in <code class="font-mono">.tack/</code>: the project's setup and one file per item
          with what it is and what makes it done. Where each item is, who answers for it and when stay on the board.{' '}
          <a href={BOOK.projectFolder} target="_blank" rel="noreferrer" class="underline">How it works</a>
        </p>
      </div>

      <p data-testid="project-folder-status" class="text-lead" style={PRIMARY} aria-live="polite">
        <Show when={!status.error} fallback={<span style={MUTED}>The folder's status could not be read: {String(status.error?.message ?? status.error)}</span>}>
          {status() ? statusLine(status()!) : 'Reading…'}
        </Show>
      </p>

      <div class="flex flex-wrap items-center gap-2">
        <Button onClick={() => void exportFolder()} disabled={busy() || !pick} title={pick ? undefined : NO_FOLDER_WRITE}>
          Export
        </Button>
        <Button variant="secondary" onClick={() => folderInput?.click()} disabled={busy()}>
          Import…
        </Button>
        <Button variant="ghost" aria-expanded={showHistory()} onClick={() => setShowHistory(!showHistory())}>
          History
        </Button>
        <input
          ref={(el) => {
            folderInput = el;
            el.setAttribute('webkitdirectory', '');
          }}
          data-testid="project-folder-input"
          type="file"
          multiple
          class="hidden"
          onChange={(e) => {
            const files = e.currentTarget.files;
            if (files && files.length > 0) void importFolder(files);
            e.currentTarget.value = '';
          }}
        />
      </div>
      <Show when={!pick}>
        <p data-testid="project-folder-no-write" class="text-caption" style={MUTED}>Export: {NO_FOLDER_WRITE}</p>
      </Show>

      <Show when={showHistory()}>
        <div data-testid="project-folder-history" class="flex flex-col gap-1">
          <h4 class="text-lead font-semibold" style={PRIMARY}>Revisions</h4>
          <Show when={!revisions.error} fallback={<p style={MUTED}>The revisions could not be read.</p>}>
            <Show when={(revisions() ?? []).length > 0} fallback={<p class="text-sm" style={MUTED}>{revisions.loading ? 'Reading…' : 'No revision yet.'}</p>}>
              <ul class="flex flex-col divide-y" style={{ 'border-color': 'var(--color-border-light)' }}>
                <For each={revisions()}>
                  {(revision, index) => (
                    <li data-testid="project-folder-revision" class="flex flex-wrap items-center gap-x-3 gap-y-1 py-2 text-sm">
                      <span class="font-semibold" style={PRIMARY}>#{revision.number}</span>
                      <span style={MUTED}>from {originLabel(revision.origin)} · {formatWhen(revision.created_at)}</span>
                      <Show when={index() > 0} fallback={<span class="ml-auto text-caption" style={MUTED}>The latest</span>}>
                        <Button
                          size="sm"
                          variant="ghost"
                          class="ml-auto"
                          onClick={() => setPlanning({ source: { revision: revision.number }, title: `Go back to revision #${revision.number}?` })}
                        >
                          Revert to this…
                        </Button>
                      </Show>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
          </Show>
        </div>
      </Show>

      <Show when={planning()}>
        {(job) => (
          <PlanDialog projectId={props.projectId} source={job().source} title={job().title} onClose={() => setPlanning(null)} onApplied={applied} />
        )}
      </Show>
    </section>
  );
};

export default ProjectFolderCard;
