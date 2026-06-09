import { createSignal, createResource, For, Show } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { api } from '../shared/api';
import { toast } from '../lib/toast';

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

  const [project] = createResource(() => api.projects.get(projectId));

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
            <button
              onClick={openCreateModal}
              class="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 transition-colors"
            >
              + Add Field
            </button>
            <button
              onClick={() => navigate(`/projects/${projectId}/board`)}
              class="px-4 py-2 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
            >
              Back to Project
            </button>
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
                            <span class="px-2 py-0.5 text-xs bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded">
                              {typeInfo.label}
                            </span>
                            <Show when={field.required}>
                              <span class="px-2 py-0.5 text-xs bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300 rounded">
                                Required
                              </span>
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
                      <button
                        onClick={() => openEditModal(field)}
                        class="px-3 py-1.5 text-sm bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300 rounded hover:bg-purple-200 dark:hover:bg-purple-900/50 transition-colors"
                      >
                        Edit
                      </button>
                      <button
                        onClick={() => handleDelete(field.id)}
                        class="px-3 py-1.5 text-sm bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300 rounded hover:bg-red-200 dark:hover:bg-red-900/50 transition-colors"
                      >
                        Delete
                      </button>
                    </div>
                  </div>
                </div>
              );
            }}
          </For>

          <Show when={fields() && fields()!.length === 0}>
            <div class="text-center py-12">
              <div class="text-4xl mb-4">📋</div>
              <p class="text-gray-500 dark:text-gray-400 mb-4">No custom fields yet</p>
              <button
                onClick={openCreateModal}
                class="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 transition-colors"
              >
                Add Your First Field
              </button>
            </div>
          </Show>
        </div>

        {/* Create/Edit Modal */}
        <Show when={showCreateModal()}>
          <div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
            <div class="bg-white dark:bg-gray-800 rounded-lg max-w-lg w-full p-6 max-h-[90vh] overflow-y-auto">
              <h2 class="text-xl font-bold text-gray-900 dark:text-white mb-4">
                {editingField() ? 'Edit Field' : 'Add Custom Field'}
              </h2>

              <form onSubmit={handleSubmit} class="space-y-4">
                <div>
                  <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Field Name *
                  </label>
                  <input
                    type="text"
                    value={name()}
                    onInput={(e) => setName(e.currentTarget.value)}
                    required
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                    placeholder="e.g., Client Name"
                  />
                </div>

                <div>
                  <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Field Type *
                  </label>
                  <select
                    value={fieldType()}
                    onChange={(e) => setFieldType(e.currentTarget.value)}
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                  >
                    <For each={fieldTypes}>
                      {(type) => (
                        <option value={type.value}>
                          {type.icon} {type.label} - {type.description}
                        </option>
                      )}
                    </For>
                  </select>
                </div>

                <div>
                  <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Description
                  </label>
                  <textarea
                    value={description()}
                    onInput={(e) => setDescription(e.currentTarget.value)}
                    rows={2}
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                    placeholder="Optional description"
                  />
                </div>

                <Show when={fieldType() === 'select' || fieldType() === 'multi_select'}>
                  <div>
                    <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                      Options (one per line) *
                    </label>
                    <textarea
                      value={options()}
                      onInput={(e) => setOptions(e.currentTarget.value)}
                      rows={4}
                      class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white font-mono text-sm"
                      placeholder="Option 1&#10;Option 2&#10;Option 3"
                      required
                    />
                    <p class="text-xs text-gray-500 dark:text-gray-400 mt-1">
                      Enter each option on a new line
                    </p>
                  </div>
                </Show>

                <div class="flex items-center gap-2">
                  <input
                    type="checkbox"
                    id="required"
                    checked={required()}
                    onChange={(e) => setRequired(e.currentTarget.checked)}
                    class="w-4 h-4 text-purple-600 rounded"
                  />
                  <label for="required" class="text-sm text-gray-700 dark:text-gray-300">
                    Required field (must be filled for all items)
                  </label>
                </div>

                <div class="flex justify-end gap-2 mt-6 pt-4 border-t border-gray-200 dark:border-gray-700">
                  <button
                    type="button"
                    onClick={() => setShowCreateModal(false)}
                    class="px-4 py-2 bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-600 transition-colors"
                  >
                    Cancel
                  </button>
                  <button
                    type="submit"
                    class="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 transition-colors"
                  >
                    {editingField() ? 'Update' : 'Create'} Field
                  </button>
                </div>
              </form>
            </div>
          </div>
        </Show>
      </div>
    </div>
  );
}
