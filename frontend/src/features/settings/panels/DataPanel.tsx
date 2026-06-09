import { type Component, createSignal } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { Button } from '../../../shared/ui';
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

/** Export this project (JSON/CSV) and import a project from a JSON snapshot. */
const DataPanel: Component = () => {
  const { projectId, project } = useProject();
  const navigate = useNavigate();
  const [busy, setBusy] = createSignal(false);
  let importInput: HTMLInputElement | undefined;

  const exportAs = async (format: 'json' | 'csv') => {
    const id = projectId();
    if (!id) return;
    setBusy(true);
    try {
      const blob = await api.data.exportProject(id, format);
      const base = (project()?.name ?? 'project').replace(/\s+/g, '-');
      downloadBlob(blob, `${base}-export.${format}`);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Export failed');
    } finally {
      setBusy(false);
    }
  };

  const importFrom = async (file: File) => {
    setBusy(true);
    try {
      const snapshot = JSON.parse(await file.text());
      const created = await api.data.importProject(snapshot);
      toast.success('Project imported');
      navigate(`/projects/${created.id}/board`);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Import failed (invalid file?)');
    } finally {
      setBusy(false);
    }
  };

  return (
    <div class="max-w-xl space-y-6">
      <section class="space-y-2">
        <h3 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          Export
        </h3>
        <div class="flex gap-2">
          <Button variant="secondary" onClick={() => void exportAs('json')} disabled={busy()}>
            Export JSON
          </Button>
          <Button variant="secondary" onClick={() => void exportAs('csv')} disabled={busy()}>
            Export CSV
          </Button>
        </div>
      </section>

      <section class="space-y-2 border-t pt-4" style={{ 'border-color': 'var(--color-border-light)' }}>
        <h3 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          Import
        </h3>
        <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          Creates a new project from a previously exported JSON snapshot.
        </p>
        <Button onClick={() => importInput?.click()} disabled={busy()}>
          Import from JSON…
        </Button>
        <input
          ref={importInput}
          type="file"
          accept="application/json,.json"
          class="hidden"
          onChange={(e) => {
            const f = e.currentTarget.files?.[0];
            e.currentTarget.value = '';
            if (f) void importFrom(f);
          }}
        />
      </section>
    </div>
  );
};

export default DataPanel;
