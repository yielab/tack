import { type Component, createSignal, createEffect, For } from 'solid-js';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { Button } from '../../../shared/ui';
import { useProject } from '../../../shared/state/projectContext';
import { VOCAB_KEYS, resolveLabel, getItemTypeList } from '../../../shared/vocab/vocab';

/** The 16-key vocabulary editor with a live preview. Saves `vocabulary` only. */
const VocabularyPanel: Component = () => {
  const { project, projectId, refetch } = useProject();
  const [edits, setEdits] = createSignal<Record<string, string>>({});
  const [saving, setSaving] = createSignal(false);
  const [dirty, setDirty] = createSignal(false);

  createEffect(() => {
    const p = project();
    if (p) {
      setEdits({ ...p.vocabulary });
      setDirty(false);
    }
  });

  const setKey = (key: string, value: string) => {
    setEdits((prev) => ({ ...prev, [key]: value }));
    setDirty(true);
  };

  const save = async () => {
    const id = projectId();
    if (!id) return;
    const vocab: Record<string, string> = {};
    for (const [k, v] of Object.entries(edits())) if (v.trim()) vocab[k] = v.trim();
    setSaving(true);
    try {
      await api.projects.update(id, { vocabulary: vocab });
      setDirty(false);
      await refetch();
      toast.success('Vocabulary saved');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to save');
    } finally {
      setSaving(false);
    }
  };

  const previewTypes = () => getItemTypeList(edits());

  return (
    <div class="space-y-4">
      <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
        Rename any term to match your domain. Blank fields fall back to the default label.
      </p>

      <div class="overflow-hidden rounded-xl border" style={{ 'border-color': 'var(--color-border-light)' }}>
        <table class="w-full text-sm">
          <thead>
            <tr style={{ 'background-color': 'var(--color-bg-subtle)' }}>
              <th class="w-1/3 px-4 py-3 text-left font-semibold" style={{ color: 'var(--color-text-secondary)' }}>Default</th>
              <th class="px-4 py-3 text-left font-semibold" style={{ color: 'var(--color-text-secondary)' }}>Custom label</th>
            </tr>
          </thead>
          <tbody>
            <For each={VOCAB_KEYS}>
              {(key) => {
                const def = resolveLabel(undefined, key);
                return (
                  <tr class="border-t" style={{ 'border-color': 'var(--color-border-light)' }}>
                    <td class="px-4 py-2.5 font-medium" style={{ color: 'var(--color-text-secondary)' }}>{def}</td>
                    <td class="px-4 py-2">
                      <input
                        type="text"
                        value={edits()[key] ?? ''}
                        placeholder={def}
                        onInput={(e) => setKey(key, e.currentTarget.value)}
                        class="w-full rounded-lg border px-3 py-1.5 text-sm focus:outline-none focus-visible:ring-2"
                        style={{
                          'background-color': 'var(--color-bg-base)',
                          'border-color': 'var(--color-border-medium)',
                          color: 'var(--color-text-primary)',
                          '--tw-ring-color': 'var(--color-focus-ring)',
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

      <div class="rounded-xl border p-4" style={{ 'border-color': 'var(--color-border-light)', 'background-color': 'var(--color-bg-subtle)' }}>
        <p class="mb-3 text-xs font-semibold uppercase tracking-wider" style={{ color: 'var(--color-text-tertiary)' }}>
          Preview — item type labels
        </p>
        <div class="flex flex-wrap gap-2">
          <For each={previewTypes()}>
            {(t) => (
              <span class="inline-flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium"
                style={{ 'background-color': 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)', color: 'var(--color-text-primary)' }}>
                {t.emoji} {t.label}
              </span>
            )}
          </For>
        </div>
      </div>

      <Button onClick={() => void save()} loading={saving()} disabled={saving() || !dirty()}>
        Save vocabulary
      </Button>
    </div>
  );
};

export default VocabularyPanel;
