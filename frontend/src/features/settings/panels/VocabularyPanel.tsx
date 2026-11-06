import { type Component, createSignal, createEffect, For } from 'solid-js';
import { A, useParams } from '@solidjs/router';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { Button } from '../../../shared/ui';
import { useProject } from '../../../shared/state/projectContext';
import { VOCAB_KEYS, resolveLabel } from '../../../shared/vocab/vocab';

/** The vocabulary editor: the words that are not item types (the types are the project's own catalog). Saves `vocabulary` only. */
const VocabularyPanel: Component = () => {
  const { project, projectId, refetch } = useProject();
  const params = useParams();
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
    <div class="grid max-w-[880px] gap-[18px]">
      <div class="flex flex-col gap-1.5">
        <p class="mb-1.5 text-small" style={{ color: 'var(--color-text-secondary)' }}>
          Rename any term to match your domain. Blank fields fall back to the default label. The types an item
          can be (an epic, a task) are named under{' '}
          <A href={`/projects/${params.id}/settings?tab=types`} class="font-semibold underline" style={{ color: 'var(--color-accent-ink)' }}>
            Types
          </A>
          .
        </p>

        <table class="w-full border-separate border-spacing-y-1.5 text-small">
          <thead>
            <tr>
              <th
                class="w-[130px] pr-3 text-left text-micro font-bold uppercase tracking-[.08em]"
                style={{ color: 'var(--color-text-tertiary)' }}
              >
                Default
              </th>
              <th
                class="text-left text-micro font-bold uppercase tracking-[.08em]"
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
                        aria-label={`Your word for “${def}”`}
                        placeholder={def}
                        onInput={(e) => setKey(key, e.currentTarget.value)}
                        class="min-h-[30px] w-full rounded-full border px-3.5 py-1 text-small focus:outline-none focus-visible:ring-2"
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
