import { createSignal, createResource, For, Show } from 'solid-js';
import { useNavigate } from '@solidjs/router';

interface Board {
  id: string;
  project_id: string;
  name: string;
  description: string | null;
  filters: any;
  grouping: string | null;
  is_default: boolean;
  created_at: string;
  updated_at: string;
}

interface BoardSelectorProps {
  projectId: string;
  currentBoardId?: string;
  onBoardChange?: (boardId: string) => void;
}

export default function BoardSelector(props: BoardSelectorProps) {
  const navigate = useNavigate();
  const [isOpen, setIsOpen] = createSignal(false);

  const [boards] = createResource(() =>
    fetch(`http://localhost:3210/api/projects/${props.projectId}/boards`)
      .then(res => res.json())
  );

  const currentBoard = () => {
    const allBoards = boards();
    if (!allBoards) return null;

    if (props.currentBoardId) {
      return allBoards.find((b: Board) => b.id === props.currentBoardId);
    }

    return allBoards.find((b: Board) => b.is_default) || allBoards[0];
  };

  const handleBoardSelect = (boardId: string) => {
    setIsOpen(false);
    if (props.onBoardChange) {
      props.onBoardChange(boardId);
    } else {
      navigate(`/projects/${props.projectId}/board/${boardId}`);
    }
  };

  const handleManageBoards = () => {
    setIsOpen(false);
    navigate(`/projects/${props.projectId}/settings/boards`);
  };

  return (
    <div class="relative">
      {/* Trigger Button */}
      <button
        onClick={() => setIsOpen(!isOpen())}
        class="flex items-center gap-2 px-4 py-2 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
      >
        <span class="text-sm font-medium text-gray-700 dark:text-gray-300">
          {currentBoard()?.name || 'Select Board'}
        </span>
        <svg
          class="w-4 h-4 text-gray-500 transition-transform"
          classList={{ 'rotate-180': isOpen() }}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {/* Dropdown Menu */}
      <Show when={isOpen()}>
        <div class="absolute top-full mt-2 w-64 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-lg z-50">
          <div class="p-2">
            <div class="text-xs font-semibold text-gray-500 dark:text-gray-400 px-3 py-2">
              BOARDS
            </div>

            <Show when={boards.loading}>
              <div class="px-3 py-2 text-sm text-gray-500">Loading...</div>
            </Show>

            <Show when={boards.error}>
              <div class="px-3 py-2 text-sm text-red-500">Failed to load boards</div>
            </Show>

            <For each={boards()}>
              {(board: Board) => (
                <button
                  onClick={() => handleBoardSelect(board.id)}
                  class="w-full px-3 py-2 text-left text-sm rounded-md hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors"
                  classList={{
                    'bg-purple-50 dark:bg-purple-900/20 text-purple-700 dark:text-purple-300':
                      board.id === currentBoard()?.id,
                    'text-gray-700 dark:text-gray-300':
                      board.id !== currentBoard()?.id,
                  }}
                >
                  <div class="flex items-center justify-between">
                    <div class="flex-1">
                      <div class="font-medium">{board.name}</div>
                      <Show when={board.description}>
                        <div class="text-xs text-gray-500 dark:text-gray-400 truncate">
                          {board.description}
                        </div>
                      </Show>
                    </div>
                    <Show when={board.is_default}>
                      <span class="ml-2 px-2 py-0.5 text-xs bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 rounded">
                        Default
                      </span>
                    </Show>
                  </div>
                </button>
              )}
            </For>
          </div>

          {/* Divider */}
          <div class="border-t border-gray-200 dark:border-gray-700"></div>

          {/* Manage Boards Button */}
          <div class="p-2">
            <button
              onClick={handleManageBoards}
              class="w-full px-3 py-2 text-left text-sm text-purple-600 dark:text-purple-400 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-md transition-colors"
            >
              + Manage Boards
            </button>
          </div>
        </div>
      </Show>

      {/* Click outside to close */}
      <Show when={isOpen()}>
        <div
          class="fixed inset-0 z-40"
          onClick={() => setIsOpen(false)}
        />
      </Show>
    </div>
  );
}
