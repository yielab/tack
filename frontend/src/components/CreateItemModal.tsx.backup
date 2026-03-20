import { type Component, createSignal, Show, createEffect } from 'solid-js';
import Modal from './Modal';
import { api } from '../lib/api';
import type { CreateItem, Item } from '../types/api';
import { toast } from '../lib/toast';

export interface CreateItemModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: () => void;
  projectId: string;
  initialStatus?: string;
  mode?: 'create' | 'edit';
  existingItem?: Item;
}

const CreateItemModal: Component<CreateItemModalProps> = (props) => {
  const mode = () => props.mode || 'create';
  const [title, setTitle] = createSignal('');
  const [description, setDescription] = createSignal('');
  const [itemType, setItemType] = createSignal('task');
  const [priority, setPriority] = createSignal('medium');
  const [estimate, setEstimate] = createSignal('');
  const [tags, setTags] = createSignal('');
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal('');

  // Pre-fill form when editing
  createEffect(() => {
    if (props.isOpen && mode() === 'edit' && props.existingItem) {
      const item = props.existingItem;
      setTitle(item.title);
      setDescription(item.description || '');
      setItemType(typeof item.item_type === 'string' ? item.item_type : 'task');
      setPriority(item.priority);
      setEstimate(item.estimate?.toString() || '');
      setTags(item.tags.join(', '));
    } else if (props.isOpen && mode() === 'create') {
      // Reset form for create mode
      setTitle('');
      setDescription('');
      setItemType('task');
      setPriority('medium');
      setEstimate('');
      setTags('');
    }
  });

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    setError('');

    if (!title().trim()) {
      setError('Title is required');
      return;
    }

    setLoading(true);
    try {
      if (mode() === 'edit' && props.existingItem) {
        // Update existing item
        await api.updateItem(props.existingItem.id, {
          title: title().trim(),
          description: description().trim() || undefined,
          priority: priority() as any,
          estimate: estimate() ? parseFloat(estimate()) : undefined,
          tags: tags()
            .split(',')
            .map((t) => t.trim())
            .filter((t) => t.length > 0),
        });
        toast.success('Item updated successfully');
      } else {
        // Create new item
        const itemData: CreateItem = {
          title: title().trim(),
          description: description().trim() || undefined,
          item_type: itemType() as any,
          priority: priority() as any,
          estimate: estimate() ? parseFloat(estimate()) : undefined,
          tags: tags()
            .split(',')
            .map((t) => t.trim())
            .filter((t) => t.length > 0),
        };

        await api.createItem(props.projectId, itemData);
        toast.success('Item created successfully');
      }

      props.onSuccess();
      props.onClose();
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : `Failed to ${mode()} item`;
      setError(errorMessage);
      toast.error(errorMessage);
    } finally {
      setLoading(false);
    }
  };

  const handleClose = () => {
    if (!loading()) {
      setTitle('');
      setDescription('');
      setItemType('task');
      setPriority('medium');
      setEstimate('');
      setTags('');
      setError('');
      props.onClose();
    }
  };

  return (
    <Modal
      isOpen={props.isOpen}
      onClose={handleClose}
      title={mode() === 'edit' ? 'Edit Item' : 'Create New Item'}
      size="lg"
    >
      <form onSubmit={handleSubmit} class="space-y-4">
        {/* Error message */}
        <Show when={error()}>
          <div class="bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg p-3 text-red-700 dark:text-red-300 text-sm">
            {error()}
          </div>
        </Show>

        {/* Title */}
        <div>
          <label
            for="item-title"
            class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
          >
            Title <span class="text-red-500">*</span>
          </label>
          <input
            id="item-title"
            type="text"
            value={title()}
            onInput={(e) => setTitle(e.currentTarget.value)}
            placeholder="What needs to be done?"
            class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white placeholder-gray-400 focus:ring-2 focus:ring-purple-500 focus:border-transparent"
            required
            disabled={loading()}
            autofocus
          />
        </div>

        {/* Description */}
        <div>
          <label
            for="item-description"
            class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
          >
            Description
          </label>
          <textarea
            id="item-description"
            value={description()}
            onInput={(e) => setDescription(e.currentTarget.value)}
            placeholder="Additional details..."
            rows={4}
            class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white placeholder-gray-400 focus:ring-2 focus:ring-purple-500 focus:border-transparent resize-none"
            disabled={loading()}
          />
        </div>

        <div class="grid grid-cols-2 gap-4">
          {/* Item Type (disabled in edit mode) */}
          <div>
            <label
              for="item-type"
              class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
            >
              Type
            </label>
            <select
              id="item-type"
              value={itemType()}
              onChange={(e) => setItemType(e.currentTarget.value)}
              class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:ring-2 focus:ring-purple-500 focus:border-transparent disabled:opacity-50 disabled:cursor-not-allowed"
              disabled={loading() || mode() === 'edit'}
            >
              <option value="epic">Epic</option>
              <option value="feature">Feature</option>
              <option value="task">Task</option>
              <option value="subtask">Subtask</option>
              <option value="bug">Bug</option>
              <option value="requirement">Requirement</option>
            </select>
            {mode() === 'edit' && (
              <p class="mt-1 text-xs text-gray-500 dark:text-gray-400">
                Item type cannot be changed
              </p>
            )}
          </div>

          {/* Priority */}
          <div>
            <label
              for="item-priority"
              class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
            >
              Priority
            </label>
            <select
              id="item-priority"
              value={priority()}
              onChange={(e) => setPriority(e.currentTarget.value)}
              class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white focus:ring-2 focus:ring-purple-500 focus:border-transparent"
              disabled={loading()}
            >
              <option value="critical">Critical</option>
              <option value="high">High</option>
              <option value="medium">Medium</option>
              <option value="low">Low</option>
              <option value="none">None</option>
            </select>
          </div>
        </div>

        {/* Estimate */}
        <div>
          <label
            for="item-estimate"
            class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
          >
            Estimate (Story Points)
          </label>
          <input
            id="item-estimate"
            type="number"
            min="0"
            step="0.5"
            value={estimate()}
            onInput={(e) => setEstimate(e.currentTarget.value)}
            placeholder="e.g., 5"
            class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white placeholder-gray-400 focus:ring-2 focus:ring-purple-500 focus:border-transparent"
            disabled={loading()}
          />
        </div>

        {/* Tags */}
        <div>
          <label
            for="item-tags"
            class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1"
          >
            Tags
          </label>
          <input
            id="item-tags"
            type="text"
            value={tags()}
            onInput={(e) => setTags(e.currentTarget.value)}
            placeholder="frontend, api, security (comma-separated)"
            class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white placeholder-gray-400 focus:ring-2 focus:ring-purple-500 focus:border-transparent"
            disabled={loading()}
          />
          <p class="mt-1 text-xs text-gray-500 dark:text-gray-400">
            Separate multiple tags with commas
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
            disabled={loading() || !title().trim()}
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
                {mode() === 'edit' ? 'Updating...' : 'Creating...'}
              </>
            ) : (
              mode() === 'edit' ? 'Update Item' : 'Create Item'
            )}
          </button>
        </div>
      </form>
    </Modal>
  );
};

export default CreateItemModal;
