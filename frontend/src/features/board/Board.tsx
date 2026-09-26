import { createMemo, For, Show, createSignal, createEffect, onMount, onCleanup, type Component } from 'solid-js';
import { useParams, useSearchParams } from '@solidjs/router';
import { api } from '../../shared/api';
import { deriveBoard } from '../../shared/api/boards';
import type { BoardColumn, Item, BoardState, Priority } from '../../shared/types';
import CreateItemModal from '../../shared/ui/CreateItemModal';
import { createBoardSocket, type BoardSocket, type SocketStatus } from '../../shared/realtime/boardSocket';
import { useKeyboard, keyboardManager, type ShortcutContext } from '../../shared/keyboard/keyboard';
import { withOptimisticUpdate } from '../../shared/state/optimistic';
import { useProject } from '../../shared/state/projectContext';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import { useVocab } from '../../shared/vocab/useVocab';
import { ITEM_UPDATED_EVENT } from '../../shared/state/itemEvents';
import EmptyProjectGuide from '../../shared/ui/EmptyProjectGuide';
import { Avatar, AvatarStack, TypeBadge, PriorityDot, WipChip, Skeleton, typeKey } from '../../shared/ui';
import { IconPlus } from '../../shared/ui/icons';
import { estimateUnitSuffix } from '../../shared/estimateUnit';
import RunWithAgentButton from '../../shared/runWithAgent/RunWithAgentButton';
import FirstRunBanner from '../../shared/agents/FirstRunBanner';

/** Short, human id for the card header (real ids are UUIDs). */
function shortId(id: string): string {
  return id.replace(/-/g, '').slice(0, 6).toUpperCase();
}

const ItemCard: Component<{
  item: Item;
  typeLabel: string;
  onEdit: (item: Item) => void;
}> = (props) => {
  const [isDragging, setIsDragging] = createSignal(false);
  const [isHovered, setIsHovered] = createSignal(false);

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

  return (
    <div
      draggable={true}
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
      onClick={handleClick}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
      style={{
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
      <div style={{ display: 'flex', 'align-items': 'center', gap: '7px' }}>
        <TypeBadge type={props.item.item_type} label={props.typeLabel} />
        <span style={{ 'font-family': 'var(--font-mono)', 'font-size': '10.5px', color: 'var(--color-text-tertiary)' }}>
          {shortId(props.item.id)}
        </span>
        <div style={{ 'margin-left': 'auto', display: 'flex', 'align-items': 'center', gap: '5px' }}>
          <RunWithAgentButton
            itemId={props.item.id}
            itemTitle={props.item.title}
            projectId={props.item.project_id}
            showStateChip
            compact
          />
        </div>
      </div>
      <h4 style={{
        'font-family': 'var(--font-body)', 'font-size': '14px', 'font-weight': 700, 'font-synthesis': 'auto',
        'letter-spacing': 'normal', margin: 0, 'line-height': 1.3, 'text-wrap': 'pretty', color: 'var(--color-text-primary)',
      }}>
        {props.item.title}
      </h4>
      <div style={{ display: 'flex', 'align-items': 'center', gap: '6px', 'font-size': '12px' }}>
        <Show when={props.item.priority !== 'none'}>
          <PriorityDot priority={props.item.priority as Priority} showLabel />
        </Show>
        <div style={{ flex: 1 }} />
        <Show when={estimate()}>
          <span style={{
            'font-family': 'var(--font-mono)', 'font-size': '10.5px', 'font-weight': 500, color: 'var(--color-text-secondary)',
            background: 'var(--color-bg-panel)', padding: '2px 7px', 'border-radius': 'var(--radius-pill)',
          }}>
            {estimate()}
          </span>
        </Show>
        <Show when={props.item.assignee}>
          <Avatar name={props.item.assignee!} size="sm" />
        </Show>
      </div>
    </div>
  );
};

const BoardColumnView: Component<{
  column: BoardColumn;
  dotColor: string;
  typeLabelOf: (item: Item) => string;
  onItemDrop: (itemId: string, newStatus: string) => void;
  onAddItem: (status: string) => void;
  onEditItem: (item: Item) => void;
}> = (props) => {
  const [isDragOver, setIsDragOver] = createSignal(false);

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

  return (
    <section style={{
      width: '282px', 'flex-shrink': 0, display: 'flex', 'flex-direction': 'column', gap: '6px',
      'max-height': '100%', background: 'var(--color-bg-panel)',
      'border-radius': '26px', padding: '12px 10px',
    }}>
      <div style={{ display: 'flex', 'align-items': 'center', gap: '8px', padding: '2px 8px 4px' }}>
        <span style={{ width: '8px', height: '8px', 'border-radius': '50%', 'flex-shrink': 0, background: props.dotColor }} />
        <h3 style={{
          'font-family': 'var(--font-body)', 'font-size': '14px', 'font-weight': 700, 'font-synthesis': 'auto',
          'letter-spacing': 'normal', margin: 0, color: 'var(--color-text-primary)',
        }}>
          {props.column.status}
        </h3>
        <span style={{ 'font-size': '12px', 'font-weight': 500, color: 'var(--color-text-tertiary)' }}>
          {props.column.items.length}
        </span>
        <Show when={props.column.wip_limit != null}>
          <WipChip count={props.column.items.length} limit={props.column.wip_limit!} />
        </Show>
        <button
          onClick={() => props.onAddItem(props.column.status)}
          title="Add item"
          class="focus:outline-none focus-visible:ring-2"
          style={{
            'margin-left': 'auto', width: '22px', height: '22px', 'border-radius': '50%', border: 'none',
            background: 'var(--color-bg-app)', cursor: 'pointer', color: 'var(--color-text-secondary)',
            display: 'grid', 'place-items': 'center', padding: 0, 'flex-shrink': 0,
            transition: 'color .12s, box-shadow .12s',
          }}
          onMouseEnter={(e) => { e.currentTarget.style.color = 'var(--color-primary-600)'; e.currentTarget.style.boxShadow = 'var(--shadow-sm)'; }}
          onMouseLeave={(e) => { e.currentTarget.style.color = 'var(--color-text-secondary)'; e.currentTarget.style.boxShadow = 'none'; }}
        >
          <IconPlus size={13} />
        </button>
      </div>
      <div
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
        style={{
          flex: 1, 'overflow-y': 'auto', display: 'flex', 'flex-direction': 'column', gap: '8px',
          padding: '2px 2px 4px',
          'border-radius': 'var(--radius-item)',
          outline: isDragOver() ? '2px dashed var(--color-primary-600)' : 'none',
          'outline-offset': '-2px',
          background: isDragOver() ? 'var(--color-accent-soft)' : 'transparent',
          transition: 'background-color .12s',
        }}
      >
        <For each={props.column.items}>
          {(item) => (
            <ItemCard
              item={item}
              typeLabel={props.typeLabelOf(item)}
              onEdit={props.onEditItem}
            />
          )}
        </For>
        <Show when={props.column.items.length === 0}>
          <div style={{
            'min-height': '70px', display: 'grid', 'place-items': 'center',
            border: '2px dashed var(--color-border-light)', 'border-radius': 'var(--radius-item)',
            padding: '12px', 'text-align': 'center', 'font-size': '12px', color: 'var(--color-text-tertiary)',
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

  const boardStateFromServer = createMemo((): BoardState | null => {
    const proj = project();
    const its = items();
    if (!proj || !its) return null;
    return deriveBoard(proj, its);
  });

  // status name → category, for the column dot color.
  const categoryOf = (status: string): 'todo' | 'in_progress' | 'done' =>
    project()?.workflow.statuses.find((s) => s.name === status)?.category ?? 'todo';
  const dotColor = (status: string): string => {
    switch (categoryOf(status)) {
      case 'in_progress': return 'var(--color-primary-600)';
      case 'done': return 'var(--color-success-600)';
      default: return 'var(--color-accent2)';
    }
  };
  const typeLabelOf = (item: Item) => vocab.t(typeKey(item.item_type));

  const [showCreateModal, setShowCreateModal] = createSignal(false);
  const [selectedColumn, setSelectedColumn] = createSignal<string | null>(null);
  const [activeContext, setActiveContext] = createSignal<ShortcutContext>('board');
  const [editingItem, setEditingItem] = createSignal<Item | null>(null);
  const [modalMode, setModalMode] = createSignal<'create' | 'edit'>('create');
  const [optimisticBoardState, setOptimisticBoardState] = createSignal<BoardState | null>(null);

  const currentBoard = (): BoardState | null | undefined => optimisticBoardState() || boardStateFromServer();

  // Unique assignees across the board → toolbar avatar stack.
  const assignees = createMemo(() => {
    const set = new Set<string>();
    for (const col of currentBoard()?.columns ?? []) {
      for (const it of col.items) if (it.assignee) set.add(it.assignee);
    }
    return [...set];
  });
  const itemCount = () => (currentBoard()?.columns ?? []).reduce((n, c) => n + c.items.length, 0);

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
    <div style={{ flex: 1, display: 'flex', 'flex-direction': 'column', 'min-width': 0, height: '100%' }}>
      {/* board toolbar */}
      <div style={{ 'flex-shrink': 0, padding: '16px 22px 10px', display: 'flex', 'align-items': 'center', gap: '12px' }}>
        <h1 style={{ 'font-size': '34px', 'line-height': 1.1, margin: 0, color: 'var(--color-text-primary)' }}>
          {vocab.t('board')}
        </h1>
        <Show when={sock()}>
          <span style={{
            display: 'inline-flex', 'align-items': 'center', gap: '6px', 'font-size': '12px', 'font-weight': 600,
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
        <span style={{ display: 'inline-flex', gap: '4px', 'font-size': '13px', color: 'var(--color-text-secondary)' }}>
          <span style={{ color: 'var(--color-text-tertiary)' }}>·</span>
          <span>{itemCount()} items</span>
        </span>
        <div style={{ flex: 1 }} />
        <Show when={assignees().length > 0}>
          <AvatarStack names={assignees()} max={5} />
        </Show>
      </div>

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

          <div style={{ flex: 1, 'overflow-x': 'auto', 'overflow-y': 'hidden', padding: '6px 22px 22px' }}>
            <div style={{ display: 'flex', gap: '14px', height: '100%', 'align-items': 'flex-start' }}>
              <For each={currentBoard()?.columns || []}>
                {(column) => (
                  <BoardColumnView
                    column={column}
                    dotColor={dotColor(column.status)}
                    typeLabelOf={typeLabelOf}
                    onItemDrop={handleItemDrop}
                    onAddItem={handleAddItem}
                    onEditItem={handleEditItem}
                  />
                )}
              </For>
            </div>
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
          initialStatus={selectedColumn() || undefined}
          mode={modalMode()}
          existingItem={editingItem() || undefined}
        />
      </Show>
    </div>
  );
};

export default Board;
