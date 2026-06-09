import { createSignal, createResource, For, Show } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { toast } from '../../shared/ui/toast';
import { api } from '../../shared/api';
import type { ProjectTemplate } from '../../shared/types';

export default function Templates() {
  const navigate = useNavigate();
  const [selectedType, setSelectedType] = createSignal<string | null>(null);
  const [showCreateProjectModal, setShowCreateProjectModal] = createSignal(false);
  const [selectedTemplate, setSelectedTemplate] = createSignal<ProjectTemplate | null>(null);
  const [projectName, setProjectName] = createSignal('');
  const [projectDescription, setProjectDescription] = createSignal('');

  const [templates, { refetch }] = createResource(() =>
    api.templates.list(selectedType() ?? undefined)
  );

  const projectTypes = [
    { value: 'software', label: 'Software Development', icon: '💻', color: 'blue' },
    { value: 'web', label: 'Web Project', icon: '🌐', color: 'cyan' },
    { value: 'mobile', label: 'Mobile App', icon: '📱', color: 'purple' },
    { value: 'construction', label: 'Construction', icon: '🏗️', color: 'orange' },
    { value: 'personal', label: 'Personal', icon: '👤', color: 'green' },
    { value: 'homework', label: 'Homework', icon: '📚', color: 'yellow' },
    { value: 'maintenance', label: 'Maintenance', icon: '🔧', color: 'red' },
    { value: 'custom', label: 'Custom', icon: '⚙️', color: 'gray' },
  ];

  const getTypeInfo = (type: string) => {
    return projectTypes.find(t => t.value === type) || projectTypes[projectTypes.length - 1];
  };

  const handleUseTemplate = (template: ProjectTemplate) => {
    setSelectedTemplate(template);
    setProjectName('');
    setProjectDescription('');
    setShowCreateProjectModal(true);
  };

  const handleCreateFromTemplate = async (e: Event) => {
    e.preventDefault();
    const template = selectedTemplate();
    if (!template) return;

    try {
      const project = await api.templates.createProject(template.id, {
        name: projectName().trim(),
        description: projectDescription().trim() || null,
      });

      toast.success(`Project "${projectName()}" created from template!`);
      setShowCreateProjectModal(false);
      navigate(`/projects/${project.id}/board`);
    } catch (error) {
      toast.error('Failed to create project from template');
      console.error(error);
    }
  };

  const handleDeleteTemplate = async (templateId: string) => {
    if (!confirm('Are you sure you want to delete this template?')) return;

    try {
      await api.templates.remove(templateId);
      toast.success('Template deleted');
      refetch();
    } catch (error) {
      toast.error('Failed to delete template');
    }
  };

  return (
    <div class="min-h-screen bg-gray-50 dark:bg-gray-900 p-6">
      <div class="max-w-7xl mx-auto">
        {/* Header */}
        <div class="mb-8">
          <div class="flex items-center justify-between mb-4">
            <div>
              <h1 class="text-3xl font-bold text-gray-900 dark:text-white">
                Project Templates
              </h1>
              <p class="text-gray-600 dark:text-gray-400 mt-1">
                Start your project with a pre-configured template
              </p>
            </div>
            <div class="flex gap-2">
              <button
                onClick={() => navigate('/templates/new')}
                class="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 transition-colors"
              >
                + Create Template
              </button>
              <button
                onClick={() => navigate('/projects')}
                class="px-4 py-2 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
              >
                Back to Projects
              </button>
            </div>
          </div>

          {/* Type Filter */}
          <div class="flex gap-2 flex-wrap">
            <button
              onClick={() => setSelectedType(null)}
              class="px-3 py-1.5 text-sm rounded-lg transition-colors"
              classList={{
                'bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300': selectedType() === null,
                'bg-gray-100 dark:bg-gray-800 text-gray-700 dark:text-gray-300': selectedType() !== null,
              }}
            >
              All Templates
            </button>
            <For each={projectTypes}>
              {(type) => (
                <button
                  onClick={() => setSelectedType(type.value)}
                  class="px-3 py-1.5 text-sm rounded-lg transition-colors"
                  classList={{
                    'bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300': selectedType() === type.value,
                    'bg-gray-100 dark:bg-gray-800 text-gray-700 dark:text-gray-300': selectedType() !== type.value,
                  }}
                >
                  {type.icon} {type.label}
                </button>
              )}
            </For>
          </div>
        </div>

        {/* Templates Grid */}
        <Show when={templates.loading}>
          <div class="text-center py-12 text-gray-500">Loading templates...</div>
        </Show>

        <Show when={templates.error}>
          <div class="text-center py-12 text-red-500">Failed to load templates</div>
        </Show>

        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          <For each={templates()}>
            {(template: ProjectTemplate) => {
              const typeInfo = getTypeInfo(template.project_type);
              return (
                <div class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-6 hover:shadow-lg transition-shadow">
                  {/* Header */}
                  <div class="flex items-start justify-between mb-4">
                    <div class="flex items-center gap-3">
                      <div class="text-3xl">{typeInfo.icon}</div>
                      <div>
                        <h3 class="text-lg font-semibold text-gray-900 dark:text-white">
                          {template.name}
                        </h3>
                        <span class={`text-xs px-2 py-0.5 rounded bg-${typeInfo.color}-100 dark:bg-${typeInfo.color}-900/30 text-${typeInfo.color}-700 dark:text-${typeInfo.color}-300`}>
                          {typeInfo.label}
                        </span>
                      </div>
                    </div>
                    <Show when={template.is_builtin}>
                      <span class="px-2 py-1 text-xs bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 rounded">
                        Built-in
                      </span>
                    </Show>
                  </div>

                  {/* Description */}
                  <Show when={template.description}>
                    <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">
                      {template.description}
                    </p>
                  </Show>

                  {/* Actions */}
                  <div class="flex gap-2 mt-4">
                    <button
                      onClick={() => handleUseTemplate(template)}
                      class="flex-1 px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 transition-colors text-sm font-medium"
                    >
                      Use Template
                    </button>
                    <Show when={!template.is_builtin}>
                      <button
                        onClick={() => handleDeleteTemplate(template.id)}
                        class="px-3 py-2 bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300 rounded-lg hover:bg-red-200 dark:hover:bg-red-900/50 transition-colors"
                        title="Delete template"
                      >
                        🗑️
                      </button>
                    </Show>
                  </div>
                </div>
              );
            }}
          </For>
        </div>

        <Show when={templates() && templates()!.length === 0}>
          <div class="text-center py-12">
            <div class="text-4xl mb-4">📋</div>
            <p class="text-gray-500 dark:text-gray-400 mb-4">
              {selectedType() ? 'No templates found for this type' : 'No templates yet'}
            </p>
            <button
              onClick={() => navigate('/templates/new')}
              class="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 transition-colors"
            >
              Create Your First Template
            </button>
          </div>
        </Show>

        {/* Create Project from Template Modal */}
        <Show when={showCreateProjectModal()}>
          <div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
            <div class="bg-white dark:bg-gray-800 rounded-lg max-w-md w-full p-6">
              <h2 class="text-xl font-bold text-gray-900 dark:text-white mb-4">
                Create Project from Template
              </h2>

              <div class="mb-4 p-3 bg-purple-50 dark:bg-purple-900/20 rounded-lg">
                <div class="text-sm text-gray-600 dark:text-gray-400">Using template:</div>
                <div class="font-semibold text-gray-900 dark:text-white">
                  {selectedTemplate()?.name}
                </div>
              </div>

              <form onSubmit={handleCreateFromTemplate} class="space-y-4">
                <div>
                  <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Project Name *
                  </label>
                  <input
                    type="text"
                    value={projectName()}
                    onInput={(e) => setProjectName(e.currentTarget.value)}
                    required
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                    placeholder="My New Project"
                  />
                </div>

                <div>
                  <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Description
                  </label>
                  <textarea
                    value={projectDescription()}
                    onInput={(e) => setProjectDescription(e.currentTarget.value)}
                    rows={3}
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                    placeholder="Optional description"
                  />
                </div>

                <div class="bg-blue-50 dark:bg-blue-900/20 p-3 rounded-lg text-sm text-blue-800 dark:text-blue-200">
                  This will create a new project with the template's workflow, vocabulary, custom fields, and boards.
                </div>

                <div class="flex justify-end gap-2 mt-6">
                  <button
                    type="button"
                    onClick={() => setShowCreateProjectModal(false)}
                    class="px-4 py-2 bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-600 transition-colors"
                  >
                    Cancel
                  </button>
                  <button
                    type="submit"
                    class="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 transition-colors"
                  >
                    Create Project
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
