import { createSignal, createMemo, For, Show, createResource } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { api } from '../lib/api';
import type { Item } from '../types/api';

export default function Calendar() {
  const params = useParams();
  const navigate = useNavigate();
  const projectId = params.id!;

  const [project] = createResource(() => api.getProject(projectId));
  const [items] = createResource(() => api.listItems(projectId));

  const [currentDate, setCurrentDate] = createSignal(new Date());

  // Calendar calculations
  const currentMonth = createMemo(() => currentDate().getMonth());
  const currentYear = createMemo(() => currentDate().getFullYear());

  const daysInMonth = createMemo(() => {
    return new Date(currentYear(), currentMonth() + 1, 0).getDate();
  });

  const firstDayOfMonth = createMemo(() => {
    return new Date(currentYear(), currentMonth(), 1).getDay();
  });

  const monthName = createMemo(() => {
    return currentDate().toLocaleDateString('en-US', { month: 'long', year: 'numeric' });
  });

  const previousMonth = () => {
    const newDate = new Date(currentYear(), currentMonth() - 1, 1);
    setCurrentDate(newDate);
  };

  const nextMonth = () => {
    const newDate = new Date(currentYear(), currentMonth() + 1, 1);
    setCurrentDate(newDate);
  };

  const today = () => {
    setCurrentDate(new Date());
  };

  // Get items for a specific date
  const getItemsForDate = (day: number): Item[] => {
    const allItems = items() || [];
    const targetDate = new Date(currentYear(), currentMonth(), day);

    return allItems.filter(item => {
      // Check if item has due_date and matches this day
      if (!item.due_date) return false;

      const dueDate = new Date(item.due_date);
      return (
        dueDate.getDate() === targetDate.getDate() &&
        dueDate.getMonth() === targetDate.getMonth() &&
        dueDate.getFullYear() === targetDate.getFullYear()
      );
    });
  };

  const getPriorityColor = (priority: string) => {
    switch (priority) {
      case 'critical': return 'bg-red-500 text-white';
      case 'high': return 'bg-orange-500 text-white';
      case 'medium': return 'bg-yellow-500 text-white';
      case 'low': return 'bg-green-500 text-white';
      default: return 'bg-gray-500 text-white';
    }
  };

  const isToday = (day: number) => {
    const today = new Date();
    return (
      day === today.getDate() &&
      currentMonth() === today.getMonth() &&
      currentYear() === today.getFullYear()
    );
  };

  // Generate calendar days array
  const calendarDays = createMemo(() => {
    const days: (number | null)[] = [];

    // Add empty cells for days before the first day of month
    for (let i = 0; i < firstDayOfMonth(); i++) {
      days.push(null);
    }

    // Add days of the month
    for (let day = 1; day <= daysInMonth(); day++) {
      days.push(day);
    }

    return days;
  });

  return (
    <div class="min-h-screen bg-gray-50 dark:bg-gray-900 p-6">
      <div class="max-w-7xl mx-auto">
        {/* Header */}
        <div class="mb-6 flex items-center justify-between">
          <div>
            <h1 class="text-3xl font-bold text-gray-900 dark:text-white">
              {project()?.name || 'Loading...'} - Calendar
            </h1>
            <p class="text-gray-600 dark:text-gray-400 mt-1">
              Items by due date
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

        {/* Calendar Controls */}
        <div class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4 mb-6">
          <div class="flex items-center justify-between">
            <button
              onClick={previousMonth}
              class="px-4 py-2 bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-600 transition-colors"
            >
              ← Previous
            </button>

            <div class="flex items-center gap-4">
              <h2 class="text-2xl font-bold text-gray-900 dark:text-white">
                {monthName()}
              </h2>
              <button
                onClick={today}
                class="px-4 py-2 bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition-colors text-sm"
              >
                Today
              </button>
            </div>

            <button
              onClick={nextMonth}
              class="px-4 py-2 bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-600 transition-colors"
            >
              Next →
            </button>
          </div>
        </div>

        {/* Calendar Grid */}
        <div class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 overflow-hidden">
          {/* Weekday Headers */}
          <div class="grid grid-cols-7 border-b border-gray-200 dark:border-gray-700">
            <For each={['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday']}>
              {(day) => (
                <div class="p-4 text-center font-semibold text-gray-700 dark:text-gray-300 bg-gray-50 dark:bg-gray-900 border-r border-gray-200 dark:border-gray-700 last:border-r-0">
                  {day}
                </div>
              )}
            </For>
          </div>

          {/* Calendar Days */}
          <div class="grid grid-cols-7">
            <For each={calendarDays()}>
              {(day) => (
                <div
                  class="min-h-32 p-2 border-r border-b border-gray-200 dark:border-gray-700 last:border-r-0"
                  classList={{
                    'bg-gray-50 dark:bg-gray-900': day === null,
                    'bg-blue-50 dark:bg-blue-900/20': day !== null && isToday(day),
                  }}
                >
                  <Show when={day !== null}>
                    <div class="flex flex-col h-full">
                      <div
                        class="text-sm font-semibold mb-2"
                        classList={{
                          'text-blue-600 dark:text-blue-400': isToday(day!),
                          'text-gray-900 dark:text-white': !isToday(day!),
                        }}
                      >
                        {day}
                      </div>

                      <div class="flex-1 space-y-1 overflow-y-auto max-h-24">
                        <For each={getItemsForDate(day!)}>
                          {(item) => (
                            <div
                              class={`text-xs p-1 rounded cursor-pointer hover:opacity-80 transition-opacity ${getPriorityColor(item.priority)}`}
                              title={`${item.title} - ${item.status}`}
                            >
                              <div class="font-medium truncate">{item.title}</div>
                              <div class="text-xs opacity-90">{item.status}</div>
                            </div>
                          )}
                        </For>
                      </div>

                      <Show when={getItemsForDate(day!).length > 3}>
                        <div class="text-xs text-gray-500 dark:text-gray-400 mt-1">
                          +{getItemsForDate(day!).length - 3} more
                        </div>
                      </Show>
                    </div>
                  </Show>
                </div>
              )}
            </For>
          </div>
        </div>

        {/* Legend */}
        <div class="mt-6 bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
          <h3 class="text-sm font-semibold text-gray-700 dark:text-gray-300 mb-3">Priority Legend</h3>
          <div class="flex flex-wrap gap-4">
            <div class="flex items-center gap-2">
              <div class="w-4 h-4 rounded bg-red-500"></div>
              <span class="text-sm text-gray-700 dark:text-gray-300">Critical</span>
            </div>
            <div class="flex items-center gap-2">
              <div class="w-4 h-4 rounded bg-orange-500"></div>
              <span class="text-sm text-gray-700 dark:text-gray-300">High</span>
            </div>
            <div class="flex items-center gap-2">
              <div class="w-4 h-4 rounded bg-yellow-500"></div>
              <span class="text-sm text-gray-700 dark:text-gray-300">Medium</span>
            </div>
            <div class="flex items-center gap-2">
              <div class="w-4 h-4 rounded bg-green-500"></div>
              <span class="text-sm text-gray-700 dark:text-gray-300">Low</span>
            </div>
          </div>
        </div>

        {/* Items without due dates */}
        <Show when={(items() || []).filter(i => !i.due_date).length > 0}>
          <div class="mt-6 bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-4">
            <h3 class="text-lg font-semibold text-gray-700 dark:text-gray-300 mb-3">
              Items Without Due Date ({(items() || []).filter(i => !i.due_date).length})
            </h3>
            <p class="text-sm text-gray-600 dark:text-gray-400">
              These items don't have a due date set and won't appear on the calendar.
            </p>
          </div>
        </Show>
      </div>
    </div>
  );
}
