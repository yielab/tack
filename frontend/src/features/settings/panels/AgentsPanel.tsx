import { type Component, createSignal, createEffect, Show } from 'solid-js';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { Button, Field, FieldShell } from '../../../shared/ui';
import { useProject } from '../../../shared/state/projectContext';
import type { ProjectModelDefault } from '../../../shared/types';

type Mode = 'unset' | 'auto' | 'explicit';

/**
 * Project-level agent settings: the default model an execution resolves to
 * when neither the request nor its agent profile names one explicitly (the
 * `Project` tier in `tack-orch::model_policy`, between the agent-profile and
 * fleet defaults — falls through to the fleet's default, then to
 * auto-select, when left unconfigured here).
 *
 * No live model catalog exists yet to populate a picker from — the operator
 * types the provider and model id exactly as the harness expects them, the
 * same honest-gap posture `AgentProfilesPanel`/`FleetsPanel` take for their
 * own opaque fields. `UpdateProject` has no way to clear a field once set
 * (true of every optional project setting today, not specific to this one),
 * so there is no path back to "unconfigured" once a default is saved.
 */
const AgentsPanel: Component = () => {
  const { project, projectId, refetch } = useProject();
  const [mode, setMode] = createSignal<Mode>('unset');
  const [provider, setProvider] = createSignal('');
  const [modelId, setModelId] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  createEffect(() => {
    const defaultModel = project()?.default_model;
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

  const save = async () => {
    const id = projectId();
    if (!id) return;
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
      await api.projects.update(id, { default_model });
      await refetch();
      toast.success('Saved');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to save');
    } finally {
      setSaving(false);
    }
  };

  // Radio dot drawn over the native input (kept for semantics + keyboard):
  // an accent-filled ring when checked, a plain outline otherwise.
  const radioStyle = (checked: boolean) => ({
    'border-color': checked ? 'var(--color-primary-600)' : 'var(--color-border-strong)',
    'background-color': checked ? 'var(--color-primary-600)' : 'var(--color-bg-base)',
    'box-shadow': checked ? 'inset 0 0 0 4px var(--color-bg-panel)' : 'none',
    '--tw-ring-color': 'var(--color-focus-ring)',
  });
  const RADIO =
    'h-[18px] w-[18px] shrink-0 cursor-pointer appearance-none rounded-full border-2 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1';

  return (
    <div class="flex max-w-xl flex-col gap-2.5 rounded-[26px] bg-panel p-[18px]">
      <div>
        <h3 class="text-[19px] leading-tight" style={{ color: 'var(--color-text-primary)' }}>
          Default model
        </h3>
        <p class="mt-1.5 text-[12.5px]" style={{ color: 'var(--color-text-secondary)' }}>
          Used when an execution request and its agent profile both leave the model
          unspecified. There's no live model catalog to pick from yet — type the provider
          and model id exactly as the harness expects them.
        </p>
      </div>

      <FieldShell label="Mode">
        <div class="flex gap-4 text-sm" style={{ color: 'var(--color-text-primary)' }}>
          <label class="flex cursor-pointer items-center gap-2">
            <input
              type="radio"
              name="default-model-mode"
              checked={mode() === 'auto'}
              onChange={() => setMode('auto')}
              class={RADIO}
              style={radioStyle(mode() === 'auto')}
            />
            Auto-select
          </label>
          <label class="flex cursor-pointer items-center gap-2">
            <input
              type="radio"
              name="default-model-mode"
              checked={mode() === 'explicit'}
              onChange={() => setMode('explicit')}
              class={RADIO}
              style={radioStyle(mode() === 'explicit')}
            />
            Specific model
          </label>
        </div>
      </FieldShell>

      <Show when={mode() === 'explicit'}>
        <div class="grid gap-2 sm:grid-cols-2 [&_input]:font-mono [&_input]:text-[12.5px]">
          <Field
            label="Provider"
            placeholder="openai"
            value={provider()}
            onInput={(e) => setProvider(e.currentTarget.value)}
          />
          <Field
            label="Model ID"
            placeholder="opaque/model-alpha"
            value={modelId()}
            onInput={(e) => setModelId(e.currentTarget.value)}
          />
        </div>
      </Show>

      <Show when={mode() === 'unset'}>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          No default configured for this project yet — falls through to the fleet's
          default, then to auto-select. Choose a mode above to set one.
        </p>
      </Show>

      <Button
        class="self-start"
        onClick={() => void save()}
        loading={saving()}
        disabled={saving() || mode() === 'unset'}
      >
        Save
      </Button>
    </div>
  );
};

export default AgentsPanel;
