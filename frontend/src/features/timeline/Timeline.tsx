import {
  createSignal,
  createMemo,
  For,
  Show,
  createResource,
} from 'solid-js';
import { useParams, useNavigate, useSearchParams } from '@solidjs/router';
import { api } from '../../shared/api';
import { Button, EmptyState, LoadError } from '../../shared/ui';
import { toast } from '../../shared/ui/toast';
import { useProject } from '../../shared/state/projectContext';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import { priorityFill } from '../../shared/ui/PriorityDot';
import { IconChevronDown, IconTimeline } from '../../shared/ui/icons';
import { progressText } from '../../shared/ui/ProgressBar';
import TypeTile from '../../shared/ui/TypeTile';
import { ancestorsOf, treeRows } from '../../shared/tree';
import type { Item, Priority } from '../../shared/types';
import { onActivateKey } from '../../shared/ui/keyActivate';
import Card from '../../shared/ui/Card';

// ── Drag state ─────────────────────────────────────────────────────────────

type DragMode = 'move' | 'resize-start' | 'resize-end';

interface DragState {
  itemId: string;
  mode: DragMode;
  startClientX: number;
  origStart: Date;
  origEnd: Date;
}

interface DragPreview {
  newStart: Date;
  newEnd: Date;
}

// ── Helpers ────────────────────────────────────────────────────────────────

function formatIso(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}T12:00:00.000Z`;
}

function addDays(d: Date, n: number): Date {
  return new Date(d.getTime() + n * 86_400_000);
}

const isWork = (item: Item) => item.node === 'work';

/**
 * Where an item's bar starts and ends. Work draws from when it started (else was created) to its due
 * date; a group draws from the earliest start of its work to the latest due date of its work, which
 * the server works out, and only falls back to its own dates when it has no work yet.
 */
function spanOf(item: Item): { start: Date; end: Date } {
  const group = item.node === 'group';
  const start = new Date((group ? item.progress?.starts : null) ?? item.started_at ?? item.created_at);
  const end = (group ? item.progress?.ends : null) ?? item.due_date;
  return { start, end: end ? new Date(end) : addDays(start, 7) };
}

/** A group whose work ends after the date the group itself is due. */
function endsAfterDue(item: Item): boolean {
  const ends = item.progress?.ends;
  return !!ends && !!item.due_date && new Date(ends).getTime() > new Date(item.due_date).getTime();
}

// ── Component ──────────────────────────────────────────────────────────────

export default function Timeline() {
  const params = useParams();
  const projectId = params.id!;
  const navigate = useNavigate();
  const [, setSearchParams] = useSearchParams();
  const { project } = useProject();

  const { items, refetch } = useProjectItems();

  const [blockedResource, { refetch: refetchBlocked }] = createResource(items, async (its) => {
    const blocked = new Set<string>();
    // A group has no dependencies of its own: its work does.
    await Promise.all(
      (its ?? []).filter(isWork).map(async (it) => {
        const deps = await api.dependencies.list(it.id);
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
  const isBlocked = (id: string) => (blockedResource.error ? false : blockedResource()?.has(id) ?? false);

  const [currentDate, setCurrentDate] = createSignal(new Date());
  const [viewMode, setViewMode] = createSignal<'week' | 'month' | 'quarter'>('month');

  // Drag state
  const [dragState, setDragState] = createSignal<DragState | null>(null);
  const [previews, setPreviews] = createSignal<Map<string, DragPreview>>(new Map());

  // Container ref for pixel→day conversion
  let containerRef: HTMLDivElement | undefined;

  // ── Timeline range ─────────────────────────────────────────────────────

  const timelineRange = createMemo(() => {
    const mode = viewMode();
    const today = currentDate();
    let startDate: Date;
    let endDate: Date;

    if (mode === 'week') {
      startDate = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 7);
      endDate   = new Date(today.getFullYear(), today.getMonth(), today.getDate() + 21);
    } else if (mode === 'month') {
      startDate = new Date(today.getFullYear(), today.getMonth() - 1, 1);
      endDate   = new Date(today.getFullYear(), today.getMonth() + 2, 0);
    } else {
      startDate = new Date(today.getFullYear(), today.getMonth() - 2, 1);
      endDate   = new Date(today.getFullYear(), today.getMonth() + 4, 0);
    }

    const dayCount = Math.ceil((endDate.getTime() - startDate.getTime()) / 86_400_000);
    return { startDate, endDate, dayCount };
  });

  // Convert pixel delta → whole-day delta
  const pixelsToDays = (deltaX: number): number => {
    if (!containerRef) return 0;
    const { dayCount } = timelineRange();
    const w = containerRef.getBoundingClientRect().width || 1;
    return Math.round(deltaX / (w / dayCount));
  };

  // ── Item positioning ───────────────────────────────────────────────────

  const positionItem = (item: Item, preview?: DragPreview) => {
    const { startDate, endDate } = timelineRange();
    const totalMs = endDate.getTime() - startDate.getTime();

    const span = spanOf(item);
    const start = preview?.newStart ?? span.start;
    const end = preview?.newEnd ?? span.end;

    const leftPercent  = Math.max(0, ((start.getTime() - startDate.getTime()) / totalMs) * 100);
    const widthPercent = Math.min(100 - leftPercent, ((end.getTime() - start.getTime()) / totalMs) * 100);
    const isVisible    = end >= startDate && start <= endDate;

    return { start, end, leftPercent, widthPercent: Math.max(0.5, widthPercent), isVisible };
  };

  // Groups folded shut by the person; their work leaves the chart until they are opened.
  const [folded, setFolded] = createSignal<ReadonlySet<string>>(new Set());
  const toggleFold = (id: string) =>
    setFolded((current) => {
      const next = new Set(current);
      if (!next.delete(id)) next.add(id);
      return next;
    });

  const hasGroups = createMemo(() => (items() ?? []).some((item) => !isWork(item)));
  const itemsById = createMemo(() => new Map((items() ?? []).map((item) => [item.id, item])));

  // The chart reads like the tree: a group, then its work. A group stays on the chart when its own bar is
  // out of the window but some of its work is in it, so the work keeps its heading.
  const timelineItems = createMemo(() => {
    const all = items() ?? [];
    const byId = itemsById();
    const inRange = all.filter((item) => positionItem(item).isVisible);
    const shown = new Set(inRange.map((item) => item.id));
    for (const item of inRange) for (const ancestor of ancestorsOf(item, byId)) shown.add(ancestor.id);
    const closed = folded();
    // Siblings are listed by when they start.
    const byStart = [...all].sort((a, b) => spanOf(a).start.getTime() - spanOf(b).start.getTime());
    return treeRows(byStart)
      .filter(({ item }) => shown.has(item.id) && !ancestorsOf(item, byId).some((ancestor) => closed.has(ancestor.id)))
      .map(({ item, depth }) => ({ ...item, ...positionItem(item), depth }));
  });

  // ── Drag handlers ──────────────────────────────────────────────────────

  const handleBarPointerDown = (
    e: PointerEvent,
    item: Item,
    mode: DragMode,
  ) => {
    e.preventDefault();
    e.stopPropagation();
    (e.target as Element).setPointerCapture(e.pointerId);

    const { start, end } = spanOf(item);

    setDragState({ itemId: item.id, mode, startClientX: e.clientX, origStart: start, origEnd: end });
  };

  const handlePointerMove = (e: PointerEvent) => {
    const ds = dragState();
    if (!ds) return;

    const deltaDays = pixelsToDays(e.clientX - ds.startClientX);

    let newStart = ds.origStart;
    let newEnd   = ds.origEnd;

    if (ds.mode === 'move') {
      newStart = addDays(ds.origStart, deltaDays);
      newEnd   = addDays(ds.origEnd,   deltaDays);
    } else if (ds.mode === 'resize-start') {
      newStart = addDays(ds.origStart, deltaDays);
      if (newStart >= newEnd) newStart = addDays(newEnd, -1);
    } else {
      newEnd = addDays(ds.origEnd, deltaDays);
      if (newEnd <= newStart) newEnd = addDays(newStart, 1);
    }

    setPreviews(new Map([[ds.itemId, { newStart, newEnd }]]));
  };

  const handlePointerUp = async () => {
    const ds = dragState();
    if (!ds) return;

    const preview = previews().get(ds.itemId);
    setDragState(null);
    setPreviews(new Map());

    if (!preview) return;
    if (
      preview.newStart.getTime() === ds.origStart.getTime() &&
      preview.newEnd.getTime()   === ds.origEnd.getTime()
    ) return;

    try {
      const patch: Record<string, string | null> = {};
      if (ds.mode !== 'resize-end')   patch.started_at = formatIso(preview.newStart);
      if (ds.mode !== 'resize-start') patch.due_date   = formatIso(preview.newEnd);

      await api.items.update(ds.itemId, patch as any);
      toast.success('Dates updated');
      void refetch();
    } catch {
      toast.error('Failed to update dates');
    }
  };

  // ── Navigation ─────────────────────────────────────────────────────────

  const previousPeriod = () => {
    const mode = viewMode(), c = currentDate();
    if (mode === 'week')    setCurrentDate(new Date(c.getFullYear(), c.getMonth(), c.getDate() - 28));
    else if (mode === 'month')   setCurrentDate(new Date(c.getFullYear(), c.getMonth() - 3, 1));
    else                         setCurrentDate(new Date(c.getFullYear(), c.getMonth() - 6, 1));
  };

  const nextPeriod = () => {
    const mode = viewMode(), c = currentDate();
    if (mode === 'week')    setCurrentDate(new Date(c.getFullYear(), c.getMonth(), c.getDate() + 28));
    else if (mode === 'month')   setCurrentDate(new Date(c.getFullYear(), c.getMonth() + 3, 1));
    else                         setCurrentDate(new Date(c.getFullYear(), c.getMonth() + 6, 1));
  };

  const goToday = () => setCurrentDate(new Date());

  // ── Month markers & today line ─────────────────────────────────────────

  const monthMarkers = createMemo(() => {
    const { startDate, endDate } = timelineRange();
    const totalMs = endDate.getTime() - startDate.getTime();
    const markers: { label: string; leftPercent: number }[] = [];
    let cur = new Date(startDate.getFullYear(), startDate.getMonth(), 1);
    while (cur <= endDate) {
      markers.push({
        label: cur.toLocaleDateString('en-US', { month: 'short', year: 'numeric' }),
        leftPercent: Math.max(0, ((cur.getTime() - startDate.getTime()) / totalMs) * 100),
      });
      cur = new Date(cur.getFullYear(), cur.getMonth() + 1, 1);
    }
    return markers;
  });

  const todayPosition = createMemo(() => {
    const { startDate, endDate } = timelineRange();
    const totalMs = endDate.getTime() - startDate.getTime();
    return Math.max(0, Math.min(100, ((Date.now() - startDate.getTime()) / totalMs) * 100));
  });

  /** Where a group's own due date falls in the window, in percent; null outside it or without one. */
  const dueLeft = (item: Item): number | null => {
    if (!item.due_date) return null;
    const { startDate, endDate } = timelineRange();
    const at = new Date(item.due_date).getTime();
    if (at < startDate.getTime() || at > endDate.getTime()) return null;
    return ((at - startDate.getTime()) / (endDate.getTime() - startDate.getTime())) * 100;
  };

  // ── Style helpers ──────────────────────────────────────────────────────

  const getPriorityColor = (p: string) => priorityFill(p as Priority);

  // Done items are marked with a checkmark + strikethrough, not dimmed: the
  // bar carries `--color-text-inverse` text over a solid priority-color
  // fill, and CSS `opacity` fades an element's whole rendered subtree
  // (fill *and* text) toward whatever's behind it — collapsing the
  // fill/text contrast gap well under WCAG AA at any status opacity below
  // ~1 (verified down to ~2:1 at the old 0.45 "done" value, across all
  // priorities and all six palette×mode combos).
  const isDone = (status: string) =>
    project()?.workflow?.statuses?.find(s => s.name === status)?.category === 'done';

  // ── Render ─────────────────────────────────────────────────────────────

  const barShadow = (id: string, dragging: boolean) => {
    const rings: string[] = [];
    if (isBlocked(id)) rings.push('0 0 0 3px var(--color-danger-600)', '0 0 0 5px var(--color-bg-panel)');
    if (dragging) rings.push('var(--shadow-lg)', '0 0 0 3px var(--color-accent-line)');
    return rings.length ? rings.join(', ') : 'none';
  };

  return (
    <div
      class="flex flex-col gap-3"
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
    >
      <div>
        <h1 class="m-0 text-content" style={{ 'font-size': '2.125rem' }}>Timeline</h1>
        <p class="mt-0.5 text-small text-content-muted">
          Drag bars to shift dates. Drag the left/right edge to resize start or due date.
        </p>
      </div>
      <Show when={blockedResource.error}>
        {(error) => <LoadError what="which items are blocked" error={error()} onRetry={() => void refetchBlocked()} />}
      </Show>
      <Show when={!(items as any).loading && (items() ?? []).length === 0}>
        <Card>
          <EmptyState
            icon={<IconTimeline size={28} />}
            title="No items to display on the timeline"
            description="Add items in Board or List — they'll appear here once created."
            action={<Button variant="secondary" size="sm" onClick={() => navigate(`/projects/${projectId}/board`)}>Go to Board</Button>}
          />
        </Card>
      </Show>

      <Show when={(items() ?? []).length > 0}>
        {/* Controls */}
        <div class="flex flex-wrap items-center gap-2.5">
          <Button variant="secondary" onClick={previousPeriod}>← Previous</Button>
          <Button variant="secondary" onClick={goToday}>Today</Button>
          <Button variant="secondary" onClick={nextPeriod}>Next →</Button>
          <div class="ml-auto flex gap-[3px] p-[3px] rounded-full bg-panel text-small">
            <For each={(['week', 'month', 'quarter'] as const)}>
              {(mode) => (
                <button
                  onClick={() => setViewMode(mode)}
                  class="px-3.5 py-[5px] rounded-full transition-colors"
                  style={viewMode() === mode
                    ? { 'background-color': 'var(--color-primary-600)', color: 'var(--color-on-accent)', 'font-weight': 700 }
                    : { 'background-color': 'transparent', color: 'var(--color-text-secondary)' }}
                >
                  {mode}
                </button>
              )}
            </For>
          </div>
        </div>

        {/* Gantt chart */}
        <Card class="relative overflow-hidden">
          {/* Month gridlines + today line, spanning header and bars. Inset to
              the bars' content box so they line up with the bar positions. */}
          <div class="absolute inset-y-0 left-4 right-4 pointer-events-none" aria-hidden="true">
            <For each={monthMarkers()}>
              {(m) => (
                <div
                  class="absolute top-0 bottom-0 w-px"
                  style={{ left: `${m.leftPercent}%`, 'background-color': 'var(--color-border-light)' }}
                />
              )}
            </For>
            <div
              class="absolute top-0 bottom-0 rounded-sm"
              style={{ left: `${todayPosition()}%`, width: '3px', 'background-color': 'var(--color-primary-600)' }}
            />
          </div>

          {/* Month header */}
          <div class="relative h-10 border-b border-line">
            <div class="absolute inset-y-0 left-4 right-4">
              <For each={monthMarkers()}>
                {(m) => (
                  <span
                    class="absolute top-3 pl-2 text-xs font-bold whitespace-nowrap text-content"
                    style={{ left: `${m.leftPercent}%` }}
                  >
                    {m.label}
                  </span>
                )}
              </For>
            </div>
          </div>

          {/* Bars */}
          <div
            ref={containerRef}
            class="relative p-4 flex flex-col gap-2.5 min-h-64 select-none"
            style={{ cursor: dragState() ? (dragState()!.mode === 'move' ? 'grabbing' : 'ew-resize') : 'default' }}
          >
            <Show when={timelineItems().length === 0}>
              <div class="flex items-center justify-center h-48 text-sm text-content-subtle">
                No items visible in this date range — navigate forward or backward.
              </div>
            </Show>

            <For each={timelineItems()}>
              {(item) => {
                const preview = () => previews().get(item.id);
                const pos = () => positionItem(item, preview());
                const dragging = () => dragState()?.itemId === item.id;

                return (
                  <div class="relative h-11 group" data-depth={item.depth}>
                    <Show when={item.depth > 0}>
                      {/* A guide in the margin: this row's bar belongs to the group above. */}
                      <div
                        aria-hidden="true"
                        class="absolute inset-y-0 w-0.5 rounded-full"
                        style={{ left: '-10px', 'background-color': 'var(--color-border-light)' }}
                      />
                    </Show>
                    <Show when={item.node === 'group'}>
                      <GroupBar
                        item={item}
                        left={pos().leftPercent}
                        width={pos().widthPercent}
                        folded={folded().has(item.id)}
                        onToggle={() => toggleFold(item.id)}
                        onOpen={() => setSearchParams({ item: item.id })}
                        dueLeft={dueLeft(item)}
                      />
                    </Show>
                    <Show when={item.node === 'work'}>
                    <div
                      class="absolute top-1 bottom-1 rounded-full transition-shadow"
                      style={{
                        left:    `${pos().leftPercent}%`,
                        width:   `${pos().widthPercent}%`,
                        'background-color': getPriorityColor(item.priority),
                        cursor:  dragState()
                          ? (dragState()!.mode === 'move' ? 'grabbing' : 'ew-resize')
                          : 'grab',
                        'box-shadow': barShadow(item.id, dragging()),
                        'z-index': dragging() ? 1 : undefined,
                      }}
                      onPointerDown={(e) => handleBarPointerDown(e, item, 'move')}
                      onClick={() => !dragState() && setSearchParams({ item: item.id })}
                      role="button"
                      tabindex={0}
                      aria-label={item.title}
                      onKeyDown={onActivateKey(() => setSearchParams({ item: item.id }))}
                      title={`${item.title}\n${pos().start.toLocaleDateString()} → ${pos().end.toLocaleDateString()}\n${item.status} · ${item.priority}${isBlocked(item.id) ? '\n⛔ Blocked' : ''}\n\nDrag to move · Drag edges to resize`}
                    >
                      {/* Left resize handle */}
                      <div
                        class="absolute left-0 top-0 bottom-0 w-2.5 rounded-l-full cursor-ew-resize hover:bg-black/20 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity"
                        onPointerDown={(e) => { e.stopPropagation(); handleBarPointerDown(e, item, 'resize-start'); }}
                      >
                        <div class="w-0.5 h-3 bg-white/70 rounded-full" />
                      </div>

                      {/* Label. Always full-opacity `--color-text-inverse` on the solid
                          priority fill — never dimmed (see isDone() above for why). Done
                          items get a checkmark + strikethrough instead of a fade. */}
                      <div
                        class="px-3.5 h-full flex items-center gap-1 pointer-events-none overflow-hidden"
                        style={{ color: 'var(--color-text-inverse)' }}
                      >
                        <Show when={isBlocked(item.id)}>
                          <span class="text-xs shrink-0" aria-label="blocked">⛔</span>
                        </Show>
                        <Show when={isDone(item.status)}>
                          <span class="text-xs shrink-0" aria-label="done">✓</span>
                        </Show>
                        <span
                          class="text-caption font-bold truncate"
                          style={{ 'text-decoration': isDone(item.status) ? 'line-through' : 'none' }}
                        >
                          {item.title}
                        </span>
                      </div>

                      {/* Right resize handle */}
                      <div
                        class="absolute right-0 top-0 bottom-0 w-2.5 rounded-r-full cursor-ew-resize hover:bg-black/20 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity"
                        onPointerDown={(e) => { e.stopPropagation(); handleBarPointerDown(e, item, 'resize-end'); }}
                      >
                        <div class="w-0.5 h-3 bg-white/70 rounded-full" />
                      </div>
                    </div>
                    </Show>

                    {/* Date tooltip during drag */}
                    <Show when={dragging() && preview()}>
                      <div
                        class="absolute -top-6 font-mono text-micro font-bold px-2.5 py-[3px] rounded-full whitespace-nowrap z-20 pointer-events-none"
                        style={{
                          left: `${pos().leftPercent}%`,
                          'background-color': 'var(--color-text-primary)',
                          color: 'var(--color-bg-app)',
                        }}
                      >
                        {preview()!.newStart.toLocaleDateString()} → {preview()!.newEnd.toLocaleDateString()}
                      </div>
                    </Show>
                  </div>
                );
              }}
            </For>
          </div>
        </Card>

        {/* Legend */}
        <div class="flex flex-wrap gap-3.5 items-center text-xs text-content-muted">
          <For each={[
            { label: 'Critical', priority: 'critical' },
            { label: 'High',     priority: 'high' },
            { label: 'Medium',   priority: 'medium' },
            { label: 'Low',      priority: 'low' },
          ] as const}>
            {(p) => (
              <span class="inline-flex items-center gap-1.5">
                <span class="w-2 h-2 rounded-[3px]" style={{ 'background-color': priorityFill(p.priority) }} />
                {p.label}
              </span>
            )}
          </For>
          <span class="text-content-subtle">
            Done items show a ✓ and strikethrough. Blocked items have a red outline.
          </span>
          <Show when={hasGroups()}>
            <span class="text-content-subtle">
              A flat bar sums up the work under a group and cannot be moved; ◆ is the group's due date, and the bar turns red when its work ends after it.
            </span>
          </Show>
        </div>
      </Show>
    </div>
  );
}

// ── Summary bar ────────────────────────────────────────────────────────────

/**
 * A group's bar: from the earliest start of its work to the latest due date of its work. It sums
 * up that work, so it takes no drag and its cursor says so; move the work to move the bar. The
 * group's own due date is a diamond on the row, and the bar turns the danger colour when the work
 * ends after it.
 */
function GroupBar(props: {
  item: Item & { start: Date; end: Date };
  left: number;
  width: number;
  folded: boolean;
  onToggle: () => void;
  onOpen: () => void;
  dueLeft: number | null;
}) {
  const late = () => endsAfterDue(props.item);
  const progress = () => (props.item.progress && props.item.progress.total > 0 ? progressText(props.item.progress) : null);
  return (
    <>
      <div
        data-testid="summary-bar"
        data-late={late() ? 'true' : 'false'}
        class="absolute top-1 bottom-1 flex items-center gap-1 overflow-hidden rounded-[6px] px-1"
        style={{
          left: `${props.left}%`,
          width: `${props.width}%`,
          'background-color': late() ? 'var(--color-danger-600)' : 'var(--color-text-secondary)',
          color: 'var(--color-text-inverse)',
          cursor: 'not-allowed',
        }}
        title={[
          props.item.title,
          `${props.item.start.toLocaleDateString()} → ${props.item.end.toLocaleDateString()}`,
          progress(),
          late() ? `Its work ends after the day it is due (${new Date(props.item.due_date!).toLocaleDateString()})` : null,
          'Sums up its work: move the work to move this bar',
        ].filter(Boolean).join('\n')}
      >
        <button
          type="button"
          aria-expanded={!props.folded}
          aria-label={`${props.folded ? 'Unfold' : 'Fold'} ${props.item.title}`}
          onClick={props.onToggle}
          class="grid h-6 w-6 flex-none place-items-center rounded-full focus:outline-none focus-visible:ring-2"
          style={{ color: 'inherit', cursor: 'pointer', '--tw-ring-color': 'var(--color-focus-ring)' }}
        >
          <span aria-hidden="true" class="inline-flex" style={{ transform: props.folded ? 'rotate(-90deg)' : 'none' }}>
            <IconChevronDown size={14} />
          </span>
        </button>
        <TypeTile type={props.item.item_type} class="flex-none" />
        <button
          type="button"
          aria-label={props.item.title}
          onClick={props.onOpen}
          class="min-w-0 truncate bg-transparent p-0 text-left text-caption font-bold focus:outline-none focus-visible:underline"
          style={{ color: 'inherit', cursor: 'pointer' }}
        >
          {props.item.title}
        </button>
        <Show when={late()}>
          <span class="flex-none text-xs" aria-label="ends after its due date">⚠</span>
        </Show>
      </div>
      <Show when={props.dueLeft != null}>
        <span
          data-testid="due-milestone"
          role="img"
          aria-label={`Due ${new Date(props.item.due_date!).toLocaleDateString()}`}
          title={`${props.item.title} is due ${new Date(props.item.due_date!).toLocaleDateString()}`}
          class="absolute"
          style={{
            left: `${props.dueLeft}%`,
            top: '50%',
            width: '12px',
            height: '12px',
            transform: 'translate(-50%, -50%) rotate(45deg)',
            'background-color': late() ? 'var(--color-danger-600)' : 'var(--color-text-primary)',
            border: '2px solid var(--color-bg-panel)',
            cursor: 'not-allowed',
            'z-index': 1,
          }}
        />
      </Show>
    </>
  );
}
