import { type Component, Show, createSignal } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { api } from '../../../shared/api';
import { BOOK } from '../../../shared/help/routes';
import { toast } from '../../../shared/ui/toast';
import { FiDownload, FiUpload } from 'solid-icons/fi';
import { Button, SectionHeading } from '../../../shared/ui';
import ImportFromFile from '../../../shared/ui/ImportFromFile';
import { useProject } from '../../../shared/state/projectContext';
import ProjectFolderCard from './projectFolder/ProjectFolderCard';

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

/** Export this project (JSON/CSV) and import items or a full snapshot. */
const DataPanel: Component = () => {
  const { projectId, project } = useProject();
  const navigate = useNavigate();
  const [busy, setBusy] = createSignal(false);
  let importJsonInput: HTMLInputElement | undefined;

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

  const SECTION = 'flex flex-col gap-4';
  const NOTE = 'text-caption';
  const CODE = 'font-mono';

  return (
    <div class="flex max-w-2xl flex-col gap-12">
      <Show when={projectId()}>{(id) => <ProjectFolderCard projectId={id()} />}</Show>

      <section class={SECTION} aria-labelledby="data-export-title">
        <SectionHeading id="data-export-title" icon={<FiDownload size={16} />}>
          Export
        </SectionHeading>
        <div class="flex flex-wrap gap-2">
          <Button variant="ghost" onClick={() => void exportAs('json')} disabled={busy()}>
            Export JSON
          </Button>
          <Button variant="ghost" onClick={() => void exportAs('yaml')} disabled={busy()}>
            Export YAML
          </Button>
          <Button variant="ghost" onClick={() => void exportAs('csv')} disabled={busy()}>
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

      <section class={SECTION} aria-labelledby="data-import-title">
        <SectionHeading id="data-import-title" icon={<FiUpload size={16} />}>
          Import
        </SectionHeading>

        <p class={NOTE} style={{ color: 'var(--color-text-secondary)' }}>
          Restore a full project from a previously exported JSON or YAML snapshot (creates a new project).
        </p>
        <Button variant="secondary" class="self-start" onClick={() => importJsonInput?.click()} disabled={busy()}>
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
          <Show when={projectId()}>
            {(id) => <ImportFromFile target={{ kind: 'project', id: id() }}>Add items from a file…</ImportFromFile>}
          </Show>
          <ImportFromFile target={{ kind: 'new' }}>New project from a file…</ImportFromFile>
        </div>
      </section>
    </div>
  );
};

export default DataPanel;
