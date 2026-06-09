import { type Component, createSignal } from 'solid-js';
import Modal from '../../shared/ui/Modal';
import { api } from '../../shared/api';
import { toast } from '../../shared/ui/toast';

export interface CreateProjectModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: () => void;
}

const CreateProjectModal: Component<CreateProjectModalProps> = (props) => {
  const [name, setName] = createSignal('');
  const [description, setDescription] = createSignal('');
  const [projectType, setProjectType] = createSignal('software');
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
        template: projectType(),
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
        {/* Error message */}
        {error() && (
          <div class="bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg p-3 text-red-700 dark:text-red-300 text-sm">
            {error()}
          </div>
        )}

        {/* Name */}
        <div>
          <label
            for="project-name"
            class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
          >
            Project Name <span class="text-red-500">*</span>
          </label>
          <input
            id="project-name"
            type="text"
            value={name()}
            onInput={(e) => setName(e.currentTarget.value)}
            placeholder="My Awesome Project"
            class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white placeholder-gray-400 focus:ring-2 focus:ring-purple-500 focus:border-transparent"
            required
            disabled={loading()}
          />
        </div>

        {/* Description */}
        <div>
          <label
            for="project-description"
            class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
          >
            Description
          </label>
          <textarea
            id="project-description"
            value={description()}
            onInput={(e) => setDescription(e.currentTarget.value)}
            placeholder="A brief description of your project..."
            rows={3}
            class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white placeholder-gray-400 focus:ring-2 focus:ring-purple-500 focus:border-transparent resize-none"
            disabled={loading()}
          />
        </div>

        {/* Project Type */}
        <div>
          <label
            for="project-type"
            class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
          >
            Project Type
          </label>
          <select
            id="project-type"
            value={projectType()}
            onChange={(e) => setProjectType(e.currentTarget.value)}
            class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:ring-2 focus:ring-purple-500 focus:border-transparent"
            disabled={loading()}
          >
            <option value="software">Software (Scrum)</option>
            <option value="web">Web Development (Scrum)</option>
            <option value="mobile">Mobile App (Scrum)</option>
            <option value="construction">Construction (Phase-based)</option>
            <option value="personal">Personal (Simple)</option>
            <option value="homework">Homework (Simple)</option>
            <option value="maintenance">Maintenance (Kanban)</option>
            <option value="custom">Custom</option>
          </select>
          <p class="mt-1 text-xs text-gray-500 dark:text-gray-400">
            Determines default workflow and terminology
          </p>
        </div>

        {/* Actions */}
        <div class="flex gap-3 pt-4">
          <button
            type="button"
            onClick={handleClose}
            disabled={loading()}
            class="flex-1 px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={loading() || !name().trim()}
            class="flex-1 px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2"
          >
            {loading() ? (
              <>
                <svg
                  class="animate-spin h-4 w-4"
                  fill="none"
                  viewBox="0 0 24 24"
                >
                  <circle
                    class="opacity-25"
                    cx="12"
                    cy="12"
                    r="10"
                    stroke="currentColor"
                    stroke-width="4"
                  />
                  <path
                    class="opacity-75"
                    fill="currentColor"
                    d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                  />
                </svg>
                Creating...
              </>
            ) : (
              'Create Project'
            )}
          </button>
        </div>
      </form>
    </Modal>
  );
};

export default CreateProjectModal;
