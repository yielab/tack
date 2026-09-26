import { type Component, createSignal, createEffect, For } from 'solid-js';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { Button, TypeBadge } from '../../../shared/ui';
import type { ItemType } from '../../../shared/types';
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

  // A customised term reads as an accent pill; a blank one shows its default
  // as a muted placeholder on the surface fill.
  const inputStyle = (custom: boolean) => ({
    'background-color': custom ? 'var(--color-accent-soft)' : 'var(--color-bg-panel)',
    'border-color': custom ? 'var(--color-accent-line)' : 'transparent',
    color: custom ? 'var(--color-accent-ink)' : 'var(--color-text-primary)',
    'font-weight': custom ? 600 : 400,
    '--tw-ring-color': 'var(--color-focus-ring)',
  });

  return (
    <div class="grid max-w-[880px] gap-[18px] md:grid-cols-[1fr_280px]">
      <div class="flex flex-col gap-1.5">
        <p class="mb-1.5 text-[13px]" style={{ color: 'var(--color-text-secondary)' }}>
          Rename any term to match your domain. Blank fields fall back to the default label.
        </p>

        <table class="w-full border-separate border-spacing-y-1.5 text-[13px]">
          <thead>
            <tr>
              <th
                class="w-[130px] pr-3 text-left text-[11px] font-bold uppercase tracking-[.08em]"
                style={{ color: 'var(--color-text-tertiary)' }}
              >
                Default
              </th>
              <th
                class="text-left text-[11px] font-bold uppercase tracking-[.08em]"
                style={{ color: 'var(--color-text-tertiary)' }}
              >
                Custom label
              </th>
            </tr>
          </thead>
          <tbody>
            <For each={VOCAB_KEYS}>
              {(key) => {
                const def = resolveLabel(undefined, key);
                return (
                  <tr>
                    <td class="pr-3" style={{ color: 'var(--color-text-primary)' }}>{def}</td>
                    <td>
                      <input
                        type="text"
                        value={edits()[key] ?? ''}
                        placeholder={def}
                        onInput={(e) => setKey(key, e.currentTarget.value)}
                        class="min-h-[30px] w-full rounded-full border px-3.5 py-1 text-[13px] focus:outline-none focus-visible:ring-2"
                        style={inputStyle(!!(edits()[key] ?? '').trim())}
                      />
                    </td>
                  </tr>
                );
              }}
            </For>
          </tbody>
        </table>
      </div>

      <div class="flex flex-col gap-3">
        <div class="flex flex-col gap-2.5 rounded-[26px] bg-panel p-4">
          <p
            class="text-[10.5px] font-bold uppercase tracking-[.1em]"
            style={{ color: 'var(--color-text-tertiary)' }}
          >
            Preview — item type labels
          </p>
          <div class="flex flex-wrap gap-1.5">
            <For each={previewTypes()}>
              {(t) => (
                <TypeBadge
                  type={t.value as ItemType}
                  label={t.label}
                  style={{ 'font-size': '12px', padding: '3px 10px' }}
                />
              )}
            </For>
          </div>
        </div>

        <Button
          class="self-start"
          onClick={() => void save()}
          loading={saving()}
          disabled={saving() || !dirty()}
        >
          Save vocabulary
        </Button>
      </div>
    </div>
  );
};

export default VocabularyPanel;
