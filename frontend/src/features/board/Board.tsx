import { createMemo, createUniqueId, For, Show, createSignal, createEffect, onMount, onCleanup, type Component } from 'solid-js';
import MergeNoteBadge from '../../shared/runWithAgent/MergeNoteBadge';
import AutomationBadge from '../../shared/ui/AutomationBadge';
import { useParams, useSearchParams } from '@solidjs/router';
import { childrenText, type ExecutionSummary } from '../../shared/execution';
import { useExecutionStore } from '../../shared/state/executionContext';
import { api } from '../../shared/api';
import { deriveBoard } from '../../shared/api/boards';
import type { BoardColumn, Item, BoardState, Priority } from '../../shared/types';
import CreateItemModal from './CreateItemModal';
import { createBoardSocket, type BoardSocket } from '../../shared/realtime/boardSocket';
import type { SocketStatus } from '../../shared/realtime/socket';
import { useKeyboard, keyboardManager, type ShortcutContext } from '../../shared/keyboard/keyboard';
import { withOptimisticUpdate } from '../../shared/state/optimistic';
import { useProject } from '../../shared/state/projectContext';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import { useVocab } from '../../shared/vocab/useVocab';
import { ITEM_UPDATED_EVENT } from '../../shared/state/itemEvents';
import { useAutoStart, type WaitingReason } from '../../shared/state/autoStart';
import EmptyProjectGuide from '../../shared/ui/EmptyProjectGuide';
import { statusDotColor } from '../../shared/ui/statusDot';
import { Avatar, AvatarStack, teamInitials, Badge, RunState, TypeBadge, PriorityDot, WipChip, Skeleton } from '../../shared/ui';
import { IconPlus } from '../../shared/ui/icons';
import { estimateUnitSuffix } from '../../shared/estimateUnit';
import RunWithAgentButton from '../../shared/runWithAgent/RunWithAgentButton';
import { ForById } from '../../shared/ui/ForById';
import FirstRunBanner from '../../shared/agents/FirstRunBanner';
import MoveMenu from './MoveMenu';
import AgentStrip, { RunMiniBar } from './AgentStrip';
import {
  cardRun, countRuns, getAlwaysPriority, orderByWork, passesFilters, showsPriority,
  type CardRun, type RunFilter,
} from '../../shared/runWithAgent/cardRun';

/** Short, human id for the card header (real ids are UUIDs). */
function shortId(id: string): string {
  return id.replace(/-/g, '').slice(0, 6).toUpperCase();
}

export const ItemCard: Component<{
  item: Item;
  /** Every column's status, for the "Move to…" menu. */
  statuses: string[];
  onEdit: (item: Item) => void;
  onMove: (itemId: string, status: string) => void;
  /** The item's newest run, if any. */
  run?: CardRun;
  /** The card sits in a column no work has started in: a live run there still marks the card. */
  notStarted?: boolean;
  /** Show the priority even when it is the project's default. */
  alwaysPriority?: boolean;
  /** Why auto-start has not started this task, when it would have. */
  waiting?: WaitingReason | null;
  /** The title of the item this one is part of; shown on a card auto-start holds or started. */
  parentTitle?: string;
  /** The team's initials among the teams on this board. */
}> = (props) => {
  const [isDragging, setIsDragging] = createSignal(false);
  const [isHovered, setIsHovered] = createSignal(false);
  const [, setSearchParams] = useSearchParams();
  const store = useExecutionStore();

  // The card holds its item's run state in the shared store whether or not it
  // offers a run: the button that used to register the item is hidden on a live run.
  onMount(() => onCleanup(store.watchItem(props.item.id)));

  const handleDragStart = (e: DragEvent) => {
    setIsDragging(true);
    e.dataTransfer!.effectAllowed = 'move';
    e.dataTransfer!.setData('text/plain', props.item.id);
  };
  const handleDragEnd = () => setIsDragging(false);
  const handleClick = () => { if (!isDragging()) props.onEdit(props.item); };

  const estimate = () => {
    const e = props.item.estimate;
    if (e == null) return null;
    const suffix = estimateUnitSuffix(props.item.estimate_unit);
    return suffix ? `${e} ${suffix}` : `${e}`;
  };

  const raised = () => isHovered() && !isDragging();

  // A run that needs a person or failed marks the card; so does a live run in a
  // column nothing has started in, so a board of running work does not read as idle.
  const rail = () => {
    const run = props.run;
    if (!run) return undefined;
    return run.state === 'needs' || run.state === 'failed' || (run.live && props.notStarted) ? run.state : undefined;
  };

  // Clicking the run state opens the Execution tab, or the run's question when it asks one.
  const openExecutionTab = (e: MouseEvent) => {
    e.stopPropagation();
    setSearchParams({ item: props.item.id, tab: 'execution', run: props.run?.asking ? props.run.requestId : undefined });
  };

  return (
    <div
      draggable={true}
      data-item-id={props.item.id}
      class="group"
      title={`Item ${shortId(props.item.id)}`}
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
      onClick={handleClick}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      style={{
        position: 'relative',
        background: 'var(--color-bg-app)',
        border: raised() ? '2px solid var(--color-primary-600)' : '2px solid transparent',
        'border-radius': 'var(--radius-item)',
        padding: '12px 13px',
        display: 'flex',
        'flex-direction': 'column',
        gap: '7px',
        cursor: 'pointer',
        'box-shadow': raised() ? 'var(--shadow-md)' : 'var(--shadow-sm)',
        transform: raised() ? 'translateY(-1px)' : 'none',
        transition: 'border-color .12s, box-shadow .12s, transform .12s, opacity .12s',
        opacity: isDragging() ? 0.4 : 1,
      }}
    >
      <Show when={rail()}>
        {(state) => (
          <span
            aria-hidden="true"
            data-testid="card-rail"
            data-state={state()}
            style={{
              position: 'absolute', left: 0, top: '10px', bottom: '10px', width: '3px',
              'border-radius': '0 3px 3px 0', background: `var(--run-${state()}-ink)`,
            }}
          />
        )}
      </Show>
      <div style={{ display: 'flex', 'align-items': 'flex-start', gap: '7px' }}>
        <h4 style={{
          flex: 1, 'min-width': 0,
          'font-family': 'var(--font-body)', 'font-size': '0.9375rem', 'font-weight': 700, 'font-synthesis': 'auto',
          'letter-spacing': 'normal', margin: 0, 'line-height': 1.3, 'text-wrap': 'pretty', color: 'var(--color-text-primary)',
        }}>
          {/* The keyboard's way in: Enter or Space clicks this button, and the
              click bubbles to the card's own handler. A real button rather than
              role="button" on the card, which holds the agent button. */}
          <button
            type="button"
            style={{
              background: 'none', border: 0, padding: 0, margin: 0, font: 'inherit', color: 'inherit',
              'text-align': 'left', cursor: 'pointer', 'border-radius': '6px', 'overflow-wrap': 'anywhere',
            }}
          >
            {props.item.title}
          </button>
        </h4>
        <div style={{ display: 'flex', 'align-items': 'center', gap: '5px', 'flex-shrink': 0 }}>
          <MoveMenu
            statuses={props.statuses.filter((status) => status !== props.item.status)}
            onMove={(status) => props.onMove(props.item.id, status)}
            itemRef={shortId(props.item.id)}
          />
          {/* The action follows the state: a run waiting for review is reviewed, not started again;
              a live run has its own link in the meta line. */}
          <Show when={props.run?.state === 'review'}>
            <button
              type="button"
              data-testid="card-review"
              aria-label={`Review the run: ${props.item.title}`}
              title="Review and merge"
              onClick={openExecutionTab}
              class="focus:outline-none focus-visible:ring-2"
              style={{
                width: '24px', height: '24px', 'border-radius': '50%', border: 'none', padding: 0, 'flex-shrink': 0,
                display: 'grid', 'place-items': 'center', cursor: 'pointer',
                background: 'var(--run-review-soft)', color: 'var(--run-review-ink)',
              }}
            >
              <RunState state="review" size="dot" />
            </button>
          </Show>
          <Show when={!props.run?.live && props.run?.state !== 'review'}>
            <RunWithAgentButton
              itemId={props.item.id}
              itemTitle={props.item.title}
              projectId={props.item.project_id}
              compact
              testId="card-run"
            />
          </Show>
        </div>
      </div>
      <Show when={props.waiting}>
        {(why) => (
          <p
            data-testid="card-waiting"
            title={`Waiting to start: ${why().text}`}
            style={{
              margin: 0, 'font-size': '0.75rem', 'line-height': 1.3, color: 'var(--color-text-secondary)',
              'overflow-wrap': 'anywhere',
            }}
          >
            Waiting to start: {why().text}
          </p>
        )}
      </Show>
      <Show when={props.parentTitle && (props.waiting || props.item.started_by === 'auto_start') ? props.parentTitle : undefined}>
        {(parent) => (
          <p
            data-testid="card-parent"
            title={`Part of: ${parent()}`}
            style={{ margin: 0, 'font-size': '0.75rem', 'line-height': 1.3, color: 'var(--color-text-secondary)', 'overflow-wrap': 'anywhere' }}
          >
            Part of: {parent()}
          </p>
        )}
      </Show>
      <Show when={childrenText(props.item.progress)}>
        {(text) => (
          <p
            data-testid="card-children"
            style={{ margin: 0, 'font-size': '0.75rem', 'line-height': 1.3, color: 'var(--color-text-secondary)', 'overflow-wrap': 'anywhere' }}
          >
            {text()}
          </p>
        )}
      </Show>
      <div
        data-testid="card-meta"
        style={{ display: 'flex', 'flex-wrap': 'wrap', 'align-items': 'center', gap: '6px 8px', 'font-size': '0.75rem' }}
      >
        <Show when={props.run}>
          {(run) => (
            <button
              type="button"
              title={run().asking ? 'Open the question' : 'Open the Execution tab'}
              onClick={openExecutionTab}
              class="rounded-pill focus:outline-none focus-visible:ring-2"
              style={{ background: 'none', border: 'none', padding: 0, cursor: 'pointer' }}
            >
              <RunState state={run().state} docket={run().docket} note={run().note} />
              <span class="sr-only">{` — open the Execution tab for ${props.item.title}`}</span>
            </button>
          )}
        </Show>
        <Show when={props.item.team}>
          {(team) => (
            <span
              data-testid="badge-team"
              title={`Team: ${team()}`}
              style={{ display: 'inline-flex', 'align-items': 'center', gap: '5px', 'min-width': 0, color: 'var(--color-text-secondary)' }}
            >
              <span aria-hidden="true" style={{ display: 'inline-flex' }}><Avatar name={team()} initials={teamInitials(team())} size="sm" /></span>
              <span style={{ 'overflow-wrap': 'anywhere' }}>{team()}</span>
            </span>
          )}
        </Show>
        <Show when={showsPriority(props.item.priority, props.alwaysPriority === true)}>
          <PriorityDot priority={props.item.priority as Priority} showLabel />
        </Show>
        <TypeBadge type={props.item.item_type} />
      </div>
      <Show when={props.item.recurrence}>
        <div><Badge>Repeats</Badge></div>
      </Show>
      <MergeNoteBadge note={props.item.merge_note} />
      <AutomationBadge startedBy={props.item.started_by} mergedBy={props.item.merged_by} />
      <Show when={estimate() || props.item.assignee}>
        <div style={{ display: 'flex', 'align-items': 'center', gap: '6px', 'font-size': '0.75rem' }}>
          <div style={{ flex: 1 }} />
          <Show when={estimate()}>
            <span style={{
              'font-family': 'var(--font-mono)', 'font-size': '0.65625rem', 'font-weight': 500, color: 'var(--color-text-secondary)',
              background: 'var(--color-bg-panel)', padding: '2px 7px', 'border-radius': 'var(--radius-pill)',
            }}>
              {estimate()}
            </span>
          </Show>
          <Show when={props.item.assignee}>
            <Avatar name={props.item.assignee!} size="sm" />
          </Show>
        </div>
      </Show>
    </div>
  );
};

const BoardColumnView: Component<{
  column: BoardColumn;
  statuses: string[];
  dotColor: string;
  onItemDrop: (itemId: string, newStatus: string) => void;
  onMoveItem: (itemId: string, newStatus: string) => void;
  onAddItem: (status: string) => void;
  onEditItem: (item: Item) => void;
  /** Whether work has started in this column: false for a "to do" status. */
  started: boolean;
  runOf: (item: Item) => CardRun | undefined;
  waitingOf: (item: Item) => WaitingReason | null;
  parentTitleOf: (item: Item) => string | undefined;
  filters: ReadonlySet<RunFilter>;
  onToggleFilter: (filter: RunFilter) => void;
  alwaysPriority: boolean;
}> = (props) => {
  const [isDragOver, setIsDragOver] = createSignal(false);
  const headingId = createUniqueId();

  const runCounts = createMemo(() => countRuns(props.column.items.map(props.runOf)));
  // A filter narrows what the column shows, never what it holds: the count and
  // the WIP chip keep speaking of every card in it. Inside the column the cards a person has to
  // act on come first, then the ones going, then the rest, each group in the column's own order.
  const shown = createMemo(() =>
    orderByWork(props.column.items.filter((item) => passesFilters(props.runOf(item), props.filters)), props.runOf),
  );

  const handleDragOver = (e: DragEvent) => {
    e.preventDefault();
    e.dataTransfer!.dropEffect = 'move';
    setIsDragOver(true);
  };
  const handleDragLeave = () => setIsDragOver(false);
  const handleDrop = (e: DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
    const itemId = e.dataTransfer!.getData('text/plain');
    if (itemId) props.onItemDrop(itemId, props.column.status);
  };

  // A column read down to its foot stays at its foot when its box shrinks (a banner
  // arriving above the board, a smaller window). The scroll offset does not move on its
  // own, so the last card would otherwise slip under the column's edge.
  let scroller: HTMLDivElement | undefined;
  onMount(() => {
    if (typeof ResizeObserver === 'undefined' || !scroller) return;
    const el = scroller;
    let lastHeight = el.clientHeight;
    const observer = new ResizeObserver(() => {
      const overflowed = el.scrollHeight > lastHeight + 2;
      const wasAtFoot = overflowed && el.scrollHeight - el.scrollTop - lastHeight <= 2;
      lastHeight = el.clientHeight;
      if (wasAtFoot) el.scrollTop = el.scrollHeight;
    });
    observer.observe(el);
    onCleanup(() => observer.disconnect());
  });

  return (
    <section
      role="region"
      aria-labelledby={headingId}
      style={{
        width: '282px', 'flex-shrink': 0, display: 'flex', 'flex-direction': 'column', gap: '6px',
        'max-height': '100%', background: 'var(--color-bg-panel)',
        'border-radius': '26px', padding: '12px 10px',
        // The scrolling body below sits on the panel, so its track is the panel.
        '--scrollbar-track': 'var(--color-bg-panel)',
      }}
    >
      <div style={{ display: 'flex', 'align-items': 'center', gap: '8px', padding: '2px 8px 4px' }}>
        <span style={{ width: '8px', height: '8px', 'border-radius': '50%', 'flex-shrink': 0, background: props.dotColor }} />
        <h3 id={headingId} title={props.column.status} style={{
          'min-width': 0, overflow: 'hidden', 'text-overflow': 'ellipsis', 'white-space': 'nowrap',
          'font-family': 'var(--font-body)', 'font-size': '0.875rem', 'font-weight': 700, 'font-synthesis': 'auto',
          'letter-spacing': 'normal', margin: 0, color: 'var(--color-text-primary)',
        }}>
          {props.column.status}
        </h3>
        <span style={{ 'font-size': '0.75rem', 'font-weight': 500, 'flex-shrink': 0, color: 'var(--color-text-tertiary)' }}>
          {shown().length === props.column.items.length ? props.column.items.length : `${shown().length} of ${props.column.items.length}`}
        </span>
        <Show when={props.column.wip_limit != null}>
          <WipChip count={props.column.items.length} limit={props.column.wip_limit!} />
        </Show>
        <button
          onClick={() => props.onAddItem(props.column.status)}
          title="Add item"
          aria-label={`Add item to ${props.column.status}`}
          class="focus:outline-none focus-visible:ring-2"
          style={{
            'margin-left': 'auto', 'min-width': '24px', 'min-height': '24px', 'border-radius': 'var(--radius-pill)', border: 'none',
            background: 'var(--color-bg-app)', cursor: 'pointer', color: 'var(--color-text-secondary)',
            display: 'inline-flex', 'align-items': 'center', gap: '4px', padding: '0 9px 0 7px', 'flex-shrink': 0,
            'font-size': '0.75rem', 'font-weight': 600, 'white-space': 'nowrap',
            transition: 'color .12s, box-shadow .12s',
          }}
          onMouseEnter={(e) => { e.currentTarget.style.color = 'var(--color-primary-600)'; e.currentTarget.style.boxShadow = 'var(--shadow-sm)'; }}
          onMouseLeave={(e) => { e.currentTarget.style.color = 'var(--color-text-secondary)'; e.currentTarget.style.boxShadow = 'none'; }}
        >
          <IconPlus size={13} />
          Add item
        </button>
      </div>
      {/* The runs of the column sit under its name as a thin line, so the name keeps its width. */}
      <div style={{ padding: '0 8px 2px' }}>
        <RunMiniBar counts={runCounts()} active={props.filters} onToggle={props.onToggleFilter} />
      </div>
      {/* The column's own scroll: focusable, so the keyboard can scroll it, and
          it keeps the wheel to itself instead of passing it to the board. */}
      <div
        ref={scroller}
        role="group"
        aria-labelledby={headingId}
        tabindex={0}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
        style={{
          flex: 1, 'min-height': 0, 'overflow-y': 'auto', 'overscroll-behavior': 'contain', 'scrollbar-gutter': 'stable',
          display: 'flex', 'flex-direction': 'column', gap: '8px',
          padding: '2px 2px 4px',
          'border-radius': 'var(--radius-item)',
          outline: isDragOver() ? '2px dashed var(--color-primary-600)' : 'none',
          'outline-offset': '-2px',
          background: isDragOver() ? 'var(--color-accent-soft)' : 'transparent',
          transition: 'background-color .12s',
        }}
      >
        <ForById each={shown()}>
          {(item) => (
            <ItemCard
              item={item()}
              statuses={props.statuses}
              onEdit={props.onEditItem}
              onMove={props.onMoveItem}
              run={props.runOf(item())}
              waiting={props.waitingOf(item())}
              parentTitle={props.parentTitleOf(item())}
              notStarted={!props.started}
              alwaysPriority={props.alwaysPriority}
            />
          )}
        </ForById>
        <Show when={props.column.items.length > 0 && shown().length === 0}>
          <div style={{
            padding: '12px', 'text-align': 'center', 'font-size': '0.75rem', color: 'var(--color-text-tertiary)',
          }}>
            No cards here match the run filter
          </div>
        </Show>
        <Show when={props.column.items.length === 0}>
          <div style={{
            'min-height': '70px', display: 'grid', 'place-items': 'center',
            border: '2px dashed var(--color-border-light)', 'border-radius': 'var(--radius-item)',
            padding: '12px', 'text-align': 'center', 'font-size': '0.75rem', color: 'var(--color-text-tertiary)',
          }}>
            Drop items here
          </div>
        </Show>
      </div>
    </section>
  );
};

/** Loading placeholder in the loaded board's shape: panel columns holding
 *  rounded card blocks. */
const BoardLoadingSkeleton: Component = () => {
  const columns = [[80, 80, 64], [80, 64], [80, 80, 80], [64]];
  return (
    <div style={{ display: 'flex', gap: '14px', 'align-items': 'flex-start' }} aria-hidden="true">
      <For each={columns}>
        {(cards) => (
          <div style={{
            width: '282px', 'flex-shrink': 0, background: 'var(--color-bg-panel)', 'border-radius': '26px',
            padding: '12px', display: 'flex', 'flex-direction': 'column', gap: '8px',
          }}>
            <Skeleton width="60%" height="14px" rounded />
            <For each={cards}>
              {(h) => (
                <div
                  class="animate-pulse"
                  style={{ height: `${h}px`, 'border-radius': '18px', background: 'var(--color-bg-app)', opacity: 0.7 }}
                />
              )}
            </For>
          </div>
        )}
      </For>
    </div>
  );
};

const Board: Component = () => {
  const params = useParams();
  const projectId = () => params.id;
  const { vocabulary, project } = useProject();
  const vocab = useVocab();
  const { items, loading, refetch } = useProjectItems();
  const [, setSearchParams] = useSearchParams();
  const executions = useExecutionStore();

  // Each item's newest run, read once from the store the cards already watch.
  const newestRuns = createMemo(() => {
    const newest = new Map<string, ExecutionSummary>();
    for (const record of executions.requests().values()) {
      const summary = record.summary;
      if (!summary) continue;
      const prior = newest.get(summary.item_id);
      if (!prior || summary.created_at > prior.created_at) newest.set(summary.item_id, summary);
    }
    return newest;
  });
  const runOf = (item: Item): CardRun | undefined => cardRun(newestRuns().get(item.id), item.needs_review === true);
  const autoStart = useAutoStart(projectId);
  const waitingOf = (item: Item): WaitingReason | null => autoStart.waiting(item.id);
  const titles = createMemo(() => new Map((items() ?? []).map((i) => [i.id, i.title] as const)));
  const parentTitleOf = (item: Item): string | undefined => (item.parent_id ? titles().get(item.parent_id) : undefined);

  const [runFilters, setRunFilters] = createSignal<ReadonlySet<RunFilter>>(new Set());
  const toggleRunFilter = (filter: RunFilter) =>
    setRunFilters((prior) => {
      const next = new Set(prior);
      if (!next.delete(filter)) next.add(filter);
      return next;
    });
  // Chosen in Settings → Display & accessibility, remembered in this browser.
  const alwaysPriority = () => getAlwaysPriority();

  const boardStateFromServer = createMemo((): BoardState | null => {
    const proj = project();
    const its = items();
    if (!proj || !its) return null;
    return deriveBoard(proj, its);
  });

  // status name → category, for the column dot color.
  const categoryOf = (status: string): 'todo' | 'in_progress' | 'done' =>
    project()?.workflow.statuses.find((s) => s.name === status)?.category ?? 'todo';
  const dotColor = (status: string): string => statusDotColor(categoryOf(status));

  const [showCreateModal, setShowCreateModal] = createSignal(false);
  const [selectedColumn, setSelectedColumn] = createSignal<string | null>(null);
  const [activeContext, setActiveContext] = createSignal<ShortcutContext>('board');
  const [editingItem, setEditingItem] = createSignal<Item | null>(null);
  const [modalMode, setModalMode] = createSignal<'create' | 'edit'>('create');
  const [optimisticBoardState, setOptimisticBoardState] = createSignal<BoardState | null>(null);

  const currentBoard = (): BoardState | null | undefined => optimisticBoardState() || boardStateFromServer();
  // Columns keyed by status, as ForById keys cards by id: a refetch builds new
  // column objects, and a column keyed by object would rebuild every card.
  const columnsByStatus = createMemo(() =>
    (currentBoard()?.columns ?? []).map((column) => ({ id: column.status, column })),
  );

  const statusNames = createMemo(() => columnsByStatus().map((entry) => entry.id));

  // Unique assignees across the board → toolbar avatar stack.
  const assignees = createMemo(() => {
    const set = new Set<string>();
    for (const col of currentBoard()?.columns ?? []) {
      for (const it of col.items) if (it.assignee) set.add(it.assignee);
    }
    return [...set];
  });
  const itemCount = () => (currentBoard()?.columns ?? []).reduce((n, c) => n + c.items.length, 0);
  const boardRunCounts = createMemo(() =>
    countRuns((currentBoard()?.columns ?? []).flatMap((column) => column.items.map(runOf))),
  );

  // Real-time board updates via the reconnecting socket.
  const [sock, setSock] = createSignal<BoardSocket>();
  const socketStatus = (): SocketStatus => sock()?.status() ?? 'closed';

  createEffect(() => {
    const pid = projectId();
    if (!pid) { setSock(undefined); return; }
    const s = createBoardSocket(pid);
    const off = s.onEvent(() => {
      void refetch();
    });
    setSock(s);
    onCleanup(() => { off(); s.close(); });
  });

  // Resync when the connection comes back after dropping.
  let wasReconnecting = false;
  createEffect(() => {
    const st = socketStatus();
    if (st === 'reconnecting') wasReconnecting = true;
    else if (st === 'open' && wasReconnecting) {
      wasReconnecting = false;
      void refetch();
    }
  });

  useKeyboard(activeContext);
  createEffect(() => setActiveContext(showCreateModal() ? 'modal' : 'board'));

  createEffect(() => {
    keyboardManager.register('board', {
      key: 'n',
      description: 'Create new item',
      action: () => {
        const firstColumn = currentBoard()?.columns[0];
        if (firstColumn) handleAddItem(firstColumn.status);
      },
    });
    keyboardManager.register('board', { key: 'r', description: 'Refresh board', action: () => refetch() });
    keyboardManager.register('modal', { key: 'Escape', description: 'Close modal', action: () => setShowCreateModal(false) });
  });

  const handleItemDrop = async (itemId: string, newStatus: string) => {
    const realBoard = boardStateFromServer();
    if (!realBoard) return;
    const item = realBoard.columns.flatMap((col) => col.items).find((i) => i.id === itemId);
    if (!item || item.status === newStatus) return;

    const optimisticBoard = {
      ...realBoard,
      columns: realBoard.columns.map((col) => ({
        ...col,
        items: col.items
          .filter((i) => i.id !== itemId)
          .concat(col.status === newStatus ? [{ ...item, status: newStatus }] : []),
      })),
    };

    await withOptimisticUpdate(
      () => api.items.update(itemId, { status: newStatus }),
      () => setOptimisticBoardState(optimisticBoard),
      async () => { setOptimisticBoardState(null); await refetch(); },
      {
        showSuccessToast: true,
        successMessage: `Item moved to ${newStatus}`,
        showErrorToast: true,
        errorMessage: 'Failed to move item',
      }
    );

    setOptimisticBoardState(null);
    await refetch();
  };

  // The menu's move: the same action as a drop, then the keyboard lands on the
  // card in its new column, which is a new element, so focus would be lost.
  const handleMoveItem = async (itemId: string, newStatus: string) => {
    await handleItemDrop(itemId, newStatus);
    document.querySelector<HTMLElement>(`[data-item-id="${itemId}"] [aria-label="Move to…"]`)?.focus();
  };

  const handleAddItem = (status: string) => {
    setModalMode('create');
    setEditingItem(null);
    setSelectedColumn(status);
    setShowCreateModal(true);
  };

  const handleEditItem = (item: Item) => setSearchParams({ item: item.id });

  onMount(() => {
    const onItemUpdated = () => void refetch();
    window.addEventListener(ITEM_UPDATED_EVENT, onItemUpdated);
    onCleanup(() => window.removeEventListener(ITEM_UPDATED_EVENT, onItemUpdated));
  });

  const liveLabel = () =>
    socketStatus() === 'open' ? 'Live'
      : socketStatus() === 'connecting' || socketStatus() === 'reconnecting' ? 'Connecting…'
        : 'Offline';
  // Realtime pill: soft fill + ink text + dot, per connection state.
  const livePill = (): { bg: string; fg: string; dot: string } =>
    socketStatus() === 'open'
      ? { bg: 'var(--color-success-100)', fg: 'var(--color-success-600)', dot: 'var(--color-success-600)' }
      : socketStatus() === 'closed'
        ? { bg: 'var(--color-bg-subtle)', fg: 'var(--color-text-secondary)', dot: 'var(--color-text-tertiary)' }
        : { bg: 'var(--color-warning-100)', fg: 'var(--color-warning-600)', dot: 'var(--color-warning-600)' };

  return (
    <div style={{ flex: 1, display: 'flex', 'flex-direction': 'column', 'min-width': 0, 'min-height': 0 }}>
      {/* board toolbar */}
      <div style={{ 'flex-shrink': 0, padding: '16px 22px 10px', display: 'flex', 'align-items': 'center', gap: '12px' }}>
        <h1 style={{ 'font-size': '2.125rem', 'line-height': 1.1, margin: 0, color: 'var(--color-text-primary)' }}>
          {vocab.t('board')}
        </h1>
        <Show when={sock()}>
          <span style={{
            display: 'inline-flex', 'align-items': 'center', gap: '6px', 'font-size': '0.75rem', 'font-weight': 600,
            color: livePill().fg, background: livePill().bg,
            padding: '3px 10px', 'border-radius': 'var(--radius-pill)', 'white-space': 'nowrap',
          }}>
            <span style={{
              width: '7px', height: '7px', 'border-radius': '50%', background: livePill().dot,
              'box-shadow': socketStatus() === 'open' ? '0 0 0 3px color-mix(in srgb, var(--color-success-600) 22%, transparent)' : 'none',
              animation: socketStatus() === 'open' ? 'tk-pulse 2s ease-in-out infinite' : 'none',
            }} />
            {liveLabel()}
          </span>
        </Show>
        <span style={{ display: 'inline-flex', gap: '4px', 'font-size': '0.8125rem', color: 'var(--color-text-secondary)' }}>
          <span style={{ color: 'var(--color-text-tertiary)' }}>·</span>
          <span>{itemCount()} {itemCount() === 1 ? 'item' : 'items'}</span>
        </span>
        <div style={{ flex: 1 }} />
        <Show when={assignees().length > 0}>
          <AvatarStack names={assignees()} max={5} />
        </Show>
      </div>

      <AgentStrip counts={boardRunCounts()} active={runFilters()} onToggle={toggleRunFilter} />

      <FirstRunBanner hasItems={itemCount() > 0} />

      <Show when={!loading()} fallback={<div style={{ padding: '6px 22px 22px', 'overflow-x': 'auto' }}><BoardLoadingSkeleton /></div>}>
        <Show
          when={projectId()}
          fallback={<div style={{ 'text-align': 'center', padding: '48px', color: 'var(--color-text-secondary)' }}>Please select a project from the Projects page</div>}
        >
          <Show when={(currentBoard()?.columns ?? []).length > 0 && (currentBoard()?.columns ?? []).every((col) => col.items.length === 0)}>
            <div style={{ padding: '10px 22px 0' }}>
              <EmptyProjectGuide onAddItem={() => handleAddItem(currentBoard()!.columns[0].status)} />
            </div>
          </Show>

          {/* The last link of the height chain (Layout → WorkLayout → here): this
              row takes what is left of the screen, so each column scrolls inside
              itself and the horizontal bar sits at the bottom of the screen. */}
          <div
            role="group"
            aria-label="Board columns"
            tabindex={0}
            style={{
              flex: 1, 'min-height': 0, display: 'flex', gap: '14px', 'align-items': 'flex-start',
              'overflow-x': 'auto', 'overflow-y': 'hidden', 'scrollbar-gutter': 'stable', padding: '6px 22px 22px',
            }}
          >
            <ForById each={columnsByStatus()}>
              {(entry) => (
                <BoardColumnView
                  column={entry().column}
                  statuses={statusNames()}
                  dotColor={dotColor(entry().id)}
                  onItemDrop={handleItemDrop}
                  onMoveItem={handleMoveItem}
                  onAddItem={handleAddItem}
                  onEditItem={handleEditItem}
                  started={categoryOf(entry().id) !== 'todo'}
                  runOf={runOf}
                  waitingOf={waitingOf}
                  parentTitleOf={parentTitleOf}
                  filters={runFilters()}
                  onToggleFilter={toggleRunFilter}
                  alwaysPriority={alwaysPriority()}
                />
              )}
            </ForById>
          </div>
        </Show>
      </Show>

      <Show when={projectId()}>
        <CreateItemModal
          isOpen={showCreateModal()}
          onClose={() => {
            setShowCreateModal(false);
            setSelectedColumn(null);
            setEditingItem(null);
            setModalMode('create');
          }}
          onSuccess={() => refetch()}
          projectId={projectId()!}
          vocabulary={vocabulary()}
          itemTypes={project()?.item_types}
          initialStatus={selectedColumn() || undefined}
          mode={modalMode()}
          existingItem={editingItem() || undefined}
        />
      </Show>
    </div>
  );
};

export default Board;
