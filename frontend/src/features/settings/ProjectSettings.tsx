import { type Component, createSignal, Show, Switch, Match } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import Tabs, { type TabItem } from '../../shared/ui/Tabs';
import { useProject } from '../../shared/state/projectContext';
import { Button, Field, FieldShell, Modal } from '../../shared/ui';
import { toast } from '../../shared/ui/toast';
import { api } from '../../shared/api';
import GeneralPanel from './panels/GeneralPanel';
import WorkflowPanel from './panels/WorkflowPanel';
import VocabularyPanel from './panels/VocabularyPanel';
import FieldsPanel from './panels/FieldsPanel';
import RolesPanel from './panels/RolesPanel';
import DataPanel from './panels/DataPanel';

const TABS: TabItem[] = [
  { id: 'general', label: 'General' },
  { id: 'workflow', label: 'Workflow' },
  { id: 'vocabulary', label: 'Vocabulary' },
  { id: 'fields', label: 'Fields' },
  { id: 'roles', label: 'Roles' },
  { id: 'data', label: 'Data' },
];

/** One tabbed surface for every project setting. */
const ProjectSettings: Component = () => {
  const { project, projectId } = useProject();
  const navigate = useNavigate();
  const [active, setActive] = createSignal('general');

  // Save as template state
  const [showSaveModal, setShowSaveModal] = createSignal(false);
  const [templateName, setTemplateName] = createSignal('');
  const [templateDesc, setTemplateDesc] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  const openSaveModal = () => {
    setTemplateName(project()?.name ?? '');
    setTemplateDesc('');
    setShowSaveModal(true);
  };

  const handleSaveAsTemplate = async (e: Event) => {
    e.preventDefault();
    const id = projectId();
    if (!id || !templateName().trim()) return;
    setSaving(true);
    try {
      await api.templates.saveProjectAsTemplate(id, {
        name: templateName().trim(),
        description: templateDesc().trim() || null,
      });
      toast.success(`Template "${templateName()}" saved`);
      setShowSaveModal(false);
      navigate('/templates');
    } catch {
      toast.error('Failed to save template');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="mx-auto max-w-4xl px-6 py-8">
      <div class="flex items-start justify-between mb-1">
        <h1 class="text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
          Project Settings
        </h1>
        <Button variant="secondary" size="sm" onClick={openSaveModal}>
          Save as Template
        </Button>
      </div>
      <Show when={project()}>
        <p class="mb-6 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          {project()!.name}
        </p>
      </Show>

      <Tabs tabs={TABS} active={active()} onChange={setActive}>
        <Switch>
          <Match when={active() === 'general'}>
            <GeneralPanel />
          </Match>
          <Match when={active() === 'workflow'}>
            <WorkflowPanel />
          </Match>
          <Match when={active() === 'vocabulary'}>
            <VocabularyPanel />
          </Match>
          <Match when={active() === 'fields'}>
            <FieldsPanel />
          </Match>
          <Match when={active() === 'roles'}>
            <RolesPanel />
          </Match>
          <Match when={active() === 'data'}>
            <DataPanel />
          </Match>
        </Switch>
      </Tabs>

      {/* Save as Template modal */}
      <Modal
        isOpen={showSaveModal()}
        onClose={() => setShowSaveModal(false)}
        title="Save Project as Template"
        size="sm"
      >
        <p class="text-sm mb-4" style={{ color: 'var(--color-text-secondary)' }}>
          Saves this project's workflow, vocabulary, custom field definitions, and boards as a
          reusable template. Items are not copied.
        </p>

        <form onSubmit={handleSaveAsTemplate} class="space-y-4">
          <Field
            label="Template Name"
            required
            value={templateName()}
            onInput={(e) => setTemplateName(e.currentTarget.value)}
            placeholder="My Template"
          />

          <FieldShell label="Description" for="save-tpl-desc">
            <textarea
              id="save-tpl-desc"
              value={templateDesc()}
              onInput={(e) => setTemplateDesc(e.currentTarget.value)}
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

          <div class="flex justify-end gap-2 pt-2">
            <Button
              type="button"
              variant="secondary"
              onClick={() => setShowSaveModal(false)}
            >
              Cancel
            </Button>
            <Button type="submit" loading={saving()} disabled={saving()}>
              Save Template
            </Button>
          </div>
        </form>
      </Modal>
    </div>
  );
};

export default ProjectSettings;
