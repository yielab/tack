import { createSignal, For, Show } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { toast } from '../../shared/ui/toast';
import { api } from '../../shared/api';
import { Button, Field, FieldShell, Select, Modal, Badge } from '../../shared/ui';
import { VOCAB_KEYS, resolveLabel } from '../../shared/vocab/vocab';
import { FiChevronDown, FiChevronRight, FiPlus, FiTrash2 } from 'solid-icons/fi';
import type { WorkflowStatus, TemplateCustomField, TemplateBoardConfig, ProjectType, CustomFieldType } from '../../shared/types';

// ── local row shape for the workflow editor ──────────────────────────────────
interface StatusRow {
  id: string;
  name: string;
  category: 'todo' | 'in_progress' | 'done';
  wip_limit: string;
}

const rowToStatus = (r: StatusRow, order: number): WorkflowStatus => ({
  name: r.name.trim(),
  category: r.category,
  wip_limit: r.wip_limit !== '' ? parseInt(r.wip_limit, 10) : undefined,
  order,
});

// ── collapsible section wrapper ───────────────────────────────────────────────
function Section(props: {
  title: string;
  description: string;
  badge?: string;
  open: boolean;
  onToggle: () => void;
  children: any;
}) {
  return (
    <div
      class="rounded-xl border overflow-hidden"
      style={{ 'border-color': 'var(--color-border-light)' }}
    >
      <button
        type="button"
        onClick={props.onToggle}
        class="w-full flex items-center justify-between p-4 text-left transition-colors hover:opacity-80"
        style={{ 'background-color': 'var(--color-bg-elevated)' }}
      >
        <div class="flex items-center gap-3">
          <div>
            <div class="flex items-center gap-2">
              <span class="font-semibold text-sm" style={{ color: 'var(--color-text-primary)' }}>
                {props.title}
              </span>
              <Show when={props.badge}>
                <Badge tone="success">{props.badge}</Badge>
              </Show>
            </div>
            <p class="text-xs mt-0.5" style={{ color: 'var(--color-text-tertiary)' }}>
              {props.description}
            </p>
          </div>
        </div>
        <Show when={props.open} fallback={<FiChevronRight size={16} style={{ color: 'var(--color-text-tertiary)' }} />}>
          <FiChevronDown size={16} style={{ color: 'var(--color-text-tertiary)' }} />
        </Show>
      </button>
      <Show when={props.open}>
        <div
          class="p-4 border-t space-y-3"
          style={{
            'border-color': 'var(--color-border-light)',
            'background-color': 'var(--color-bg-base)',
          }}
        >
          {props.children}
        </div>
      </Show>
    </div>
  );
}

// ── field type meta ───────────────────────────────────────────────────────────
const FIELD_TYPES = [
  { value: 'text',         label: 'Text',         icon: '📝', description: 'Short text input' },
  { value: 'long_text',    label: 'Long Text',     icon: '📄', description: 'Multi-line text area' },
  { value: 'number',       label: 'Number',        icon: '🔢', description: 'Numeric input' },
  { value: 'date',         label: 'Date',          icon: '📅', description: 'Date picker' },
  { value: 'boolean',      label: 'Checkbox',      icon: '☑️', description: 'True/false checkbox' },
  { value: 'select',       label: 'Select',        icon: '📋', description: 'Single choice dropdown' },
  { value: 'multi_select', label: 'Multi-Select',  icon: '✅', description: 'Multiple choice' },
  { value: 'url',          label: 'URL',           icon: '🔗', description: 'Website link' },
  { value: 'email',        label: 'Email',         icon: '📧', description: 'Email address' },
];

const fieldTypeLabel = (type: string) =>
  FIELD_TYPES.find(ft => ft.value === type)?.label ?? type;
const fieldTypeIcon = (type: string) =>
  FIELD_TYPES.find(ft => ft.value === type)?.icon ?? '📝';

// ── main component ────────────────────────────────────────────────────────────
export default function TemplateCreator() {
  const navigate = useNavigate();

  // basic info
  const [name, setName] = createSignal('');
  const [description, setDescription] = createSignal('');
  const [projectType, setProjectType] = createSignal<ProjectType>('software');

  // section open/closed
  const [workflowOpen, setWorkflowOpen] = createSignal(false);
  const [vocabOpen, setVocabOpen] = createSignal(false);
  const [fieldsOpen, setFieldsOpen] = createSignal(false);
  const [boardsOpen, setBoardsOpen] = createSignal(false);

  // ── workflow state ────────────────────────────────────────────────────────
  const [statusRows, setStatusRows] = createSignal<StatusRow[]>([]);

  const addStatus = () =>
    setStatusRows(rs => [
      ...rs,
      { id: `new-${rs.length}`, name: 'New Status', category: 'todo', wip_limit: '' },
    ]);
  const removeStatus = (id: string) =>
    setStatusRows(rs => rs.filter(r => r.id !== id));
  const setStatusField = (id: string, field: keyof StatusRow, value: string) =>
    setStatusRows(rs => rs.map(r => (r.id === id ? { ...r, [field]: value } : r)));

  // ── vocabulary state ──────────────────────────────────────────────────────
  const [vocabEdits, setVocabEdits] = createSignal<Record<string, string>>({});
  const setVocabKey = (key: string, value: string) =>
    setVocabEdits(prev => ({ ...prev, [key]: value }));
  const vocabOverrideCount = () =>
    Object.values(vocabEdits()).filter(v => v.trim().length > 0).length;

  // ── custom fields state ───────────────────────────────────────────────────
  const [customFields, setCustomFields] = createSignal<TemplateCustomField[]>([]);
  const [showFieldModal, setShowFieldModal] = createSignal(false);
  const [editingFieldIdx, setEditingFieldIdx] = createSignal<number | null>(null);
  const [fieldName, setFieldName] = createSignal('');
  const [fieldType, setFieldType] = createSignal<CustomFieldType>('text');
  const [fieldDescription, setFieldDescription] = createSignal('');
  const [fieldRequired, setFieldRequired] = createSignal(false);
  const [fieldOptions, setFieldOptions] = createSignal('');

  const openAddFieldModal = () => {
    setEditingFieldIdx(null);
    setFieldName('');
    setFieldType('text');
    setFieldDescription('');
    setFieldRequired(false);
    setFieldOptions('');
    setShowFieldModal(true);
  };

  const openEditFieldModal = (idx: number) => {
    const f = customFields()[idx];
    setEditingFieldIdx(idx);
    setFieldName(f.name);
    setFieldType(f.field_type);
    setFieldDescription(f.description ?? '');
    setFieldRequired(f.required ?? false);
    setFieldOptions(f.options?.join('\n') ?? '');
    setShowFieldModal(true);
  };

  const saveFieldModal = (e: Event) => {
    e.preventDefault();
    if (!fieldName().trim()) return;

    const needsOptions = fieldType() === 'select' || fieldType() === 'multi_select';
    const parsedOptions = fieldOptions()
      .split('\n')
      .map(o => o.trim())
      .filter(o => o.length > 0);

    if (needsOptions && parsedOptions.length === 0) {
      toast.error('Select fields require at least one option');
      return;
    }

    const def: TemplateCustomField = {
      name: fieldName().trim(),
      field_type: fieldType(),
      description: fieldDescription().trim() || null,
      required: fieldRequired(),
      options: needsOptions ? parsedOptions : null,
    };

    const idx = editingFieldIdx();
    if (idx !== null) {
      setCustomFields(fs => fs.map((f, i) => (i === idx ? def : f)));
    } else {
      setCustomFields(fs => [...fs, def]);
    }
    setShowFieldModal(false);
  };

  const removeField = (idx: number) =>
    setCustomFields(fs => fs.filter((_, i) => i !== idx));

  // ── boards state ──────────────────────────────────────────────────────────
  const [boards, setBoards] = createSignal<{ name: string; description: string }[]>([]);

  const addBoard = () =>
    setBoards(bs => [...bs, { name: `Board ${bs.length + 1}`, description: '' }]);
  const removeBoard = (idx: number) =>
    setBoards(bs => bs.filter((_, i) => i !== idx));
  const setBoardField = (idx: number, field: 'name' | 'description', value: string) =>
    setBoards(bs => bs.map((b, i) => (i === idx ? { ...b, [field]: value } : b)));

  // ── submit ────────────────────────────────────────────────────────────────
  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    if (!name().trim()) return;

    // workflow — only include if user added at least one status
    const validStatuses = statusRows().filter(r => r.name.trim());
    const workflow =
      validStatuses.length > 0
        ? {
            workflow_type: 'custom' as const,
            statuses: validStatuses.map(rowToStatus),
            transitions: undefined,
          }
        : null;

    // vocabulary — only include non-empty overrides
    const vocabEntries = Object.entries(vocabEdits()).filter(([, v]) => v.trim().length > 0);
    const vocabulary =
      vocabEntries.length > 0
        ? Object.fromEntries(vocabEntries.map(([k, v]) => [k, v.trim()]))
        : null;

    // custom fields
    const custom_fields = customFields().length > 0 ? customFields() : null;

    // boards — columns derived from workflow statuses (or empty means backend uses defaults)
    const statusNames = validStatuses.map(r => r.name.trim());
    const default_boards: TemplateBoardConfig[] | null =
      boards().length > 0
        ? boards().map((b, i) => ({
            name: b.name.trim() || `Board ${i + 1}`,
            description: b.description.trim() || null,
            is_default: i === 0,
            columns: statusNames.map(s => ({ status: s, wip_limit: null, collapsed: false })),
          }))
        : null;

    try {
      await api.templates.create({
        name: name().trim(),
        description: description().trim() || null,
        project_type: projectType(),
        vocabulary,
        workflow,
        custom_fields,
        default_boards,
      });

      toast.success(`Template "${name()}" created`);
      navigate('/templates');
    } catch (error) {
      toast.error('Failed to create template');
      console.error(error);
    }
  };

  const projectTypes = [
    { value: 'software',     label: 'Software Development', icon: '💻' },
    { value: 'web',          label: 'Web Project',          icon: '🌐' },
    { value: 'mobile',       label: 'Mobile App',           icon: '📱' },
    { value: 'construction', label: 'Construction',         icon: '🏗️' },
    { value: 'personal',     label: 'Personal',             icon: '👤' },
    { value: 'homework',     label: 'Homework',             icon: '📚' },
    { value: 'maintenance',  label: 'Maintenance',          icon: '🔧' },
    { value: 'custom',       label: 'Custom',               icon: '⚙️' },
  ];

  return (
    <div class="min-h-screen p-6" style={{ 'background-color': 'var(--color-bg-base)' }}>
      <div class="max-w-2xl mx-auto">
        {/* Header */}
        <div class="mb-8">
          <h1 class="text-3xl font-bold mb-2" style={{ color: 'var(--color-text-primary)' }}>
            Create Project Template
          </h1>
          <p style={{ color: 'var(--color-text-secondary)' }}>
            Define a reusable project blueprint — workflow, vocabulary, custom fields, and boards.
            Sections left collapsed use the project type defaults.
          </p>
        </div>

        <form onSubmit={handleSubmit} class="space-y-4">
          {/* ── Basic Information ─────────────────────────────────────────── */}
          <div
            class="rounded-xl border p-6 space-y-4"
            style={{
              'border-color': 'var(--color-border-light)',
              'background-color': 'var(--color-bg-elevated)',
            }}
          >
            <h3 class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
              Basic Information
            </h3>

            <Field
              label="Template Name"
              required
              value={name()}
              onInput={(e) => setName(e.currentTarget.value)}
              placeholder="e.g., Web Development Project"
            />

            <FieldShell label="Description" for="template-description">
              <textarea
                id="template-description"
                value={description()}
                onInput={(e) => setDescription(e.currentTarget.value)}
                rows={2}
                placeholder="Describe what this template is for..."
                class="w-full resize-none rounded-lg border px-3 py-2 text-sm transition-colors focus:outline-none focus-visible:ring-2"
                style={{
                  'background-color': 'var(--color-bg-base)',
                  color: 'var(--color-text-primary)',
                  'border-color': 'var(--color-border-medium)',
                  '--tw-ring-color': 'var(--color-focus-ring)',
                }}
              />
            </FieldShell>

            <Select
              label="Project Type"
              required
              value={projectType()}
              onChange={(e) => setProjectType(e.currentTarget.value as ProjectType)}
            >
              <For each={projectTypes}>
                {(type) => (
                  <option value={type.value}>
                    {type.icon} {type.label}
                  </option>
                )}
              </For>
            </Select>
          </div>

          {/* ── Workflow ──────────────────────────────────────────────────── */}
          <Section
            title="Workflow Statuses"
            description="Define the columns and WIP limits for this template's board."
            badge={statusRows().filter(r => r.name.trim()).length > 0
              ? `${statusRows().filter(r => r.name.trim()).length} statuses`
              : undefined}
            open={workflowOpen()}
            onToggle={() => setWorkflowOpen(o => !o)}
          >
            <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
              Leave empty to use the default workflow for the selected project type.
            </p>

            <div class="space-y-2">
              <For each={statusRows()}>
                {(row) => (
                  <div
                    class="flex items-center gap-3 rounded-lg border p-3"
                    style={{
                      'border-color': 'var(--color-border-light)',
                      'background-color': 'var(--color-bg-elevated)',
                    }}
                  >
                    <input
                      type="text"
                      value={row.name}
                      placeholder="Status name"
                      onInput={(e) => setStatusField(row.id, 'name', e.currentTarget.value)}
                      class="flex-1 rounded-lg border px-3 py-1.5 text-sm focus:outline-none focus-visible:ring-2"
                      style={{
                        'background-color': 'var(--color-bg-base)',
                        'border-color': 'var(--color-border-medium)',
                        color: 'var(--color-text-primary)',
                        '--tw-ring-color': 'var(--color-focus-ring)',
                      }}
                    />
                    <select
                      value={row.category}
                      onChange={(e) => setStatusField(row.id, 'category', e.currentTarget.value)}
                      class="rounded-lg border px-3 py-1.5 text-sm focus:outline-none focus-visible:ring-2"
                      style={{
                        'background-color': 'var(--color-bg-base)',
                        'border-color': 'var(--color-border-medium)',
                        color: 'var(--color-text-primary)',
                        '--tw-ring-color': 'var(--color-focus-ring)',
                      }}
                    >
                      <option value="todo">To Do</option>
                      <option value="in_progress">In Progress</option>
                      <option value="done">Done</option>
                    </select>
                    <div class="flex flex-shrink-0 items-center gap-1.5">
                      <span class="whitespace-nowrap text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                        WIP
                      </span>
                      <input
                        type="number"
                        min="1"
                        value={row.wip_limit}
                        placeholder="∞"
                        onInput={(e) => setStatusField(row.id, 'wip_limit', e.currentTarget.value)}
                        class="w-16 rounded-lg border px-2 py-1.5 text-center text-sm focus:outline-none focus-visible:ring-2"
                        style={{
                          'background-color': 'var(--color-bg-base)',
                          'border-color': 'var(--color-border-medium)',
                          color: 'var(--color-text-primary)',
                          '--tw-ring-color': 'var(--color-focus-ring)',
                        }}
                      />
                    </div>
                    <button
                      type="button"
                      onClick={() => removeStatus(row.id)}
                      class="shrink-0 rounded-md p-1.5"
                      style={{ color: 'var(--color-text-tertiary)' }}
                      title="Remove status"
                    >
                      <FiTrash2 size={16} />
                    </button>
                  </div>
                )}
              </For>
            </div>

            <button
              type="button"
              onClick={addStatus}
              class="flex items-center gap-2 rounded-lg border border-dashed px-4 py-2 text-sm font-medium w-full justify-center"
              style={{ 'border-color': 'var(--color-border-medium)', color: 'var(--color-text-secondary)' }}
            >
              <FiPlus size={16} /> Add Status
            </button>
          </Section>

          {/* ── Vocabulary ────────────────────────────────────────────────── */}
          <Section
            title="Vocabulary"
            description="Rename terms to match your domain (Task → Work Order, Sprint → Phase, etc.)"
            badge={vocabOverrideCount() > 0 ? `${vocabOverrideCount()} overrides` : undefined}
            open={vocabOpen()}
            onToggle={() => setVocabOpen(o => !o)}
          >
            <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
              Leave fields blank to keep the default label.
            </p>

            <div
              class="overflow-hidden rounded-xl border"
              style={{ 'border-color': 'var(--color-border-light)' }}
            >
              <table class="w-full text-sm">
                <thead>
                  <tr style={{ 'background-color': 'var(--color-bg-subtle)' }}>
                    <th
                      class="w-1/3 px-4 py-3 text-left font-semibold"
                      style={{ color: 'var(--color-text-secondary)' }}
                    >
                      Default
                    </th>
                    <th
                      class="px-4 py-3 text-left font-semibold"
                      style={{ color: 'var(--color-text-secondary)' }}
                    >
                      Custom label
                    </th>
                  </tr>
                </thead>
                <tbody>
                  <For each={VOCAB_KEYS}>
                    {(key) => {
                      const def = resolveLabel(undefined, key);
                      return (
                        <tr
                          class="border-t"
                          style={{ 'border-color': 'var(--color-border-light)' }}
                        >
                          <td
                            class="px-4 py-2.5 font-medium"
                            style={{ color: 'var(--color-text-secondary)' }}
                          >
                            {def}
                          </td>
                          <td class="px-4 py-2">
                            <input
                              type="text"
                              value={vocabEdits()[key] ?? ''}
                              placeholder={def}
                              onInput={(e) => setVocabKey(key, e.currentTarget.value)}
                              class="w-full rounded-lg border px-3 py-1.5 text-sm focus:outline-none focus-visible:ring-2"
                              style={{
                                'background-color': 'var(--color-bg-base)',
                                'border-color': 'var(--color-border-medium)',
                                color: 'var(--color-text-primary)',
                                '--tw-ring-color': 'var(--color-focus-ring)',
                              }}
                            />
                          </td>
                        </tr>
                      );
                    }}
                  </For>
                </tbody>
              </table>
            </div>
          </Section>

          {/* ── Custom Fields ─────────────────────────────────────────────── */}
          <Section
            title="Custom Fields"
            description="Pre-define extra fields that all items in this project will have."
            badge={customFields().length > 0 ? `${customFields().length} fields` : undefined}
            open={fieldsOpen()}
            onToggle={() => setFieldsOpen(o => !o)}
          >
            <Show
              when={customFields().length > 0}
              fallback={
                <p class="text-xs text-center py-3" style={{ color: 'var(--color-text-tertiary)' }}>
                  No custom fields defined — add one below.
                </p>
              }
            >
              <div class="space-y-2">
                <For each={customFields()}>
                  {(field, i) => (
                    <div
                      class="flex items-center gap-3 rounded-lg border px-4 py-3"
                      style={{
                        'border-color': 'var(--color-border-light)',
                        'background-color': 'var(--color-bg-elevated)',
                      }}
                    >
                      <span class="text-lg">{fieldTypeIcon(field.field_type)}</span>
                      <div class="flex-1 min-w-0">
                        <div class="flex items-center gap-2">
                          <span class="font-medium text-sm" style={{ color: 'var(--color-text-primary)' }}>
                            {field.name}
                          </span>
                          <Badge>{fieldTypeLabel(field.field_type)}</Badge>
                          <Show when={field.required}>
                            <Badge tone="danger">Required</Badge>
                          </Show>
                        </div>
                        <Show when={field.options && field.options.length > 0}>
                          <p class="text-xs mt-0.5 truncate" style={{ color: 'var(--color-text-tertiary)' }}>
                            Options: {field.options!.join(', ')}
                          </p>
                        </Show>
                      </div>
                      <button
                        type="button"
                        onClick={() => openEditFieldModal(i())}
                        class="text-xs px-2 py-1 rounded"
                        style={{ color: 'var(--color-text-secondary)', 'background-color': 'var(--color-bg-base)' }}
                      >
                        Edit
                      </button>
                      <button
                        type="button"
                        onClick={() => removeField(i())}
                        class="shrink-0 rounded-md p-1.5"
                        style={{ color: 'var(--color-text-tertiary)' }}
                        title="Remove field"
                      >
                        <FiTrash2 size={16} />
                      </button>
                    </div>
                  )}
                </For>
              </div>
            </Show>

            <button
              type="button"
              onClick={openAddFieldModal}
              class="flex items-center gap-2 rounded-lg border border-dashed px-4 py-2 text-sm font-medium w-full justify-center"
              style={{ 'border-color': 'var(--color-border-medium)', color: 'var(--color-text-secondary)' }}
            >
              <FiPlus size={16} /> Add Custom Field
            </button>
          </Section>

          {/* ── Board Configuration ───────────────────────────────────────── */}
          <Section
            title="Board Configuration"
            description="Name the boards for this template. Columns are derived from the workflow statuses."
            badge={boards().length > 0 ? `${boards().length} board${boards().length > 1 ? 's' : ''}` : undefined}
            open={boardsOpen()}
            onToggle={() => setBoardsOpen(o => !o)}
          >
            <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
              Leave empty for a single default board. The first board is always the default.
            </p>

            <div class="space-y-2">
              <For each={boards()}>
                {(board, i) => (
                  <div
                    class="flex items-start gap-3 rounded-lg border p-3"
                    style={{
                      'border-color': 'var(--color-border-light)',
                      'background-color': 'var(--color-bg-elevated)',
                    }}
                  >
                    <div class="flex-1 space-y-2">
                      <input
                        type="text"
                        value={board.name}
                        placeholder="Board name"
                        onInput={(e) => setBoardField(i(), 'name', e.currentTarget.value)}
                        class="w-full rounded-lg border px-3 py-1.5 text-sm focus:outline-none focus-visible:ring-2"
                        style={{
                          'background-color': 'var(--color-bg-base)',
                          'border-color': 'var(--color-border-medium)',
                          color: 'var(--color-text-primary)',
                          '--tw-ring-color': 'var(--color-focus-ring)',
                        }}
                      />
                      <input
                        type="text"
                        value={board.description}
                        placeholder="Description (optional)"
                        onInput={(e) => setBoardField(i(), 'description', e.currentTarget.value)}
                        class="w-full rounded-lg border px-3 py-1.5 text-sm focus:outline-none focus-visible:ring-2"
                        style={{
                          'background-color': 'var(--color-bg-base)',
                          'border-color': 'var(--color-border-medium)',
                          color: 'var(--color-text-primary)',
                          '--tw-ring-color': 'var(--color-focus-ring)',
                        }}
                      />
                    </div>
                    <Show when={i() === 0}>
                      <span class="text-xs px-2 py-1 rounded mt-1" style={{ color: 'var(--color-text-tertiary)', 'background-color': 'var(--color-bg-subtle)' }}>
                        Default
                      </span>
                    </Show>
                    <button
                      type="button"
                      onClick={() => removeBoard(i())}
                      class="flex-shrink-0 rounded-md p-1.5 mt-1"
                      style={{ color: 'var(--color-text-tertiary)' }}
                      title="Remove board"
                    >
                      <FiTrash2 size={16} />
                    </button>
                  </div>
                )}
              </For>
            </div>

            <button
              type="button"
              onClick={addBoard}
              class="flex items-center gap-2 rounded-lg border border-dashed px-4 py-2 text-sm font-medium w-full justify-center"
              style={{ 'border-color': 'var(--color-border-medium)', color: 'var(--color-text-secondary)' }}
            >
              <FiPlus size={16} /> Add Board
            </button>
          </Section>

          {/* ── Form Actions ──────────────────────────────────────────────── */}
          <div
            class="flex justify-end gap-2 border-t pt-4"
            style={{ 'border-color': 'var(--color-border-light)' }}
          >
            <Button type="button" variant="secondary" onClick={() => navigate('/templates')}>
              Cancel
            </Button>
            <Button type="submit">Create Template</Button>
          </div>
        </form>
      </div>

      {/* ── Custom Field Modal ─────────────────────────────────────────────── */}
      <Modal
        isOpen={showFieldModal()}
        onClose={() => setShowFieldModal(false)}
        title={editingFieldIdx() !== null ? 'Edit Custom Field' : 'Add Custom Field'}
      >
        <form onSubmit={saveFieldModal} class="space-y-4">
          <Field
            label="Field Name"
            required
            value={fieldName()}
            onInput={(e) => setFieldName(e.currentTarget.value)}
            placeholder="e.g., Client Name"
          />

          <Select
            label="Field Type"
            required
            value={fieldType()}
            onChange={(e) => setFieldType(e.currentTarget.value as CustomFieldType)}
          >
            <For each={FIELD_TYPES}>
              {(type) => (
                <option value={type.value}>
                  {type.icon} {type.label} — {type.description}
                </option>
              )}
            </For>
          </Select>

          <FieldShell label="Description" for="cf-description">
            <textarea
              id="cf-description"
              value={fieldDescription()}
              onInput={(e) => setFieldDescription(e.currentTarget.value)}
              rows={2}
              placeholder="Optional description"
              class="w-full resize-none rounded-lg border px-3 py-2 text-sm transition-colors focus:outline-none focus-visible:ring-2"
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
              for="cf-options"
              hint="Enter each option on a new line"
            >
              <textarea
                id="cf-options"
                value={fieldOptions()}
                onInput={(e) => setFieldOptions(e.currentTarget.value)}
                rows={4}
                required
                placeholder={'Option 1\nOption 2\nOption 3'}
                class="w-full rounded-lg border px-3 py-2 font-mono text-sm transition-colors focus:outline-none focus-visible:ring-2"
                style={{
                  'background-color': 'var(--color-bg-base)',
                  color: 'var(--color-text-primary)',
                  'border-color': 'var(--color-border-medium)',
                  '--tw-ring-color': 'var(--color-focus-ring)',
                }}
              />
            </FieldShell>
          </Show>

          <label
            class="flex items-center gap-2 text-sm"
            style={{ color: 'var(--color-text-secondary)' }}
          >
            <input
              type="checkbox"
              checked={fieldRequired()}
              onChange={(e) => setFieldRequired(e.currentTarget.checked)}
              class="h-4 w-4 rounded"
            />
            Required field (must be filled for all items)
          </label>

          <div
            class="flex justify-end gap-2 border-t pt-4"
            style={{ 'border-color': 'var(--color-border-light)' }}
          >
            <Button type="button" variant="secondary" onClick={() => setShowFieldModal(false)}>
              Cancel
            </Button>
            <Button type="submit">
              {editingFieldIdx() !== null ? 'Update Field' : 'Add Field'}
            </Button>
          </div>
        </form>
      </Modal>
    </div>
  );
}
