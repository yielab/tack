import { createResource, For, Show, createSignal, createEffect, type Component } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { api } from '../../shared/api';
import type { BoardColumn, Item, BoardState } from '../../types/api';
import CreateItemModal from '../../shared/ui/CreateItemModal';
import BoardSelector from './BoardSelector';
import { useWebSocket, type BoardEvent } from '../../shared/realtime/websocket';
import { useKeyboard, keyboardManager, type ShortcutContext } from '../../shared/keyboard/keyboard';
import CommandPalette, { type Command } from '../../shared/ui/CommandPalette';
import { withOptimisticUpdate } from '../../shared/state/optimistic';
import { BoardSkeleton } from '../../shared/ui/SkeletonScreen';
import { Button } from '../../shared/ui';
import { useProject } from '../../shared/state/projectContext';

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
                ? "var(--color-warning)"
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
    <div class="flex-shrink-0 w-80">
      <div
        class="rounded-lg p-4 min-h-[500px]"
        style={{ "background-color": "var(--color-bg-subtle)" }}
        classList={{
          'ring-2 ring-purple-500 ring-inset': isDragOver(),
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
  const navigate = useNavigate();
  const params = useParams();
  const projectId = () => params.id;
  const { vocabulary } = useProject();

  const [board, { refetch }] = createResource(
    projectId,
    (id) => (id ? api.boards.projectBoardState(id) : Promise.resolve(null))
  );

  const [showCreateModal, setShowCreateModal] = createSignal(false);
  const [selectedColumn, setSelectedColumn] = createSignal<string | null>(null);
  const [showCommandPalette, setShowCommandPalette] = createSignal(false);
  const [activeContext, setActiveContext] = createSignal<ShortcutContext>('board');
  const [editingItem, setEditingItem] = createSignal<Item | null>(null);
  const [modalMode, setModalMode] = createSignal<'create' | 'edit'>('create');
  const [optimisticBoardState, setOptimisticBoardState] = createSignal<BoardState | null>(null);

  // Get the current board state (optimistic or real)
  const currentBoard = (): BoardState | null | undefined => optimisticBoardState() || board();

  // WebSocket connection for real-time updates
  const wsManager = useWebSocket(projectId());

  // Handle WebSocket events
  createEffect(() => {
    if (!wsManager) return;

    const cleanup = wsManager.onEvent((event: BoardEvent) => {
      console.log('[Board] Received WebSocket event:', event.event_type);

      // Auto-refresh board on any item change
      if (
        event.event_type === 'ItemCreated' ||
        event.event_type === 'ItemUpdated' ||
        event.event_type === 'ItemDeleted' ||
        event.event_type === 'BoardConfigUpdated'
      ) {
        refetch();
      }
    });

    return cleanup;
  });

  // Keyboard shortcuts
  useKeyboard(activeContext);

  // Register board-specific shortcuts
  createEffect(() => {
    // Update context when modal opens/closes
    setActiveContext(showCreateModal() || showCommandPalette() ? 'modal' : 'board');

    // Register shortcuts
    keyboardManager.register('global', {
      key: 'k',
      ctrl: true,
      description: 'Open command palette',
      action: () => setShowCommandPalette(true),
    });

    keyboardManager.register('board', {
      key: 'n',
      description: 'Create new item',
      action: () => {
        // Default to first column if available
        const firstColumn = currentBoard()?.columns[0];
        if (firstColumn) {
          handleAddItem(firstColumn.status);
        }
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
      action: () => {
        setShowCreateModal(false);
        setShowCommandPalette(false);
      },
    });
  });

  // Command palette commands
  const commands = (): Command[] => {
    const cmds: Command[] = [
      {
        id: 'new-item',
        label: 'Create New Item',
        description: 'Add a new item to the board',
        icon: '➕',
        shortcut: 'N',
        action: () => {
          const firstColumn = currentBoard()?.columns[0];
          if (firstColumn) {
            handleAddItem(firstColumn.status);
          }
        },
      },
      {
        id: 'refresh',
        label: 'Refresh Board',
        description: 'Reload board data',
        icon: '🔄',
        shortcut: 'R',
        action: () => refetch(),
      },
      {
        id: 'go-dashboard',
        label: 'Switch to Dashboard',
        description: 'View project statistics and analytics',
        icon: '📊',
        action: () => projectId() && navigate(`/projects/${projectId()}/dashboard`),
      },
      {
        id: 'go-list',
        label: 'Switch to List View',
        description: 'View items in table format',
        icon: '📋',
        action: () => projectId() && navigate(`/projects/${projectId()}/list`),
      },
      {
        id: 'go-sprints',
        label: 'Switch to Sprints',
        description: 'Manage sprints and backlog',
        icon: '🏃',
        action: () => projectId() && navigate(`/projects/${projectId()}/sprints`),
      },
      {
        id: 'go-calendar',
        label: 'Switch to Calendar',
        description: 'View items on calendar',
        icon: '📅',
        action: () => projectId() && navigate(`/projects/${projectId()}/calendar`),
      },
      {
        id: 'go-timeline',
        label: 'Switch to Timeline',
        description: 'View Gantt-style timeline',
        icon: '📈',
        action: () => projectId() && navigate(`/projects/${projectId()}/timeline`),
      },
      {
        id: 'go-projects',
        label: 'Go to Projects',
        description: 'Navigate to projects list',
        icon: '📁',
        action: () => navigate('/projects'),
      },
      {
        id: 'go-home',
        label: 'Go to Home',
        description: 'Navigate to home page',
        icon: '🏠',
        action: () => navigate('/'),
      },
    ];

    // Add "Create in Column" commands for each column
    currentBoard()?.columns.forEach((column) => {
      cmds.push({
        id: `add-${column.status}`,
        label: `Add Item to ${column.status}`,
        description: `Create a new item in the ${column.status} column`,
        icon: '📝',
        action: () => handleAddItem(column.status),
      });
    });

    return cmds;
  };

  const handleItemDrop = async (itemId: string, newStatus: string) => {
    const realBoard = board();
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

  const handleEditItem = (item: Item) => {
    setModalMode('edit');
    setEditingItem(item);
    setSelectedColumn(null);
    setShowCreateModal(true);
  };

  return (
    <div>
      <div class="mb-8">
        <div class="flex items-center justify-between">
          <div>
            <h1 class="text-3xl font-bold" style={{ color: "var(--color-text-primary)" }}>Board</h1>
            <p class="mt-2" style={{ color: "var(--color-text-secondary)" }}>
              Drag items between columns to change their status
            </p>
          </div>

          <div class="flex items-center gap-4">
            {/* Board Selector */}
            <Show when={projectId()}>
              <BoardSelector projectId={projectId()!} />
            </Show>

            {/* View Navigation Buttons */}
            <Show when={projectId()}>
              <div class="flex items-center gap-2">
                <Button size="sm" variant="secondary" onClick={() => navigate(`/projects/${projectId()}/dashboard`)}>
                  Dashboard
                </Button>
                <Button size="sm" variant="secondary" onClick={() => navigate(`/projects/${projectId()}/list`)}>
                  List
                </Button>
                <Button size="sm" variant="secondary" onClick={() => navigate(`/projects/${projectId()}/sprints`)}>
                  Sprints
                </Button>
                <Button size="sm" variant="secondary" onClick={() => navigate(`/projects/${projectId()}/calendar`)}>
                  Calendar
                </Button>
                <Button size="sm" variant="secondary" onClick={() => navigate(`/projects/${projectId()}/timeline`)}>
                  Timeline
                </Button>
              </div>
            </Show>

            {/* Keyboard Shortcut Hint */}
            <button
              onClick={() => setShowCommandPalette(true)}
              class="px-3 py-1.5 text-sm border rounded-md transition-colors"
              style={{
                color: "var(--color-text-secondary)",
                "border-color": "var(--color-border-medium)",
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.color = "var(--color-text-primary)";
                e.currentTarget.style.borderColor = "var(--color-primary-500)";
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.color = "var(--color-text-secondary)";
                e.currentTarget.style.borderColor = "var(--color-border-medium)";
              }}
            >
              <span class="font-mono">Ctrl+K</span>
              <span class="ml-2">Commands</span>
            </button>

            {/* Connection Status Indicator */}
            <Show when={wsManager}>
              <div class="flex items-center gap-2 text-sm">
                <div
                  class="w-2 h-2 rounded-full"
                  classList={{
                    'bg-green-500 animate-pulse': wsManager!.status() === 'connected',
                    'bg-yellow-500': wsManager!.status() === 'connecting',
                    'bg-gray-400': wsManager!.status() === 'disconnected',
                    'bg-red-500': wsManager!.status() === 'error',
                  }}
                />
                <span style={{ color: "var(--color-text-secondary)" }}>
                  {wsManager!.status() === 'connected' && 'Live'}
                  {wsManager!.status() === 'connecting' && 'Connecting...'}
                  {wsManager!.status() === 'disconnected' && 'Offline'}
                  {wsManager!.status() === 'error' && 'Connection Error'}
                </span>
              </div>
            </Show>
          </div>
        </div>
      </div>

      <Show
        when={!board.loading}
        fallback={<BoardSkeleton />}
      >
        <Show when={projectId()}>
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

      {/* Command Palette */}
      <CommandPalette
        isOpen={showCommandPalette()}
        onClose={() => setShowCommandPalette(false)}
        commands={commands()}
      />
    </div>
  );
};

export default Board;
