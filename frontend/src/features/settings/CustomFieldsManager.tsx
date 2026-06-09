import { createSignal, createResource, For, Show } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { api } from '../../shared/api';
import { toast } from '../../shared/ui/toast';
import { Button, Field, FieldShell, Select, Modal, Badge, EmptyState } from '../../shared/ui';
import { useProject } from '../../shared/state/projectContext';

interface CustomField {
  id: string;
  project_id: string;
  name: string;
  field_type: string;
  description: string | null;
  required: boolean;
  default_value: any;
  options: string[] | null;
  validation: any;
  created_at: string;
  updated_at: string;
}

export default function CustomFieldsManager() {
  const params = useParams();
  const navigate = useNavigate();
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

  const { project } = useProject();

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
    <div class="min-h-screen bg-gray-50 dark:bg-gray-900 p-6">
      <div class="max-w-4xl mx-auto">
        {/* Header */}
        <div class="mb-6 flex items-center justify-between">
          <div>
            <h1 class="text-3xl font-bold text-gray-900 dark:text-white">
              Custom Fields
            </h1>
            <p class="text-gray-600 dark:text-gray-400 mt-1">
              {project()?.name || 'Loading...'}
            </p>
          </div>
          <div class="flex gap-2">
            <Button onClick={openCreateModal}>+ Add Field</Button>
            <Button variant="secondary" onClick={() => navigate(`/projects/${projectId}/board`)}>
              Back to Project
            </Button>
          </div>
        </div>

        {/* Info Box */}
        <div class="mb-6 bg-blue-50 dark:bg-blue-900/20 p-4 rounded-lg">
          <h3 class="text-sm font-semibold text-blue-900 dark:text-blue-200 mb-2">
            💡 About Custom Fields
          </h3>
          <p class="text-sm text-blue-800 dark:text-blue-300">
            Custom fields let you add project-specific metadata to items. For example, add a "Client Name" field for agency projects,
            or "Contractor" field for construction projects. Fields can be text, numbers, dates, checkboxes, and more.
          </p>
        </div>

        {/* Fields List */}
        <div class="space-y-4">
          <Show when={fields.loading}>
            <div class="text-center py-12 text-gray-500">Loading fields...</div>
          </Show>

          <Show when={fields.error}>
            <div class="text-center py-12 text-red-500">Failed to load fields</div>
          </Show>

          <For each={fields()}>
            {(field: CustomField) => {
              const typeInfo = getFieldTypeInfo(field.field_type);
              return (
                <div class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-6">
                  <div class="flex items-start justify-between">
                    <div class="flex-1">
                      <div class="flex items-center gap-3 mb-2">
                        <span class="text-2xl">{typeInfo.icon}</span>
                        <div>
                          <h3 class="text-lg font-semibold text-gray-900 dark:text-white">
                            {field.name}
                          </h3>
                          <div class="flex items-center gap-2 mt-1">
                            <Badge>{typeInfo.label}</Badge>
                            <Show when={field.required}>
                              <Badge tone="danger">Required</Badge>
                            </Show>
                          </div>
                        </div>
                      </div>

                      <Show when={field.description}>
                        <p class="text-sm text-gray-600 dark:text-gray-400 mb-2">
                          {field.description}
                        </p>
                      </Show>

                      <Show when={field.options && field.options.length > 0}>
                        <div class="mt-2">
                          <span class="text-xs text-gray-500 dark:text-gray-400">Options: </span>
                          <span class="text-sm text-gray-700 dark:text-gray-300">
                            {field.options!.join(', ')}
                          </span>
                        </div>
                      </Show>
                    </div>

                    <div class="flex items-center gap-2">
                      <Button size="sm" variant="ghost" onClick={() => openEditModal(field)}>
                        Edit
                      </Button>
                      <Button size="sm" variant="danger" onClick={() => handleDelete(field.id)}>
                        Delete
                      </Button>
                    </div>
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
                class="w-full resize-none rounded-lg border px-3 py-2 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1"
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
                  class="w-full rounded-lg border px-3 py-2 font-mono text-sm transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1"
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
