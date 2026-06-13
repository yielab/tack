import { createSignal, createMemo, For, Show } from 'solid-js';
import { useParams, useNavigate, useSearchParams } from '@solidjs/router';
import { api } from '../../shared/api';
import { Button, EmptyState } from '../../shared/ui';
import { toast } from '../../shared/ui/toast';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import type { Item } from '../../shared/types';

export default function Calendar() {
  const params = useParams();
  const projectId = params.id!;
  const navigate = useNavigate();
  const [, setSearchParams] = useSearchParams();

  const { items, refetch } = useProjectItems();

  const [currentDate, setCurrentDate] = createSignal(new Date());
  const [dragOverDay, setDragOverDay] = createSignal<number | null>(null);
  const [dragOverUnscheduled, setDragOverUnscheduled] = createSignal(false);

  const currentMonth = createMemo(() => currentDate().getMonth());
  const currentYear = createMemo(() => currentDate().getFullYear());
  const daysInMonth = createMemo(() => new Date(currentYear(), currentMonth() + 1, 0).getDate());
  const firstDayOfMonth = createMemo(() => new Date(currentYear(), currentMonth(), 1).getDay());
  const monthName = createMemo(() =>
    currentDate().toLocaleDateString('en-US', { month: 'long', year: 'numeric' }),
  );

  const previousMonth = () => setCurrentDate(new Date(currentYear(), currentMonth() - 1, 1));
  const nextMonth = () => setCurrentDate(new Date(currentYear(), currentMonth() + 1, 1));
  const goToday = () => setCurrentDate(new Date());

  const getItemsForDate = (day: number): Item[] => {
    const allItems = items() || [];
    const target = new Date(currentYear(), currentMonth(), day);
    return allItems.filter(item => {
      if (!item.due_date) return false;
      const d = new Date(item.due_date);
      return d.getFullYear() === target.getFullYear()
        && d.getMonth() === target.getMonth()
        && d.getDate() === target.getDate();
    });
  };

  const unscheduledItems = createMemo(() => (items() || []).filter(i => !i.due_date));

  const getPriorityStyle = (priority: string) => {
    switch (priority) {
      case 'critical': return { 'background-color': '#ef4444', color: 'var(--color-text-inverse)' };
      case 'high':     return { 'background-color': '#f97316', color: 'var(--color-text-inverse)' };
      case 'medium':   return { 'background-color': '#eab308', color: 'var(--color-text-inverse)' };
      case 'low':      return { 'background-color': '#22c55e', color: 'var(--color-text-inverse)' };
      default:         return { 'background-color': 'var(--color-border-medium)', color: 'var(--color-text-inverse)' };
    }
  };

  const isToday = (day: number) => {
    const t = new Date();
    return day === t.getDate() && currentMonth() === t.getMonth() && currentYear() === t.getFullYear();
  };

  const calendarDays = createMemo(() => {
    const days: (number | null)[] = [];
    for (let i = 0; i < firstDayOfMonth(); i++) days.push(null);
    for (let d = 1; d <= daysInMonth(); d++) days.push(d);
    return days;
  });

  // ── drag handlers ──────────────────────────────────────────────────────────

  const handleDragStart = (e: DragEvent, itemId: string) => {
    e.dataTransfer!.effectAllowed = 'move';
    e.dataTransfer!.setData('text/plain', itemId);
  };

  const handleDayDragOver = (e: DragEvent, day: number) => {
    e.preventDefault();
    e.dataTransfer!.dropEffect = 'move';
    setDragOverDay(day);
    setDragOverUnscheduled(false);
  };

  const handleDayDrop = async (e: DragEvent, day: number) => {
    e.preventDefault();
    setDragOverDay(null);
    const itemId = e.dataTransfer!.getData('text/plain');
    if (!itemId) return;

    // Format as local noon to avoid timezone-flip issues
    const y = currentYear(), m = currentMonth() + 1, d = day;
    const dueDate = `${y}-${String(m).padStart(2, '0')}-${String(d).padStart(2, '0')}T12:00:00.000Z`;

    try {
      await api.items.update(itemId, { due_date: dueDate });
      toast.success(`Due date set to ${new Date(dueDate).toLocaleDateString()}`);
      void refetch();
    } catch {
      toast.error('Failed to reschedule item');
    }
  };

  const handleUnscheduledDragOver = (e: DragEvent) => {
    e.preventDefault();
    e.dataTransfer!.dropEffect = 'move';
    setDragOverUnscheduled(true);
    setDragOverDay(null);
  };

  const handleUnscheduledDrop = async (e: DragEvent) => {
    e.preventDefault();
    setDragOverUnscheduled(false);
    const itemId = e.dataTransfer!.getData('text/plain');
    if (!itemId) return;

    // Clear the due date — use undefined in the PATCH body (omit the field)
    // The API accepts null/undefined for due_date to clear it
    try {
      await api.items.update(itemId, { due_date: null });
      toast.success('Due date cleared');
      void refetch();
    } catch {
      toast.error('Failed to clear due date');
    }
  };

  const clearDragOver = () => {
    setDragOverDay(null);
    setDragOverUnscheduled(false);
  };

  return (
    <div class="min-h-screen p-6" style={{ 'background-color': 'var(--color-bg-base)' }}>
      <div class="max-w-7xl mx-auto">
        <div class="mb-6">
          <h1 class="text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>Calendar</h1>
          <p class="mt-1 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
            Drag items between days to reschedule. Drop on "Unscheduled" to clear a due date.
          </p>
        </div>

        <Show when={(items() ?? []).length === 0}>
          <EmptyState
            icon="📅"
            title="No items to show on the calendar"
            description="Add items with due dates in Board or List — they'll appear here automatically."
            action={
              <Button onClick={() => navigate(`/projects/${projectId}/board`)}>
                Go to Board
              </Button>
            }
          />
        </Show>

        <Show when={(items() ?? []).length > 0}>
          {/* Calendar Controls */}
          <div
            class="rounded-lg p-4 mb-6"
            style={{ 'background-color': 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}
          >
            <div class="flex items-center justify-between">
              <Button variant="secondary" onClick={previousMonth}>← Previous</Button>
              <div class="flex items-center gap-4">
                <h2 class="text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
                  {monthName()}
                </h2>
                <Button size="sm" onClick={goToday}>Today</Button>
              </div>
              <Button variant="secondary" onClick={nextMonth}>Next →</Button>
            </div>
          </div>

          {/* Calendar Grid */}
          <div
            class="rounded-lg overflow-hidden"
            style={{ 'background-color': 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}
          >
            {/* Weekday Headers */}
            <div class="grid grid-cols-7" style={{ 'border-bottom': '1px solid var(--color-border-light)' }}>
              <For each={['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']}>
                {(day) => (
                  <div
                    class="p-3 text-center text-sm font-semibold"
                    style={{
                      color: 'var(--color-text-secondary)',
                      'background-color': 'var(--color-bg-base)',
                      'border-right': '1px solid var(--color-border-light)',
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
                    class="min-h-32 p-2"
                    style={{
                      'border-right': '1px solid var(--color-border-light)',
                      'border-bottom': '1px solid var(--color-border-light)',
                      'background-color': day === null
                        ? 'var(--color-bg-base)'
                        : dragOverDay() === day
                        ? 'var(--color-primary-50)'
                        : isToday(day)
                        ? 'rgba(59,130,246,0.07)'
                        : 'transparent',
                      'outline': dragOverDay() === day ? '2px solid var(--color-primary-400)' : 'none',
                      'outline-offset': '-2px',
                    }}
                    onDragOver={day !== null ? (e) => handleDayDragOver(e, day) : undefined}
                    onDragLeave={day !== null ? clearDragOver : undefined}
                    onDrop={day !== null ? (e) => handleDayDrop(e, day) : undefined}
                  >
                    <Show when={day !== null}>
                      <div class="flex flex-col h-full">
                        <div
                          class="text-sm font-semibold mb-2"
                          style={{
                            color: isToday(day!)
                              ? 'var(--color-primary-600)'
                              : 'var(--color-text-primary)',
                          }}
                        >
                          {day}
                        </div>

                        <div class="flex-1 space-y-1 overflow-y-auto max-h-24">
                          <For each={getItemsForDate(day!)}>
                            {(item) => (
                              <div
                                draggable={true}
                                onDragStart={(e) => handleDragStart(e, item.id)}
                                onClick={() => setSearchParams({ item: item.id })}
                                class="text-xs p-1 rounded cursor-grab active:cursor-grabbing hover:opacity-80 transition-opacity select-none"
                                style={getPriorityStyle(item.priority)}
                                title={`${item.title} — drag to reschedule`}
                              >
                                <div class="font-medium truncate">{item.title}</div>
                                <div class="opacity-80 truncate">{item.status}</div>
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

          {/* Unscheduled tray — drag items here to clear their due date */}
          <div
            class="mt-6 rounded-lg p-4 transition-colors"
            style={{
              'background-color': dragOverUnscheduled()
                ? 'var(--color-primary-50)'
                : 'var(--color-bg-elevated)',
              border: dragOverUnscheduled()
                ? '2px dashed var(--color-primary-400)'
                : '2px dashed var(--color-border-medium)',
            }}
            onDragOver={handleUnscheduledDragOver}
            onDragLeave={clearDragOver}
            onDrop={handleUnscheduledDrop}
          >
            <h3 class="text-sm font-semibold mb-2" style={{ color: 'var(--color-text-secondary)' }}>
              Unscheduled ({unscheduledItems().length}) — drop here to remove a due date
            </h3>
            <Show
              when={unscheduledItems().length > 0}
              fallback={
                <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                  All items have due dates — drag one here to unschedule it.
                </p>
              }
            >
              <div class="flex flex-wrap gap-2">
                <For each={unscheduledItems()}>
                  {(item) => (
                    <div
                      draggable={true}
                      onDragStart={(e) => handleDragStart(e, item.id)}
                      onClick={() => setSearchParams({ item: item.id })}
                      class="text-xs px-2 py-1 rounded cursor-grab active:cursor-grabbing hover:opacity-80 transition-opacity select-none"
                      style={getPriorityStyle(item.priority)}
                      title={`${item.title} — drag onto a day to schedule`}
                    >
                      {item.title}
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>

          {/* Priority Legend */}
          <div
            class="mt-4 rounded-lg p-4"
            style={{ 'background-color': 'var(--color-bg-elevated)', border: '1px solid var(--color-border-light)' }}
          >
            <div class="flex flex-wrap gap-4">
              <For each={[
                { label: 'Critical', color: '#ef4444' },
                { label: 'High',     color: '#f97316' },
                { label: 'Medium',   color: '#eab308' },
                { label: 'Low',      color: '#22c55e' },
              ]}>
                {(p) => (
                  <div class="flex items-center gap-2">
                    <div class="w-3 h-3 rounded" style={{ 'background-color': p.color }} />
                    <span class="text-xs" style={{ color: 'var(--color-text-secondary)' }}>{p.label}</span>
                  </div>
                )}
              </For>
            </div>
          </div>
        </Show>
      </div>
    </div>
  );
}
