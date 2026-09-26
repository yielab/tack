import { type Component, createSignal, createResource, createMemo, For, Show } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { Modal, Button, Field, Select, FieldShell, Badge } from '../../shared/ui';
import { api } from '../../shared/api';
import type { ProjectType, ProjectTemplate } from '../../shared/types';
import { toast } from '../../shared/ui/toast';
import { projectTypeTone } from '../../shared/ui/projectTypeTone';

const PROJECT_TYPE_OPTIONS = [
  { value: 'software', label: 'Software (Scrum)' },
  { value: 'web', label: 'Web Development (Scrum)' },
  { value: 'mobile', label: 'Mobile App (Scrum)' },
  { value: 'construction', label: 'Construction (Phase-based)' },
  { value: 'personal', label: 'Personal (Simple)' },
  { value: 'homework', label: 'Homework (Simple)' },
  { value: 'maintenance', label: 'Maintenance (Kanban)' },
  { value: 'legal', label: 'Legal / Case (Phase-based)' },
  { value: 'research', label: 'Research / Lab (Kanban)' },
  { value: 'event', label: 'Event Planning (Phase-based)' },
  { value: 'custom', label: 'Custom' },
];

/** Domain metadata for grouping template cards. Order defines section order. */
const DOMAINS: { type: ProjectType; label: string }[] = [
  { type: 'software', label: 'Software Development' },
  { type: 'web', label: 'Web Project' },
  { type: 'mobile', label: 'Mobile App' },
  { type: 'construction', label: 'Construction' },
  { type: 'personal', label: 'Personal' },
  { type: 'homework', label: 'Homework' },
  { type: 'maintenance', label: 'Maintenance' },
  { type: 'legal', label: 'Legal / Case' },
  { type: 'research', label: 'Research / Lab' },
  { type: 'event', label: 'Event Planning' },
  { type: 'custom', label: 'Custom' },
];

/** Small type-hue dot (replaces the old decorative domain emoji). */
const TypeDot = (props: { type: string }) => (
  <span
    class="inline-block h-2 w-2 shrink-0 rounded-full"
    style={{ 'background-color': projectTypeTone(props.type).fg }}
    aria-hidden="true"
  />
);

/** Workflow column names in board order. */
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

export interface CreateProjectModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: () => void;
}

const CreateProjectModal: Component<CreateProjectModalProps> = (props) => {
  const navigate = useNavigate();
  const [name, setName] = createSignal('');
  const [description, setDescription] = createSignal('');
  const [projectType, setProjectType] = createSignal<ProjectType>('software');
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal('');

  // null selection = "Start blank" (bare project-type flow, the original behavior).
  const [selectedTemplate, setSelectedTemplate] = createSignal<ProjectTemplate | null>(null);
  // Card being hovered/focused — drives the inline preset preview (Task 31.3).
  const [previewId, setPreviewId] = createSignal<string | null>(null);

  const [templates] = createResource(
    () => props.isOpen,
    (open) => (open ? api.templates.list() : Promise.resolve([] as ProjectTemplate[])),
  );

  // Templates grouped by domain, in DOMAINS order, only for domains that have any.
  const grouped = createMemo(() => {
    const list = templates() ?? [];
    return DOMAINS.map((d) => ({
      ...d,
      items: list.filter((t) => t.project_type === d.type),
    })).filter((g) => g.items.length > 0);
  });

  const reset = () => {
    setName('');
    setDescription('');
    setProjectType('software');
    setSelectedTemplate(null);
    setPreviewId(null);
    setError('');
  };

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    setError('');

    if (!name().trim()) {
      setError('Project name is required');
      return;
    }

    setLoading(true);
    try {
      const tmpl = selectedTemplate();
      let created: { id: string };
      if (tmpl) {
        created = await api.templates.createProject(tmpl.id, {
          name: name().trim(),
          description: description().trim() || null,
        });
      } else {
        created = await api.projects.create({
          name: name().trim(),
          description: description().trim() || undefined,
          project_type: projectType(),
        });
      }

      reset();
      toast.success('Project created successfully');
      props.onSuccess();
      props.onClose();
      if (created?.id) navigate(`/projects/${created.id}/board`);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Failed to create project';
      setError(errorMessage);
      toast.error(errorMessage);
    } finally {
      setLoading(false);
    }
  };

  const handleClose = () => {
    if (!loading()) {
      reset();
      props.onClose();
    }
  };

  // Items inside the picker: item radius, app fill, 2px accent border when chosen.
  const cardBaseStyle = {
    'background-color': 'var(--color-bg-app)',
    'border-color': 'transparent',
    'box-shadow': 'var(--shadow-sm)',
  };
  const cardSelectedStyle = {
    'background-color': 'var(--color-accent-soft)',
    'border-color': 'var(--color-primary-600)',
    'box-shadow': 'var(--shadow-md)',
  };
  const cardClass =
    'rounded-[20px] border-2 p-3 text-left transition-[border-color,box-shadow,background-color] ' +
    'hover:border-[var(--color-primary-600)] focus:outline-none focus-visible:ring-2';

  const renderPreview = (t: ProjectTemplate) => {
    const columns = workflowColumns(t);
    const samples = vocabSamples(t);
    return (
      <div
        class="mt-2 flex flex-col gap-1.5 rounded-[14px] px-2.5 py-2 text-[11px]"
        style={{ 'background-color': 'var(--color-bg-panel)', color: 'var(--color-text-secondary)' }}
      >
        <Show when={columns.length > 0}>
          <div class="flex flex-wrap items-center gap-1">
            <For each={columns}>
              {(col, i) => (
                <>
                  <span
                    class="rounded-full px-2 py-0.5"
                    style={{ 'background-color': 'var(--color-bg-app)', color: 'var(--color-text-primary)' }}
                  >
                    {col}
                  </span>
                  <Show when={i() < columns.length - 1}>
                    <span aria-hidden="true">→</span>
                  </Show>
                </>
              )}
            </For>
          </div>
        </Show>
        <Show when={samples.length > 0}>
          <div class="flex flex-wrap gap-x-3 gap-y-0.5 font-mono">
            <For each={samples}>{(s) => <span>{s}</span>}</For>
          </div>
        </Show>
      </div>
    );
  };

  return (
    <Modal isOpen={props.isOpen} onClose={handleClose} title="Create New Project" size="lg">
      <form onSubmit={handleSubmit} class="space-y-4">
        <Show when={error()}>
          <div
            class="rounded-[22px] px-4 py-2.5 text-sm font-semibold"
            style={{
              'background-color': 'var(--color-danger-100)',
              color: 'var(--color-danger-600)',
            }}
          >
            {error()}
          </div>
        </Show>

        <Field
          label="Project Name"
          required
          value={name()}
          onInput={(e) => setName(e.currentTarget.value)}
          placeholder="My Awesome Project"
          disabled={loading()}
        />

        <FieldShell label="Description" for="project-description">
          <textarea
            id="project-description"
            value={description()}
            onInput={(e) => setDescription(e.currentTarget.value)}
            placeholder="A brief description of your project..."
            rows={2}
            disabled={loading()}
            class="w-full resize-none rounded-[20px] border px-3.5 py-2 text-sm transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1 disabled:opacity-50"
            style={{
              'background-color': 'var(--color-bg-base)',
              color: 'var(--color-text-primary)',
              'border-color': 'var(--color-border-medium)',
              '--tw-ring-color': 'var(--color-focus-ring)',
            }}
          />
        </FieldShell>

        {/* Template picker */}
        <FieldShell label="Start from" for="template-picker">
          <div id="template-picker" class="space-y-3">
            {/* Start blank card + bare project-type fallback */}
            <button
              type="button"
              onClick={() => setSelectedTemplate(null)}
              class={`w-full ${cardClass}`}
              style={{
                ...(selectedTemplate() === null ? cardSelectedStyle : cardBaseStyle),
                '--tw-ring-color': 'var(--color-focus-ring)',
              }}
            >
              <div class="flex items-center justify-between">
                <span class="font-medium" style={{ color: 'var(--color-text-primary)' }}>
                  ✨ Start blank
                </span>
                <Show when={selectedTemplate() === null}>
                  <Badge tone="primary">Selected</Badge>
                </Show>
              </div>
              <p class="mt-0.5 text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                Pick a project type; workflow and terminology are derived from it.
              </p>
            </button>

            <Show when={selectedTemplate() === null}>
              <Select
                label="Project Type"
                value={projectType()}
                onChange={(e) => setProjectType(e.currentTarget.value as ProjectType)}
                disabled={loading()}
                options={PROJECT_TYPE_OPTIONS}
                hint="Determines default workflow and terminology"
              />
            </Show>

            <Show when={templates.loading}>
              <div class="text-sm" style={{ color: 'var(--color-text-tertiary)' }}>
                Loading templates…
              </div>
            </Show>

            {/* Template cards grouped by domain */}
            <div class="max-h-72 space-y-4 overflow-y-auto pr-1">
              <For each={grouped()}>
                {(group) => (
                  <div>
                    <div
                      class="mb-1.5 flex items-center gap-1.5 text-[10.5px] font-bold uppercase tracking-[.1em]"
                      style={{ color: 'var(--color-text-tertiary)' }}
                    >
                      <TypeDot type={group.type} /> {group.label}
                    </div>
                    <div class="grid grid-cols-1 gap-2 sm:grid-cols-2">
                      <For each={group.items}>
                        {(t) => {
                          const isSelected = () => selectedTemplate()?.id === t.id;
                          const showPreview = () => isSelected() || previewId() === t.id;
                          return (
                            <button
                              type="button"
                              onClick={() => setSelectedTemplate(t)}
                              onMouseEnter={() => setPreviewId(t.id)}
                              onMouseLeave={() =>
                                setPreviewId((cur) => (cur === t.id ? null : cur))
                              }
                              onFocus={() => setPreviewId(t.id)}
                              onBlur={() =>
                                setPreviewId((cur) => (cur === t.id ? null : cur))
                              }
                              class={cardClass}
                              style={{
                                ...(isSelected() ? cardSelectedStyle : cardBaseStyle),
                                '--tw-ring-color': 'var(--color-focus-ring)',
                              }}
                            >
                              <div class="flex items-start justify-between gap-2">
                                <span
                                  class="flex items-center gap-1.5 font-semibold"
                                  style={{ color: 'var(--color-text-primary)' }}
                                >
                                  <TypeDot type={t.project_type} /> {t.name}
                                </span>
                                <Show when={t.is_builtin}>
                                  <Badge tone="neutral">Built-in</Badge>
                                </Show>
                              </div>
                              <Show when={t.description}>
                                <p
                                  class="mt-0.5 line-clamp-2 text-xs"
                                  style={{ color: 'var(--color-text-secondary)' }}
                                >
                                  {t.description}
                                </p>
                              </Show>
                              <Show when={showPreview()}>{renderPreview(t)}</Show>
                            </button>
                          );
                        }}
                      </For>
                    </div>
                  </div>
                )}
              </For>
            </div>
          </div>
        </FieldShell>

        <div class="flex gap-3 pt-2">
          <Button
            type="button"
            variant="secondary"
            class="flex-1"
            onClick={handleClose}
            disabled={loading()}
          >
            Cancel
          </Button>
          <Button
            type="submit"
            class="flex-1"
            loading={loading()}
            disabled={loading() || !name().trim()}
          >
            {loading() ? 'Creating...' : 'Create Project'}
          </Button>
        </div>
      </form>
    </Modal>
  );
};

export default CreateProjectModal;
