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

  return (
    <div class="max-w-xl space-y-4">
      <div>
        <h3 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
          Default model
        </h3>
        <p class="mt-1 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
          Used when an execution request and its agent profile both leave the model
          unspecified. There's no live model catalog to pick from yet — type the provider
          and model id exactly as the harness expects them.
        </p>
      </div>

      <FieldShell label="Mode">
        <div class="flex gap-4 text-sm" style={{ color: 'var(--color-text-primary)' }}>
          <label class="flex items-center gap-1.5">
            <input
              type="radio"
              name="default-model-mode"
              checked={mode() === 'auto'}
              onChange={() => setMode('auto')}
            />
            Auto-select
          </label>
          <label class="flex items-center gap-1.5">
            <input
              type="radio"
              name="default-model-mode"
              checked={mode() === 'explicit'}
              onChange={() => setMode('explicit')}
            />
            Specific model
          </label>
        </div>
      </FieldShell>

      <Show when={mode() === 'explicit'}>
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
      </Show>

      <Show when={mode() === 'unset'}>
        <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
          No default configured for this project yet — falls through to the fleet's
          default, then to auto-select. Choose a mode above to set one.
        </p>
      </Show>

      <Button onClick={() => void save()} loading={saving()} disabled={saving() || mode() === 'unset'}>
        Save
      </Button>
    </div>
  );
};

export default AgentsPanel;
