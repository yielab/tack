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
    <div class="flex max-w-[440px] flex-col gap-2.5">
      <Show
        when={(roles() ?? []).length > 0}
        fallback={<EmptyState title="No roles yet" description="Add a role below." />}
      >
        <ul class="flex flex-col gap-2.5">
          <For each={roles()}>
            {(role) => (
              <li
                class="flex items-center gap-2.5 rounded-full bg-panel py-2 pl-3.5 pr-2 text-sm"
                style={{ color: 'var(--color-text-primary)' }}
              >
                <span
                  class="inline-block h-3 w-3 shrink-0 rounded-full"
                  style={{ 'background-color': role.color }}
                />
                {role.name}
                <Button
                  size="sm"
                  variant="ghost"
                  class="ml-auto"
                  onClick={() => void remove(role.id)}
                >
                  Delete
                </Button>
              </li>
            )}
          </For>
        </ul>
      </Show>

      <form class="flex flex-col gap-2.5 rounded-[26px] bg-panel px-4 py-3.5" onSubmit={add}>
        <Field
          label="New role"
          value={name()}
          onInput={(e) => setName(e.currentTarget.value)}
          placeholder="e.g. Designer"
        />
        <div class="flex items-center gap-3">
          <input
            type="color"
            aria-label="Role color"
            value={color()}
            onInput={(e) => setColor(e.currentTarget.value)}
            class="h-[30px] w-[30px] shrink-0 cursor-pointer appearance-none overflow-hidden rounded-full border-0 bg-transparent p-0 focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 [&::-moz-color-swatch]:rounded-full [&::-moz-color-swatch]:border-0 [&::-webkit-color-swatch-wrapper]:p-0 [&::-webkit-color-swatch]:rounded-full [&::-webkit-color-swatch]:border-0"
            style={{
              'box-shadow': 'inset 0 0 0 1px var(--color-border-medium)',
              '--tw-ring-color': 'var(--color-focus-ring)',
            }}
          />
          <Button type="submit" loading={busy()} disabled={busy() || !name().trim()}>
            Add
          </Button>
        </div>
      </form>
    </div>
  );
};

export default RolesPanel;
