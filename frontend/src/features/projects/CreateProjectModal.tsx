import { type Component, createSignal, Show } from 'solid-js';
import { Modal, Button, Field, Select, FieldShell } from '../../shared/ui';
import { api } from '../../shared/api';
// 1. Added ProjectType to the imports
import type { ProjectType } from '../../shared/types'; 
import { toast } from '../../shared/ui/toast';

const PROJECT_TYPE_OPTIONS = [
  { value: 'software', label: 'Software (Scrum)' },
  { value: 'web', label: 'Web Development (Scrum)' },
  { value: 'mobile', label: 'Mobile App (Scrum)' },
  { value: 'construction', label: 'Construction (Phase-based)' },
  { value: 'personal', label: 'Personal (Simple)' },
  { value: 'homework', label: 'Homework (Simple)' },
  { value: 'maintenance', label: 'Maintenance (Kanban)' },
  { value: 'custom', label: 'Custom' },
];

export interface CreateProjectModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: () => void;
}

const CreateProjectModal: Component<CreateProjectModalProps> = (props) => {
  const [name, setName] = createSignal('');
  const [description, setDescription] = createSignal('');
  // 2. Strongly typed the signal
  const [projectType, setProjectType] = createSignal<ProjectType>('software'); 
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal('');

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    setError('');

    if (!name().trim()) {
      setError('Project name is required');
      return;
    }

    setLoading(true);
    try {
      await api.projects.create({
        name: name().trim(),
        description: description().trim() || undefined,
        // Since projectType is strictly typed now, no need for the "as CreateProject['project_type']" cast here!
        project_type: projectType(), 
      });

      // Reset form
      setName('');
      setDescription('');
      setProjectType('software');

      toast.success('Project created successfully');
      props.onSuccess();
      props.onClose();
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
      setName('');
      setDescription('');
      setProjectType('software');
      setError('');
      props.onClose();
    }
  };

  return (
    <Modal isOpen={props.isOpen} onClose={handleClose} title="Create New Project">
      <form onSubmit={handleSubmit} class="space-y-4">
        <Show when={error()}>
          <div
            class="rounded-lg border p-3 text-sm"
            style={{
              'background-color': 'var(--color-danger-50)',
              'border-color': 'var(--color-danger-100)',
              color: 'var(--color-danger-700)',
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
            rows={3}
            disabled={loading()}
            class="w-full resize-none rounded-lg border px-3 py-2 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1 disabled:opacity-50"
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
          value={projectType()}
          // 3. Cast the generic string from the HTML event to ProjectType
          onChange={(e) => setProjectType(e.currentTarget.value as ProjectType)} 
          disabled={loading()}
          options={PROJECT_TYPE_OPTIONS}
          hint="Determines default workflow and terminology"
        />

        <div class="flex gap-3 pt-4">
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
