import { createSignal, createMemo, For, Show, createResource } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { api } from '../../shared/api';
import { Button, EmptyState } from '../../shared/ui';
import { useProject } from '../../shared/state/projectContext';

export default function Timeline() {
  const params = useParams();
  const projectId = params.id!;

  const navigate = useNavigate();
  const { project } = useProject();
  const [items] = createResource(() => api.items.list(projectId));

  // Dependency overlay (T-514): the set of items that are blocked by another
  // item. Each item's deps are fetched and an item is "blocked" when something
  // blocks it (target of a `blocks`, or source of an `is_blocked_by`).
  const [blockedIds] = createResource(items, async (its) => {
    const blocked = new Set<string>();
    await Promise.all(
      (its ?? []).map(async (it) => {
        const deps = await api.dependencies.list(it.id).catch(() => []);
        for (const d of deps) {
          const blockedHere =
            (d.target_item_id === it.id && d.dependency_type === 'blocks') ||
            (d.source_item_id === it.id && d.dependency_type === 'is_blocked_by');
          if (blockedHere) blocked.add(it.id);
        }
      }),
    );
    return blocked;
  });
  const isBlocked = (id: string) => blockedIds()?.has(id) ?? false;

  const [currentDate, setCurrentDate] = createSignal(new Date());
  const [viewMode, setViewMode] = createSignal<'week' | 'month' | 'quarter'>('month');

  // Timeline calculations based on view mode
  const timelineRange = createMemo(() => {
    const mode = viewMode();
    const today = currentDate();

    let startDate: Date;
    let endDate: Date;
    let dayCount: number;

    if (mode === 'week') {
      // Show 4 weeks
      startDate = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 7);
      endDate = new Date(today.getFullYear(), today.getMonth(), today.getDate() + 21);
      dayCount = 28;
    } else if (mode === 'month') {
      // Show 3 months
      startDate = new Date(today.getFullYear(), today.getMonth() - 1, 1);
      endDate = new Date(today.getFullYear(), today.getMonth() + 2, 0);
      dayCount = Math.ceil((endDate.getTime() - startDate.getTime()) / (1000 * 60 * 60 * 24));
    } else {
      // Show 6 months (quarter)
      startDate = new Date(today.getFullYear(), today.getMonth() - 2, 1);
      endDate = new Date(today.getFullYear(), today.getMonth() + 4, 0);
      dayCount = Math.ceil((endDate.getTime() - startDate.getTime()) / (1000 * 60 * 60 * 24));
    }

    return { startDate, endDate, dayCount };
  });

  // Get items with date ranges
  const timelineItems = createMemo(() => {
    const allItems = items() || [];
    const { startDate, endDate } = timelineRange();

    return allItems
      .map(item => {
        // Use created_at as start, due_date as end (or started_at/completed_at if available)
        const start = item.started_at
          ? new Date(item.started_at)
          : new Date(item.created_at);

        const end = item.completed_at
          ? new Date(item.completed_at)
          : item.due_date
            ? new Date(item.due_date)
            : new Date(start.getTime() + (7 * 24 * 60 * 60 * 1000)); // Default 7 days

        // Calculate position and width
        const totalMs = endDate.getTime() - startDate.getTime();
        const startOffset = start.getTime() - startDate.getTime();
        const duration = end.getTime() - start.getTime();

        const leftPercent = Math.max(0, (startOffset / totalMs) * 100);
        const widthPercent = Math.min(100 - leftPercent, (duration / totalMs) * 100);

        // Only show if overlaps with visible range
        const isVisible = end >= startDate && start <= endDate;

        return {
          ...item,
          startDate: start,
          endDate: end,
          leftPercent,
          widthPercent,
          isVisible,
        };
      })
      .filter(item => item.isVisible)
      .sort((a, b) => a.startDate.getTime() - b.startDate.getTime());
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

  const getStatusOpacity = (status: string) => {
    const proj = project();
    if (!proj) return 'opacity-100';

    const statusConfig = proj.workflow?.statuses?.find(s => s.name === status);
    if (statusConfig?.category === 'done') return 'opacity-50';
    if (statusConfig?.category === 'in_progress') return 'opacity-90';
    return 'opacity-100';
  };

  const previousPeriod = () => {
    const mode = viewMode();
    const current = currentDate();

    if (mode === 'week') {
      setCurrentDate(new Date(current.getFullYear(), current.getMonth(), current.getDate() - 28));
    } else if (mode === 'month') {
      setCurrentDate(new Date(current.getFullYear(), current.getMonth() - 3, 1));
    } else {
      setCurrentDate(new Date(current.getFullYear(), current.getMonth() - 6, 1));
    }
  };

  const nextPeriod = () => {
    const mode = viewMode();
    const current = currentDate();

    if (mode === 'week') {
      setCurrentDate(new Date(current.getFullYear(), current.getMonth(), current.getDate() + 28));
    } else if (mode === 'month') {
      setCurrentDate(new Date(current.getFullYear(), current.getMonth() + 3, 1));
    } else {
      setCurrentDate(new Date(current.getFullYear(), current.getMonth() + 6, 1));
    }
  };

  const today = () => {
    setCurrentDate(new Date());
  };

  // Generate month markers for timeline
  const monthMarkers = createMemo(() => {
    const { startDate, endDate } = timelineRange();
    const markers: { date: Date; label: string; leftPercent: number }[] = [];

    let current = new Date(startDate.getFullYear(), startDate.getMonth(), 1);
    const totalMs = endDate.getTime() - startDate.getTime();

    while (current <= endDate) {
      const offset = current.getTime() - startDate.getTime();
      const leftPercent = (offset / totalMs) * 100;

      markers.push({
        date: current,
        label: current.toLocaleDateString('en-US', { month: 'short', year: 'numeric' }),
        leftPercent: Math.max(0, leftPercent),
      });

      current = new Date(current.getFullYear(), current.getMonth() + 1, 1);
    }

    return markers;
  });

  // Calculate today's position
  const todayPosition = createMemo(() => {
    const { startDate, endDate } = timelineRange();
    const now = new Date();
    const totalMs = endDate.getTime() - startDate.getTime();
    const offset = now.getTime() - startDate.getTime();
    return Math.max(0, Math.min(100, (offset / totalMs) * 100));
  });

  return (
    <div class="min-h-screen bg-gray-50 dark:bg-gray-900 p-6">
      <div class="max-w-400 mx-auto">
        {/* Header */}
        <div class="mb-6">
          <h1 class="text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>Timeline</h1>
          <p class="mt-1" style={{ color: 'var(--color-text-secondary)' }}>Gantt-style timeline view</p>
        </div>

        {/* No items → point to Board */}
        <Show when={!items.loading && (items() ?? []).length === 0}>
          <EmptyState
            icon="📊"
            title="No items to display on the timeline"
            description="Add items in Board or List — they'll appear here once created."
            action={
              <Button onClick={() => navigate(`/projects/${projectId}/board`)}>
                Go to Board
              </Button>
            }
          />
        </Show>

        {/* Controls */}
        <Show when={(items() ?? []).length > 0}>
        <div class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4 mb-6">
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-4">
              <Button variant="secondary" onClick={previousPeriod}>← Previous</Button>
              <Button onClick={today}>Today</Button>
              <Button variant="secondary" onClick={nextPeriod}>Next →</Button>
            </div>

            <div class="flex items-center gap-2">
              <span class="text-sm text-gray-600 dark:text-gray-400">View:</span>
              <button
                onClick={() => setViewMode('week')}
                class="px-3 py-1 rounded-lg transition-colors"
                classList={{
                  'bg-blue-500 text-white': viewMode() === 'week',
                  'bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300': viewMode() !== 'week',
                }}
              >
                Week
              </button>
              <button
                onClick={() => setViewMode('month')}
                class="px-3 py-1 rounded-lg transition-colors"
                classList={{
                  'bg-blue-500 text-white': viewMode() === 'month',
                  'bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300': viewMode() !== 'month',
                }}
              >
                Month
              </button>
              <button
                onClick={() => setViewMode('quarter')}
                class="px-3 py-1 rounded-lg transition-colors"
                classList={{
                  'bg-blue-500 text-white': viewMode() === 'quarter',
                  'bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300': viewMode() !== 'quarter',
                }}
              >
                Quarter
              </button>
            </div>
          </div>
        </div>

        {/* Timeline */}
        <div class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden">
          {/* Timeline Header with Month Markers */}
          <div class="relative border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-900 h-12">
            <For each={monthMarkers()}>
              {(marker) => (
                <div
                  class="absolute top-0 bottom-0 border-l border-gray-300 dark:border-gray-600"
                  style={{ left: `${marker.leftPercent}%` }}
                >
                  <span class="absolute top-2 left-2 text-xs font-semibold text-gray-700 dark:text-gray-300">
                    {marker.label}
                  </span>
                </div>
              )}
            </For>

            {/* Today line */}
            <div
              class="absolute top-0 bottom-0 w-0.5 bg-blue-500 z-10"
              style={{ left: `${todayPosition()}%` }}
            >
              <div class="absolute -top-1 left-1/2 transform -translate-x-1/2 w-2 h-2 bg-blue-500 rounded-full"></div>
            </div>
          </div>

          {/* Timeline Items */}
          <div class="p-4 space-y-2 min-h-100">
            <Show when={timelineItems().length === 0}>
              <div class="text-center py-12 text-gray-500 dark:text-gray-400">
                No items to display in timeline
              </div>
            </Show>

            <For each={timelineItems()}>
              {(item) => (
                <div class="relative h-12">
                  {/* Item bar */}
                  <div
                    class={`absolute top-1 bottom-1 rounded-lg cursor-pointer transition-all hover:brightness-110 ${getPriorityColor(item.priority)} ${getStatusOpacity(item.status)}`}
                    classList={{ 'ring-2 ring-offset-1 ring-red-500': isBlocked(item.id) }}
                    style={{
                      left: `${item.leftPercent}%`,
                      width: `${Math.max(1, item.widthPercent)}%`
                    }}
                    title={`${item.title}\n${item.startDate.toLocaleDateString()} - ${item.endDate.toLocaleDateString()}\nStatus: ${item.status}\nPriority: ${item.priority}${isBlocked(item.id) ? '\n⛔ Blocked by a dependency' : ''}`}
                  >
                    <div class="px-2 py-1 h-full flex items-center gap-1">
                      <Show when={isBlocked(item.id)}>
                        <span class="text-xs" aria-label="blocked">⛔</span>
                      </Show>
                      <span class="text-white text-xs font-medium truncate">
                        {item.title}
                      </span>
                    </div>
                  </div>
                </div>
              )}
            </For>
          </div>
        </div>

        {/* Legend */}
        <div class="mt-6 bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
          <h3 class="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-3">Legend</h3>
          <div class="grid grid-cols-2 gap-4">
            <div>
              <h4 class="text-xs font-semibold text-gray-600 dark:text-gray-400 mb-2">Priority</h4>
              <div class="flex flex-wrap gap-3">
                <div class="flex items-center gap-2">
                  <div class="w-6 h-4 rounded bg-red-500"></div>
                  <span class="text-sm text-gray-700 dark:text-gray-300">Critical</span>
                </div>
                <div class="flex items-center gap-2">
                  <div class="w-6 h-4 rounded bg-orange-500"></div>
                  <span class="text-sm text-gray-700 dark:text-gray-300">High</span>
                </div>
                <div class="flex items-center gap-2">
                  <div class="w-6 h-4 rounded bg-yellow-500"></div>
                  <span class="text-sm text-gray-700 dark:text-gray-300">Medium</span>
                </div>
                <div class="flex items-center gap-2">
                  <div class="w-6 h-4 rounded bg-green-500"></div>
                  <span class="text-sm text-gray-700 dark:text-gray-300">Low</span>
                </div>
              </div>
            </div>
            <div>
              <h4 class="text-xs font-semibold text-gray-600 dark:text-gray-400 mb-2">Status</h4>
              <div class="flex flex-wrap gap-3">
                <div class="flex items-center gap-2">
                  <div class="w-6 h-4 rounded bg-gray-500"></div>
                  <span class="text-sm text-gray-700 dark:text-gray-300">To Do</span>
                </div>
                <div class="flex items-center gap-2">
                  <div class="w-6 h-4 rounded bg-gray-500 opacity-90"></div>
                  <span class="text-sm text-gray-700 dark:text-gray-300">In Progress</span>
                </div>
                <div class="flex items-center gap-2">
                  <div class="w-6 h-4 rounded bg-gray-500 opacity-50"></div>
                  <span class="text-sm text-gray-700 dark:text-gray-300">Done</span>
                </div>
              </div>
            </div>
          </div>

          <div class="mt-4 pt-4 border-t border-gray-200 dark:border-gray-700">
            <p class="text-xs text-gray-600 dark:text-gray-400">
              <strong>Date Range:</strong> Items use created_at (or started_at) as start date, and due_date (or completed_at) as end date. Items without dates default to 7-day duration.
            </p>
          </div>
        </div>
        </Show>{/* end: items > 0 */}
      </div>
    </div>
  );
}
