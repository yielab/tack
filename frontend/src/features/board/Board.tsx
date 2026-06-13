import { createMemo, For, Show, createSignal, createEffect, onMount, onCleanup, type Component } from 'solid-js';
import { useParams, useSearchParams } from '@solidjs/router';
import { api } from '../../shared/api';
import { deriveBoard } from '../../shared/api/boards';
import type { BoardColumn, Item, BoardState } from '../../shared/types';
import CreateItemModal from '../../shared/ui/CreateItemModal';
import { createBoardSocket, type BoardSocket, type SocketStatus } from '../../shared/realtime/boardSocket';
import { useKeyboard, keyboardManager, type ShortcutContext } from '../../shared/keyboard/keyboard';
import { withOptimisticUpdate } from '../../shared/state/optimistic';
import { BoardSkeleton } from '../../shared/ui/SkeletonScreen';
import { useProject } from '../../shared/state/projectContext';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import { ITEM_UPDATED_EVENT } from '../../shared/state/itemEvents';
import EmptyProjectGuide from '../../shared/ui/EmptyProjectGuide';

const ItemCard: Component<{
  item: Item;
  onStatusChange: (itemId: string, newStatus: string) => void;
  onEdit: (item: Item) => void;
}> = (props) => {
  const [isDragging, setIsDragging] = createSignal(false);

  const handleDragStart = (e: DragEvent) => {
    setIsDragging(true);
    e.dataTransfer!.effectAllowed = 'move';
    e.dataTransfer!.setData('text/plain', props.item.id);
  };

  const handleDragEnd = () => {
    setIsDragging(false);
  };

  const handleClick = (e: MouseEvent) => {
    // Only trigger edit if not dragging and clicking the card itself (not badges)
    if (!isDragging() && (e.target as HTMLElement).closest('.item-card-content')) {
      props.onEdit(props.item);
    }
  };

  return (
    <div
      draggable={true}
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
      onClick={handleClick}
      class="rounded-lg p-4 shadow-sm border transition-colors cursor-pointer group"
      style={{
        "background-color": "var(--color-bg-elevated)",
        "border-color": "var(--color-border-light)",
      }}
      onMouseEnter={(e) => (e.currentTarget.style.borderColor = "var(--color-primary-500)")}
      onMouseLeave={(e) => (e.currentTarget.style.borderColor = "var(--color-border-light)")}
      classList={{
        'opacity-40': isDragging(),
      }}
    >
      <div class="item-card-content">
        <div class="flex items-start justify-between mb-2">
          <h4 class="font-medium flex-1" style={{ color: "var(--color-text-primary)" }}>
            {props.item.title}
          </h4>
          <span class="text-xs opacity-0 group-hover:opacity-100 transition-opacity ml-2" style={{ color: "var(--color-text-tertiary)" }}>
            Click to edit
          </span>
        </div>
        <Show when={props.item.description}>
          <p class="text-sm mb-2 line-clamp-2" style={{ color: "var(--color-text-secondary)" }}>
            {props.item.description}
          </p>
        </Show>
      </div>
      <div class="flex items-center gap-2 flex-wrap">
        <span
          class="text-xs px-2 py-1 rounded"
          style={{
            "background-color":
              props.item.priority === 'critical' || props.item.priority === 'high'
                ? "var(--color-danger-light)"
                : props.item.priority === 'medium'
                ? "var(--color-warning-light)"
                : "var(--color-bg-subtle)",
            color:
              props.item.priority === 'critical' || props.item.priority === 'high'
                ? "var(--color-danger)"
                : props.item.priority === 'medium'
                ? "var(--color-warning-700)"
                : "var(--color-text-secondary)",
          }}
        >
          {props.item.priority}
        </span>
        <span class="text-xs" style={{ color: "var(--color-text-secondary)" }}>
          {props.item.item_type.toString()}
        </span>
        <Show when={props.item.estimate}>
          <span class="text-xs" style={{ color: "var(--color-primary-600)" }}>
            {props.item.estimate} pts
          </span>
        </Show>
      </div>
    </div>
  );
};

const BoardColumn: Component<{
  column: BoardColumn;
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

  const handleDragLeave = () => {
    setIsDragOver(false);
  };

  const handleDrop = async (e: DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);

    const itemId = e.dataTransfer!.getData('text/plain');
    if (itemId) {
      props.onItemDrop(itemId, props.column.status);
    }
  };

  return (
    <div class="shrink-0 w-80">
      <div
        class="rounded-lg p-4 min-h-125"
        style={{ "background-color": "var(--color-bg-subtle)" }}
        classList={{
          'ring-2 ring-brand-500 ring-inset': isDragOver(),
        }}
        onDragOver={handleDragOver}
        onDragLeave={handleDragLeave}
        onDrop={handleDrop}
      >
        <div class="flex items-center justify-between mb-4">
          <h3 class="font-semibold" style={{ color: "var(--color-text-primary)" }}>
            {props.column.status}
          </h3>
          <div class="flex items-center gap-2">
            <span class="text-sm" style={{ color: "var(--color-text-secondary)" }}>
              {props.column.items.length}
            </span>
            <Show when={props.column.wip_limit}>
              <span
                class="text-xs px-2 py-1 rounded"
                style={{
                  "background-color": props.column.wip_exceeded
                    ? "var(--color-danger-light)"
                    : "var(--color-bg-subtle)",
                  color: props.column.wip_exceeded
                    ? "var(--color-danger)"
                    : "var(--color-text-secondary)",
                }}
              >
                / {props.column.wip_limit}
              </span>
            </Show>
          </div>
        </div>

        <div class="space-y-3">
          <For each={props.column.items}>
            {(item) => (
              <ItemCard
                item={item}
                onStatusChange={props.onItemDrop}
                onEdit={props.onEditItem}
              />
            )}
          </For>

          <Show when={props.column.items.length === 0}>
            <div class="border-2 border-dashed rounded-lg p-8 text-center text-sm" style={{ "border-color": "var(--color-border-medium)", color: "var(--color-text-secondary)" }}>
              Drop items here or click "+ Add item" below
            </div>
          </Show>
        </div>

        <button
          onClick={() => props.onAddItem(props.column.status)}
          class="w-full mt-3 py-2 text-sm transition-colors"
          style={{ color: "var(--color-text-secondary)" }}
          onMouseEnter={(e) => (e.currentTarget.style.color = "var(--color-primary-600)")}
          onMouseLeave={(e) => (e.currentTarget.style.color = "var(--color-text-secondary)")}
        >
          + Add item
        </button>
      </div>
    </div>
  );
};

const Board: Component = () => {
  const params = useParams();
  const projectId = () => params.id;
  const { vocabulary, project } = useProject();
  const { items, loading, refetch } = useProjectItems();
  const [, setSearchParams] = useSearchParams();

  // Derive board columns from shared items + project workflow (no extra API call)
  const boardStateFromServer = createMemo((): BoardState | null => {
    const proj = project();
    const its = items();
    if (!proj || !its) return null;
    return deriveBoard(proj, its);
  });

  const [showCreateModal, setShowCreateModal] = createSignal(false);
  const [selectedColumn, setSelectedColumn] = createSignal<string | null>(null);
  const [activeContext, setActiveContext] = createSignal<ShortcutContext>('board');
  const [editingItem, setEditingItem] = createSignal<Item | null>(null);
  const [modalMode, setModalMode] = createSignal<'create' | 'edit'>('create');
  const [optimisticBoardState, setOptimisticBoardState] = createSignal<BoardState | null>(null);

  // Get the current board state (optimistic or real)
  const currentBoard = (): BoardState | null | undefined => optimisticBoardState() || boardStateFromServer();

  // Real-time board updates via the reconnecting socket (T-513). The socket
  // filters to this project and parses the backend's snake_case events; any
  // board-relevant event refreshes the board so other clients' edits appear
  // without a manual reload.
  const [sock, setSock] = createSignal<BoardSocket>();
  const socketStatus = (): SocketStatus => sock()?.status() ?? 'closed';

  createEffect(() => {
    const pid = projectId();
    if (!pid) {
      setSock(undefined);
      return;
    }
    const s = createBoardSocket(pid);
    const off = s.onEvent(() => void refetch());
    setSock(s);
    onCleanup(() => {
      off();
      s.close();
    });
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

  // Keyboard shortcuts
  useKeyboard(activeContext);

  // Update keyboard context when modal opens/closes
  createEffect(() => {
    setActiveContext(showCreateModal() ? 'modal' : 'board');
  });

  createEffect(() => {
    keyboardManager.register('board', {
      key: 'n',
      description: 'Create new item',
      action: () => {
        const firstColumn = currentBoard()?.columns[0];
        if (firstColumn) handleAddItem(firstColumn.status);
      },
    });
    keyboardManager.register('board', {
      key: 'r',
      description: 'Refresh board',
      action: () => refetch(),
    });
    keyboardManager.register('modal', {
      key: 'Escape',
      description: 'Close modal',
      action: () => setShowCreateModal(false),
    });
  });

  const handleItemDrop = async (itemId: string, newStatus: string) => {
    const realBoard = boardStateFromServer();
    if (!realBoard) return;

    // Find the item being moved
    const item = realBoard.columns
      .flatMap(col => col.items)
      .find(i => i.id === itemId);

    if (!item || item.status === newStatus) return;

    // Create optimistic board state
    const optimisticBoard = {
      ...realBoard,
      columns: realBoard.columns.map(col => ({
        ...col,
        items: col.items
          .filter(i => i.id !== itemId) // Remove from old column
          .concat(col.status === newStatus ? [{ ...item, status: newStatus }] : []) // Add to new column
      }))
    };

    await withOptimisticUpdate(
      () => api.items.update(itemId, { status: newStatus }),
      () => setOptimisticBoardState(optimisticBoard),
      async () => {
        setOptimisticBoardState(null);
        await refetch();
      },
      {
        showSuccessToast: true,
        successMessage: `Item moved to ${newStatus}`,
        showErrorToast: true,
        errorMessage: 'Failed to move item',
      }
    );

    // Clear optimistic state and refetch real data
    setOptimisticBoardState(null);
    await refetch();
  };

  const handleAddItem = (status: string) => {
    setModalMode('create');
    setEditingItem(null);
    setSelectedColumn(status);
    setShowCreateModal(true);
  };

  // Open the item detail drawer (deep-linkable via ?item=).
  const handleEditItem = (item: Item) => {
    setSearchParams({ item: item.id });
  };

  // Refresh the board when the drawer edits an item.
  onMount(() => {
    const onItemUpdated = () => void refetch();
    window.addEventListener(ITEM_UPDATED_EVENT, onItemUpdated);
    onCleanup(() => window.removeEventListener(ITEM_UPDATED_EVENT, onItemUpdated));
  });

  return (
    <div>
      <Show when={sock()}>
        <div class="mb-4 flex items-center justify-end">
          <div class="flex items-center gap-1.5 text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
            <div
              class="w-1.5 h-1.5 rounded-full"
              classList={{
                'bg-success-500': socketStatus() === 'open',
                'bg-warning-500': socketStatus() === 'connecting' || socketStatus() === 'reconnecting',
                'bg-content-subtle': socketStatus() === 'closed',
              }}
            />
            {socketStatus() === 'open' && 'Live'}
            {(socketStatus() === 'connecting' || socketStatus() === 'reconnecting') && 'Connecting…'}
            {socketStatus() === 'closed' && 'Offline'}
          </div>
        </div>
      </Show>

      <Show
        when={!loading()}
        fallback={<BoardSkeleton />}
      >
        <Show when={projectId()}>
          {/* Guide shown when project exists but has no items yet */}
          <Show when={(currentBoard()?.columns ?? []).every(col => col.items.length === 0) && (currentBoard()?.columns ?? []).length > 0}>
            <EmptyProjectGuide onAddItem={() => handleAddItem(currentBoard()!.columns[0].status)} />
          </Show>

          <div class="flex gap-4 overflow-x-auto pb-4">
            <For each={currentBoard()?.columns || []}>
              {(column) => (
                <BoardColumn
                  column={column}
                  onItemDrop={handleItemDrop}
                  onAddItem={handleAddItem}
                  onEditItem={handleEditItem}
                />
              )}
            </For>
          </div>
        </Show>

        <Show when={!projectId()}>
          <div class="text-center py-12" style={{ color: "var(--color-text-secondary)" }}>
            Please select a project from the Projects page
          </div>
        </Show>
      </Show>

      {/* Create/Edit Item Modal */}
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
