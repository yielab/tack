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

/** Export this project (JSON/CSV) and import items or a full snapshot. */
const DataPanel: Component = () => {
  const { projectId, project } = useProject();
  const navigate = useNavigate();
  const [busy, setBusy] = createSignal(false);
  let importJsonInput: HTMLInputElement | undefined;
  let importCsvInput: HTMLInputElement | undefined;

  const exportAs = async (format: 'json' | 'yaml' | 'csv') => {
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

  const importFromCsv = async (file: File) => {
    const id = projectId();
    if (!id) return;
    setBusy(true);
    try {
      const text = await file.text();
      const result = await api.data.importCsv(id, text);
      toast.success(`Imported ${result.created} item${result.created !== 1 ? 's' : ''}${result.skipped > 0 ? ` (${result.skipped} skipped)` : ''}`);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'CSV import failed');
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
          <Button variant="secondary" onClick={() => void exportAs('yaml')} disabled={busy()}>
            Export YAML
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

        <div class="space-y-3">
          <div>
            <p class="text-sm mb-2" style={{ color: 'var(--color-text-secondary)' }}>
              Restore a full project from a previously exported JSON or YAML snapshot (creates a new project).
            </p>
            <Button onClick={() => importJsonInput?.click()} disabled={busy()}>
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
          </div>

          <div>
            <p class="text-sm mb-2" style={{ color: 'var(--color-text-secondary)' }}>
              Add items to this project from a CSV file. Required column: <code>title</code>.
              Optional: <code>description</code>, <code>type</code>, <code>status</code>, <code>priority</code>, <code>assignee</code>, <code>estimate</code>.
            </p>
            <Button variant="secondary" onClick={() => importCsvInput?.click()} disabled={busy()}>
              Import items from CSV…
            </Button>
            <input
              ref={importCsvInput}
              type="file"
              accept="text/csv,.csv"
              class="hidden"
              onChange={(e) => {
                const f = e.currentTarget.files?.[0];
                e.currentTarget.value = '';
                if (f) void importFromCsv(f);
              }}
            />
          </div>
        </div>
      </section>
    </div>
  );
};

export default DataPanel;
