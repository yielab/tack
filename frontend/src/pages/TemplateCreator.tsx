import { createSignal } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { toast } from '../lib/toast';
import { api } from '../shared/api';

export default function TemplateCreator() {
  const navigate = useNavigate();

  const [name, setName] = createSignal('');
  const [description, setDescription] = createSignal('');
  const [projectType, setProjectType] = createSignal('software');

  const projectTypes = [
    { value: 'software', label: 'Software Development', icon: '💻' },
    { value: 'web', label: 'Web Project', icon: '🌐' },
    { value: 'mobile', label: 'Mobile App', icon: '📱' },
    { value: 'construction', label: 'Construction', icon: '🏗️' },
    { value: 'personal', label: 'Personal', icon: '👤' },
    { value: 'homework', label: 'Homework', icon: '📚' },
    { value: 'maintenance', label: 'Maintenance', icon: '🔧' },
    { value: 'custom', label: 'Custom', icon: '⚙️' },
  ];

  const handleSubmit = async (e: Event) => {
    e.preventDefault();

    try {
      // For now, use defaults. In a full implementation, we'd have forms for
      // customizing vocabulary, workflow, custom_fields, etc.
      await api.templates.create({
        name: name().trim(),
        description: description().trim() || null,
        project_type: projectType(),
        vocabulary: null,
        workflow: null,
        custom_fields: null,
        default_boards: null,
      });

      toast.success(`Template "${name()}" created!`);
      navigate('/templates');
    } catch (error) {
      toast.error('Failed to create template');
      console.error(error);
    }
  };

  return (
    <div class="min-h-screen bg-gray-50 dark:bg-gray-900 p-6">
      <div class="max-w-2xl mx-auto">
        {/* Header */}
        <div class="mb-8">
          <h1 class="text-3xl font-bold text-gray-900 dark:text-white mb-2">
            Create Project Template
          </h1>
          <p class="text-gray-600 dark:text-gray-400">
            Define a reusable project configuration
          </p>
        </div>

        {/* Form */}
        <div class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-6">
          <form onSubmit={handleSubmit} class="space-y-6">
            {/* Basic Info */}
            <div>
              <h3 class="text-lg font-semibold text-gray-900 dark:text-white mb-4">
                Basic Information
              </h3>

              <div class="space-y-4">
                <div>
                  <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Template Name *
                  </label>
                  <input
                    type="text"
                    value={name()}
                    onInput={(e) => setName(e.currentTarget.value)}
                    required
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                    placeholder="e.g., Web Development Project"
                  />
                </div>

                <div>
                  <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Description
                  </label>
                  <textarea
                    value={description()}
                    onInput={(e) => setDescription(e.currentTarget.value)}
                    rows={3}
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                    placeholder="Describe what this template is for..."
                  />
                </div>

                <div>
                  <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Project Type *
                  </label>
                  <select
                    value={projectType()}
                    onChange={(e) => setProjectType(e.currentTarget.value)}
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                  >
                    {projectTypes.map(type => (
                      <option value={type.value}>
                        {type.icon} {type.label}
                      </option>
                    ))}
                  </select>
                </div>
              </div>
            </div>

            {/* Info Box */}
            <div class="bg-blue-50 dark:bg-blue-900/20 p-4 rounded-lg">
              <h4 class="text-sm font-semibold text-blue-900 dark:text-blue-200 mb-2">
                Coming Soon: Advanced Configuration
              </h4>
              <p class="text-sm text-blue-800 dark:text-blue-300">
                Future versions will allow you to customize:
              </p>
              <ul class="text-sm text-blue-800 dark:text-blue-300 mt-2 space-y-1 list-disc list-inside">
                <li>Custom workflow statuses and transitions</li>
                <li>Project-specific vocabulary (terminology)</li>
                <li>Pre-defined custom fields</li>
                <li>Default board configurations</li>
              </ul>
              <p class="text-sm text-blue-800 dark:text-blue-300 mt-2">
                For now, templates will use the default configuration for the selected project type.
              </p>
            </div>

            {/* Actions */}
            <div class="flex justify-end gap-2 pt-4 border-t border-gray-200 dark:border-gray-700">
              <button
                type="button"
                onClick={() => navigate('/templates')}
                class="px-4 py-2 bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-600 transition-colors"
              >
                Cancel
              </button>
              <button
                type="submit"
                class="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 transition-colors"
              >
                Create Template
              </button>
            </div>
          </form>
        </div>
      </div>
    </div>
  );
}
