import { type Component, createResource, createSignal, For, Show } from 'solid-js';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { FieldShell, Select, EmptyState } from '../../../shared/ui';
import type { Item, CustomField } from '../../../shared/types';

export interface FieldsTabProps {
  item: Item;
}

const controlClass =
  'w-full rounded-lg border px-3 py-2 text-sm transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1';
const controlStyle = {
  'background-color': 'var(--color-bg-base)',
  color: 'var(--color-text-primary)',
  'border-color': 'var(--color-border-medium)',
  '--tw-ring-color': 'var(--color-focus-ring)',
} as const;

/** Custom field values + role assignment for an item. */
const FieldsTab: Component<FieldsTabProps> = (props) => {
  const projectId = () => props.item.project_id;

  const [defs] = createResource(projectId, (id) => api.customFields.list(id));
  const [values, { refetch: refetchValues }] = createResource(
    () => props.item.id,
    (id) => api.customFields.listValues(id),
  );
  const [roles] = createResource(projectId, (id) => api.roles.list(id));

  const valueFor = (fieldId: string): unknown =>
    values()?.find((v) => v.field_id === fieldId)?.value;

  const setValue = async (fieldId: string, value: unknown) => {
    try {
      await api.customFields.setValue(props.item.id, fieldId, value);
      await refetchValues();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to save value');
    }
  };
  const clearValue = async (fieldId: string) => {
    try {
      await api.customFields.clearValue(props.item.id, fieldId);
      await refetchValues();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to clear value');
    }
  };

  // Commit a text-like value: empty clears, otherwise set.
  const commit = (fieldId: string, raw: string, coerce: (s: string) => unknown) =>
    raw === '' ? void clearValue(fieldId) : void setValue(fieldId, coerce(raw));

  const renderInput = (def: CustomField) => {
    const cur = () => valueFor(def.id);
    const str = () => (cur() == null ? '' : String(cur()));
    switch (def.field_type) {
      case 'long_text':
        return (
          <textarea
            rows={3}
            value={str()}
            onChange={(e) => commit(def.id, e.currentTarget.value, (s) => s)}
            class={controlClass + ' resize-none'}
            style={controlStyle}
          />
        );
      case 'number':
        return (
          <input
            type="number"
            value={str()}
            onChange={(e) => commit(def.id, e.currentTarget.value, Number)}
            class={controlClass}
            style={controlStyle}
          />
        );
      case 'date':
        return (
          <input
            type="date"
            value={str()}
            onChange={(e) => commit(def.id, e.currentTarget.value, (s) => s)}
            class={controlClass}
            style={controlStyle}
          />
        );
      case 'boolean':
        return (
          <input
            type="checkbox"
            checked={cur() === true}
            onChange={(e) => void setValue(def.id, e.currentTarget.checked)}
            class="h-4 w-4 rounded"
          />
        );
      case 'select':
        return (
          <Select
            value={str()}
            onChange={(e) => commit(def.id, e.currentTarget.value, (s) => s)}
          >
            <option value="">— None —</option>
            <For each={def.options ?? []}>{(o) => <option value={o}>{o}</option>}</For>
          </Select>
        );
      default: // text, url, email
        return (
          <input
            type={def.field_type === 'email' ? 'email' : def.field_type === 'url' ? 'url' : 'text'}
            value={str()}
            onChange={(e) => commit(def.id, e.currentTarget.value, (s) => s)}
            class={controlClass}
            style={controlStyle}
          />
        );
    }
  };

  // ── Roles ── (no GET item-roles endpoint exists; track assignment locally)
  const [assigned, setAssigned] = createSignal<Set<string>>(new Set());
  const toggleRole = async (roleId: string) => {
    const isOn = assigned().has(roleId);
    try {
      if (isOn) await api.roles.unassign(props.item.id, roleId);
      else await api.roles.assign(props.item.id, roleId);
      setAssigned((prev) => {
        const next = new Set(prev);
        if (isOn) next.delete(roleId);
        else next.add(roleId);
        return next;
      });
    } catch (err) {
      toast.error(err instanceof Error ? err.message : 'Failed to update role');
    }
  };

  return (
    <div class="space-y-6">
      <section class="space-y-3">
        <h3 class="text-sm font-semibold" style={{ color: 'var(--color-text-secondary)' }}>
          Custom fields
        </h3>
        <Show
          when={(defs() ?? []).length > 0}
          fallback={<EmptyState title="No custom fields" description="Define fields in project settings." />}
        >
          <For each={defs()}>
            {(def) => (
              <FieldShell label={def.name} required={def.required}>
                {renderInput(def)}
              </FieldShell>
            )}
          </For>
        </Show>
      </section>

      <section
        class="space-y-3 border-t pt-4"
        style={{ 'border-color': 'var(--color-border-light)' }}
      >
        <h3 class="text-sm font-semibold" style={{ color: 'var(--color-text-secondary)' }}>
          Roles
        </h3>
        <Show when={(roles() ?? []).length > 0} fallback={<EmptyState title="No roles defined" />}>
          <div class="flex flex-wrap gap-2">
            <For each={roles()}>
              {(role) => (
                <label
                  class="flex cursor-pointer items-center gap-2 rounded-md border px-3 py-1.5 text-sm"
                  style={{
                    'background-color': 'var(--color-bg-base)',
                    'border-color': 'var(--color-border-light)',
                    color: 'var(--color-text-primary)',
                  }}
                >
                  <input
                    type="checkbox"
                    checked={assigned().has(role.id)}
                    onChange={() => void toggleRole(role.id)}
                    class="h-4 w-4 rounded"
                  />
                  {role.name}
                </label>
              )}
            </For>
          </div>
        </Show>
      </section>
    </div>
  );
};

export default FieldsTab;
