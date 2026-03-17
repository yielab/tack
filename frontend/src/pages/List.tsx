import { createSignal, createMemo, For, Show, createResource } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { api } from '../lib/api';
import { toast } from '../lib/toast';
import { withOptimisticUpdate } from '../lib/optimistic';
import type { Item } from '../types/api';

type SortField = 'title' | 'status' | 'priority' | 'item_type' | 'created_at' | 'updated_at';
type SortDirection = 'asc' | 'desc';

interface FilterState {
  search: string;
  status: string;
  priority: string;
  itemType: string;
}

export default function List() {
  const params = useParams();
  const navigate = useNavigate();
  const projectId = params.id!;

  // Resources
  const [project] = createResource(() => api.getProject(projectId));
  const [items, { refetch }] = createResource(() => api.listItems(projectId));

  // State
  const [sortField, setSortField] = createSignal<SortField>('created_at');
  const [sortDirection, setSortDirection] = createSignal<SortDirection>('desc');
  const [filters, setFilters] = createSignal<FilterState>({
    search: '',
    status: '',
    priority: '',
    itemType: '',
  });
  const [selectedItems, setSelectedItems] = createSignal<Set<string>>(new Set());
  const [showFilters, setShowFilters] = createSignal(false);

  // Computed values
  const statuses = createMemo(() => {
    const proj = project();
    return proj?.workflow?.statuses || [];
  });

  const uniqueItemTypes = createMemo(() => {
    const allItems = items();
    if (!allItems) return [];
    return Array.from(new Set(allItems.map((item: Item) => {
      const type = item.item_type;
      return typeof type === 'string' ? type : type.custom;
    }))).sort();
  });

  const filteredAndSortedItems = createMemo(() => {
    let result: Item[] = items() || [];
    const currentFilters = filters();

    // Apply filters
    if (currentFilters.search) {
      const search = currentFilters.search.toLowerCase();
      result = result.filter((item: Item) =>
        item.title.toLowerCase().includes(search) ||
        (item.description?.toLowerCase().includes(search)) ||
        (item.tags?.some((tag: string) => tag.toLowerCase().includes(search)))
      );
    }

    if (currentFilters.status) {
      result = result.filter((item: Item) => item.status === currentFilters.status);
    }

    if (currentFilters.priority) {
      result = result.filter((item: Item) => item.priority === currentFilters.priority);
    }

    if (currentFilters.itemType) {
      result = result.filter((item: Item) => {
        const type = item.item_type;
        const typeStr = typeof type === 'string' ? type : type.custom;
        return typeStr === currentFilters.itemType;
      });
    }

    // Apply sorting
    const field = sortField();
    const direction = sortDirection();

    result.sort((a: Item, b: Item) => {
      let aVal: any = a[field];
      let bVal: any = b[field];

      // Handle null/undefined
      if (aVal == null && bVal == null) return 0;
      if (aVal == null) return 1;
      if (bVal == null) return -1;

      // String comparison
      if (typeof aVal === 'string') {
        aVal = aVal.toLowerCase();
        bVal = bVal.toLowerCase();
      }

      const comparison = aVal < bVal ? -1 : aVal > bVal ? 1 : 0;
      return direction === 'asc' ? comparison : -comparison;
    });

    return result;
  });

  // Handlers
  const handleSort = (field: SortField) => {
    if (sortField() === field) {
      setSortDirection(prev => prev === 'asc' ? 'desc' : 'asc');
    } else {
      setSortField(field);
      setSortDirection('asc');
    }
  };

  const handleFilterChange = (key: keyof FilterState, value: string) => {
    setFilters(prev => ({ ...prev, [key]: value }));
  };

  const clearFilters = () => {
    setFilters({
      search: '',
      status: '',
      priority: '',
      itemType: '',
    });
  };

  const toggleItemSelection = (itemId: string) => {
    setSelectedItems(prev => {
      const newSet = new Set(prev);
      if (newSet.has(itemId)) {
        newSet.delete(itemId);
      } else {
        newSet.add(itemId);
      }
      return newSet;
    });
  };

  const toggleSelectAll = () => {
    const allItems = filteredAndSortedItems();
    if (selectedItems().size === allItems.length) {
      setSelectedItems(new Set<string>());
    } else {
      setSelectedItems(new Set<string>(allItems.map((item: Item) => item.id)));
    }
  };

  const handleBulkStatusChange = async (newStatus: string) => {
    const selected = Array.from(selectedItems());
    if (selected.length === 0) {
      toast.error('No items selected');
      return;
    }

    await withOptimisticUpdate(
      async () => {
        await Promise.all(selected.map(id => api.updateItem(id, { status: newStatus })));
      },
      () => {},
      async () => { await refetch(); },
      {
        showSuccessToast: true,
        successMessage: `Updated ${selected.length} item(s)`,
      }
    );

    setSelectedItems(new Set<string>());
    await refetch();
  };

  const handleDeleteSelected = async () => {
    const selected = Array.from(selectedItems());
    if (selected.length === 0) {
      toast.error('No items selected');
      return;
    }

    if (!confirm(`Delete ${selected.length} item(s)? This cannot be undone.`)) {
      return;
    }

    await withOptimisticUpdate(
      async () => {
        await Promise.all(selected.map(id => api.deleteItem(id)));
      },
      () => {},
      async () => { await refetch(); },
      {
        showSuccessToast: true,
        successMessage: `Deleted ${selected.length} item(s)`,
      }
    );

    setSelectedItems(new Set<string>());
    await refetch();
  };

  const getPriorityBadgeClass = (priority: string) => {
    switch (priority) {
      case 'critical': return 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200';
      case 'high': return 'bg-orange-100 text-orange-800 dark:bg-orange-900 dark:text-orange-200';
      case 'medium': return 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200';
      case 'low': return 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200';
      default: return 'bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-200';
    }
  };

  const formatDate = (dateStr: string) => {
    const date = new Date(dateStr);
    return new Intl.DateTimeFormat('en-US', {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
    }).format(date);
  };

  const SortIcon = (props: { field: SortField }) => (
    <Show when={sortField() === props.field}>
      <span class="ml-1 text-xs">
        {sortDirection() === 'asc' ? '↑' : '↓'}
      </span>
    </Show>
  );

  const hasActiveFilters = createMemo(() => {
    const f = filters();
    return f.search || f.status || f.priority || f.itemType;
  });

  return (
    <div class="min-h-screen bg-gray-50 dark:bg-gray-900">
      <div class="max-w-7xl mx-auto px-4 py-6">
        {/* Header */}
        <div class="mb-6">
          <div class="flex items-center justify-between mb-4">
            <div>
              <h1 class="text-3xl font-bold text-gray-900 dark:text-white">
                {project()?.name || 'Loading...'}
              </h1>
              <p class="text-gray-600 dark:text-gray-400 mt-1">
                {filteredAndSortedItems().length} item(s)
                <Show when={selectedItems().size > 0}>
                  {' '}• {selectedItems().size} selected
                </Show>
              </p>
            </div>
            <div class="flex gap-2">
              <button
                onClick={() => navigate(`/projects/${projectId}/board`)}
                class="px-4 py-2 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
              >
                Board View
              </button>
              <button
                onClick={() => navigate('/projects')}
                class="px-4 py-2 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
              >
                Back to Projects
              </button>
            </div>
          </div>

          {/* Toolbar */}
          <div class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
            <div class="flex items-center justify-between gap-4">
              {/* Search */}
              <input
                type="text"
                placeholder="Search items..."
                value={filters().search}
                onInput={(e) => handleFilterChange('search', e.currentTarget.value)}
                class="flex-1 px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-white focus:ring-2 focus:ring-blue-500 focus:border-transparent"
              />

              {/* Filter Toggle */}
              <button
                onClick={() => setShowFilters(!showFilters())}
                class={`px-4 py-2 rounded-lg border transition-colors ${
                  hasActiveFilters()
                    ? 'bg-blue-500 text-white border-blue-500'
                    : 'bg-white dark:bg-gray-800 border-gray-300 dark:border-gray-600 hover:bg-gray-50 dark:hover:bg-gray-700'
                }`}
              >
                Filters {hasActiveFilters() ? '(Active)' : ''}
              </button>

              {/* Bulk Actions */}
              <Show when={selectedItems().size > 0}>
                <div class="flex gap-2">
                  <select
                    onChange={(e) => {
                      const value = e.currentTarget.value;
                      if (value) {
                        handleBulkStatusChange(value);
                        e.currentTarget.value = '';
                      }
                    }}
                    class="px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-white"
                  >
                    <option value="">Change Status...</option>
                    <For each={statuses()}>
                      {(status) => <option value={status.name}>{status.name}</option>}
                    </For>
                  </select>
                  <button
                    onClick={handleDeleteSelected}
                    class="px-4 py-2 bg-red-500 text-white rounded-lg hover:bg-red-600 transition-colors"
                  >
                    Delete
                  </button>
                </div>
              </Show>
            </div>

            {/* Advanced Filters */}
            <Show when={showFilters()}>
              <div class="mt-4 pt-4 border-t border-gray-200 dark:border-gray-700 grid grid-cols-3 gap-4">
                <select
                  value={filters().status}
                  onChange={(e) => handleFilterChange('status', e.currentTarget.value)}
                  class="px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-white"
                >
                  <option value="">All Statuses</option>
                  <For each={statuses()}>
                    {(status) => <option value={status.name}>{status.name}</option>}
                  </For>
                </select>

                <select
                  value={filters().priority}
                  onChange={(e) => handleFilterChange('priority', e.currentTarget.value)}
                  class="px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-white"
                >
                  <option value="">All Priorities</option>
                  <option value="critical">Critical</option>
                  <option value="high">High</option>
                  <option value="medium">Medium</option>
                  <option value="low">Low</option>
                </select>

                <select
                  value={filters().itemType}
                  onChange={(e) => handleFilterChange('itemType', e.currentTarget.value)}
                  class="px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-white"
                >
                  <option value="">All Types</option>
                  <For each={uniqueItemTypes()}>
                    {(type: string) => <option value={type}>{type}</option>}
                  </For>
                </select>

                <button
                  onClick={clearFilters}
                  class="col-span-3 px-4 py-2 text-sm text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white transition-colors"
                >
                  Clear All Filters
                </button>
              </div>
            </Show>
          </div>
        </div>

        {/* Table */}
        <div class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden">
          <div class="overflow-x-auto">
            <table class="w-full">
              <thead class="bg-gray-50 dark:bg-gray-900 border-b border-gray-200 dark:border-gray-700">
                <tr>
                  <th class="px-4 py-3 text-left">
                    <input
                      type="checkbox"
                      checked={selectedItems().size > 0 && selectedItems().size === filteredAndSortedItems().length}
                      onChange={toggleSelectAll}
                      class="rounded border-gray-300 dark:border-gray-600"
                    />
                  </th>
                  <th
                    onClick={() => handleSort('title')}
                    class="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-800"
                  >
                    Title <SortIcon field="title" />
                  </th>
                  <th
                    onClick={() => handleSort('item_type')}
                    class="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-800"
                  >
                    Type <SortIcon field="item_type" />
                  </th>
                  <th
                    onClick={() => handleSort('status')}
                    class="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-800"
                  >
                    Status <SortIcon field="status" />
                  </th>
                  <th
                    onClick={() => handleSort('priority')}
                    class="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-800"
                  >
                    Priority <SortIcon field="priority" />
                  </th>
                  <th
                    onClick={() => handleSort('created_at')}
                    class="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-800"
                  >
                    Created <SortIcon field="created_at" />
                  </th>
                  <th
                    onClick={() => handleSort('updated_at')}
                    class="px-4 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider cursor-pointer hover:bg-gray-100 dark:hover:bg-gray-800"
                  >
                    Updated <SortIcon field="updated_at" />
                  </th>
                </tr>
              </thead>
              <tbody class="divide-y divide-gray-200 dark:divide-gray-700">
                <For each={filteredAndSortedItems()}>
                  {(item) => (
                    <tr class="hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors">
                      <td class="px-4 py-3">
                        <input
                          type="checkbox"
                          checked={selectedItems().has(item.id)}
                          onChange={() => toggleItemSelection(item.id)}
                          class="rounded border-gray-300 dark:border-gray-600"
                        />
                      </td>
                      <td class="px-4 py-3">
                        <div class="flex flex-col">
                          <span class="font-medium text-gray-900 dark:text-white">
                            {item.title}
                          </span>
                          <Show when={item.description}>
                            <span class="text-sm text-gray-500 dark:text-gray-400 truncate max-w-md">
                              {item.description}
                            </span>
                          </Show>
                          <Show when={item.tags && item.tags.length > 0}>
                            <div class="flex gap-1 mt-1">
                              <For each={item.tags}>
                                {(tag) => (
                                  <span class="text-xs px-2 py-0.5 bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300 rounded">
                                    {tag}
                                  </span>
                                )}
                              </For>
                            </div>
                          </Show>
                        </div>
                      </td>
                      <td class="px-4 py-3">
                        <span class="text-sm text-gray-900 dark:text-white capitalize">
                          {typeof item.item_type === 'string' ? item.item_type : item.item_type.custom}
                        </span>
                      </td>
                      <td class="px-4 py-3">
                        <span class="inline-flex px-2 py-1 text-xs font-semibold rounded-full bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200">
                          {item.status}
                        </span>
                      </td>
                      <td class="px-4 py-3">
                        <span class={`inline-flex px-2 py-1 text-xs font-semibold rounded-full ${getPriorityBadgeClass(item.priority)}`}>
                          {item.priority}
                        </span>
                      </td>
                      <td class="px-4 py-3 text-sm text-gray-500 dark:text-gray-400">
                        {formatDate(item.created_at)}
                      </td>
                      <td class="px-4 py-3 text-sm text-gray-500 dark:text-gray-400">
                        {formatDate(item.updated_at)}
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>

            <Show when={filteredAndSortedItems().length === 0}>
              <div class="text-center py-12 text-gray-500 dark:text-gray-400">
                <Show when={hasActiveFilters()} fallback={<p>No items yet</p>}>
                  <p>No items match your filters</p>
                  <button
                    onClick={clearFilters}
                    class="mt-2 text-blue-500 hover:text-blue-600"
                  >
                    Clear Filters
                  </button>
                </Show>
              </div>
            </Show>
          </div>
        </div>
      </div>
    </div>
  );
}
