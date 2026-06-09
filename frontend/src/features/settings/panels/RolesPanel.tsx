import { type Component, createResource, createSignal, For, Show } from 'solid-js';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { Button, Field, EmptyState } from '../../../shared/ui';
import { useProject } from '../../../shared/state/projectContext';

/** Role / specialty CRUD for a project. */
const RolesPanel: Component = () => {
  const { projectId } = useProject();
  const [roles, { refetch }] = createResource(
    () => projectId(),
    (id) => (id ? api.roles.list(id) : []),
  );
  const [name, setName] = createSignal('');
  const [color, setColor] = createSignal('#7c3aed');
  const [busy, setBusy] = createSignal(false);

  const add = async (e: Event) => {
    e.preventDefault();
    const id = projectId();
    if (!id || !name().trim()) return;
    setBusy(true);
    try {
      await api.roles.create(id, { name: name().trim(), color: color() });
      setName('');
      await refetch();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to add role');
    } finally {
      setBusy(false);
    }
  };

  const remove = async (roleId: string) => {
    try {
      await api.roles.remove(roleId);
      await refetch();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to delete role');
    }
  };

  return (
    <div class="max-w-xl space-y-4">
      <Show
        when={(roles() ?? []).length > 0}
        fallback={<EmptyState title="No roles yet" description="Add a role below." />}
      >
        <ul class="space-y-1">
          <For each={roles()}>
            {(role) => (
              <li
                class="flex items-center justify-between rounded-md border px-3 py-2"
                style={{
                  'background-color': 'var(--color-bg-base)',
                  'border-color': 'var(--color-border-light)',
                }}
              >
                <span class="flex items-center gap-2 text-sm" style={{ color: 'var(--color-text-primary)' }}>
                  <span
                    class="inline-block h-3 w-3 rounded-full"
                    style={{ 'background-color': role.color }}
                  />
                  {role.name}
                </span>
                <Button size="sm" variant="ghost" onClick={() => void remove(role.id)}>
                  Delete
                </Button>
              </li>
            )}
          </For>
        </ul>
      </Show>

      <form class="flex items-end gap-2" onSubmit={add}>
        <Field
          class="flex-1"
          label="New role"
          value={name()}
          onInput={(e) => setName(e.currentTarget.value)}
          placeholder="e.g. Designer"
        />
        <input
          type="color"
          aria-label="Role color"
          value={color()}
          onInput={(e) => setColor(e.currentTarget.value)}
          class="h-10 w-12 rounded border"
          style={{ 'border-color': 'var(--color-border-medium)' }}
        />
        <Button type="submit" loading={busy()} disabled={busy() || !name().trim()}>
          Add
        </Button>
      </form>
    </div>
  );
};

export default RolesPanel;
