import { createSignal, createResource, For, Show } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { api } from '../lib/api';
import { toast } from '../lib/toast';

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

export default function BoardsManager() {
  const params = useParams();
  const navigate = useNavigate();
  const projectId = params.id!;

  const [showCreateModal, setShowCreateModal] = createSignal(false);
  const [editingBoard, setEditingBoard] = createSignal<Board | null>(null);

  // Form state
  const [name, setName] = createSignal('');
  const [description, setDescription] = createSignal('');
  const [grouping, setGrouping] = createSignal<string>('status');
  const [isDefault, setIsDefault] = createSignal(false);

  const [boards, { refetch }] = createResource(() =>
    fetch(`http://localhost:3210/api/projects/${projectId}/boards`)
      .then(res => res.json())
  );

  const [project] = createResource(() => api.getProject(projectId));

  const openCreateModal = () => {
    setName('');
    setDescription('');
    setGrouping('status');
    setIsDefault(false);
    setEditingBoard(null);
    setShowCreateModal(true);
  };

  const openEditModal = (board: Board) => {
    setName(board.name);
    setDescription(board.description || '');
    setGrouping(board.grouping || 'status');
    setIsDefault(board.is_default);
    setEditingBoard(board);
    setShowCreateModal(true);
  };

  const handleSubmit = async (e: Event) => {
    e.preventDefault();

    const body = {
      name: name().trim(),
      description: description().trim() || null,
      grouping: grouping(),
      is_default: isDefault(),
    };

    try {
      if (editingBoard()) {
        await fetch(`http://localhost:3210/api/boards/${editingBoard()!.id}`, {
          method: 'PATCH',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(body),
        });
        toast.success('Board updated successfully');
      } else {
        await fetch(`http://localhost:3210/api/projects/${projectId}/boards`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(body),
        });
        toast.success('Board created successfully');
      }

      setShowCreateModal(false);
      refetch();
    } catch (error) {
      toast.error('Failed to save board');
    }
  };

  const handleDelete = async (boardId: string) => {
    if (!confirm('Are you sure you want to delete this board?')) return;

    try {
      await fetch(`http://localhost:3210/api/boards/${boardId}`, {
        method: 'DELETE',
      });
      toast.success('Board deleted successfully');
      refetch();
    } catch (error) {
      toast.error('Failed to delete board');
    }
  };

  const handleSetDefault = async (boardId: string) => {
    try {
      await fetch(`http://localhost:3210/api/boards/${boardId}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ is_default: true }),
      });
      toast.success('Default board updated');
      refetch();
    } catch (error) {
      toast.error('Failed to update default board');
    }
  };

  return (
    <div class="min-h-screen bg-gray-50 dark:bg-gray-900 p-6">
      <div class="max-w-4xl mx-auto">
        {/* Header */}
        <div class="mb-6 flex items-center justify-between">
          <div>
            <h1 class="text-3xl font-bold text-gray-900 dark:text-white">
              Boards Manager
            </h1>
            <p class="text-gray-600 dark:text-gray-400 mt-1">
              {project()?.name || 'Loading...'}
            </p>
          </div>
          <div class="flex gap-2">
            <button
              onClick={openCreateModal}
              class="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 transition-colors"
            >
              + Create Board
            </button>
            <button
              onClick={() => navigate(`/projects/${projectId}/board`)}
              class="px-4 py-2 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
            >
              Back to Project
            </button>
          </div>
        </div>

        {/* Boards List */}
        <div class="space-y-4">
          <Show when={boards.loading}>
            <div class="text-center py-12 text-gray-500">Loading boards...</div>
          </Show>

          <Show when={boards.error}>
            <div class="text-center py-12 text-red-500">Failed to load boards</div>
          </Show>

          <For each={boards()}>
            {(board: Board) => (
              <div class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-6">
                <div class="flex items-start justify-between">
                  <div class="flex-1">
                    <div class="flex items-center gap-3">
                      <h3 class="text-lg font-semibold text-gray-900 dark:text-white">
                        {board.name}
                      </h3>
                      <Show when={board.is_default}>
                        <span class="px-2 py-1 text-xs bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 rounded">
                          Default
                        </span>
                      </Show>
                    </div>
                    <Show when={board.description}>
                      <p class="text-sm text-gray-600 dark:text-gray-400 mt-1">
                        {board.description}
                      </p>
                    </Show>
                    <div class="flex items-center gap-4 mt-3 text-sm text-gray-500 dark:text-gray-400">
                      <div>
                        <span class="font-medium">Grouping:</span>{' '}
                        {board.grouping || 'status'}
                      </div>
                      <div>
                        <span class="font-medium">Created:</span>{' '}
                        {new Date(board.created_at).toLocaleDateString()}
                      </div>
                    </div>
                  </div>

                  <div class="flex items-center gap-2">
                    <Show when={!board.is_default}>
                      <button
                        onClick={() => handleSetDefault(board.id)}
                        class="px-3 py-1.5 text-sm bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded hover:bg-gray-200 dark:hover:bg-gray-600 transition-colors"
                      >
                        Set as Default
                      </button>
                    </Show>
                    <button
                      onClick={() => openEditModal(board)}
                      class="px-3 py-1.5 text-sm bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300 rounded hover:bg-purple-200 dark:hover:bg-purple-900/50 transition-colors"
                    >
                      Edit
                    </button>
                    <button
                      onClick={() => handleDelete(board.id)}
                      class="px-3 py-1.5 text-sm bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300 rounded hover:bg-red-200 dark:hover:bg-red-900/50 transition-colors"
                    >
                      Delete
                    </button>
                    <button
                      onClick={() => navigate(`/projects/${projectId}/board/${board.id}`)}
                      class="px-3 py-1.5 text-sm bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 rounded hover:bg-blue-200 dark:hover:bg-blue-900/50 transition-colors"
                    >
                      View
                    </button>
                  </div>
                </div>
              </div>
            )}
          </For>

          <Show when={boards() && boards()!.length === 0}>
            <div class="text-center py-12">
              <p class="text-gray-500 dark:text-gray-400 mb-4">No boards yet</p>
              <button
                onClick={openCreateModal}
                class="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 transition-colors"
              >
                Create Your First Board
              </button>
            </div>
          </Show>
        </div>

        {/* Create/Edit Modal */}
        <Show when={showCreateModal()}>
          <div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
            <div class="bg-white dark:bg-gray-800 rounded-lg max-w-md w-full p-6">
              <h2 class="text-xl font-bold text-gray-900 dark:text-white mb-4">
                {editingBoard() ? 'Edit Board' : 'Create Board'}
              </h2>

              <form onSubmit={handleSubmit} class="space-y-4">
                <div>
                  <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Board Name *
                  </label>
                  <input
                    type="text"
                    value={name()}
                    onInput={(e) => setName(e.currentTarget.value)}
                    required
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                    placeholder="Main Board"
                  />
                </div>

                <div>
                  <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Description
                  </label>
                  <textarea
                    value={description()}
                    onInput={(e) => setDescription(e.currentTarget.value)}
                    rows={3}
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                    placeholder="Optional description"
                  />
                </div>

                <div>
                  <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Group By
                  </label>
                  <select
                    value={grouping()}
                    onChange={(e) => setGrouping(e.currentTarget.value)}
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-700 text-gray-900 dark:text-white"
                  >
                    <option value="status">Status (Kanban)</option>
                    <option value="priority">Priority</option>
                    <option value="item_type">Item Type</option>
                    <option value="sprint">Sprint</option>
                  </select>
                </div>

                <div class="flex items-center gap-2">
                  <input
                    type="checkbox"
                    id="is_default"
                    checked={isDefault()}
                    onChange={(e) => setIsDefault(e.currentTarget.checked)}
                    class="w-4 h-4 text-purple-600 rounded"
                  />
                  <label for="is_default" class="text-sm text-gray-700 dark:text-gray-300">
                    Set as default board
                  </label>
                </div>

                <div class="flex justify-end gap-2 mt-6">
                  <button
                    type="button"
                    onClick={() => setShowCreateModal(false)}
                    class="px-4 py-2 bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-600 transition-colors"
                  >
                    Cancel
                  </button>
                  <button
                    type="submit"
                    class="px-4 py-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 transition-colors"
                  >
                    {editingBoard() ? 'Update' : 'Create'} Board
                  </button>
                </div>
              </form>
            </div>
          </div>
        </Show>
      </div>
    </div>
  );
}
