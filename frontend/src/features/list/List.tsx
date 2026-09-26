import { createSignal, For, Show, createMemo, onMount, onCleanup } from 'solid-js';
import { useParams, useSearchParams } from '@solidjs/router';
import {
  DragDropProvider,
  DragDropSensors,
  SortableProvider,
  createSortable,
  closestCenter
} from '@thisbeyond/solid-dnd';
import { api } from '../../shared/api';
import { toast } from '../../shared/ui/toast';
import { type ItemTypeConfig } from '../../shared/vocab/vocab';
import { useProject } from '../../shared/state/projectContext';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import { useVocab } from '../../shared/vocab/useVocab';
import type { Item } from '../../shared/types';
import { Button, Badge, EmptyState } from '../../shared/ui';
import { IconList } from '../../shared/ui/icons';
import { priorityColor } from '../../shared/ui/PriorityDot';
import { typeBadgeTone } from '../../shared/ui/TypeBadge';
import type { Priority } from '../../shared/types';
import { ITEM_UPDATED_EVENT } from '../../shared/state/itemEvents';
import { FiPlus, FiMenu, FiCheck, FiX, FiChevronRight, FiChevronDown, FiTrash2, FiMaximize2, FiMinimize2 } from 'solid-icons/fi';

type ItemType = 'epic' | 'feature' | 'task' | 'subtask' | 'bug' | 'requirement';
type ItemWithChildren = Item & { children: ItemWithChildren[]; level: number };

const PRIORITIES = [
  { value: 'critical', emoji: '🔥', label: 'Critical' },
  { value: 'high', emoji: '⬆️', label: 'High' },
  { value: 'medium', emoji: '➡️', label: 'Medium' },
  { value: 'low', emoji: '⬇️', label: 'Low' },
];

export default function List() {
  const params = useParams();
  const projectId = params.id;

  const { items, refetch } = useProjectItems();
  const { project } = useProject();
  const { types: vocabTypes } = useVocab();
  const types = createMemo(() => vocabTypes());

  const [, setSearchParams] = useSearchParams();
  const [expandedItems, setExpandedItems] = createSignal<Set<string>>(new Set());
  const [creatingAt, setCreatingAt] = createSignal<{ parentId?: string } | null>(null);
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

  const allExpandableIds = createMemo(() => {
    const ids: string[] = [];
    const collect = (items: ItemWithChildren[]) => {
      for (const item of items) {
        if (item.children.length > 0) {
          ids.push(item.id);
          collect(item.children);
        }
      }
    };
    collect(organizedItems());
    return ids;
  });

  const expandAll = () => setExpandedItems(new Set(allExpandableIds()));
  const collapseAll = () => setExpandedItems(new Set<string>());

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

  const startCreating = (parentId?: string) => {
    setCreatingAt({ parentId });
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
    if (!creating || !projectId) return;

    try {
      await api.items.create(projectId, {
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
      await api.items.remove(item.id);
      toast.success('Deleted');
      await refetch();
    } catch {
      toast.error('Failed to delete');
    }
  };

  const handleMove = (_itemId: string, _newParentId: string | null) => {
    // Re-parenting is not supported by the API: `PATCH /items/:id` (UpdateItem in
    // the OpenAPI contract) has no `parent_id` field, so the backend would
    // silently ignore the change. Surface that honestly rather than showing a
    // misleading "moved" toast. (Re-parenting needs a backend UpdateItem change.)
    toast.info("Re-parenting items isn't supported yet");
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

  // Open the item detail drawer (deep-linkable via ?item=).
  const handleViewItem = (item: Item) => {
    setSearchParams({ item: item.id });
  };

  // Refresh the list when the drawer edits an item.
  onMount(() => {
    const onItemUpdated = () => void refetch();
    window.addEventListener(ITEM_UPDATED_EVENT, onItemUpdated);
    onCleanup(() => window.removeEventListener(ITEM_UPDATED_EVENT, onItemUpdated));
  });

  return (
    <div class="h-full flex flex-col">
      {/* Header */}
      <div class="sticky top-0 z-10 bg-app pb-4">
        <div class="flex items-end gap-2.5">
          <div class="min-w-0">
            <h1 class="text-content m-0 leading-tight truncate" style={{ 'font-size': '34px' }}>
              {project()?.name || 'List View'}
            </h1>
            <p class="text-[13px] text-content-subtle">
              {flattenedItems().length} {flattenedItems().length === 1 ? 'item' : 'items'}
            </p>
          </div>

          <div class="ml-auto flex items-center gap-2">
            <Show when={allExpandableIds().length > 0}>
              <Button variant="ghost" onClick={expandAll} title="Expand all">
                <FiMaximize2 size={14} />
                Expand all
              </Button>
              <Button variant="ghost" onClick={collapseAll} title="Collapse all">
                <FiMinimize2 size={14} />
                Collapse all
              </Button>
            </Show>
            <Button onClick={() => startCreating()}>
              <FiPlus size={16} />
              New Item
            </Button>
          </div>
        </div>
      </div>

      <div class="flex-1 overflow-auto">
        <Show when={projectId} fallback={
          <div class="rounded-[28px] bg-panel">
            <EmptyState icon={<IconList size={28} />} title="Select a project from the sidebar" />
          </div>
        }>
          {/* Items */}
          <DragDropProvider onDragEnd={onDragEnd} collisionDetector={closestCenter}>
            <DragDropSensors />
            <SortableProvider ids={flattenedItems().map(i => i.id)}>
              <div class="max-w-6xl">
                <Show when={flattenedItems().length > 0} fallback={
                  <div class="rounded-[28px] bg-panel">
                    <EmptyState
                      icon={<IconList size={28} />}
                      title="No items yet"
                      action={
                        <Button variant="secondary" size="sm" onClick={() => startCreating()}>
                          Create your first item
                        </Button>
                      }
                    />
                  </div>
                }>
                  <div class="flex flex-col gap-2.5">
                    <For each={flattenedItems()}>
                      {(item) => (
                        <>
                          <ItemRow
                            item={item}
                            types={types()}
                            onToggleExpand={() => toggleExpand(item.id)}
                            onAddChild={() => startCreating(item.id)}
                            onDelete={() => handleDelete(item)}
                            onView={() => handleViewItem(item)}
                            isExpanded={expandedItems().has(item.id)}
                          />

                          {/* Inline creation form */}
                          <Show when={creatingAt()?.parentId === item.id && expandedItems().has(item.id)}>
                            <CreateForm
                              level={item.level + 1}
                              types={types()}
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
                        types={types()}
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
                </Show>
              </div>
            </SortableProvider>
          </DragDropProvider>
        </Show>
      </div>

    </div>
  );
}

// Sortable item row — a pill on the page ground, indented by level.
function ItemRow(props: {
  item: ItemWithChildren;
  types: ItemTypeConfig[];
  onToggleExpand: () => void;
  onAddChild: () => void;
  onDelete: () => void;
  onView: () => void;
  isExpanded: boolean;
}) {
  const sortable = createSortable(props.item.id);
  const [showActions, setShowActions] = createSignal(false);
  const itemType = (typeof props.item.item_type === 'string' ? props.item.item_type : 'task') as ItemType;
  const typeConfig = () => props.types.find(t => t.value === itemType) ?? props.types[2] ?? { emoji: '📝', label: itemType };
  const typeTone = typeBadgeTone(itemType);
  const priorityConfig = PRIORITIES.find(p => p.value === props.item.priority) || PRIORITIES[2];
  const hasChildren = props.item.children.length > 0;

  return (
    <div
      ref={sortable.ref}
      class={`group relative rounded-full transition-[box-shadow,border-color,opacity] duration-200 ${sortable.isActiveDraggable ? 'opacity-50' : ''}`}
      style={{
        'margin-left': `${props.item.level * 40}px`,
        'background-color': 'var(--color-bg-panel)',
        border: showActions() ? '2px solid var(--color-accent-line)' : '2px solid transparent',
        'box-shadow': showActions() || sortable.isActiveDraggable ? 'var(--shadow-md)' : 'none',
      }}
      onMouseEnter={() => setShowActions(true)}
      onMouseLeave={() => setShowActions(false)}
    >
      <div class="flex items-center gap-2.5 px-3.5 py-2.5">
        {/* Drag Handle */}
        <div {...sortable.dragActivators} class="cursor-grab active:cursor-grabbing flex-shrink-0">
          <FiMenu size={15} class="text-content-subtle hover:text-brand transition-colors" />
        </div>

        {/* Expand/Collapse */}
        <button
          onClick={props.onToggleExpand}
          class={`flex-shrink-0 grid place-items-center w-5 h-5 rounded-full text-content-muted hover:bg-sunken transition-colors ${hasChildren ? '' : 'invisible'}`}
        >
          {props.isExpanded ? <FiChevronDown size={14} /> : <FiChevronRight size={14} />}
        </button>

        {/* Type tile */}
        <div
          class="flex-shrink-0 w-8 h-8 rounded-full grid place-items-center font-heading text-sm"
          style={{ 'background-color': typeTone.bg, color: typeTone.fg }}
          title={typeConfig().label}
        >
          {typeConfig().label.charAt(0).toUpperCase()}
        </div>

        {/* Title - Clickable */}
        <button
          onClick={props.onView}
          class="flex-1 min-w-0 text-left group/title"
        >
          <div class="flex items-center gap-2">
            <span class="text-sm font-bold text-content group-hover/title:text-brand transition-colors truncate">
              {props.item.title}
            </span>
            <Show when={hasChildren}>
              <Badge class="flex-shrink-0">{props.item.children.length}</Badge>
            </Show>
          </div>
          <Show when={props.item.description}>
            <p class="text-xs text-content-subtle truncate">
              {props.item.description?.replace(/<[^>]*>/g, '').substring(0, 80)}
            </p>
          </Show>
        </button>

        {/* Priority pill — coloured edge instead of an emoji */}
        <span
          class="flex-shrink-0 inline-flex items-center rounded-full px-2.5 py-[3px] text-xs font-semibold whitespace-nowrap text-content-muted"
          style={{
            'background-color': 'var(--color-bg-app)',
            'box-shadow': `inset 3px 0 0 ${priorityColor(priorityConfig.value as Priority)}`,
          }}
        >
          {priorityConfig.label}
        </span>

        {/* Status */}
        <Badge tone="info" class="flex-shrink-0">{props.item.status}</Badge>

        {/* Tags */}
        <Show when={props.item.tags && props.item.tags.length > 0}>
          <div class="flex-shrink-0 flex gap-1">
            <For each={props.item.tags?.slice(0, 2)}>
              {(tag) => (
                <span class="text-[11px] font-semibold px-2.5 py-[2px] rounded-full border border-line text-content-muted">
                  {tag}
                </span>
              )}
            </For>
          </div>
        </Show>

        {/* Actions */}
        <div class={`flex-shrink-0 flex items-center gap-1 transition-opacity ${showActions() ? 'opacity-100' : 'opacity-0'}`}>
          <button
            onClick={props.onAddChild}
            class="w-7 h-7 rounded-full grid place-items-center bg-app text-content-muted hover:text-brand transition-colors"
            title="Add child item"
          >
            <FiPlus size={14} />
          </button>
          <button
            onClick={props.onDelete}
            class="w-7 h-7 rounded-full grid place-items-center transition-[filter] hover:brightness-95"
            style={{ 'background-color': 'var(--color-danger-100)', color: 'var(--color-danger-600)' }}
            title="Delete"
          >
            <FiTrash2 size={13} />
          </button>
        </div>
      </div>
    </div>
  );
}

const pillControl =
  'flex-shrink-0 min-h-8 text-[13px] px-3 rounded-full border-0 bg-app text-content ' +
  'focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-focus-ring)]';

// Inline create form — a soft accent pill under its parent.
function CreateForm(props: {
  level: number;
  types: ItemTypeConfig[];
  title: string;
  type: ItemType;
  priority: string;
  onTitleChange: (v: string) => void;
  onTypeChange: (v: ItemType) => void;
  onPriorityChange: (v: string) => void;
  onSave: () => void;
  onCancel: () => void;
}) {
  return (
    <div
      class="flex items-center gap-2 p-2 rounded-full bg-accent-soft"
      style={{ 'margin-left': `${props.level * 40}px` }}
    >
      {/* Type Selector */}
      <select
        value={props.type}
        onChange={(e) => props.onTypeChange(e.currentTarget.value as ItemType)}
        class={pillControl}
      >
        <For each={props.types}>
          {(t) => <option value={t.value}>{t.emoji} {t.label}</option>}
        </For>
      </select>

      {/* Priority Selector */}
      <select
        value={props.priority}
        onChange={(e) => props.onPriorityChange(e.currentTarget.value)}
        class={pillControl}
      >
        <For each={PRIORITIES}>
          {(p) => <option value={p.value}>{p.emoji} {p.label}</option>}
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
        placeholder="Item title... (Enter to save, Esc to cancel)"
        class={`${pillControl} flex-1 min-w-0 px-4 placeholder-[var(--color-text-tertiary)]`}
        autofocus
      />

      {/* Actions */}
      <button
        onClick={props.onSave}
        class="flex-shrink-0 w-8 h-8 rounded-full grid place-items-center transition-[filter] hover:brightness-95"
        style={{ color: 'var(--color-on-accent)', 'background-color': 'var(--color-primary-600)' }}
        title="Save (Enter)"
      >
        <FiCheck size={16} />
      </button>
      <button
        onClick={props.onCancel}
        class="flex-shrink-0 w-8 h-8 rounded-full grid place-items-center bg-app text-content-muted hover:text-content transition-colors"
        title="Cancel (Esc)"
      >
        <FiX size={16} />
      </button>
    </div>
  );
}
