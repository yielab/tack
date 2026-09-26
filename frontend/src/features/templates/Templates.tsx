import { createSignal, createResource, For, Show } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { toast } from '../../shared/ui/toast';
import { api } from '../../shared/api';
import { FiTrash2 } from 'solid-icons/fi';
import { Button, Field, FieldShell, Modal, Badge, EmptyState, Skeleton } from '../../shared/ui';
import { IconTemplates } from '../../shared/ui/icons';
import { projectTypePillStyle, projectTypeTone } from '../../shared/ui/projectTypeTone';
import type { ProjectTemplate } from '../../shared/types';

/** Workflow column names in board order (for the preset preview). */
function workflowColumns(t: ProjectTemplate): string[] {
  const statuses = t.workflow?.statuses ?? [];
  return [...statuses].sort((a, b) => a.order - b.order).map((s) => s.name);
}

/** Up to 3 sample "term → Renamed" vocabulary mappings, preferring common terms. */
function vocabSamples(t: ProjectTemplate): string[] {
  const vocab = t.vocabulary ?? {};
  const preferred = ['task', 'sprint', 'milestone', 'epic', 'release', 'phase'];
  const keys = [
    ...preferred.filter((k) => k in vocab),
    ...Object.keys(vocab).filter((k) => !preferred.includes(k)),
  ];
  return keys.slice(0, 3).map((k) => `${k} → ${vocab[k]}`);
}

function templateSummaryChips(t: ProjectTemplate) {
  const chips: { label: string; tone?: 'info' | 'success' | 'warning' }[] = [];

  const statusCount = t.workflow?.statuses?.length ?? 0;
  if (statusCount > 0) chips.push({ label: `${statusCount} statuses` });

  const vocabCount = t.vocabulary ? Object.keys(t.vocabulary).length : 0;
  if (vocabCount > 0) chips.push({ label: `${vocabCount} vocab overrides` });

  const fieldCount = t.custom_fields?.length ?? 0;
  if (fieldCount > 0) chips.push({ label: `${fieldCount} custom fields`, tone: 'info' });

  const boardCount = t.default_boards?.length ?? 0;
  if (boardCount > 0) chips.push({ label: `${boardCount} board${boardCount > 1 ? 's' : ''}` });

  return chips;
}

export default function Templates() {
  const navigate = useNavigate();
  const [selectedType, setSelectedType] = createSignal<string | null>(null);
  const [showCreateProjectModal, setShowCreateProjectModal] = createSignal(false);
  const [selectedTemplate, setSelectedTemplate] = createSignal<ProjectTemplate | null>(null);
  // Card being hovered/focused — drives the inline preset preview (Task 31.3).
  const [previewId, setPreviewId] = createSignal<string | null>(null);
  const [projectName, setProjectName] = createSignal('');
  const [projectDescription, setProjectDescription] = createSignal('');

  const [templates, { refetch }] = createResource(() =>
    api.templates.list(selectedType() ?? undefined)
  );

  const projectTypes = [
    { value: 'software', label: 'Software Development' },
    { value: 'web', label: 'Web Project' },
    { value: 'mobile', label: 'Mobile App' },
    { value: 'construction', label: 'Construction' },
    { value: 'personal', label: 'Personal' },
    { value: 'homework', label: 'Homework' },
    { value: 'maintenance', label: 'Maintenance' },
    { value: 'legal', label: 'Legal / Case' },
    { value: 'research', label: 'Research / Lab' },
    { value: 'event', label: 'Event Planning' },
    { value: 'custom', label: 'Custom' },
  ];

  /** Filter chip: a pill, accent-filled when active, with a type-hue dot. */
  const chipClass =
    'inline-flex items-center gap-1.5 whitespace-nowrap rounded-full px-3 py-1.5 text-[12.5px] transition-colors ' +
    'focus:outline-none focus-visible:ring-2';
  const chipStyle = (active: boolean) => ({
    'background-color': active ? 'var(--color-primary-600)' : 'var(--color-bg-panel)',
    color: active ? 'var(--color-on-accent)' : 'var(--color-text-primary)',
    'font-weight': active ? 600 : 400,
    '--tw-ring-color': 'var(--color-focus-ring)',
  });

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
    <div class="flex flex-col gap-5 lg:px-6 lg:py-3">
        {/* Header */}
        <div class="flex flex-wrap items-end gap-3">
          <div>
            <h1 class="text-[34px] leading-tight text-content">
              Project Templates
            </h1>
            <p class="text-content-muted mt-1">
              Start your project with a pre-configured template
            </p>
          </div>
          <div class="ml-auto flex gap-3">
            <Button onClick={() => navigate('/templates/new')}>+ Create Template</Button>
            <Button variant="secondary" onClick={() => navigate('/projects')}>
              Back to Projects
            </Button>
          </div>
        </div>

        {/* Type Filter */}
        <div class="flex flex-wrap gap-2">
          <button
            onClick={() => setSelectedType(null)}
            aria-pressed={selectedType() === null}
            class={chipClass}
            style={chipStyle(selectedType() === null)}
          >
            All Templates
          </button>
          <For each={projectTypes}>
            {(type) => (
              <button
                onClick={() => setSelectedType(type.value)}
                aria-pressed={selectedType() === type.value}
                class={chipClass}
                style={chipStyle(selectedType() === type.value)}
              >
                <span
                  class="h-[9px] w-[9px] shrink-0 rounded-full"
                  style={{
                    'background-color':
                      selectedType() === type.value
                        ? 'var(--color-on-accent)'
                        : projectTypeTone(type.value).fg,
                  }}
                  aria-hidden="true"
                />
                {type.label}
              </button>
            )}
          </For>
        </div>

        {/* Templates Grid */}
        <Show when={templates.loading}>
          <div class="flex flex-col gap-2.5" role="status">
            <p class="text-sm text-content-subtle">Loading templates...</p>
            <div class="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3" aria-hidden="true">
              <For each={[1, 2, 3]}>
                {() => (
                  <div class="flex h-[190px] flex-col gap-2.5 rounded-[28px] bg-panel p-5">
                    <Skeleton width="60%" height="18px" />
                    <Skeleton width="40%" height="12px" />
                    <Skeleton width="90%" height="10px" />
                    <div class="mt-auto">
                      <Skeleton height="34px" rounded />
                    </div>
                  </div>
                )}
              </For>
            </div>
          </div>
        </Show>

        <Show when={templates.error}>
          <div
            class="rounded-full px-5 py-3 text-sm font-semibold"
            style={{ 'background-color': 'var(--color-danger-100)', color: 'var(--color-danger-600)' }}
            role="alert"
          >
            Failed to load templates
          </div>
        </Show>

        <div class="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
          <For each={templates()}>
            {(template: ProjectTemplate) => {
              const typeInfo = getTypeInfo(template.project_type);
              const tone = projectTypeTone(typeInfo.value);
              const showPreview = () => previewId() === template.id;
              const columns = () => workflowColumns(template);
              const samples = () => vocabSamples(template);
              return (
                <div
                  class="flex flex-col gap-[9px] rounded-[28px] border-2 border-transparent bg-panel p-5 transition-[border-color,box-shadow] hover:border-brand hover:shadow-[var(--shadow-md)] focus:outline-none focus-visible:border-brand"
                  tabindex="0"
                  onMouseEnter={() => setPreviewId(template.id)}
                  onMouseLeave={() =>
                    setPreviewId((cur) => (cur === template.id ? null : cur))
                  }
                  onFocusIn={() => setPreviewId(template.id)}
                  onFocusOut={() =>
                    setPreviewId((cur) => (cur === template.id ? null : cur))
                  }
                >
                  {/* Header */}
                  <div class="flex items-center gap-2.5">
                    <span
                      class="h-[30px] w-[30px] shrink-0 rounded-full"
                      style={{
                        'background-color': tone.bg,
                        'box-shadow': `inset 0 0 0 2px ${tone.fg}`,
                      }}
                      aria-hidden="true"
                    />
                    <h3 class="min-w-0 text-lg leading-[1.15] text-content">
                      {template.name}
                    </h3>
                  </div>
                  <div class="flex flex-wrap gap-1.5">
                    <span style={projectTypePillStyle(typeInfo.value)}>
                      {typeInfo.label}
                    </span>
                    <Show when={template.is_builtin}>
                      <Badge tone="info">Built-in</Badge>
                    </Show>
                  </div>

                  {/* Description */}
                  <Show when={template.description}>
                    <p class="text-[13px] leading-[1.4] text-content-muted">
                      {template.description}
                    </p>
                  </Show>

                  {/* Content summary chips */}
                  {(() => {
                    const chips = templateSummaryChips(template);
                    return chips.length > 0 ? (
                      <div class="flex flex-wrap gap-1.5">
                        <For each={chips}>
                          {(chip) => <Badge tone={chip.tone}>{chip.label}</Badge>}
                        </For>
                      </div>
                    ) : null;
                  })()}

                  {/* Preset preview (workflow columns + sample vocab) on hover/focus */}
                  <Show when={showPreview() && (columns().length > 0 || samples().length > 0)}>
                    <div class="flex flex-col gap-1.5 rounded-[18px] bg-app px-3 py-2.5 text-[11px] text-content-muted">
                      <Show when={columns().length > 0}>
                        <div class="flex flex-wrap items-center gap-1">
                          <For each={columns()}>
                            {(col, i) => (
                              <>
                                <span class="rounded-full bg-panel px-2 py-0.5 text-content">{col}</span>
                                <Show when={i() < columns().length - 1}>
                                  <span aria-hidden="true">→</span>
                                </Show>
                              </>
                            )}
                          </For>
                        </div>
                      </Show>
                      <Show when={samples().length > 0}>
                        <div class="flex flex-wrap gap-x-3 gap-y-0.5 font-mono">
                          <For each={samples()}>{(s) => <span>{s}</span>}</For>
                        </div>
                      </Show>
                    </div>
                  </Show>

                  {/* Actions */}
                  <div class="mt-auto flex gap-2 pt-1">
                    <Button class="flex-1" onClick={() => handleUseTemplate(template)}>
                      Use Template
                    </Button>
                    <Show when={!template.is_builtin}>
                      <Button
                        variant="danger"
                        onClick={() => handleDeleteTemplate(template.id)}
                        title="Delete template"
                        aria-label="Delete template"
                      >
                        <FiTrash2 size={16} />
                      </Button>
                    </Show>
                  </div>
                </div>
              );
            }}
          </For>
        </div>

        <Show when={templates() && templates()!.length === 0}>
          <div class="rounded-[28px] border-2 border-dashed border-line px-6">
            <EmptyState
              icon={<IconTemplates size={26} style={{ color: 'var(--color-accent2-ink)' }} />}
              title={selectedType() ? 'No templates found for this type' : 'No templates yet'}
              action={
                <Button onClick={() => navigate('/templates/new')}>
                  Create Your First Template
                </Button>
              }
            />
          </div>
        </Show>

        {/* Create Project from Template Modal */}
        <Modal
          isOpen={showCreateProjectModal()}
          onClose={() => setShowCreateProjectModal(false)}
          title="Create Project from Template"
          size="sm"
        >
          <div
            class="mb-4 rounded-[20px] px-4 py-3"
            style={{ 'background-color': 'var(--color-accent-soft)' }}
          >
            <div class="text-xs" style={{ color: 'var(--color-accent-ink)' }}>
              Using template:
            </div>
            <div class="font-heading text-lg" style={{ color: 'var(--color-text-primary)' }}>
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
                class="w-full resize-none rounded-[20px] border px-3.5 py-2 text-sm transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1"
                style={{
                  'background-color': 'var(--color-bg-base)',
                  color: 'var(--color-text-primary)',
                  'border-color': 'var(--color-border-medium)',
                  '--tw-ring-color': 'var(--color-focus-ring)',
                }}
              />
            </FieldShell>

            <div
              class="rounded-[22px] px-4 py-2.5 text-sm"
              style={{
                'background-color': 'var(--color-info-50)',
                color: 'var(--color-info-700)',
              }}
            >
              This will create a new project with the template's workflow, vocabulary, custom fields, and boards.
            </div>

            <div class="flex justify-end gap-2.5 pt-2">
              <Button type="button" variant="secondary" onClick={() => setShowCreateProjectModal(false)}>
                Cancel
              </Button>
              <Button type="submit">Create Project</Button>
            </div>
          </form>
        </Modal>
    </div>
  );
}
