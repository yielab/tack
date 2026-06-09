import { type Component, createSignal, Show, createEffect, For, createResource, createMemo } from 'solid-js';
import Modal from './Modal';
import RichTextEditor from './RichTextEditor';
import { api } from '../lib/api';
import type { CreateItem, Item } from '../types/api';
import { toast } from '../lib/toast';
import { getItemTypeMap } from '../lib/vocab';
import { FiPlus, FiX, FiTrash2 } from 'solid-icons/fi';

export interface CreateItemModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: () => void;
  projectId: string;
  vocabulary?: Record<string, string>;
  initialStatus?: string;
  mode?: 'create' | 'edit';
  existingItem?: Item;
  parentId?: string;
}

type ItemType = 'epic' | 'feature' | 'task' | 'subtask' | 'bug' | 'requirement';
type Priority = 'critical' | 'high' | 'medium' | 'low' | 'none';

interface SubtaskItem {
  id: string;
  title: string;
}


const priorityConfig: Record<Priority, { label: string; color: string; emoji: string }> = {
  critical: { label: 'Critical', color: 'red', emoji: '🔥' },
  high: { label: 'High', color: 'orange', emoji: '⬆️' },
  medium: { label: 'Medium', color: 'yellow', emoji: '➡️' },
  low: { label: 'Low', color: 'green', emoji: '⬇️' },
  none: { label: 'None', color: 'gray', emoji: '➖' },
};

const estimateOptions = [0, 0.5, 1, 2, 3, 5, 8, 13, 21];

const CreateItemModal: Component<CreateItemModalProps> = (props) => {
  const mode = () => props.mode || 'create';
  const itemTypeConfig = createMemo(() => getItemTypeMap(props.vocabulary));
  const [title, setTitle] = createSignal('');
  const [description, setDescription] = createSignal('');
  const [itemType, setItemType] = createSignal<ItemType>('task');
  const [priority, setPriority] = createSignal<Priority>('medium');
  const [estimate, setEstimate] = createSignal<number | null>(null);
  const [tags, setTags] = createSignal<string[]>([]);
  const [tagInput, setTagInput] = createSignal('');
  const [subtasks, setSubtasks] = createSignal<SubtaskItem[]>([]);
  const [subtaskInput, setSubtaskInput] = createSignal('');
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal('');

  // Load child items when in edit mode
  const [childItems, { refetch: refetchChildren }] = createResource(
    () => (mode() === 'edit' && props.existingItem ? props.existingItem.id : null),
    async (parentId) => {
      if (!parentId || !props.projectId) return [];
      const items = await api.listItems(props.projectId);
      return items.filter(item => item.parent_id === parentId);
    }
  );

  // Pre-fill form when editing
  createEffect(() => {
    if (props.isOpen && mode() === 'edit' && props.existingItem) {
      const item = props.existingItem;
      setTitle(item.title);
      setDescription(item.description || '');
      setItemType(typeof item.item_type === 'string' ? item.item_type as ItemType : 'task');
      setPriority(item.priority as Priority);
      setEstimate(item.estimate || null);
      setTags(item.tags || []);
    } else if (props.isOpen && mode() === 'create') {
      // Reset form for create mode
      setTitle('');
      setDescription('');
      setItemType('task');
      setPriority('medium');
      setEstimate(null);
      setTags([]);
      setSubtasks([]);
    }
  });

  const addTag = () => {
    const tag = tagInput().trim();
    if (tag && !tags().includes(tag)) {
      setTags([...tags(), tag]);
      setTagInput('');
    }
  };

  const removeTag = (tag: string) => {
    setTags(tags().filter((t) => t !== tag));
  };

  const addSubtask = () => {
    const title = subtaskInput().trim();
    if (title) {
      setSubtasks([...subtasks(), { id: crypto.randomUUID(), title }]);
      setSubtaskInput('');
    }
  };

  const removeSubtask = (id: string) => {
    setSubtasks(subtasks().filter((s) => s.id !== id));
  };

  const handleCreateChild = async () => {
    const title = subtaskInput().trim();
    if (!title || !props.existingItem) return;

    try {
      await api.createItem(props.projectId, {
        title,
        item_type: 'subtask',
        priority: 'medium',
        parent_id: props.existingItem.id,
      });
      toast.success('Child item created');
      setSubtaskInput('');
      await refetchChildren();
    } catch (err) {
      toast.error('Failed to create child item');
    }
  };

  const handleDeleteChild = async (itemId: string) => {
    if (!confirm('Delete this child item?')) return;

    try {
      await api.deleteItem(itemId);
      toast.success('Child item deleted');
      await refetchChildren();
    } catch (err) {
      toast.error('Failed to delete child item');
    }
  };

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
          priority: priority(),
          estimate: estimate() || undefined,
          tags: tags(),
        });
        toast.success('Item updated successfully');
      } else {
        // Create new item
        const itemData: CreateItem = {
          title: title().trim(),
          description: description().trim() || undefined,
          item_type: itemType(),
          priority: priority(),
          estimate: estimate() || undefined,
          tags: tags(),
          parent_id: props.parentId,
        };

        const createdItem = await api.createItem(props.projectId, itemData);

        // Create subtasks if any
        if (subtasks().length > 0) {
          for (const subtask of subtasks()) {
            await api.createItem(props.projectId, {
              title: subtask.title,
              item_type: 'subtask',
              priority: 'medium',
              parent_id: createdItem.id,
            });
          }
        }

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
      props.onClose();
    }
  };

  return (
    <Modal
      isOpen={props.isOpen}
      onClose={handleClose}
      title={mode() === 'edit' ? 'Edit Item' : 'Create New Item'}
      size="xl"
    >
      <form onSubmit={handleSubmit} class="space-y-5">
        {/* Error message */}
        <Show when={error()}>
          <div class="bg-red-50 border border-red-200 rounded-lg p-3 text-red-700 text-sm">
            {error()}
          </div>
        </Show>

        {/* Title */}
        <div>
          <label for="item-title" class="block text-sm font-semibold mb-2" style={{ color: "var(--color-text-primary)" }}>
            Title <span class="text-red-500">*</span>
          </label>
          <input
            id="item-title"
            type="text"
            value={title()}
            onInput={(e) => setTitle(e.currentTarget.value)}
            placeholder="What needs to be done?"
            class="w-full px-4 py-2.5 border rounded-lg text-base focus:ring-2 focus:ring-violet-500 focus:border-violet-500 transition-all"
            style={{
              "background-color": "var(--color-bg-base)",
              "border-color": "var(--color-border-medium)",
              color: "var(--color-text-primary)"
            }}
            required
            disabled={loading()}
            autofocus
          />
        </div>

        {/* Type & Priority Combined Row */}
        <div class="grid grid-cols-2 gap-4">
          {/* Item Type (only in create mode) */}
          <Show when={mode() === 'create'} fallback={<div />}>
            <div>
              <label class="block text-sm font-semibold mb-2" style={{ color: "var(--color-text-primary)" }}>
                Type
              </label>
              <div class="flex flex-wrap gap-1.5">
                <For each={Object.entries(itemTypeConfig())}>
                  {([type, config]) => (
                    <button
                      type="button"
                      onClick={() => setItemType(type as ItemType)}
                      class={`
                        px-2.5 py-1.5 rounded-md text-xs font-medium transition-all
                        ${
                          itemType() === type
                            ? 'bg-violet-100 text-violet-700 ring-2 ring-violet-500'
                            : 'hover:bg-gray-100'
                        }
                      `}
                      style={{
                        "background-color": itemType() === type ? undefined : "var(--color-bg-subtle)",
                        color: itemType() === type ? undefined : "var(--color-text-secondary)"
                      }}
                      disabled={loading()}
                    >
                      <span class="mr-0.5">{config.emoji}</span>
                      {config.label}
                    </button>
                  )}
                </For>
              </div>
            </div>
          </Show>

          {/* Priority */}
          <div>
            <label class="block text-sm font-semibold mb-2" style={{ color: "var(--color-text-primary)" }}>
              Priority
            </label>
            <div class="flex flex-wrap gap-1.5">
              <For each={Object.entries(priorityConfig)}>
                {([prio, config]) => (
                  <button
                    type="button"
                    onClick={() => setPriority(prio as Priority)}
                    class={`
                      px-2.5 py-1.5 rounded-md text-xs font-medium transition-all
                      ${
                        priority() === prio
                          ? 'bg-violet-100 text-violet-700 ring-2 ring-violet-500'
                          : 'hover:bg-gray-100'
                      }
                    `}
                    style={{
                      "background-color": priority() === prio ? undefined : "var(--color-bg-subtle)",
                      color: priority() === prio ? undefined : "var(--color-text-secondary)"
                    }}
                    disabled={loading()}
                  >
                    <span class="mr-0.5">{config.emoji}</span>
                    {config.label}
                  </button>
                )}
              </For>
            </div>
          </div>
        </div>

        {/* Description with WYSIWYG */}
        <div>
          <label class="block text-sm font-semibold mb-2" style={{ color: "var(--color-text-primary)" }}>
            Description
          </label>
          <RichTextEditor
            value={description()}
            onChange={setDescription}
            placeholder="Add details, acceptance criteria, or notes..."
            disabled={loading()}
          />
        </div>

        {/* Story Points - Compact */}
        <div>
          <label class="block text-sm font-semibold mb-2" style={{ color: "var(--color-text-primary)" }}>
            Story Points
          </label>
          <div class="flex flex-wrap gap-1.5">
            <For each={estimateOptions}>
              {(value) => (
                <button
                  type="button"
                  onClick={() => setEstimate(estimate() === value ? null : value)}
                  class={`
                    w-9 h-9 rounded-md text-sm font-semibold transition-all
                    ${
                      estimate() === value
                        ? 'bg-violet-100 text-violet-700 ring-2 ring-violet-500'
                        : 'hover:bg-gray-100'
                    }
                  `}
                  style={{
                    "background-color": estimate() === value ? undefined : "var(--color-bg-subtle)",
                    color: estimate() === value ? undefined : "var(--color-text-secondary)"
                  }}
                  disabled={loading()}
                >
                  {value}
                </button>
              )}
            </For>
          </div>
        </div>

        {/* Subtasks/Child Items */}
        <Show when={(mode() === 'create' && itemType() !== 'subtask') || mode() === 'edit'}>
          <div>
            <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
              {mode() === 'edit' ? 'Child Items' : 'Subtasks'}
            </label>

            {/* CREATE MODE: Subtask list (will be created on submit) */}
            <Show when={mode() === 'create' && subtasks().length > 0}>
              <div class="space-y-2 mb-3">
                <For each={subtasks()}>
                  {(subtask) => (
                    <div class="flex items-center gap-2 bg-gray-50 dark:bg-gray-800 px-3 py-2 rounded-lg">
                      <span class="flex-1 text-gray-900 dark:text-white">{subtask.title}</span>
                      <button
                        type="button"
                        onClick={() => removeSubtask(subtask.id)}
                        class="text-gray-400 hover:text-red-500 transition-colors"
                      >
                        <FiX size={18} />
                      </button>
                    </div>
                  )}
                </For>
              </div>
            </Show>

            {/* EDIT MODE: Existing child items (live from database) */}
            <Show when={mode() === 'edit' && childItems() && childItems()!.length > 0}>
              <div class="space-y-2 mb-3">
                <For each={childItems()}>
                  {(child) => (
                    <div class="flex items-center gap-2 bg-gray-50 dark:bg-gray-800 px-3 py-2 rounded-lg">
                      <span class="text-sm text-gray-500 dark:text-gray-400">
                        {typeof child.item_type === 'string' ? itemTypeConfig()[child.item_type as ItemType]?.emoji : '📌'}
                      </span>
                      <span class="flex-1 text-gray-900 dark:text-white">{child.title}</span>
                      <span class="text-xs px-2 py-0.5 rounded bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300">
                        {child.status}
                      </span>
                      <button
                        type="button"
                        onClick={() => handleDeleteChild(child.id)}
                        class="text-gray-400 hover:text-red-500 transition-colors"
                      >
                        <FiTrash2 size={16} />
                      </button>
                    </div>
                  )}
                </For>
              </div>
            </Show>

            {/* Add subtask/child item input */}
            <div class="flex gap-2">
              <input
                type="text"
                value={subtaskInput()}
                onInput={(e) => setSubtaskInput(e.currentTarget.value)}
                onKeyPress={(e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault();
                    if (mode() === 'edit') {
                      handleCreateChild();
                    } else {
                      addSubtask();
                    }
                  }
                }}
                placeholder={mode() === 'edit' ? 'Add a child item...' : 'Add a subtask...'}
                class="flex-1 px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-800 text-gray-900 dark:text-white placeholder-gray-400 focus:ring-2 focus:ring-purple-500 focus:border-transparent"
                disabled={loading()}
              />
              <button
                type="button"
                onClick={mode() === 'edit' ? handleCreateChild : addSubtask}
                class="px-4 py-2 bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300 rounded-lg hover:bg-purple-200 dark:hover:bg-purple-900/50 transition-colors flex items-center gap-2"
                disabled={loading()}
              >
                <FiPlus size={18} />
                Add
              </button>
            </div>
          </div>
        </Show>

        {/* Tags */}
        <div>
          <label class="block text-sm font-semibold mb-2" style={{ color: "var(--color-text-primary)" }}>
            Tags
          </label>

          {/* Tag list */}
          <Show when={tags().length > 0}>
            <div class="flex flex-wrap gap-1.5 mb-2">
              <For each={tags()}>
                {(tag) => (
                  <span class="inline-flex items-center gap-1 px-2.5 py-1 bg-violet-100 text-violet-700 rounded-md text-xs font-medium">
                    {tag}
                    <button
                      type="button"
                      onClick={() => removeTag(tag)}
                      class="hover:text-red-500 transition-colors"
                    >
                      <FiX size={12} />
                    </button>
                  </span>
                )}
              </For>
            </div>
          </Show>

          {/* Add tag input */}
          <div class="flex gap-2">
            <input
              type="text"
              value={tagInput()}
              onInput={(e) => setTagInput(e.currentTarget.value)}
              onKeyPress={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  addTag();
                }
              }}
              placeholder="Add tags (frontend, api...)"
              class="flex-1 px-3 py-2 border rounded-lg text-sm focus:ring-2 focus:ring-violet-500 focus:border-violet-500 transition-all"
              style={{
                "background-color": "var(--color-bg-base)",
                "border-color": "var(--color-border-medium)",
                color: "var(--color-text-primary)"
              }}
              disabled={loading()}
            />
            <button
              type="button"
              onClick={addTag}
              class="px-3 py-2 bg-violet-100 text-violet-700 rounded-lg hover:bg-violet-200 transition-colors flex items-center gap-1.5 text-sm font-medium"
              disabled={loading()}
            >
              <FiPlus size={16} />
              Add
            </button>
          </div>
        </div>

        {/* Actions */}
        <div class="flex gap-3 pt-4" style={{ "border-top": "1px solid var(--color-border-light)" }}>
          <button
            type="button"
            onClick={handleClose}
            disabled={loading()}
            class="flex-1 px-5 py-2.5 border rounded-lg font-medium hover:bg-gray-50 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            style={{
              "border-color": "var(--color-border-medium)",
              color: "var(--color-text-secondary)"
            }}
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={loading() || !title().trim()}
            class="flex-1 px-5 py-2.5 bg-violet-600 text-white rounded-lg hover:bg-violet-700 transition-colors disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2 font-semibold shadow-sm"
          >
            {loading() ? (
              <>
                <svg
                  class="animate-spin h-5 w-5"
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
