import { createResource, For, Show, createMemo } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { api } from '../../shared/api';

export default function Dashboard() {
  const params = useParams();
  const navigate = useNavigate();
  const projectId = params.id!;

  const [project] = createResource(() => api.projects.get(projectId));
  const [items] = createResource(() => api.items.list(projectId));

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
      case 'critical': return 'var(--color-danger)';
      case 'high': return 'var(--color-warning)';
      case 'medium': return 'var(--color-primary-400)';
      case 'low': return 'var(--color-success)';
      default: return 'var(--color-text-tertiary)';
    }
  };

  const getStatusCategoryColor = (category: string) => {
    switch (category) {
      case 'done': return 'var(--color-success)';
      case 'in_progress': return 'var(--color-primary-600)';
      case 'todo': return 'var(--color-text-tertiary)';
      default: return 'var(--color-primary-500)';
    }
  };

  return (
    <div class="min-h-screen p-6" style={{ background: 'var(--color-bg-base)' }}>
      <div class="max-w-7xl mx-auto">
        {/* Header */}
        <div class="mb-6 flex items-center justify-between">
          <div>
            <h1 class="text-3xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
              {project()?.name || 'Loading...'} - Dashboard
            </h1>
            <p class="mt-1" style={{ color: 'var(--color-text-secondary)' }}>
              Project overview and statistics
            </p>
          </div>
          <div class="flex gap-2">
            <button
              onClick={() => navigate(`/projects/${projectId}/board`)}
              class="px-4 py-2 rounded-lg transition-colors"
              style={{
                background: 'var(--color-bg-elevated)',
                border: '1px solid var(--color-border-light)',
                color: 'var(--color-text-primary)'
              }}
            >
              Board View
            </button>
            <button
              onClick={() => navigate(`/projects/${projectId}/list`)}
              class="px-4 py-2 rounded-lg transition-colors"
              style={{
                background: 'var(--color-bg-elevated)',
                border: '1px solid var(--color-border-light)',
                color: 'var(--color-text-primary)'
              }}
            >
              List View
            </button>
            <button
              onClick={() => navigate('/projects')}
              class="px-4 py-2 rounded-lg transition-colors"
              style={{
                background: 'var(--color-bg-elevated)',
                border: '1px solid var(--color-border-light)',
                color: 'var(--color-text-primary)'
              }}
            >
              Back to Projects
            </button>
          </div>
        </div>

        {/* Stats Grid */}
        <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 mb-6">
          {/* Total Items */}
          <div class="rounded-lg p-6" style={{ background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}>
            <div class="flex items-center justify-between">
              <div>
                <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>Total Items</p>
                <p class="text-3xl font-bold mt-2" style={{ color: 'var(--color-text-primary)' }}>
                  {stats().totalItems}
                </p>
              </div>
              <div class="w-12 h-12 rounded-lg flex items-center justify-center" style={{ background: 'var(--color-primary-100)' }}>
                <span class="text-2xl">📊</span>
              </div>
            </div>
          </div>

          {/* Completed Items */}
          <div class="rounded-lg p-6" style={{ background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}>
            <div class="flex items-center justify-between">
              <div>
                <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>Completed</p>
                <p class="text-3xl font-bold mt-2" style={{ color: 'var(--color-text-primary)' }}>
                  {stats().doneItems}
                </p>
              </div>
              <div class="w-12 h-12 rounded-lg flex items-center justify-center" style={{ background: 'rgba(34, 197, 94, 0.1)' }}>
                <span class="text-2xl">✅</span>
              </div>
            </div>
          </div>

          {/* Completion Rate */}
          <div class="rounded-lg p-6" style={{ background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}>
            <div class="flex items-center justify-between">
              <div>
                <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>Completion Rate</p>
                <p class="text-3xl font-bold mt-2" style={{ color: 'var(--color-text-primary)' }}>
                  {stats().completionRate}%
                </p>
              </div>
              <div class="w-12 h-12 rounded-lg flex items-center justify-center" style={{ background: 'var(--color-primary-100)' }}>
                <span class="text-2xl">📈</span>
              </div>
            </div>
          </div>

          {/* Recent Activity */}
          <div class="rounded-lg p-6" style={{ background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}>
            <div class="flex items-center justify-between">
              <div>
                <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>Added (7 days)</p>
                <p class="text-3xl font-bold mt-2" style={{ color: 'var(--color-text-primary)' }}>
                  {stats().recentItems}
                </p>
              </div>
              <div class="w-12 h-12 rounded-lg flex items-center justify-center" style={{ background: 'rgba(251, 146, 60, 0.1)' }}>
                <span class="text-2xl">🔥</span>
              </div>
            </div>
          </div>
        </div>

        <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Status Distribution */}
          <div class="rounded-lg p-6" style={{ background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}>
            <h2 class="text-xl font-bold mb-4" style={{ color: 'var(--color-text-primary)' }}>Status Distribution</h2>
            <div class="space-y-3">
              <For each={stats().byStatus}>
                {(status) => {
                  const percentage = stats().totalItems > 0
                    ? Math.round((status.count / stats().totalItems) * 100)
                    : 0;
                  return (
                    <div>
                      <div class="flex items-center justify-between mb-1">
                        <span class="text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>
                          {status.name}
                        </span>
                        <span class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                          {status.count} ({percentage}%)
                        </span>
                      </div>
                      <div class="w-full rounded-full h-2" style={{ background: 'var(--color-border-light)' }}>
                        <div
                          class="h-2 rounded-full"
                          style={{ width: `${percentage}%`, background: getStatusCategoryColor(status.category) }}
                        />
                      </div>
                    </div>
                  );
                }}
              </For>
            </div>
          </div>

          {/* Priority Distribution */}
          <div class="rounded-lg p-6" style={{ background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}>
            <h2 class="text-xl font-bold mb-4" style={{ color: 'var(--color-text-primary)' }}>Priority Distribution</h2>
            <div class="space-y-3">
              <For each={Object.entries(stats().byPriority)}>
                {([priority, count]) => {
                  const percentage = stats().totalItems > 0
                    ? Math.round((count / stats().totalItems) * 100)
                    : 0;
                  return (
                    <div>
                      <div class="flex items-center justify-between mb-1">
                        <span class="text-sm font-medium capitalize" style={{ color: 'var(--color-text-primary)' }}>
                          {priority}
                        </span>
                        <span class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                          {count} ({percentage}%)
                        </span>
                      </div>
                      <div class="w-full rounded-full h-2" style={{ background: 'var(--color-border-light)' }}>
                        <div
                          class="h-2 rounded-full"
                          style={{ width: `${percentage}%`, background: getPriorityColor(priority) }}
                        />
                      </div>
                    </div>
                  );
                }}
              </For>
            </div>
          </div>

          {/* Type Distribution */}
          <div class="rounded-lg p-6" style={{ background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}>
            <h2 class="text-xl font-bold mb-4" style={{ color: 'var(--color-text-primary)' }}>Item Types</h2>
            <div class="space-y-3">
              <For each={stats().byType}>
                {(type) => {
                  const percentage = stats().totalItems > 0
                    ? Math.round((type.count / stats().totalItems) * 100)
                    : 0;
                  return (
                    <div>
                      <div class="flex items-center justify-between mb-1">
                        <span class="text-sm font-medium capitalize" style={{ color: 'var(--color-text-primary)' }}>
                          {type.name}
                        </span>
                        <span class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                          {type.count} ({percentage}%)
                        </span>
                      </div>
                      <div class="w-full rounded-full h-2" style={{ background: 'var(--color-border-light)' }}>
                        <div
                          class="h-2 rounded-full"
                          style={{ width: `${percentage}%`, background: 'var(--color-primary-600)' }}
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
            <div class="rounded-lg p-6" style={{ background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}>
              <h2 class="text-xl font-bold mb-4" style={{ color: 'var(--color-text-primary)' }}>Story Points Progress</h2>
              <div class="space-y-4">
                <div class="flex items-center justify-between">
                  <span class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>Total Points</span>
                  <span class="text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
                    {stats().totalEstimate}
                  </span>
                </div>
                <div class="flex items-center justify-between">
                  <span class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>Completed</span>
                  <span class="text-2xl font-bold" style={{ color: 'var(--color-success)' }}>
                    {stats().completedEstimate}
                  </span>
                </div>
                <div>
                  <div class="flex items-center justify-between mb-2">
                    <span class="text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>
                      Progress
                    </span>
                    <span class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
                      {Math.round((stats().completedEstimate / stats().totalEstimate) * 100)}%
                    </span>
                  </div>
                  <div class="w-full rounded-full h-3" style={{ background: 'var(--color-border-light)' }}>
                    <div
                      class="h-3 rounded-full"
                      style={{
                        width: `${(stats().completedEstimate / stats().totalEstimate) * 100}%`,
                        background: 'linear-gradient(to right, var(--color-success), var(--color-success))'
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
