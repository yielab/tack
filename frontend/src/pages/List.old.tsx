import { createSignal, createResource, For, Show, createMemo } from 'solid-js';
import { useParams } from '@solidjs/router';
import { api } from '../lib/api';
import { toast } from '../lib/toast';
import type { Item, Project } from '../types/api';
import CreateItemModal from '../components/CreateItemModal';
import { FiPlus, FiChevronRight, FiChevronDown, FiEdit2, FiTrash2 } from 'solid-icons/fi';

type ItemType = 'epic' | 'feature' | 'task' | 'subtask' | 'bug' | 'requirement';
type ItemWithChildren = Item & { children: ItemWithChildren[] };

const itemTypeConfig: Record<ItemType, { emoji: string; color: string; canHaveChildren: boolean }> = {
  epic: { emoji: '🎯', color: 'purple', canHaveChildren: true },
  feature: { emoji: '✨', color: 'blue', canHaveChildren: true },
  task: { emoji: '📝', color: 'green', canHaveChildren: true },
  subtask: { emoji: '📌', color: 'gray', canHaveChildren: false },
  bug: { emoji: '🐛', color: 'red', canHaveChildren: true },
  requirement: { emoji: '📋', color: 'yellow', canHaveChildren: true },
};

const priorityConfig = {
  critical: { emoji: '🔥', color: 'red' },
  high: { emoji: '⬆️', color: 'orange' },
  medium: { emoji: '➡️', color: 'yellow' },
  low: { emoji: '⬇️', color: 'green' },
  none: { emoji: '➖', color: 'gray' },
};

export default function List() {
  const params = useParams();
  const projectId = params.id;

  // Resources
  const [projects] = createResource<Project[]>(() => api.listProjects());
  const [selectedProject, setSelectedProject] = createSignal<string | null>(projectId || null);

  const [items, { refetch: refetchItems }] = createResource(
    selectedProject,
    async (projId) => {
      if (!projId) return [];
      return api.listItems(projId);
    }
  );

  // State
  const [expandedItems, setExpandedItems] = createSignal<Set<string>>(new Set());
  const [createModalOpen, setCreateModalOpen] = createSignal(false);
  const [createModalParentId, setCreateModalParentId] = createSignal<string | undefined>();
  const [editingItem, setEditingItem] = createSignal<Item | undefined>();
  const [searchQuery, setSearchQuery] = createSignal('');

  // Organize items into hierarchy
  const organizedItems = createMemo(() => {
    const allItems = items() || [];
    const itemMap = new Map<string, ItemWithChildren>();
    const rootItems: ItemWithChildren[] = [];

    // First pass: create map
    allItems.forEach(item => {
      itemMap.set(item.id, { ...item, children: [] });
    });

    // Second pass: build hierarchy
    allItems.forEach(item => {
      const itemWithChildren = itemMap.get(item.id)!;
      if (item.parent_id) {
        const parent = itemMap.get(item.parent_id);
        if (parent) {
          parent.children.push(itemWithChildren);
        } else {
          rootItems.push(itemWithChildren);
        }
      } else {
        rootItems.push(itemWithChildren);
      }
    });

    // Sort by item type hierarchy (epic > feature > task > subtask > bug > requirement)
    const typeOrder: Record<string, number> = {
      epic: 1,
      feature: 2,
      requirement: 3,
      task: 4,
      bug: 5,
      subtask: 6,
    };

    const sortItems = (items: ItemWithChildren[]) => {
      items.sort((a, b) => {
        const aType = typeof a.item_type === 'string' ? a.item_type : 'task';
        const bType = typeof b.item_type === 'string' ? b.item_type : 'task';
        const typeComparison = (typeOrder[aType] || 99) - (typeOrder[bType] || 99);
        if (typeComparison !== 0) return typeComparison;
        return a.created_at.localeCompare(b.created_at);
      });
      items.forEach(item => {
        if (item.children.length > 0) {
          sortItems(item.children);
        }
      });
    };

    sortItems(rootItems);
    return rootItems;
  });

  // Filter items by search
  const filteredItems = createMemo(() => {
    const query = searchQuery().toLowerCase();
    if (!query) return organizedItems();

    const filterRecursive = (items: ItemWithChildren[]): ItemWithChildren[] => {
      return items.filter(item => {
        const matchesSearch =
          item.title.toLowerCase().includes(query) ||
          item.description?.toLowerCase().includes(query) ||
          item.tags?.some(tag => tag.toLowerCase().includes(query));

        const filteredChildren = filterRecursive(item.children);
        if (matchesSearch || filteredChildren.length > 0) {
          return true;
        }
        return false;
      }).map(item => ({
        ...item,
        children: filterRecursive(item.children)
      }));
    };

    return filterRecursive(organizedItems());
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

  const handleCreateChild = (parentId: string) => {
    setCreateModalParentId(parentId);
    setCreateModalOpen(true);
  };

  const handleCreateRoot = () => {
    setCreateModalParentId(undefined);
    setCreateModalOpen(true);
  };

  const handleEdit = (item: Item) => {
    setEditingItem(item);
    setCreateModalOpen(true);
  };

  const handleDelete = async (item: Item) => {
    if (!confirm(`Delete "${item.title}"? This will also delete all child items.`)) {
      return;
    }

    try {
      await api.deleteItem(item.id);
      toast.success('Item deleted successfully');
      await refetchItems();
    } catch (err) {
      toast.error('Failed to delete item');
    }
  };

  const handleModalClose = () => {
    setCreateModalOpen(false);
    setEditingItem(undefined);
    setCreateModalParentId(undefined);
  };

  const handleModalSuccess = async () => {
    await refetchItems();
  };

  const renderItem = (item: ItemWithChildren, depth: number = 0) => {
    const itemType = (typeof item.item_type === 'string' ? item.item_type : 'task') as ItemType;
    const config = itemTypeConfig[itemType] || itemTypeConfig.task;
    const priorityConf = priorityConfig[item.priority as keyof typeof priorityConfig] || priorityConfig.medium;
    const hasChildren = item.children.length > 0;
    const isExpanded = expandedItems().has(item.id);

    return (
      <div class="border-l-2 border-gray-200 dark:border-gray-700">
        <div
          class="flex items-center gap-3 py-3 px-4 hover:bg-gray-50 dark:hover:bg-gray-800 transition-colors group"
          style={{ 'padding-left': `${depth * 2 + 1}rem` }}
        >
          {/* Expand/Collapse */}
          <button
            onClick={() => hasChildren && toggleExpand(item.id)}
            class={`text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors ${hasChildren ? '' : 'invisible'}`}
          >
            {hasChildren && (isExpanded ? <FiChevronDown size={18} /> : <FiChevronRight size={18} />)}
          </button>

          {/* Type Icon */}
          <span class="text-xl">{config.emoji}</span>

          {/* Content */}
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <h3 class="font-medium text-gray-900 dark:text-white truncate">
                {item.title}
              </h3>
              <span class={`text-xs px-2 py-0.5 rounded-full bg-${config.color}-100 dark:bg-${config.color}-900/30 text-${config.color}-700 dark:text-${config.color}-300`}>
                {itemType}
              </span>
              <span class="text-sm">{priorityConf.emoji}</span>
            </div>
            <Show when={item.description}>
              <p class="text-sm text-gray-600 dark:text-gray-400 truncate mt-0.5">
                {item.description?.replace(/<[^>]*>/g, '')}
              </p>
            </Show>
            <div class="flex items-center gap-2 mt-1">
              <span class="text-xs px-2 py-0.5 rounded-full bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300">
                {item.status}
              </span>
              <Show when={item.tags && item.tags.length > 0}>
                <For each={item.tags?.slice(0, 3)}>
                  {(tag) => (
                    <span class="text-xs px-2 py-0.5 rounded bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300">
                      {tag}
                    </span>
                  )}
                </For>
              </Show>
              <Show when={item.estimate}>
                <span class="text-xs text-gray-500 dark:text-gray-400">
                  {item.estimate} pts
                </span>
              </Show>
            </div>
          </div>

          {/* Actions */}
          <div class="flex items-center gap-2 opacity-0 group-hover:opacity-100 transition-opacity">
            <Show when={config.canHaveChildren}>
              <button
                onClick={() => handleCreateChild(item.id)}
                class="p-2 text-gray-400 hover:text-purple-600 dark:hover:text-purple-400 transition-colors"
                title="Add child item"
              >
                <FiPlus size={16} />
              </button>
            </Show>
            <button
              onClick={() => handleEdit(item)}
              class="p-2 text-gray-400 hover:text-blue-600 dark:hover:text-blue-400 transition-colors"
              title="Edit"
            >
              <FiEdit2 size={16} />
            </button>
            <button
              onClick={() => handleDelete(item)}
              class="p-2 text-gray-400 hover:text-red-600 dark:hover:text-red-400 transition-colors"
              title="Delete"
            >
              <FiTrash2 size={16} />
            </button>
          </div>
        </div>

        {/* Children */}
        <Show when={hasChildren && isExpanded}>
          <div>
            <For each={item.children}>
              {(child) => renderItem(child, depth + 1)}
            </For>
          </div>
        </Show>
      </div>
    );
  };

  return (
    <div class="space-y-6">
      {/* Header */}
      <div class="flex items-center justify-between">
        <div>
          <h1 class="text-3xl font-bold text-gray-900 dark:text-white">
            Items List
          </h1>
          <p class="text-gray-600 dark:text-gray-400 mt-1">
            Hierarchical view of all work items
          </p>
        </div>
      </div>

      {/* Project Selector */}
      <div class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
        <div class="flex items-center gap-4">
          <label class="text-sm font-medium text-gray-700 dark:text-gray-300">
            Project:
          </label>
          <select
            value={selectedProject() || ''}
            onChange={(e) => setSelectedProject(e.currentTarget.value || null)}
            class="flex-1 px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-white focus:ring-2 focus:ring-purple-500 focus:border-transparent"
          >
            <option value="">Select a project...</option>
            <For each={projects()}>
              {(project) => (
                <option value={project.id}>{project.name}</option>
              )}
            </For>
          </select>
        </div>
      </div>

      <Show when={selectedProject()}>
        {/* Toolbar */}
        <div class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
          <div class="flex items-center gap-4">
            <input
              type="text"
              placeholder="Search items..."
              value={searchQuery()}
              onInput={(e) => setSearchQuery(e.currentTarget.value)}
              class="flex-1 px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-white focus:ring-2 focus:ring-purple-500 focus:border-transparent"
            />
            <button
              onClick={handleCreateRoot}
              class="px-6 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 transition-colors flex items-center gap-2 font-medium"
            >
              <FiPlus size={18} />
              New Item
            </button>
          </div>
        </div>

        {/* Items List */}
        <div class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden">
          <Show
            when={filteredItems().length > 0}
            fallback={
              <div class="text-center py-12 text-gray-500 dark:text-gray-400">
                <Show when={searchQuery()}>
                  <p>No items match your search</p>
                  <button
                    onClick={() => setSearchQuery('')}
                    class="mt-2 text-purple-600 hover:text-purple-700 dark:text-purple-400 dark:hover:text-purple-300"
                  >
                    Clear search
                  </button>
                </Show>
                <Show when={!searchQuery()}>
                  <p class="mb-4">No items yet</p>
                  <button
                    onClick={handleCreateRoot}
                    class="px-6 py-3 bg-purple-600 text-white rounded-lg hover:bg-purple-700 transition-colors inline-flex items-center gap-2 font-medium"
                  >
                    <FiPlus size={20} />
                    Create Your First Item
                  </button>
                </Show>
              </div>
            }
          >
            <div class="divide-y divide-gray-200 dark:divide-gray-700">
              <For each={filteredItems()}>
                {(item) => renderItem(item)}
              </For>
            </div>
          </Show>
        </div>

        {/* Quick Guide */}
        <div class="bg-blue-50 dark:bg-blue-900/20 rounded-lg border border-blue-200 dark:border-blue-800 p-4">
          <h3 class="font-medium text-blue-900 dark:text-blue-300 mb-2">
            💡 Quick Tips
          </h3>
          <ul class="text-sm text-blue-800 dark:text-blue-400 space-y-1">
            <li>• Click <FiChevronRight class="inline" size={14} /> to expand items with children</li>
            <li>• Hover over items to see actions: Add Child, Edit, Delete</li>
            <li>• Create hierarchy: Epic → Feature → Task → Subtask</li>
            <li>• Use search to quickly find items by title, description, or tags</li>
            <li>• Items are auto-sorted by type (Epic, Feature, Task, etc.)</li>
          </ul>
        </div>
      </Show>

      {/* Create/Edit Modal */}
      <CreateItemModal
        isOpen={createModalOpen()}
        onClose={handleModalClose}
        onSuccess={handleModalSuccess}
        projectId={selectedProject()!}
        mode={editingItem() ? 'edit' : 'create'}
        existingItem={editingItem()}
        parentId={createModalParentId()}
      />
    </div>
  );
}
