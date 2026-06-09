import { type Component, createSignal, createEffect, For } from 'solid-js';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { Button } from '../../../shared/ui';
import { useProject } from '../../../shared/state/projectContext';
import type { WorkflowStatus } from '../../../shared/types';
import { FiPlus, FiTrash2 } from 'solid-icons/fi';

interface StatusRow {
  id: string;
  name: string;
  category: 'todo' | 'in_progress' | 'done';
  wip_limit: string;
}

const toRow = (s: WorkflowStatus, i: number): StatusRow => ({
  id: `${i}-${s.name}`,
  name: s.name,
  category: s.category,
  wip_limit: s.wip_limit != null ? String(s.wip_limit) : '',
});

/** Workflow status-column editor. Saves `workflow` only. */
const WorkflowPanel: Component = () => {
  const { project, projectId, refetch } = useProject();
  const [rows, setRows] = createSignal<StatusRow[]>([]);
  const [saving, setSaving] = createSignal(false);
  const [dirty, setDirty] = createSignal(false);

  createEffect(() => {
    const p = project();
    if (p) {
      setRows(p.workflow.statuses.map(toRow));
      setDirty(false);
    }
  });

  const setField = (id: string, field: keyof StatusRow, value: string) => {
    setRows((rs) => rs.map((r) => (r.id === id ? { ...r, [field]: value } : r)));
    setDirty(true);
  };
  const addStatus = () => {
    setRows((rs) => [...rs, { id: `new-${rs.length}`, name: 'New Status', category: 'todo', wip_limit: '' }]);
    setDirty(true);
  };
  const removeStatus = (id: string) => {
    setRows((rs) => rs.filter((r) => r.id !== id));
    setDirty(true);
  };

  const save = async () => {
    const id = projectId();
    const p = project();
    if (!id || !p) return;
    const statuses: WorkflowStatus[] = rows()
      .filter((r) => r.name.trim())
      .map((r, idx) => ({
        name: r.name.trim(),
        category: r.category,
        wip_limit: r.wip_limit !== '' ? parseInt(r.wip_limit, 10) : undefined,
        order: idx,
      }));
    if (statuses.length === 0) {
      toast.error('Workflow must have at least one status');
      return;
    }
    setSaving(true);
    try {
      await api.projects.update(id, {
        workflow: { workflow_type: p.workflow.workflow_type, statuses, transitions: p.workflow.transitions },
      });
      setDirty(false);
      await refetch();
      toast.success('Workflow saved');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to save');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="space-y-3">
      <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
        Define the columns and WIP limits for this project's board.
      </p>

      <div class="space-y-2">
        <For each={rows()}>
          {(row) => (
            <div class="flex items-center gap-3 rounded-lg border p-3" style={{ 'border-color': 'var(--color-border-light)', 'background-color': 'var(--color-bg-elevated)' }}>
              <input
                type="text"
                value={row.name}
                placeholder="Status name"
                onInput={(e) => setField(row.id, 'name', e.currentTarget.value)}
                class="flex-1 rounded-lg border px-3 py-1.5 text-sm focus:outline-none focus-visible:ring-2"
                style={{ 'background-color': 'var(--color-bg-base)', 'border-color': 'var(--color-border-medium)', color: 'var(--color-text-primary)', '--tw-ring-color': 'var(--color-focus-ring)' }}
              />
              <select
                value={row.category}
                onChange={(e) => setField(row.id, 'category', e.currentTarget.value)}
                class="rounded-lg border px-3 py-1.5 text-sm focus:outline-none focus-visible:ring-2"
                style={{ 'background-color': 'var(--color-bg-base)', 'border-color': 'var(--color-border-medium)', color: 'var(--color-text-primary)', '--tw-ring-color': 'var(--color-focus-ring)' }}
              >
                <option value="todo">To Do</option>
                <option value="in_progress">In Progress</option>
                <option value="done">Done</option>
              </select>
              <div class="flex flex-shrink-0 items-center gap-1.5">
                <span class="whitespace-nowrap text-xs" style={{ color: 'var(--color-text-tertiary)' }}>WIP</span>
                <input
                  type="number"
                  min="1"
                  value={row.wip_limit}
                  placeholder="∞"
                  onInput={(e) => setField(row.id, 'wip_limit', e.currentTarget.value)}
                  class="w-16 rounded-lg border px-2 py-1.5 text-center text-sm focus:outline-none focus-visible:ring-2"
                  style={{ 'background-color': 'var(--color-bg-base)', 'border-color': 'var(--color-border-medium)', color: 'var(--color-text-primary)', '--tw-ring-color': 'var(--color-focus-ring)' }}
                />
              </div>
              <button
                onClick={() => removeStatus(row.id)}
                class="flex-shrink-0 rounded-md p-1.5"
                style={{ color: 'var(--color-text-tertiary)' }}
                title="Remove status"
                aria-label="Remove status"
              >
                <FiTrash2 size={16} />
              </button>
            </div>
          )}
        </For>
      </div>

      <button
        onClick={addStatus}
        class="flex items-center gap-2 rounded-lg border border-dashed px-4 py-2 text-sm font-medium"
        style={{ 'border-color': 'var(--color-border-medium)', color: 'var(--color-text-secondary)' }}
      >
        <FiPlus size={16} /> Add Status
      </button>

      <div class="pt-2">
        <Button onClick={() => void save()} loading={saving()} disabled={saving() || !dirty()}>
          Save workflow
        </Button>
      </div>
    </div>
  );
};

export default WorkflowPanel;
