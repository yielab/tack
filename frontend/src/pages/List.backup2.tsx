import { createSignal, createResource, For, Show, createMemo } from 'solid-js';
import { useParams } from '@solidjs/router';
import {
  DragDropProvider,
  DragDropSensors,
  SortableProvider,
  createSortable,
  closestCenter
} from '@thisbeyond/solid-dnd';
import { api } from '../lib/api';
import { toast } from '../lib/toast';
import type { Item, Project } from '../types/api';
import { FiPlus, FiMenu, FiCheck, FiX } from 'solid-icons/fi';

type ItemType = 'epic' | 'feature' | 'task' | 'subtask' | 'bug' | 'requirement';
type ItemWithChildren = Item & { children: ItemWithChildren[]; level: number };

const TYPES: { value: ItemType; emoji: string; label: string }[] = [
  { value: 'epic', emoji: '🎯', label: 'Epic' },
  { value: 'feature', emoji: '✨', label: 'Feature' },
  { value: 'task', emoji: '📝', label: 'Task' },
  { value: 'subtask', emoji: '📌', label: 'Subtask' },
  { value: 'bug', emoji: '🐛', label: 'Bug' },
  { value: 'requirement', emoji: '📋', label: 'Req' },
];

const PRIORITIES = [
  { value: 'critical', emoji: '🔥', color: 'red' },
  { value: 'high', emoji: '⬆️', color: 'orange' },
  { value: 'medium', emoji: '➡️', color: 'yellow' },
  { value: 'low', emoji: '⬇️', color: 'green' },
];

export default function ListCompact() {
  const params = useParams();
  const projectId = params.id;

  const [projects] = createResource<Project[]>(() => api.listProjects());
  const [selectedProject, setSelectedProject] = createSignal<string | null>(projectId || null);
  const [items, { refetch }] = createResource(
    selectedProject,
    async (projId) => projId ? api.listItems(projId) : []
  );

  const [expandedItems, setExpandedItems] = createSignal<Set<string>>(new Set());
  const [creatingAt, setCreatingAt] = createSignal<{ parentId?: string; index: number } | null>(null);
  const [newItemTitle, setNewItemTitle] = createSignal('');
  const [newItemType, setNewItemType] = createSignal<ItemType>('task');
  const [newItemPriority, setNewItemPriority] = createSignal('medium');

  const organizedItems = createMemo(() => {
    const allItems = items() || [];
    const itemMap = new Map<string, ItemWithChildren>();
    const rootItems: ItemWithChildren[] = [];

    allItems.forEach(item => {
      itemMap.set(item.id, { ...item, children: [], level: 0 });
    });

    allItems.forEach(item => {
      const itemWithChildren = itemMap.get(item.id)!;
      if (item.parent_id) {
        const parent = itemMap.get(item.parent_id);
        if (parent) {
          itemWithChildren.level = parent.level + 1;
          parent.children.push(itemWithChildren);
        } else {
          rootItems.push(itemWithChildren);
        }
      } else {
        rootItems.push(itemWithChildren);
      }
    });

    const sortItems = (items: ItemWithChildren[]) => {
      items.sort((a, b) => a.created_at.localeCompare(b.created_at));
      items.forEach(item => sortItems(item.children));
    };
    sortItems(rootItems);

    return rootItems;
  });

  const flattenedItems = createMemo(() => {
    const result: ItemWithChildren[] = [];
    const expanded = expandedItems();

    const flatten = (items: ItemWithChildren[]) => {
      items.forEach(item => {
        result.push(item);
        if (expanded.has(item.id) && item.children.length > 0) {
          flatten(item.children);
        }
      });
    };

    flatten(organizedItems());
    return result;
  });

  const toggleExpand = (itemId: string) => {
    setExpandedItems(prev => {
      const newSet = new Set(prev);
      if (newSet.has(itemId)) {
        newSet.delete(itemId);
      } else {
        newSet.add(itemId);
      }
      return newSet;
    });
  };

  const startCreating = (parentId?: string, index: number = 0) => {
    setCreatingAt({ parentId, index });
    setNewItemTitle('');
    setNewItemType('task');
    setNewItemPriority('medium');
  };

  const cancelCreating = () => {
    setCreatingAt(null);
    setNewItemTitle('');
  };

  const handleCreate = async () => {
    const title = newItemTitle().trim();
    if (!title) return;

    const creating = creatingAt();
    if (!creating) return;

    try {
      await api.createItem(selectedProject()!, {
        title,
        item_type: newItemType(),
        priority: newItemPriority() as any,
        parent_id: creating.parentId,
      });
      toast.success('Item created');
      await refetch();
      setCreatingAt(null);
      setNewItemTitle('');

      if (creating.parentId) {
        setExpandedItems(prev => new Set([...prev, creating.parentId!]));
      }
    } catch (err) {
      toast.error('Failed to create item');
    }
  };

  const handleDelete = async (item: Item) => {
    if (!confirm(`Delete "${item.title}"?`)) return;
    try {
      await api.deleteItem(item.id);
      toast.success('Deleted');
      await refetch();
    } catch {
      toast.error('Failed to delete');
    }
  };

  const handleMove = async (itemId: string, newParentId: string | null) => {
    try {
      await api.updateItem(itemId, { parent_id: newParentId || undefined });
      toast.success('Item moved');
      await refetch();
    } catch {
      toast.error('Failed to move');
    }
  };

  const onDragEnd = ({ draggable, droppable }: any) => {
    if (draggable && droppable && draggable.id !== droppable.id) {
      const draggedItem = flattenedItems().find(i => i.id === draggable.id);
      const targetItem = flattenedItems().find(i => i.id === droppable.id);

      if (draggedItem && targetItem) {
        handleMove(draggedItem.id, targetItem.parent_id || null);
      }
    }
  };

  return (
    <div class="space-y-4">
      {/* Header */}
      <div>
        <h1 class="text-2xl font-bold text-gray-900 dark:text-white mb-2">Items List</h1>
        <select
          value={selectedProject() || ''}
          onChange={(e) => setSelectedProject(e.currentTarget.value || null)}
          class="px-3 py-1.5 text-sm border border-gray-300 dark:border-gray-600 rounded bg-white dark:bg-gray-800 text-gray-900 dark:text-white"
        >
          <option value="">Select project...</option>
          <For each={projects()}>
            {(p) => <option value={p.id}>{p.name}</option>}
          </For>
        </select>
      </div>

      <Show when={selectedProject()}>
        {/* Quick Add Button */}
        <button
          onClick={() => startCreating(undefined, 0)}
          class="w-full px-3 py-2 text-sm text-left text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white hover:bg-gray-100 dark:hover:bg-gray-800 rounded border border-dashed border-gray-300 dark:border-gray-600 transition-colors flex items-center gap-2"
        >
          <FiPlus size={16} />
          Add item...
        </button>

        {/* Items List */}
        <DragDropProvider onDragEnd={onDragEnd} collisionDetector={closestCenter}>
          <DragDropSensors />
          <SortableProvider ids={flattenedItems().map(i => i.id)}>
            <div class="space-y-0.5">
              <For each={flattenedItems()}>
                {(item) => (
                  <>
                    <ItemRow
                      item={item}
                      onToggleExpand={() => toggleExpand(item.id)}
                      onAddChild={() => startCreating(item.id, 0)}
                      onDelete={() => handleDelete(item)}
                      isExpanded={expandedItems().has(item.id)}
                    />

                    {/* Inline creation form */}
                    <Show when={creatingAt()?.parentId === item.id && expandedItems().has(item.id)}>
                      <CreateForm
                        level={item.level + 1}
                        title={newItemTitle()}
                        type={newItemType()}
                        priority={newItemPriority()}
                        onTitleChange={setNewItemTitle}
                        onTypeChange={setNewItemType}
                        onPriorityChange={setNewItemPriority}
                        onSave={handleCreate}
                        onCancel={cancelCreating}
                      />
                    </Show>
                  </>
                )}
              </For>

              {/* Top-level create form */}
              <Show when={creatingAt() && !creatingAt()?.parentId}>
                <CreateForm
                  level={0}
                  title={newItemTitle()}
                  type={newItemType()}
                  priority={newItemPriority()}
                  onTitleChange={setNewItemTitle}
                  onTypeChange={setNewItemType}
                  onPriorityChange={setNewItemPriority}
                  onSave={handleCreate}
                  onCancel={cancelCreating}
                />
              </Show>
            </div>
          </SortableProvider>
        </DragDropProvider>

        {/* Empty State */}
        <Show when={flattenedItems().length === 0 && !creatingAt()}>
          <div class="text-center py-12 text-gray-500 dark:text-gray-400">
            <p class="mb-2">No items yet</p>
            <button
              onClick={() => startCreating()}
              class="text-purple-600 hover:text-purple-700 dark:text-purple-400"
            >
              Create your first item
            </button>
          </div>
        </Show>
      </Show>
    </div>
  );
}

// Sortable Item Row
function ItemRow(props: {
  item: ItemWithChildren;
  onToggleExpand: () => void;
  onAddChild: () => void;
  onDelete: () => void;
  isExpanded: boolean;
}) {
  const sortable = createSortable(props.item.id);
  const itemType = (typeof props.item.item_type === 'string' ? props.item.item_type : 'task') as ItemType;
  const typeConfig = TYPES.find(t => t.value === itemType) || TYPES[2];
  const priorityConfig = PRIORITIES.find(p => p.value === props.item.priority) || PRIORITIES[2];
  const hasChildren = props.item.children.length > 0;
  const indent = props.item.level * 24;

  return (
    <div
      ref={sortable.ref}
      class={`
        group flex items-center gap-2 px-2 py-1.5 rounded
        hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors
        ${sortable.isActiveDraggable ? 'opacity-50' : ''}
      `}
      style={{ 'padding-left': `${indent + 8}px` }}
    >
      {/* Drag Handle */}
      <div {...sortable.dragActivators} class="cursor-grab active:cursor-grabbing opacity-0 group-hover:opacity-100 transition-opacity">
        <FiMenu size={14} class="text-gray-400" />
      </div>

      {/* Expand/Collapse */}
      <button
        onClick={props.onToggleExpand}
        class={`text-xs ${hasChildren ? 'text-gray-600 dark:text-gray-400' : 'invisible'}`}
      >
        {props.isExpanded ? '▼' : '▶'}
      </button>

      {/* Type */}
      <span class="text-sm" title={typeConfig.label}>{typeConfig.emoji}</span>

      {/* Priority */}
      <span class="text-xs" title={props.item.priority}>{priorityConfig.emoji}</span>

      {/* Title */}
      <div class="flex-1 min-w-0">
        <span class="text-sm text-gray-900 dark:text-white truncate block">
          {props.item.title}
        </span>
      </div>

      {/* Status */}
      <span class="text-xs px-2 py-0.5 rounded bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300">
        {props.item.status}
      </span>

      {/* Tags */}
      <Show when={props.item.tags && props.item.tags.length > 0}>
        <span class="text-xs text-gray-500 dark:text-gray-400">
          {props.item.tags![0]}
        </span>
      </Show>

      {/* Actions */}
      <div class="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
        <button
          onClick={props.onAddChild}
          class="p-1 text-gray-400 hover:text-purple-600 dark:hover:text-purple-400 transition-colors"
          title="Add child"
        >
          <FiPlus size={14} />
        </button>
        <button
          onClick={props.onDelete}
          class="p-1 text-gray-400 hover:text-red-600 dark:hover:text-red-400 transition-colors"
          title="Delete"
        >
          <FiX size={14} />
        </button>
      </div>
    </div>
  );
}

// Inline Create Form
function CreateForm(props: {
  level: number;
  title: string;
  type: ItemType;
  priority: string;
  onTitleChange: (v: string) => void;
  onTypeChange: (v: ItemType) => void;
  onPriorityChange: (v: string) => void;
  onSave: () => void;
  onCancel: () => void;
}) {
  const indent = props.level * 24;

  return (
    <div
      class="flex items-center gap-2 px-2 py-1.5 bg-purple-50 dark:bg-purple-900/20 rounded border border-purple-200 dark:border-purple-800"
      style={{ 'padding-left': `${indent + 8}px` }}
    >
      {/* Spacer for drag handle */}
      <div class="w-3.5" />

      {/* Spacer for expand button */}
      <div class="w-3" />

      {/* Type Selector */}
      <select
        value={props.type}
        onChange={(e) => props.onTypeChange(e.currentTarget.value as ItemType)}
        class="text-xs px-1 py-0.5 border-0 bg-transparent rounded focus:ring-1 focus:ring-purple-500"
      >
        <For each={TYPES}>
          {(t) => <option value={t.value}>{t.emoji} {t.label}</option>}
        </For>
      </select>

      {/* Priority Selector */}
      <select
        value={props.priority}
        onChange={(e) => props.onPriorityChange(e.currentTarget.value)}
        class="text-xs px-1 py-0.5 border-0 bg-transparent rounded focus:ring-1 focus:ring-purple-500"
      >
        <For each={PRIORITIES}>
          {(p) => <option value={p.value}>{p.emoji}</option>}
        </For>
      </select>

      {/* Title Input */}
      <input
        type="text"
        value={props.title}
        onInput={(e) => props.onTitleChange(e.currentTarget.value)}
        onKeyPress={(e) => {
          if (e.key === 'Enter') props.onSave();
          if (e.key === 'Escape') props.onCancel();
        }}
        placeholder="Item title..."
        class="flex-1 text-sm px-2 py-1 border-0 bg-white dark:bg-gray-800 rounded focus:ring-1 focus:ring-purple-500"
        autofocus
      />

      {/* Actions */}
      <button
        onClick={props.onSave}
        class="p-1 text-green-600 dark:text-green-400 hover:bg-green-100 dark:hover:bg-green-900/30 rounded transition-colors"
        title="Save (Enter)"
      >
        <FiCheck size={16} />
      </button>
      <button
        onClick={props.onCancel}
        class="p-1 text-gray-600 dark:text-gray-400 hover:bg-gray-200 dark:hover:bg-gray-700 rounded transition-colors"
        title="Cancel (Esc)"
      >
        <FiX size={16} />
      </button>
    </div>
  );
}
