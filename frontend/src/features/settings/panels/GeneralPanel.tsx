import { type Component, createSignal, createEffect, Show } from 'solid-js';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { Button, Field, FieldShell, Badge, Switch } from '../../../shared/ui';
import {
  disableNotifyWhenHidden,
  enableNotifyWhenHidden,
  notifyUnavailableReason,
  notifyWhenHidden,
} from '../../../shared/notify';
import { useProject } from '../../../shared/state/projectContext';

/** General project settings: name, description, type (read-only), archive; and this browser's own choice to be notified while the tab is hidden. */
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

  // The switch follows the click at once and goes back when the browser says no.
  const [notify, setNotify] = createSignal(notifyWhenHidden());
  const toggleNotify = async (on: boolean) => {
    setNotify(on);
    if (!on) return disableNotifyWhenHidden();
    if (!(await enableNotifyWhenHidden())) {
      setNotify(false);
      toast.info('Notifications stay off: the browser did not allow them.');
    }
  };

  return (
    <div class="flex max-w-[560px] flex-col gap-3">
      <Field label="Name" value={name()} onInput={(e) => setName(e.currentTarget.value)} />

      <FieldShell label="Description" for="project-desc">
        <textarea
          id="project-desc"
          rows={3}
          value={description()}
          onInput={(e) => setDescription(e.currentTarget.value)}
          class="min-h-20 w-full resize-none rounded-xl border px-3.5 py-2.5 text-sm transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1"
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
          <Badge tone="primary" class="text-xs!">{project()?.project_type ?? '—'}</Badge>
        </div>
      </FieldShell>

      <FieldShell label="Notifications">
        <Switch
          checked={notify()}
          disabled={notifyUnavailableReason() !== null}
          onChange={(on) => void toggleNotify(on)}
        >
          <span>
            Notify me when this tab is hidden
            <Show when={notifyUnavailableReason()}>
              {(reason) => (
                <span class="block text-xs" style={{ color: 'var(--color-text-secondary)' }}>{reason()}</span>
              )}
            </Show>
          </span>
        </Switch>
      </FieldShell>

      <div class="mt-1.5 flex items-center gap-2.5">
        <Button onClick={() => void save()} loading={saving()} disabled={saving()}>
          Save
        </Button>
        <Show
          when={project()?.archived}
          fallback={
            <Button
              variant="secondary"
              class="ml-auto"
              style={{ color: 'var(--color-danger-600)' }}
              onClick={() => void setArchived(true)}
            >
              Archive project
            </Button>
          }
        >
          <Button variant="secondary" class="ml-auto" onClick={() => void setArchived(false)}>
            Restore project
          </Button>
        </Show>
      </div>
    </div>
  );
};

export default GeneralPanel;
