import { createSignal, createResource, createEffect, For, Show, type Component } from 'solid-js';
import { useParams } from '@solidjs/router';
import { api } from '../lib/api';
import { toast } from '../lib/toast';
import { VOCAB_KEYS, resolveLabel, getItemTypeList } from '../lib/vocab';
import type { WorkflowStatus } from '../types/api';
import { FiPlus, FiTrash2, FiSave } from 'solid-icons/fi';

// ── Workflow status row (local editing state) ─────────────────────────────────

interface StatusRow {
  id: string; // local key for For-loop stability
  name: string;
  category: 'todo' | 'in_progress' | 'done';
  wip_limit: string; // empty string = no limit
}

function toStatusRow(s: WorkflowStatus, idx: number): StatusRow {
  return {
    id: `${idx}-${s.name}`,
    name: s.name,
    category: s.category,
    wip_limit: s.wip_limit != null ? String(s.wip_limit) : '',
  };
}

// ── Settings page ─────────────────────────────────────────────────────────────

const Settings: Component = () => {
  const params = useParams();
  const projectId = () => params.id;

  const [project, { refetch }] = createResource(
    () => projectId(),
    (id) => api.getProject(id),
  );

  const [vocabEdits, setVocabEdits] = createSignal<Record<string, string>>({});
  const [statusRows, setStatusRows] = createSignal<StatusRow[]>([]);
  const [saving, setSaving] = createSignal(false);
  const [dirty, setDirty] = createSignal(false);

  // Seed local state when project loads (once)
  createEffect(() => {
    const p = project();
    if (!p) return;
    setVocabEdits({ ...p.vocabulary });
    setStatusRows(p.workflow.statuses.map(toStatusRow));
  });

  const markDirty = () => setDirty(true);

  const setVocabKey = (key: string, value: string) => {
    setVocabEdits(prev => ({ ...prev, [key]: value }));
    markDirty();
  };

  const setStatusField = (id: string, field: keyof StatusRow, value: string) => {
    setStatusRows(rows =>
      rows.map(r => (r.id === id ? { ...r, [field]: value } : r)),
    );
    markDirty();
  };

  const addStatus = () => {
    const idx = statusRows().length;
    setStatusRows(rows => [
      ...rows,
      { id: `new-${idx}`, name: 'New Status', category: 'todo', wip_limit: '' },
    ]);
    markDirty();
  };

  const removeStatus = (id: string) => {
    setStatusRows(rows => rows.filter(r => r.id !== id));
    markDirty();
  };

  const handleSave = async () => {
    const id = projectId();
    if (!id || !project()) return;

    const vocab: Record<string, string> = {};
    for (const [k, v] of Object.entries(vocabEdits())) {
      if (v.trim()) vocab[k] = v.trim();
    }

    const statuses: WorkflowStatus[] = statusRows()
      .filter(r => r.name.trim())
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
      await api.updateProject(id, {
        vocabulary: vocab,
        workflow: {
          workflow_type: project()!.workflow.workflow_type,
          statuses,
          transitions: project()!.workflow.transitions,
        },
      });
      setDirty(false);
      await refetch();
      toast.success('Settings saved');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to save settings');
    } finally {
      setSaving(false);
    }
  };

  // Live preview of item type labels using current vocabEdits
  const previewTypes = () => getItemTypeList(vocabEdits());

  return (
    <div class="max-w-3xl mx-auto px-6 py-8 space-y-10">
      <div class="flex items-center justify-between">
        <div>
          <h1 class="text-2xl font-bold text-[var(--color-text-primary)]">Project Settings</h1>
          <Show when={project()}>
            <p class="text-sm text-[var(--color-text-secondary)] mt-1">{project()!.name}</p>
          </Show>
        </div>
        <button
          onClick={handleSave}
          disabled={saving() || !dirty()}
          class="flex items-center gap-2 px-5 py-2.5 bg-violet-600 text-white rounded-lg hover:bg-violet-700 transition-colors disabled:opacity-40 disabled:cursor-not-allowed font-medium"
        >
          <FiSave size={16} />
          {saving() ? 'Saving…' : 'Save'}
        </button>
      </div>

      <Show when={!projectId()}>
        <div class="text-center py-16">
          <p class="text-[var(--color-text-secondary)]">Open a project to configure its vocabulary and workflow.</p>
        </div>
      </Show>

      <Show when={projectId() && project()}>
        {/* ── Vocabulary ───────────────────────────────────────── */}
        <section class="space-y-4">
          <div>
            <h2 class="text-lg font-semibold text-[var(--color-text-primary)]">Vocabulary</h2>
            <p class="text-sm text-[var(--color-text-secondary)] mt-1">
              Rename any term to match your domain. Blank fields fall back to the default label.
            </p>
          </div>

          <div class="rounded-xl border border-[var(--color-border-light)] overflow-hidden">
            <table class="w-full text-sm">
              <thead>
                <tr style={{ "background-color": "var(--color-bg-subtle)" }}>
                  <th class="px-4 py-3 text-left font-semibold text-[var(--color-text-secondary)] w-1/3">Default</th>
                  <th class="px-4 py-3 text-left font-semibold text-[var(--color-text-secondary)]">Custom label</th>
                </tr>
              </thead>
              <tbody>
                <For each={VOCAB_KEYS}>
                  {(key) => {
                    const defaultLabel = resolveLabel(undefined, key);
                    return (
                      <tr class="border-t border-[var(--color-border-light)]">
                        <td class="px-4 py-2.5 text-[var(--color-text-secondary)] font-medium">
                          {defaultLabel}
                        </td>
                        <td class="px-4 py-2">
                          <input
                            type="text"
                            value={vocabEdits()[key] ?? ''}
                            placeholder={defaultLabel}
                            onInput={(e) => setVocabKey(key, e.currentTarget.value)}
                            class="w-full px-3 py-1.5 rounded-lg border text-sm focus:ring-2 focus:ring-violet-500 focus:border-violet-500 transition-all"
                            style={{
                              "background-color": "var(--color-bg-base)",
                              "border-color": "var(--color-border-medium)",
                              color: "var(--color-text-primary)",
                            }}
                          />
                        </td>
                      </tr>
                    );
                  }}
                </For>
              </tbody>
            </table>
          </div>

          {/* Live preview */}
          <div class="rounded-xl border border-[var(--color-border-light)] p-4" style={{ "background-color": "var(--color-bg-subtle)" }}>
            <p class="text-xs font-semibold text-[var(--color-text-tertiary)] uppercase tracking-wider mb-3">Preview — item type labels</p>
            <div class="flex flex-wrap gap-2">
              <For each={previewTypes()}>
                {(t) => (
                  <span class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm font-medium"
                    style={{ "background-color": "var(--color-bg-elevated)", border: "1px solid var(--color-border-light)", color: "var(--color-text-primary)" }}>
                    {t.emoji} {t.label}
                  </span>
                )}
              </For>
              <span class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md text-sm font-medium"
                style={{ "background-color": "var(--color-bg-elevated)", border: "1px solid var(--color-border-light)", color: "var(--color-text-primary)" }}>
                🏃 {resolveLabel(vocabEdits(), 'sprint')}
              </span>
            </div>
          </div>
        </section>

        {/* ── Workflow ──────────────────────────────────────────── */}
        <section class="space-y-4">
          <div>
            <h2 class="text-lg font-semibold text-[var(--color-text-primary)]">Workflow Statuses</h2>
            <p class="text-sm text-[var(--color-text-secondary)] mt-1">
              Define the columns and WIP limits for this project's board.
            </p>
          </div>

          <div class="space-y-2">
            <For each={statusRows()}>
              {(row) => (
                <div class="flex items-center gap-3 p-3 rounded-lg border border-[var(--color-border-light)]"
                  style={{ "background-color": "var(--color-bg-elevated)" }}>
                  {/* Status name */}
                  <input
                    type="text"
                    value={row.name}
                    placeholder="Status name"
                    onInput={(e) => setStatusField(row.id, 'name', e.currentTarget.value)}
                    class="flex-1 px-3 py-1.5 rounded-lg border text-sm focus:ring-2 focus:ring-violet-500 focus:border-violet-500 transition-all"
                    style={{
                      "background-color": "var(--color-bg-base)",
                      "border-color": "var(--color-border-medium)",
                      color: "var(--color-text-primary)",
                    }}
                  />

                  {/* Category */}
                  <select
                    value={row.category}
                    onChange={(e) => setStatusField(row.id, 'category', e.currentTarget.value)}
                    class="px-3 py-1.5 rounded-lg border text-sm focus:ring-2 focus:ring-violet-500 transition-all"
                    style={{
                      "background-color": "var(--color-bg-base)",
                      "border-color": "var(--color-border-medium)",
                      color: "var(--color-text-primary)",
                    }}
                  >
                    <option value="todo">To Do</option>
                    <option value="in_progress">In Progress</option>
                    <option value="done">Done</option>
                  </select>

                  {/* WIP limit */}
                  <div class="flex items-center gap-1.5 flex-shrink-0">
                    <span class="text-xs text-[var(--color-text-tertiary)] whitespace-nowrap">WIP</span>
                    <input
                      type="number"
                      min="1"
                      value={row.wip_limit}
                      placeholder="∞"
                      onInput={(e) => setStatusField(row.id, 'wip_limit', e.currentTarget.value)}
                      class="w-16 px-2 py-1.5 rounded-lg border text-sm text-center focus:ring-2 focus:ring-violet-500 transition-all"
                      style={{
                        "background-color": "var(--color-bg-base)",
                        "border-color": "var(--color-border-medium)",
                        color: "var(--color-text-primary)",
                      }}
                    />
                  </div>

                  <button
                    onClick={() => removeStatus(row.id)}
                    class="flex-shrink-0 p-1.5 rounded-md transition-colors"
                    style={{ color: "var(--color-text-tertiary)" }}
                    onMouseEnter={(e) => { e.currentTarget.style.color = 'var(--color-danger)'; e.currentTarget.style.backgroundColor = 'var(--color-danger-light)'; }}
                    onMouseLeave={(e) => { e.currentTarget.style.color = 'var(--color-text-tertiary)'; e.currentTarget.style.backgroundColor = 'transparent'; }}
                    title="Remove status"
                  >
                    <FiTrash2 size={16} />
                  </button>
                </div>
              )}
            </For>
          </div>

          <button
            onClick={addStatus}
            class="flex items-center gap-2 px-4 py-2 text-sm font-medium rounded-lg border border-dashed transition-colors"
            style={{
              "border-color": "var(--color-border-medium)",
              color: "var(--color-text-secondary)",
            }}
            onMouseEnter={(e) => { e.currentTarget.style.borderColor = 'var(--color-primary-400)'; e.currentTarget.style.color = 'var(--color-primary-600)'; }}
            onMouseLeave={(e) => { e.currentTarget.style.borderColor = 'var(--color-border-medium)'; e.currentTarget.style.color = 'var(--color-text-secondary)'; }}
          >
            <FiPlus size={16} />
            Add Status
          </button>
        </section>
      </Show>
    </div>
  );
};

export default Settings;
