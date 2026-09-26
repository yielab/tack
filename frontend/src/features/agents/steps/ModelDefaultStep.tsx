import { type Component, For, Show, createEffect, createSignal } from 'solid-js';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { Button, Field, FieldShell } from '../../../shared/ui';
import type { Project, ProjectModelDefault } from '../../../shared/types';
import type { RunnerSummary } from '../../../shared/execution/api';
import { anyHarnessAttestsPassthrough, unionModelCombinations } from '../runnerObservations';

export interface ModelDefaultStepProps {
  project: Project | null | undefined;
  runners: readonly RunnerSummary[];
  onSaved: () => void;
}

type Mode = 'unset' | 'auto' | 'explicit';

/** Native radios, tinted with the accent so they read as the design's dot. */
const RADIO_STYLE = { 'accent-color': 'var(--color-primary-600)' } as const;

/**
 * Step 4 — a project's default model, read and written directly against
 * `Project.default_model` (the same field `features/settings/panels/
 * AgentsPanel.tsx`'s Settings tab already edits). Not a re-mount of that
 * panel: it reads via `useProject()` (route `:id`-keyed), and this page is
 * not nested under a project route, so this step calls the same
 * `api.projects.update` directly against the project this page's own
 * picker selected.
 */
const ModelDefaultStep: Component<ModelDefaultStepProps> = (props) => {
  const [mode, setMode] = createSignal<Mode>('unset');
  const [provider, setProvider] = createSignal('');
  const [modelId, setModelId] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  createEffect(() => {
    const defaultModel = props.project?.default_model;
    if (!defaultModel) {
      setMode('unset');
      setProvider('');
      setModelId('');
    } else if (defaultModel.kind === 'auto') {
      setMode('auto');
    } else {
      setMode('explicit');
      setProvider(defaultModel.provider);
      setModelId(defaultModel.model_id);
    }
  });

  const combinations = () => unionModelCombinations(props.runners);
  const passthroughAvailable = () => anyHarnessAttestsPassthrough(props.runners);

  const save = async () => {
    const project = props.project;
    if (!project) return;
    let default_model: ProjectModelDefault;
    if (mode() === 'auto') {
      default_model = { kind: 'auto' };
    } else if (mode() === 'explicit') {
      if (!provider().trim() || !modelId().trim()) {
        toast.error('Provider and model id are both required');
        return;
      }
      default_model = { kind: 'explicit', provider: provider().trim(), model_id: modelId().trim() };
    } else {
      return;
    }
    setSaving(true);
    try {
      await api.projects.update(project.id, { default_model });
      toast.success('Saved');
      props.onSaved();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to save');
    } finally {
      setSaving(false);
    }
  };

  return (
    <section class="flex flex-col gap-2.5 rounded-[28px] bg-panel px-[22px] py-5">
      <div>
        <h2 class="text-[20px] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
          Default model
        </h2>
        <p class="text-[13px]" style={{ color: 'var(--color-text-secondary)' }}>
          Used when a run leaves the model unspecified.
        </p>
      </div>

      <Show
        when={props.project}
        fallback={
          <p class="text-[13px]" style={{ color: 'var(--color-text-tertiary)' }}>
            Choose a project above to set its default model.
          </p>
        }
      >
        <Show when={combinations().length > 0}>
          <ul class="flex flex-wrap gap-1.5 text-[12.5px]" style={{ color: 'var(--color-text-secondary)' }}>
            <For each={combinations()}>
              {(combo) => (
                <li class="rounded-full bg-app px-3 py-1 font-mono">
                  {combo.model_provider} — {combo.model_ids.join(', ')}
                </li>
              )}
            </For>
          </ul>
        </Show>

        <FieldShell label="Mode">
          <div class="flex flex-wrap gap-[18px] text-sm" style={{ color: 'var(--color-text-primary)' }}>
            <label class="flex cursor-pointer items-center gap-2">
              <input type="radio" class="h-4 w-4" style={RADIO_STYLE} name="agents-default-model-mode" checked={mode() === 'auto'} onChange={() => setMode('auto')} />
              Auto
            </label>
            <Show when={passthroughAvailable()}>
              <label class="flex cursor-pointer items-center gap-2">
                <input type="radio" class="h-4 w-4" style={RADIO_STYLE} name="agents-default-model-mode" checked={mode() === 'explicit'} onChange={() => setMode('explicit')} />
                Type a model id
              </label>
            </Show>
          </div>
        </FieldShell>

        <Show when={mode() === 'explicit'}>
          <Field label="Provider" placeholder="openai" value={provider()} onInput={(e) => setProvider(e.currentTarget.value)} />
          <Field label="Model ID" placeholder="opaque/model-alpha" value={modelId()} onInput={(e) => setModelId(e.currentTarget.value)} />
        </Show>

        <Show when={mode() === 'unset'}>
          <p class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>
            No default set yet — falls through to the next configured default, then auto-select.
          </p>
        </Show>

        <Button class="self-start" onClick={() => void save()} loading={saving()} disabled={saving() || mode() === 'unset'}>
          Save
        </Button>
      </Show>
    </section>
  );
};

export default ModelDefaultStep;
