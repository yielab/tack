import { createSignal, createResource, For, Show } from 'solid-js';
import { useParams } from '@solidjs/router';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { Button, Field, FieldShell, Select, Modal, Badge, EmptyState } from '../../../shared/ui';
import type { CustomField } from '../../../shared/types';

export default function FieldsPanel() {
  const params = useParams();
  const projectId = params.id!;

  const [showCreateModal, setShowCreateModal] = createSignal(false);
  const [editingField, setEditingField] = createSignal<CustomField | null>(null);

  // Form state
  const [name, setName] = createSignal('');
  const [fieldType, setFieldType] = createSignal('text');
  const [description, setDescription] = createSignal('');
  const [required, setRequired] = createSignal(false);
  const [options, setOptions] = createSignal('');

  const [fields, { refetch }] = createResource(() =>
    api.customFields.list(projectId)
  );

  const fieldTypes = [
    { value: 'text', label: 'Text', icon: '📝', description: 'Short text input' },
    { value: 'long_text', label: 'Long Text', icon: '📄', description: 'Multi-line text area' },
    { value: 'number', label: 'Number', icon: '🔢', description: 'Numeric input' },
    { value: 'date', label: 'Date', icon: '📅', description: 'Date picker' },
    { value: 'boolean', label: 'Checkbox', icon: '☑️', description: 'True/false checkbox' },
    { value: 'select', label: 'Select', icon: '📋', description: 'Single choice dropdown' },
    { value: 'multi_select', label: 'Multi-Select', icon: '✅', description: 'Multiple choice' },
    { value: 'url', label: 'URL', icon: '🔗', description: 'Website link' },
    { value: 'email', label: 'Email', icon: '📧', description: 'Email address' },
  ];

  const openCreateModal = () => {
    setName('');
    setFieldType('text');
    setDescription('');
    setRequired(false);
    setOptions('');
    setEditingField(null);
    setShowCreateModal(true);
  };

  const openEditModal = (field: CustomField) => {
    setName(field.name);
    setFieldType(field.field_type);
    setDescription(field.description || '');
    setRequired(field.required);
    setOptions(field.options ? field.options.join('\n') : '');
    setEditingField(field);
    setShowCreateModal(true);
  };

  const handleSubmit = async (e: Event) => {
    e.preventDefault();

    const body: any = {
      name: name().trim(),
      field_type: fieldType(),
      description: description().trim() || null,
      required: required(),
    };

    // Parse options for select/multi-select fields
    if (fieldType() === 'select' || fieldType() === 'multi_select') {
      const optionsList = options()
        .split('\n')
        .map(o => o.trim())
        .filter(o => o.length > 0);

      if (optionsList.length === 0) {
        toast.error('Select fields must have at least one option');
        return;
      }

      body.options = optionsList;
    }

    try {
      if (editingField()) {
        await api.customFields.update(editingField()!.id, body);
        toast.success('Field updated successfully');
      } else {
        await api.customFields.create(projectId, body);
        toast.success('Field created successfully');
      }

      setShowCreateModal(false);
      refetch();
    } catch (error) {
      toast.error('Failed to save field');
    }
  };

  const handleDelete = async (fieldId: string) => {
    if (!confirm('Are you sure? This will delete all values for this field.')) return;

    try {
      await api.customFields.remove(fieldId);
      toast.success('Field deleted');
      refetch();
    } catch (error) {
      toast.error('Failed to delete field');
    }
  };

  const getFieldTypeInfo = (type: string) => {
    return fieldTypes.find(ft => ft.value === type) || fieldTypes[0];
  };

  return (
    <div class="max-w-[560px]">
      <div>
        <div class="mb-2.5 flex items-center justify-end">
          <Button onClick={openCreateModal}>+ Add Field</Button>
        </div>

        {/* Fields List */}
        <div class="flex flex-col gap-2.5">
          <Show when={fields.loading}>
            <div class="text-center py-12 text-content-subtle">Loading fields...</div>
          </Show>

          <Show when={fields.error}>
            <div class="text-center py-12 text-danger-500">Failed to load fields</div>
          </Show>

          <For each={fields()}>
            {(field: CustomField) => {
              const typeInfo = getFieldTypeInfo(field.field_type);
              return (
                <div class="flex flex-col gap-1.5 rounded-[26px] bg-panel px-4 py-3.5">
                  <div class="flex flex-wrap items-center gap-2">
                    <span
                      class="grid h-7 w-7 shrink-0 place-items-center rounded-full text-sm"
                      style={{ 'background-color': 'var(--color-bg-app)' }}
                      aria-hidden="true"
                    >
                      {typeInfo.icon}
                    </span>
                    <h3 class="text-sm font-bold text-content" style={{ 'font-family': 'var(--font-body)' }}>
                      {field.name}
                    </h3>
                    <Badge>{typeInfo.label}</Badge>
                    <Show when={field.required}>
                      <Badge tone="danger">Required</Badge>
                    </Show>
                  </div>

                  <Show when={field.description}>
                    <p class="text-[12.5px] text-content-muted">{field.description}</p>
                  </Show>

                  <Show when={field.options && field.options.length > 0}>
                    <div class="text-[12.5px]">
                      <span class="text-content-subtle">Options: </span>
                      <span class="text-content-muted">{field.options!.join(', ')}</span>
                    </div>
                  </Show>

                  <div class="flex items-center gap-1.5">
                    <Button size="sm" variant="ghost" onClick={() => openEditModal(field)}>
                      Edit
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      style={{ color: 'var(--color-danger-600)' }}
                      onClick={() => handleDelete(field.id)}
                    >
                      Delete
                    </Button>
                  </div>
                </div>
              );
            }}
          </For>

          <Show when={fields() && fields()!.length === 0}>
            <EmptyState
              icon="📋"
              title="No custom fields yet"
              action={<Button onClick={openCreateModal}>Add Your First Field</Button>}
            />
          </Show>
        </div>

        {/* Create/Edit Modal */}
        <Modal
          isOpen={showCreateModal()}
          onClose={() => setShowCreateModal(false)}
          title={editingField() ? 'Edit Field' : 'Add Custom Field'}
        >
          <form onSubmit={handleSubmit} class="space-y-4">
            <Field
              label="Field Name"
              required
              value={name()}
              onInput={(e) => setName(e.currentTarget.value)}
              placeholder="e.g., Client Name"
            />

            <Select
              label="Field Type"
              required
              value={fieldType()}
              onChange={(e) => setFieldType(e.currentTarget.value)}
            >
              <For each={fieldTypes}>
                {(type) => (
                  <option value={type.value}>
                    {type.icon} {type.label} - {type.description}
                  </option>
                )}
              </For>
            </Select>

            <FieldShell label="Description" for="field-description">
              <textarea
                id="field-description"
                value={description()}
                onInput={(e) => setDescription(e.currentTarget.value)}
                rows={2}
                placeholder="Optional description"
                class="w-full resize-none rounded-xl border px-3.5 py-2.5 text-sm transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1"
                style={{
                  'background-color': 'var(--color-bg-base)',
                  color: 'var(--color-text-primary)',
                  'border-color': 'var(--color-border-medium)',
                  '--tw-ring-color': 'var(--color-focus-ring)',
                }}
              />
            </FieldShell>

            <Show when={fieldType() === 'select' || fieldType() === 'multi_select'}>
              <FieldShell
                label="Options (one per line)"
                required
                for="field-options"
                hint="Enter each option on a new line"
              >
                <textarea
                  id="field-options"
                  value={options()}
                  onInput={(e) => setOptions(e.currentTarget.value)}
                  rows={4}
                  required
                  placeholder="Option 1&#10;Option 2&#10;Option 3"
                  class="w-full rounded-xl border px-3.5 py-2.5 font-mono text-sm transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1"
                  style={{
                    'background-color': 'var(--color-bg-base)',
                    color: 'var(--color-text-primary)',
                    'border-color': 'var(--color-border-medium)',
                    '--tw-ring-color': 'var(--color-focus-ring)',
                  }}
                />
              </FieldShell>
            </Show>

            <label class="flex items-center gap-2 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
              <input
                type="checkbox"
                checked={required()}
                onChange={(e) => setRequired(e.currentTarget.checked)}
                class="h-4 w-4 rounded"
              />
              Required field (must be filled for all items)
            </label>

            <div
              class="flex justify-end gap-2 border-t pt-4"
              style={{ 'border-color': 'var(--color-border-light)' }}
            >
              <Button type="button" variant="secondary" onClick={() => setShowCreateModal(false)}>
                Cancel
              </Button>
              <Button type="submit">{editingField() ? 'Update' : 'Create'} Field</Button>
            </div>
          </form>
        </Modal>
      </div>
    </div>
  );
}
