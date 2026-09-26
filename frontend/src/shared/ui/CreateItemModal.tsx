import { type Component, createSignal, Show, createEffect, For, createResource, createMemo } from 'solid-js';
import Modal from './Modal';
import RichTextEditor from './RichTextEditor';
import Button from './Button';
import Field from './Field';
import { api } from '../api';
import type { CreateItem, Item } from '../types';
import { toast } from './toast';
import { getItemTypeMap, resolveLabel } from '../vocab/vocab';
import { FiPlus, FiX, FiTrash2 } from 'solid-icons/fi';
import { priorityColor } from './PriorityDot';

/** Choice chip (type / priority / estimate): a pill; the chosen one gets the
 *  accent-soft fill, accent ink and a 2px accent ring. */
const chipClass = (selected: boolean) =>
  'transition-all focus:outline-none focus-visible:ring-2 ' +
  (selected ? 'ring-2 ring-[var(--color-primary-600)]' : 'hover:brightness-[.96]');
const chipStyle = (selected: boolean) => ({
  'background-color': selected ? 'var(--color-accent-soft)' : 'var(--color-bg-subtle)',
  color: selected ? 'var(--color-accent-ink)' : 'var(--color-text-secondary)',
  '--tw-ring-color': 'var(--color-primary-600)',
});
const sectionLabel = 'block text-xs font-semibold mb-2';
const pillInputStyle = {
  'background-color': 'var(--color-bg-base)',
  'border-color': 'var(--color-border-medium)',
  color: 'var(--color-text-primary)',
  '--tw-ring-color': 'var(--color-focus-ring)',
};

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
      const items = await api.items.list(props.projectId);
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
      await api.items.create(props.projectId, {
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
      await api.items.remove(itemId);
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
        await api.items.update(props.existingItem.id, {
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

        const createdItem = await api.items.create(props.projectId, itemData);

        // Create subtasks if any
        if (subtasks().length > 0) {
          for (const subtask of subtasks()) {
            await api.items.create(props.projectId, {
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

        {/* Title */}
        <Field
          label="Title"
          required
          value={title()}
          onInput={(e) => setTitle(e.currentTarget.value)}
          placeholder="What needs to be done?"
          disabled={loading()}
          autofocus
        />

        {/* Type & Priority Combined Row */}
        <div class="grid grid-cols-2 gap-4">
          {/* Item Type (only in create mode) */}
          <Show when={mode() === 'create'} fallback={<div />}>
            <div>
              <label class={sectionLabel} style={{ color: "var(--color-text-secondary)" }}>
                Type
              </label>
              <div class="flex flex-wrap gap-1.5">
                <For each={Object.entries(itemTypeConfig())}>
                  {([type, config]) => (
                    <button
                      type="button"
                      onClick={() => setItemType(type as ItemType)}
                      class={`px-3 py-1.5 rounded-full text-xs font-semibold ${chipClass(itemType() === type)}`}
                      style={chipStyle(itemType() === type)}
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
            <label class={sectionLabel} style={{ color: "var(--color-text-secondary)" }}>
              Priority
            </label>
            <div class="flex flex-wrap gap-1.5">
              <For each={Object.entries(priorityConfig)}>
                {([prio, config]) => (
                  <button
                    type="button"
                    onClick={() => setPriority(prio as Priority)}
                    class={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-xs font-semibold ${chipClass(priority() === prio)}`}
                    style={chipStyle(priority() === prio)}
                    disabled={loading()}
                  >
                    <span
                      class="h-2 w-2 shrink-0 rounded-[3px]"
                      style={{ background: priorityColor(prio as Priority) }}
                      aria-hidden="true"
                    />
                    {config.label}
                  </button>
                )}
              </For>
            </div>
          </div>
        </div>

        {/* Description with WYSIWYG */}
        <div>
          <label class={sectionLabel} style={{ color: "var(--color-text-secondary)" }}>
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
          <label class={sectionLabel} style={{ color: "var(--color-text-secondary)" }}>
            {resolveLabel(props.vocabulary, 'story_points')}
          </label>
          <div class="flex flex-wrap gap-1.5">
            <For each={estimateOptions}>
              {(value) => (
                <button
                  type="button"
                  onClick={() => setEstimate(estimate() === value ? null : value)}
                  class={`w-9 h-9 rounded-full font-mono text-[13px] font-semibold ${chipClass(estimate() === value)}`}
                  style={chipStyle(estimate() === value)}
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
            <label class={`${sectionLabel} text-content-muted`}>
              {mode() === 'edit' ? 'Child Items' : 'Subtasks'}
            </label>

            {/* CREATE MODE: Subtask list (will be created on submit) */}
            <Show when={mode() === 'create' && subtasks().length > 0}>
              <div class="space-y-2 mb-3">
                <For each={subtasks()}>
                  {(subtask) => (
                    <div class="flex items-center gap-2 bg-sunken px-4 py-2 rounded-full">
                      <span class="flex-1 text-content">{subtask.title}</span>
                      <button
                        type="button"
                        onClick={() => removeSubtask(subtask.id)}
                        class="text-content-subtle hover:text-danger-500 transition-colors"
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
                    <div class="flex items-center gap-2 bg-sunken px-4 py-2 rounded-full">
                      <span class="text-sm text-content-subtle">
                        {typeof child.item_type === 'string' ? itemTypeConfig()[child.item_type as ItemType]?.emoji : '📌'}
                      </span>
                      <span class="flex-1 text-content">{child.title}</span>
                      <span class="text-[11px] font-bold px-2.5 py-[3px] rounded-full bg-info-100 text-info-700">
                        {child.status}
                      </span>
                      <button
                        type="button"
                        onClick={() => handleDeleteChild(child.id)}
                        class="text-content-subtle hover:text-danger-500 transition-colors"
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
                class="flex-1 min-h-9 px-3.5 py-1.5 text-sm border rounded-full placeholder-content-faint focus:outline-none focus-visible:ring-2"
                style={pillInputStyle}
                disabled={loading()}
              />
              <Button
                type="button"
                variant="secondary"
                onClick={mode() === 'edit' ? handleCreateChild : addSubtask}
                disabled={loading()}
              >
                <FiPlus size={18} />
                Add
              </Button>
            </div>
          </div>
        </Show>

        {/* Tags */}
        <div>
          <label class={sectionLabel} style={{ color: "var(--color-text-secondary)" }}>
            Tags
          </label>

          {/* Tag list */}
          <Show when={tags().length > 0}>
            <div class="flex flex-wrap gap-1.5 mb-2">
              <For each={tags()}>
                {(tag) => (
                  <span class="inline-flex items-center gap-1 px-2.5 py-[3px] bg-accent-soft text-accent-ink rounded-full text-[11px] font-bold">
                    {tag}
                    <button
                      type="button"
                      onClick={() => removeTag(tag)}
                      class="hover:text-danger-500 transition-colors"
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
              class="flex-1 min-h-9 px-3.5 py-1.5 border rounded-full text-sm focus:outline-none focus-visible:ring-2 transition-all"
              style={pillInputStyle}
              disabled={loading()}
            />
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={addTag}
              disabled={loading()}
            >
              <FiPlus size={16} />
              Add
            </Button>
          </div>
        </div>

        {/* Actions */}
        <div class="flex gap-3 pt-4" style={{ "border-top": "1px solid var(--color-border-light)" }}>
          <Button type="button" variant="secondary" class="flex-1" onClick={handleClose} disabled={loading()}>
            Cancel
          </Button>
          <Button type="submit" class="flex-1" loading={loading()} disabled={loading() || !title().trim()}>
            {loading()
              ? mode() === 'edit'
                ? 'Updating...'
                : 'Creating...'
              : mode() === 'edit'
                ? 'Update Item'
                : 'Create Item'}
          </Button>
        </div>
      </form>
    </Modal>
  );
};

export default CreateItemModal;
