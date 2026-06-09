import { type Component, createSignal, createEffect, Show } from 'solid-js';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { Button, Field, FieldShell, Badge } from '../../../shared/ui';
import { useProject } from '../../../shared/state/projectContext';

/** General project settings: name, description, type (read-only), archive. */
const GeneralPanel: Component = () => {
  const { project, projectId, refetch } = useProject();
  const [name, setName] = createSignal('');
  const [description, setDescription] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  createEffect(() => {
    const p = project();
    if (p) {
      setName(p.name);
      setDescription(p.description ?? '');
    }
  });

  const save = async () => {
    const id = projectId();
    if (!id) return;
    setSaving(true);
    try {
      await api.projects.update(id, {
        name: name().trim(),
        description: description().trim() || undefined,
      });
      await refetch();
      toast.success('Saved');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to save');
    } finally {
      setSaving(false);
    }
  };

  const setArchived = async (archived: boolean) => {
    const id = projectId();
    if (!id) return;
    try {
      await api.projects.update(id, { archived });
      await refetch();
      toast.success(archived ? 'Project archived' : 'Project restored');
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to update');
    }
  };

  return (
    <div class="max-w-xl space-y-4">
      <Field label="Name" value={name()} onInput={(e) => setName(e.currentTarget.value)} />

      <FieldShell label="Description" for="project-desc">
        <textarea
          id="project-desc"
          rows={3}
          value={description()}
          onInput={(e) => setDescription(e.currentTarget.value)}
          class="w-full resize-none rounded-lg border px-3 py-2 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1"
          style={{
            'background-color': 'var(--color-bg-base)',
            color: 'var(--color-text-primary)',
            'border-color': 'var(--color-border-medium)',
            '--tw-ring-color': 'var(--color-focus-ring)',
          }}
        />
      </FieldShell>

      <FieldShell label="Type">
        <div>
          <Badge>{project()?.project_type ?? '—'}</Badge>
        </div>
      </FieldShell>

      <div class="flex items-center gap-2 pt-2">
        <Button onClick={() => void save()} loading={saving()} disabled={saving()}>
          Save
        </Button>
        <Show
          when={project()?.archived}
          fallback={
            <Button variant="secondary" onClick={() => void setArchived(true)}>
              Archive project
            </Button>
          }
        >
          <Button variant="secondary" onClick={() => void setArchived(false)}>
            Restore project
          </Button>
        </Show>
      </div>
    </div>
  );
};

export default GeneralPanel;
