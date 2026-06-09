import { createSignal, createMemo, For, Show, createResource } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { api } from '../../shared/api';
import type { Item } from '../../types/api';

export default function Calendar() {
  const params = useParams();
  const navigate = useNavigate();
  const projectId = params.id!;

  const [project] = createResource(() => api.projects.get(projectId));
  const [items] = createResource(() => api.items.list(projectId));

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

  const getPriorityStyle = (priority: string) => {
    switch (priority) {
      case 'critical': return { "background-color": '#ef4444', color: 'var(--color-text-inverse)' };
      case 'high': return { "background-color": '#f97316', color: 'var(--color-text-inverse)' };
      case 'medium': return { "background-color": '#eab308', color: 'var(--color-text-inverse)' };
      case 'low': return { "background-color": '#22c55e', color: 'var(--color-text-inverse)' };
      default: return { "background-color": 'var(--color-border-medium)', color: 'var(--color-text-inverse)' };
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
    <div class="min-h-screen p-6" style={{ "background-color": 'var(--color-bg-base)' }}>
      <div class="max-w-7xl mx-auto">
        {/* Header */}
        <div class="mb-6 flex items-center justify-between">
          <div>
            <h1 class="text-3xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
              {project()?.name || 'Loading...'} - Calendar
            </h1>
            <p class="mt-1" style={{ color: 'var(--color-text-secondary)' }}>
              Items by due date
            </p>
          </div>
          <div class="flex gap-2">
            <button
              onClick={() => navigate(`/projects/${projectId}/board`)}
              class="px-4 py-2 rounded-lg transition-colors"
              style={{
                "background-color": 'var(--color-bg-elevated)',
                border: '1px solid var(--color-border-light)',
                color: 'var(--color-text-primary)'
              }}
              onMouseEnter={(e) => e.currentTarget.style.backgroundColor = 'var(--color-bg-subtle)'}
              onMouseLeave={(e) => e.currentTarget.style.backgroundColor = 'var(--color-bg-elevated)'}
            >
              Board View
            </button>
            <button
              onClick={() => navigate(`/projects/${projectId}/list`)}
              class="px-4 py-2 rounded-lg transition-colors"
              style={{
                "background-color": 'var(--color-bg-elevated)',
                border: '1px solid var(--color-border-light)',
                color: 'var(--color-text-primary)'
              }}
              onMouseEnter={(e) => e.currentTarget.style.backgroundColor = 'var(--color-bg-subtle)'}
              onMouseLeave={(e) => e.currentTarget.style.backgroundColor = 'var(--color-bg-elevated)'}
            >
              List View
            </button>
            <button
              onClick={() => navigate('/projects')}
              class="px-4 py-2 rounded-lg transition-colors"
              style={{
                "background-color": 'var(--color-bg-elevated)',
                border: '1px solid var(--color-border-light)',
                color: 'var(--color-text-primary)'
              }}
              onMouseEnter={(e) => e.currentTarget.style.backgroundColor = 'var(--color-bg-subtle)'}
              onMouseLeave={(e) => e.currentTarget.style.backgroundColor = 'var(--color-bg-elevated)'}
            >
              Back to Projects
            </button>
          </div>
        </div>

        {/* Calendar Controls */}
        <div class="rounded-lg p-4 mb-6" style={{ "background-color": 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}>
          <div class="flex items-center justify-between">
            <button
              onClick={previousMonth}
              class="px-4 py-2 rounded-lg transition-colors"
              style={{
                "background-color": 'var(--color-bg-subtle)',
                color: 'var(--color-text-primary)'
              }}
              onMouseEnter={(e) => e.currentTarget.style.backgroundColor = 'var(--color-border-light)'}
              onMouseLeave={(e) => e.currentTarget.style.backgroundColor = 'var(--color-bg-subtle)'}
            >
              ← Previous
            </button>

            <div class="flex items-center gap-4">
              <h2 class="text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
                {monthName()}
              </h2>
              <button
                onClick={today}
                class="px-4 py-2 rounded-lg transition-colors text-sm"
                style={{
                  "background-color": 'var(--color-primary-600)',
                  color: 'var(--color-text-inverse)'
                }}
                onMouseEnter={(e) => e.currentTarget.style.backgroundColor = 'var(--color-primary-700)'}
                onMouseLeave={(e) => e.currentTarget.style.backgroundColor = 'var(--color-primary-600)'}
              >
                Today
              </button>
            </div>

            <button
              onClick={nextMonth}
              class="px-4 py-2 rounded-lg transition-colors"
              style={{
                "background-color": 'var(--color-bg-subtle)',
                color: 'var(--color-text-primary)'
              }}
              onMouseEnter={(e) => e.currentTarget.style.backgroundColor = 'var(--color-border-light)'}
              onMouseLeave={(e) => e.currentTarget.style.backgroundColor = 'var(--color-bg-subtle)'}
            >
              Next →
            </button>
          </div>
        </div>

        {/* Calendar Grid */}
        <div class="rounded-lg overflow-hidden" style={{ "background-color": 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}>
          {/* Weekday Headers */}
          <div class="grid grid-cols-7" style={{ "border-bottom": '1px solid var(--color-border-light)' }}>
            <For each={['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday']}>
              {(day) => (
                <div
                  class="p-4 text-center font-semibold last:border-r-0"
                  style={{
                    color: 'var(--color-text-primary)',
                    "background-color": 'var(--color-bg-base)',
                    "border-right": '1px solid var(--color-border-light)'
                  }}
                >
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
                  class="min-h-32 p-2 last:border-r-0"
                  style={{
                    "border-right": '1px solid var(--color-border-light)',
                    "border-bottom": '1px solid var(--color-border-light)',
                    "background-color": day === null
                      ? 'var(--color-bg-base)'
                      : (day !== null && isToday(day) ? 'rgba(59, 130, 246, 0.1)' : 'transparent')
                  }}
                >
                  <Show when={day !== null}>
                    <div class="flex flex-col h-full">
                      <div
                        class="text-sm font-semibold mb-2"
                        style={{
                          color: isToday(day!)
                            ? 'var(--color-primary-600)'
                            : 'var(--color-text-primary)'
                        }}
                      >
                        {day}
                      </div>

                      <div class="flex-1 space-y-1 overflow-y-auto max-h-24">
                        <For each={getItemsForDate(day!)}>
                          {(item) => (
                            <div
                              class="text-xs p-1 rounded cursor-pointer hover:opacity-80 transition-opacity"
                              style={getPriorityStyle(item.priority)}
                              title={`${item.title} - ${item.status}`}
                            >
                              <div class="font-medium truncate">{item.title}</div>
                              <div class="text-xs opacity-90">{item.status}</div>
                            </div>
                          )}
                        </For>
                      </div>

                      <Show when={getItemsForDate(day!).length > 3}>
                        <div class="text-xs mt-1" style={{ color: 'var(--color-text-tertiary)' }}>
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
        <div class="mt-6 rounded-lg p-4" style={{ "background-color": 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}>
          <h3 class="text-sm font-semibold mb-3" style={{ color: 'var(--color-text-primary)' }}>Priority Legend</h3>
          <div class="flex flex-wrap gap-4">
            <div class="flex items-center gap-2">
              <div class="w-4 h-4 rounded" style={{ "background-color": '#ef4444' }}></div>
              <span class="text-sm" style={{ color: 'var(--color-text-primary)' }}>Critical</span>
            </div>
            <div class="flex items-center gap-2">
              <div class="w-4 h-4 rounded" style={{ "background-color": '#f97316' }}></div>
              <span class="text-sm" style={{ color: 'var(--color-text-primary)' }}>High</span>
            </div>
            <div class="flex items-center gap-2">
              <div class="w-4 h-4 rounded" style={{ "background-color": '#eab308' }}></div>
              <span class="text-sm" style={{ color: 'var(--color-text-primary)' }}>Medium</span>
            </div>
            <div class="flex items-center gap-2">
              <div class="w-4 h-4 rounded" style={{ "background-color": '#22c55e' }}></div>
              <span class="text-sm" style={{ color: 'var(--color-text-primary)' }}>Low</span>
            </div>
          </div>
        </div>

        {/* Items without due dates */}
        <Show when={(items() || []).filter(i => !i.due_date).length > 0}>
          <div class="mt-6 rounded-lg p-4" style={{ "background-color": 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}>
            <h3 class="text-lg font-semibold mb-3" style={{ color: 'var(--color-text-primary)' }}>
              Items Without Due Date ({(items() || []).filter(i => !i.due_date).length})
            </h3>
            <p class="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
              These items don't have a due date set and won't appear on the calendar.
            </p>
          </div>
        </Show>
      </div>
    </div>
  );
}
