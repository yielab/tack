import { createResource, For, Show, createMemo } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { api } from '../lib/api';

export default function Dashboard() {
  const params = useParams();
  const navigate = useNavigate();
  const projectId = params.id!;

  const [project] = createResource(() => api.getProject(projectId));
  const [items] = createResource(() => api.listItems(projectId));

  // Computed statistics
  const stats = createMemo(() => {
    const allItems = items() || [];
    const proj = project();

    const statuses = proj?.workflow?.statuses || [];
    const totalItems = allItems.length;

    // Status distribution
    const byStatus = statuses.map(status => ({
      name: status.name,
      count: allItems.filter(item => item.status === status.name).length,
      category: status.category,
    }));

    // Priority distribution
    const byPriority = {
      critical: allItems.filter(i => i.priority === 'critical').length,
      high: allItems.filter(i => i.priority === 'high').length,
      medium: allItems.filter(i => i.priority === 'medium').length,
      low: allItems.filter(i => i.priority === 'low').length,
    };

    // Type distribution
    const typeSet = new Set(allItems.map(i => typeof i.item_type === 'string' ? i.item_type : i.item_type.custom));
    const byType = Array.from(typeSet).map(type => ({
      name: type,
      count: allItems.filter(i => {
        const itemType = typeof i.item_type === 'string' ? i.item_type : i.item_type.custom;
        return itemType === type;
      }).length,
    }));

    // Completion rate
    const doneItems = byStatus.filter(s => s.category === 'done').reduce((sum, s) => sum + s.count, 0);
    const completionRate = totalItems > 0 ? Math.round((doneItems / totalItems) * 100) : 0;

    // Items with estimates
    const withEstimates = allItems.filter(i => i.estimate && i.estimate > 0);
    const totalEstimate = withEstimates.reduce((sum, i) => sum + (i.estimate || 0), 0);
    const completedEstimate = withEstimates
      .filter(i => byStatus.find(s => s.name === i.status)?.category === 'done')
      .reduce((sum, i) => sum + (i.estimate || 0), 0);

    // Recent activity (last 7 days)
    const sevenDaysAgo = new Date();
    sevenDaysAgo.setDate(sevenDaysAgo.getDate() - 7);
    const recentItems = allItems.filter(i => new Date(i.created_at) > sevenDaysAgo);

    return {
      totalItems,
      byStatus,
      byPriority,
      byType,
      completionRate,
      totalEstimate,
      completedEstimate,
      recentItems: recentItems.length,
      doneItems,
    };
  });

  const getPriorityColor = (priority: string) => {
    switch (priority) {
      case 'critical': return 'bg-red-500';
      case 'high': return 'bg-orange-500';
      case 'medium': return 'bg-yellow-500';
      case 'low': return 'bg-green-500';
      default: return 'bg-gray-500';
    }
  };

  const getStatusCategoryColor = (category: string) => {
    switch (category) {
      case 'done': return 'bg-green-500';
      case 'in_progress': return 'bg-blue-500';
      case 'todo': return 'bg-gray-400';
      default: return 'bg-purple-500';
    }
  };

  return (
    <div class="min-h-screen bg-gray-50 dark:bg-gray-900 p-6">
      <div class="max-w-7xl mx-auto">
        {/* Header */}
        <div class="mb-6 flex items-center justify-between">
          <div>
            <h1 class="text-3xl font-bold text-gray-900 dark:text-white">
              {project()?.name || 'Loading...'} - Dashboard
            </h1>
            <p class="text-gray-600 dark:text-gray-400 mt-1">
              Project overview and statistics
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
              onClick={() => navigate(`/projects/${projectId}/list`)}
              class="px-4 py-2 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
            >
              List View
            </button>
            <button
              onClick={() => navigate('/projects')}
              class="px-4 py-2 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
            >
              Back to Projects
            </button>
          </div>
        </div>

        {/* Stats Grid */}
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 mb-6">
          {/* Total Items */}
          <div class="bg-white dark:bg-gray-800 rounded-lg p-6 border border-gray-200 dark:border-gray-700">
            <div class="flex items-center justify-between">
              <div>
                <p class="text-sm text-gray-600 dark:text-gray-400">Total Items</p>
                <p class="text-3xl font-bold text-gray-900 dark:text-white mt-2">
                  {stats().totalItems}
                </p>
              </div>
              <div class="w-12 h-12 bg-blue-100 dark:bg-blue-900 rounded-lg flex items-center justify-center">
                <span class="text-2xl">📊</span>
              </div>
            </div>
          </div>

          {/* Completed Items */}
          <div class="bg-white dark:bg-gray-800 rounded-lg p-6 border border-gray-200 dark:border-gray-700">
            <div class="flex items-center justify-between">
              <div>
                <p class="text-sm text-gray-600 dark:text-gray-400">Completed</p>
                <p class="text-3xl font-bold text-gray-900 dark:text-white mt-2">
                  {stats().doneItems}
                </p>
              </div>
              <div class="w-12 h-12 bg-green-100 dark:bg-green-900 rounded-lg flex items-center justify-center">
                <span class="text-2xl">✅</span>
              </div>
            </div>
          </div>

          {/* Completion Rate */}
          <div class="bg-white dark:bg-gray-800 rounded-lg p-6 border border-gray-200 dark:border-gray-700">
            <div class="flex items-center justify-between">
              <div>
                <p class="text-sm text-gray-600 dark:text-gray-400">Completion Rate</p>
                <p class="text-3xl font-bold text-gray-900 dark:text-white mt-2">
                  {stats().completionRate}%
                </p>
              </div>
              <div class="w-12 h-12 bg-purple-100 dark:bg-purple-900 rounded-lg flex items-center justify-center">
                <span class="text-2xl">📈</span>
              </div>
            </div>
          </div>

          {/* Recent Activity */}
          <div class="bg-white dark:bg-gray-800 rounded-lg p-6 border border-gray-200 dark:border-gray-700">
            <div class="flex items-center justify-between">
              <div>
                <p class="text-sm text-gray-600 dark:text-gray-400">Added (7 days)</p>
                <p class="text-3xl font-bold text-gray-900 dark:text-white mt-2">
                  {stats().recentItems}
                </p>
              </div>
              <div class="w-12 h-12 bg-orange-100 dark:bg-orange-900 rounded-lg flex items-center justify-center">
                <span class="text-2xl">🔥</span>
              </div>
            </div>
          </div>
        </div>

        <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Status Distribution */}
          <div class="bg-white dark:bg-gray-800 rounded-lg p-6 border border-gray-200 dark:border-gray-700">
            <h2 class="text-xl font-bold text-gray-900 dark:text-white mb-4">Status Distribution</h2>
            <div class="space-y-3">
              <For each={stats().byStatus}>
                {(status) => {
                  const percentage = stats().totalItems > 0
                    ? Math.round((status.count / stats().totalItems) * 100)
                    : 0;
                  return (
                    <div>
                      <div class="flex items-center justify-between mb-1">
                        <span class="text-sm font-medium text-gray-700 dark:text-gray-300">
                          {status.name}
                        </span>
                        <span class="text-sm text-gray-600 dark:text-gray-400">
                          {status.count} ({percentage}%)
                        </span>
                      </div>
                      <div class="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
                        <div
                          class={`h-2 rounded-full ${getStatusCategoryColor(status.category)}`}
                          style={{ width: `${percentage}%` }}
                        />
                      </div>
                    </div>
                  );
                }}
              </For>
            </div>
          </div>

          {/* Priority Distribution */}
          <div class="bg-white dark:bg-gray-800 rounded-lg p-6 border border-gray-200 dark:border-gray-700">
            <h2 class="text-xl font-bold text-gray-900 dark:text-white mb-4">Priority Distribution</h2>
            <div class="space-y-3">
              <For each={Object.entries(stats().byPriority)}>
                {([priority, count]) => {
                  const percentage = stats().totalItems > 0
                    ? Math.round((count / stats().totalItems) * 100)
                    : 0;
                  return (
                    <div>
                      <div class="flex items-center justify-between mb-1">
                        <span class="text-sm font-medium text-gray-700 dark:text-gray-300 capitalize">
                          {priority}
                        </span>
                        <span class="text-sm text-gray-600 dark:text-gray-400">
                          {count} ({percentage}%)
                        </span>
                      </div>
                      <div class="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
                        <div
                          class={`h-2 rounded-full ${getPriorityColor(priority)}`}
                          style={{ width: `${percentage}%` }}
                        />
                      </div>
                    </div>
                  );
                }}
              </For>
            </div>
          </div>

          {/* Type Distribution */}
          <div class="bg-white dark:bg-gray-800 rounded-lg p-6 border border-gray-200 dark:border-gray-700">
            <h2 class="text-xl font-bold text-gray-900 dark:text-white mb-4">Item Types</h2>
            <div class="space-y-3">
              <For each={stats().byType}>
                {(type) => {
                  const percentage = stats().totalItems > 0
                    ? Math.round((type.count / stats().totalItems) * 100)
                    : 0;
                  return (
                    <div>
                      <div class="flex items-center justify-between mb-1">
                        <span class="text-sm font-medium text-gray-700 dark:text-gray-300 capitalize">
                          {type.name}
                        </span>
                        <span class="text-sm text-gray-600 dark:text-gray-400">
                          {type.count} ({percentage}%)
                        </span>
                      </div>
                      <div class="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2">
                        <div
                          class="h-2 rounded-full bg-indigo-500"
                          style={{ width: `${percentage}%` }}
                        />
                      </div>
                    </div>
                  );
                }}
              </For>
            </div>
          </div>

          {/* Story Points Progress */}
          <Show when={stats().totalEstimate > 0}>
            <div class="bg-white dark:bg-gray-800 rounded-lg p-6 border border-gray-200 dark:border-gray-700">
              <h2 class="text-xl font-bold text-gray-900 dark:text-white mb-4">Story Points Progress</h2>
              <div class="space-y-4">
                <div class="flex items-center justify-between">
                  <span class="text-sm text-gray-600 dark:text-gray-400">Total Points</span>
                  <span class="text-2xl font-bold text-gray-900 dark:text-white">
                    {stats().totalEstimate}
                  </span>
                </div>
                <div class="flex items-center justify-between">
                  <span class="text-sm text-gray-600 dark:text-gray-400">Completed</span>
                  <span class="text-2xl font-bold text-green-600 dark:text-green-400">
                    {stats().completedEstimate}
                  </span>
                </div>
                <div>
                  <div class="flex items-center justify-between mb-2">
                    <span class="text-sm font-medium text-gray-700 dark:text-gray-300">
                      Progress
                    </span>
                    <span class="text-sm text-gray-600 dark:text-gray-400">
                      {Math.round((stats().completedEstimate / stats().totalEstimate) * 100)}%
                    </span>
                  </div>
                  <div class="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-3">
                    <div
                      class="h-3 rounded-full bg-gradient-to-r from-green-500 to-green-600"
                      style={{
                        width: `${(stats().completedEstimate / stats().totalEstimate) * 100}%`,
                      }}
                    />
                  </div>
                </div>
              </div>
            </div>
          </Show>
        </div>
      </div>
    </div>
  );
}
