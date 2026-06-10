import { createSignal, createResource, For, Show } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { toast } from '../../shared/ui/toast';
import { api } from '../../shared/api';
import { Button, Field, FieldShell, Modal, Badge, EmptyState } from '../../shared/ui';
import type { ProjectTemplate } from '../../shared/types';

function templateSummaryChips(t: ProjectTemplate) {
  const chips: { label: string; tone?: 'info' | 'success' | 'warning' }[] = [];

  const statusCount = t.workflow?.statuses?.length ?? 0;
  if (statusCount > 0) chips.push({ label: `${statusCount} statuses` });

  const vocabCount = t.vocabulary ? Object.keys(t.vocabulary).length : 0;
  if (vocabCount > 0) chips.push({ label: `${vocabCount} vocab overrides` });

  const fieldCount = t.custom_fields?.length ?? 0;
  if (fieldCount > 0) chips.push({ label: `${fieldCount} custom fields`, tone: 'success' });

  const boardCount = t.default_boards?.length ?? 0;
  if (boardCount > 0) chips.push({ label: `${boardCount} board${boardCount > 1 ? 's' : ''}` });

  return chips;
}

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
              <Button onClick={() => navigate('/templates/new')}>+ Create Template</Button>
              <Button variant="secondary" onClick={() => navigate('/projects')}>
                Back to Projects
              </Button>
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
                      <Badge tone="info">Built-in</Badge>
                    </Show>
                  </div>

                  {/* Description */}
                  <Show when={template.description}>
                    <p class="text-sm text-gray-600 dark:text-gray-400 mb-3">
                      {template.description}
                    </p>
                  </Show>

                  {/* Content summary chips */}
                  {(() => {
                    const chips = templateSummaryChips(template);
                    return chips.length > 0 ? (
                      <div class="flex flex-wrap gap-1.5 mb-3">
                        <For each={chips}>
                          {(chip) => <Badge tone={chip.tone}>{chip.label}</Badge>}
                        </For>
                      </div>
                    ) : null;
                  })()}

                  {/* Actions */}
                  <div class="flex gap-2 mt-4">
                    <Button class="flex-1" onClick={() => handleUseTemplate(template)}>
                      Use Template
                    </Button>
                    <Show when={!template.is_builtin}>
                      <Button
                        variant="danger"
                        onClick={() => handleDeleteTemplate(template.id)}
                        title="Delete template"
                      >
                        🗑️
                      </Button>
                    </Show>
                  </div>
                </div>
              );
            }}
          </For>
        </div>

        <Show when={templates() && templates()!.length === 0}>
          <EmptyState
            icon="📋"
            title={selectedType() ? 'No templates found for this type' : 'No templates yet'}
            action={
              <Button onClick={() => navigate('/templates/new')}>
                Create Your First Template
              </Button>
            }
          />
        </Show>

        {/* Create Project from Template Modal */}
        <Modal
          isOpen={showCreateProjectModal()}
          onClose={() => setShowCreateProjectModal(false)}
          title="Create Project from Template"
          size="sm"
        >
          <div
            class="mb-4 rounded-lg p-3"
            style={{ 'background-color': 'var(--color-bg-active)' }}
          >
            <div class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
              Using template:
            </div>
            <div class="font-semibold" style={{ color: 'var(--color-text-primary)' }}>
              {selectedTemplate()?.name}
            </div>
          </div>

          <form onSubmit={handleCreateFromTemplate} class="space-y-4">
            <Field
              label="Project Name"
              required
              value={projectName()}
              onInput={(e) => setProjectName(e.currentTarget.value)}
              placeholder="My New Project"
            />

            <FieldShell label="Description" for="from-template-description">
              <textarea
                id="from-template-description"
                value={projectDescription()}
                onInput={(e) => setProjectDescription(e.currentTarget.value)}
                rows={3}
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

            <div
              class="rounded-lg p-3 text-sm"
              style={{
                'background-color': 'var(--color-info-50)',
                color: 'var(--color-info-700)',
              }}
            >
              This will create a new project with the template's workflow, vocabulary, custom fields, and boards.
            </div>

            <div class="flex justify-end gap-2 pt-2">
              <Button type="button" variant="secondary" onClick={() => setShowCreateProjectModal(false)}>
                Cancel
              </Button>
              <Button type="submit">Create Project</Button>
            </div>
          </form>
        </Modal>
      </div>
    </div>
  );
}
