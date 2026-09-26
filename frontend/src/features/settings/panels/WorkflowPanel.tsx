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

  // Row controls: pills on the page ground, sitting inside a surface-filled row.
  const control = {
    'background-color': 'var(--color-bg-app)',
    'border-color': 'var(--color-border-light)',
    color: 'var(--color-text-primary)',
    '--tw-ring-color': 'var(--color-focus-ring)',
  };

  return (
    <div class="flex max-w-[880px] flex-col gap-2.5">
      <p class="text-[13px]" style={{ color: 'var(--color-text-secondary)' }}>
        Define the columns and WIP limits for this project's board.
      </p>

      <div class="flex flex-col gap-2">
        <For each={rows()}>
          {(row) => (
            <div class="flex flex-wrap items-center gap-2 rounded-[22px] bg-panel px-2.5 py-1.5 sm:flex-nowrap sm:rounded-full">
              <input
                type="text"
                value={row.name}
                placeholder="Status name"
                onInput={(e) => setField(row.id, 'name', e.currentTarget.value)}
                class="min-h-8 min-w-0 flex-1 rounded-full border px-3.5 py-1 text-sm focus:outline-none focus-visible:ring-2"
                style={control}
              />
              <select
                value={row.category}
                onChange={(e) => setField(row.id, 'category', e.currentTarget.value)}
                class="min-h-8 w-[150px] rounded-full border px-3.5 py-1 text-[13px] focus:outline-none focus-visible:ring-2"
                style={control}
              >
                <option value="todo">To Do</option>
                <option value="in_progress">In Progress</option>
                <option value="done">Done</option>
              </select>
              <label
                class="flex min-h-8 w-[96px] flex-shrink-0 items-center gap-1 rounded-full border px-3.5 text-[13px] focus-within:ring-2"
                style={control}
              >
                <span class="whitespace-nowrap" style={{ color: 'var(--color-text-tertiary)' }}>WIP</span>
                <input
                  type="number"
                  min="1"
                  value={row.wip_limit}
                  placeholder="∞"
                  onInput={(e) => setField(row.id, 'wip_limit', e.currentTarget.value)}
                  class="w-full min-w-0 bg-transparent text-[13px] focus:outline-none"
                  style={{ color: 'var(--color-text-primary)' }}
                />
              </label>
              <button
                onClick={() => removeStatus(row.id)}
                class="flex-shrink-0 rounded-full p-1.5 transition-colors hover:bg-hover focus:outline-none focus-visible:ring-2"
                style={{ color: 'var(--color-text-tertiary)', '--tw-ring-color': 'var(--color-focus-ring)' }}
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
        class="flex items-center justify-center gap-2 rounded-full border-2 border-dashed p-2 text-[13px] font-semibold transition-colors hover:bg-hover focus:outline-none focus-visible:ring-2"
        style={{
          'border-color': 'var(--color-border-light)',
          color: 'var(--color-text-secondary)',
          '--tw-ring-color': 'var(--color-focus-ring)',
        }}
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
