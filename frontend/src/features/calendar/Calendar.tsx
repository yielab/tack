import { createSignal, createMemo, For, Show } from 'solid-js';
import { useParams, useNavigate, useSearchParams } from '@solidjs/router';
import { api } from '../../shared/api';
import { Button, EmptyState } from '../../shared/ui';
import { toast } from '../../shared/ui/toast';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import { priorityColor } from '../../shared/ui/PriorityDot';
import { IconCalendar } from '../../shared/ui/icons';
import type { Item, Priority } from '../../shared/types';

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

  const getPriorityStyle = (priority: string) => ({
    'background-color': priorityColor(priority as Priority),
    color: 'var(--color-text-inverse)',
  });

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

  const PRIORITY_LEGEND = [
    { label: 'Critical', priority: 'critical' },
    { label: 'High',     priority: 'high' },
    { label: 'Medium',   priority: 'medium' },
    { label: 'Low',      priority: 'low' },
  ] as const;

  return (
    <div class="flex flex-col gap-3">
      <div>
        <h1 class="m-0 text-content" style={{ 'font-size': '34px' }}>Calendar</h1>
        <p class="mt-0.5 text-[13px] text-content-muted">
          Drag items between days to reschedule. Drop on "Unscheduled" to clear a due date.
        </p>
      </div>

      <Show when={(items() ?? []).length === 0}>
        <div class="rounded-[28px] bg-panel">
          <EmptyState
            icon={<IconCalendar size={28} />}
            title="No items to show on the calendar"
            description="Add items with due dates in Board or List — they'll appear here automatically."
            action={
              <Button variant="secondary" size="sm" onClick={() => navigate(`/projects/${projectId}/board`)}>
                Go to Board
              </Button>
            }
          />
        </div>
      </Show>

      <Show when={(items() ?? []).length > 0}>
        {/* Calendar controls + priority legend */}
        <div class="flex flex-wrap items-center gap-2.5">
          <Button variant="secondary" onClick={previousMonth}>← Previous</Button>
          <h2 class="mx-1.5 text-content" style={{ 'font-size': '22px' }}>
            {monthName()}
          </h2>
          <Button variant="secondary" size="sm" onClick={goToday}>Today</Button>
          <Button variant="secondary" onClick={nextMonth}>Next →</Button>
          <div class="ml-auto flex flex-wrap gap-3 text-xs text-content-muted">
            <For each={PRIORITY_LEGEND}>
              {(p) => (
                <span class="inline-flex items-center gap-1.5">
                  <span class="w-2 h-2 rounded-[3px]" style={{ 'background-color': priorityColor(p.priority) }} />
                  {p.label}
                </span>
              )}
            </For>
          </div>
        </div>

        {/* Calendar Grid */}
        <div class="grid grid-cols-7 gap-1.5">
          {/* Weekday Headers */}
          <For each={['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']}>
            {(day) => (
              <div class="px-2 text-[11px] font-bold uppercase tracking-[.08em] text-content-subtle">
                {day}
              </div>
            )}
          </For>

          {/* Calendar Days */}
          <For each={calendarDays()}>
            {(day) => (
              <div
                class="min-w-0 rounded-[18px] p-2 flex flex-col gap-1"
                style={{
                  'min-height': '118px',
                  'background-color': day === null
                    ? 'transparent'
                    : dragOverDay() === day
                    ? 'var(--color-accent2-soft)'
                    : isToday(day)
                    ? 'var(--color-accent-soft)'
                    : 'var(--color-bg-panel)',
                  outline: dragOverDay() === day ? '2px solid var(--color-accent2)' : 'none',
                  'outline-offset': '-2px',
                }}
                onDragOver={day !== null ? (e) => handleDayDragOver(e, day) : undefined}
                onDragLeave={day !== null ? clearDragOver : undefined}
                onDrop={day !== null ? (e) => handleDayDrop(e, day) : undefined}
              >
                <Show when={day !== null}>
                  <div
                    class="text-xs font-bold"
                    style={{ color: isToday(day!) ? 'var(--color-accent-ink)' : 'var(--color-text-primary)' }}
                  >
                    {day}
                  </div>

                  <div class="flex-1 flex flex-col gap-1 overflow-y-auto max-h-24">
                    <For each={getItemsForDate(day!)}>
                      {(item) => (
                        <div
                          draggable={true}
                          onDragStart={(e) => handleDragStart(e, item.id)}
                          onClick={() => setSearchParams({ item: item.id })}
                          class="text-[11px] px-2 py-[3px] rounded-[10px] cursor-grab active:cursor-grabbing hover:brightness-95 transition-[filter] select-none"
                          style={getPriorityStyle(item.priority)}
                          title={`${item.title} — drag to reschedule`}
                        >
                          <div class="font-bold truncate">{item.title}</div>
                          <div class="truncate">{item.status}</div>
                        </div>
                      )}
                    </For>
                  </div>

                  <Show when={getItemsForDate(day!).length > 3}>
                    <div class="text-[11px] text-content-muted">
                      +{getItemsForDate(day!).length - 3} more
                    </div>
                  </Show>
                </Show>
              </div>
            )}
          </For>
        </div>

        {/* Unscheduled tray — drag items here to clear their due date */}
        <div
          class="rounded-[28px] px-4 py-3 flex flex-col gap-2 transition-colors"
          style={{
            'background-color': dragOverUnscheduled() ? 'var(--color-accent2-soft)' : 'transparent',
            border: dragOverUnscheduled()
              ? '2px dashed var(--color-accent2)'
              : '2px dashed var(--color-border-light)',
          }}
          onDragOver={handleUnscheduledDragOver}
          onDragLeave={clearDragOver}
          onDrop={handleUnscheduledDrop}
        >
          <h3 class="font-sans text-[13px] font-bold text-content">
            Unscheduled ({unscheduledItems().length}) — drop here to remove a due date
          </h3>
          <Show
            when={unscheduledItems().length > 0}
            fallback={
              <p class="text-xs text-content-subtle">
                All items have due dates — drag one here to unschedule it.
              </p>
            }
          >
            <div class="flex flex-wrap gap-1.5">
              <For each={unscheduledItems()}>
                {(item) => (
                  <div
                    draggable={true}
                    onDragStart={(e) => handleDragStart(e, item.id)}
                    onClick={() => setSearchParams({ item: item.id })}
                    class="text-xs font-semibold px-2.5 py-[3px] rounded-full cursor-grab active:cursor-grabbing hover:brightness-95 transition-[filter] select-none"
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
      </Show>
    </div>
  );
}
