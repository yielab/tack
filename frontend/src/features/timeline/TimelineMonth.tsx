import { createSignal, createMemo, For, Show } from 'solid-js';
import { useSearchParams } from '@solidjs/router';
import { api } from '../../shared/api';
import { Button } from '../../shared/ui';
import { toast } from '../../shared/ui/toast';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import { priorityFill } from '../../shared/ui/PriorityDot';
import type { Item, Priority } from '../../shared/types';
import { onActivateKey } from '../../shared/ui/keyActivate';
import TypeTile from '../../shared/ui/TypeTile';

/** The Timeline's Month layout: a grid of days with each item on the day it is due, and a tray for what has no date. */
export default function TimelineMonth() {
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

  // A group is planned, not scheduled by dragging: its due date is a milestone, and the day cell
  // lists it before the work. Only work is dragged between days and into the tray.
  const isWork = (item: Item) => item.node === 'work';

  const dueOn = (day: number): Item[] => {
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
  const getItemsForDate = (day: number): Item[] => dueOn(day).filter(isWork);
  const getMilestonesForDate = (day: number): Item[] => dueOn(day).filter(item => !isWork(item));

  const unscheduledItems = createMemo(() => (items() || []).filter(i => isWork(i) && !i.due_date));

  const getPriorityStyle = (priority: string) => ({
    'background-color': priorityFill(priority as Priority),
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
    <div class="flex flex-col gap-4">
      {/* Month controls + priority legend */}
      <div class="flex flex-wrap items-center gap-2">
        <Button variant="ghost" onClick={previousMonth}>← Previous</Button>
        <h2 class="mx-2 text-section text-content">{monthName()}</h2>
        <Button variant="secondary" onClick={goToday}>Today</Button>
        <Button variant="ghost" onClick={nextMonth}>Next →</Button>
        <div class="ml-auto flex flex-wrap gap-4 text-small text-content-muted">
          <For each={PRIORITY_LEGEND}>
            {(p) => (
              <span class="inline-flex items-center gap-1.5">
                <span class="h-2 w-2 rounded-sm" style={{ 'background-color': priorityFill(p.priority) }} />
                {p.label}
              </span>
            )}
          </For>
        </div>
      </div>

      {/* The grid of days */}
      <div class="grid grid-cols-7 gap-2">
        <For each={['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']}>
          {(day) => (
            <div class="px-2 text-micro font-bold uppercase tracking-wider text-content-subtle">
              {day}
            </div>
          )}
        </For>

        <For each={calendarDays()}>
          {(day) => (
            <div
              class="flex min-w-0 flex-col gap-1 rounded-md p-2"
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
                  class="text-small font-bold"
                  style={{ color: isToday(day!) ? 'var(--color-accent-ink)' : 'var(--color-text-primary)' }}
                >
                  {day}
                </div>

                <div class="flex max-h-24 flex-1 flex-col gap-1 overflow-y-auto">
                  <For each={getMilestonesForDate(day!)}>
                    {(group) => (
                      <button
                        type="button"
                        data-testid="milestone"
                        onClick={() => setSearchParams({ item: group.id })}
                        class="flex items-center gap-1 rounded-sm px-1.5 py-1 text-left text-micro focus:outline-none focus-visible:ring-2"
                        style={{
                          border: '1.5px solid var(--color-border-medium)',
                          'background-color': 'var(--color-bg-app)',
                          color: 'var(--color-text-primary)',
                          cursor: 'pointer',
                          '--tw-ring-color': 'var(--color-focus-ring)',
                        }}
                        title={`${group.title} is due — a group's due date is set on its page`}
                      >
                        <span aria-hidden="true">◆</span>
                        <TypeTile type={group.item_type} class="flex-none" />
                        <span class="min-w-0 truncate font-bold">{group.title}</span>
                      </button>
                    )}
                  </For>
                  <For each={getItemsForDate(day!)}>
                    {(item) => (
                      <div
                        draggable={true}
                        onDragStart={(e) => handleDragStart(e, item.id)}
                        onClick={() => setSearchParams({ item: item.id })}
                        role="button"
                        tabindex={0}
                        onKeyDown={onActivateKey(() => setSearchParams({ item: item.id }))}
                        class="cursor-grab select-none rounded-sm px-2 py-1 text-micro transition-[filter] hover:brightness-95 active:cursor-grabbing"
                        style={getPriorityStyle(item.priority)}
                        title={`${item.title} — drag to reschedule`}
                      >
                        <div class="truncate font-bold">{item.title}</div>
                        <div class="truncate">{item.status}</div>
                      </div>
                    )}
                  </For>
                </div>

                <Show when={getItemsForDate(day!).length > 3}>
                  <div class="text-micro text-content-muted">
                    +{getItemsForDate(day!).length - 3} more
                  </div>
                </Show>
              </Show>
            </div>
          )}
        </For>
      </div>

      {/* Unscheduled tray: drag items here to clear their due date */}
      <div
        class="flex flex-col gap-3 rounded-lg px-5 py-4 transition-colors"
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
        <h3 class="text-lead text-content">
          Unscheduled ({unscheduledItems().length}) — drop here to remove a due date
        </h3>
        <Show
          when={unscheduledItems().length > 0}
          fallback={
            <p class="text-small text-content-muted">
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
                  role="button"
                  tabindex={0}
                  onKeyDown={onActivateKey(() => setSearchParams({ item: item.id }))}
                  class="cursor-grab select-none rounded-sm px-2.5 py-1 text-small font-semibold transition-[filter] hover:brightness-95 active:cursor-grabbing"
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
    </div>
  );
}
