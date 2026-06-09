import { createSignal, createResource, For, Show } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { api } from '../../../shared/api';
import { toast } from '../../../shared/ui/toast';
import { resolveLabel } from '../../../shared/vocab/vocab';
import { Button, Field, FieldShell, Select, Modal, Badge, EmptyState } from '../../../shared/ui';
import { useProject } from '../../../shared/state/projectContext';

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

export default function BoardsPanel() {
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

  const [boards, { refetch }] = createResource(() => api.boards.list(projectId));

  const { project } = useProject();

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
        await api.boards.update(editingBoard()!.id, body);
        toast.success('Board updated successfully');
      } else {
        await api.boards.create(projectId, body);
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
      await api.boards.remove(boardId);
      toast.success('Board deleted successfully');
      refetch();
    } catch (error) {
      toast.error('Failed to delete board');
    }
  };

  const handleSetDefault = async (boardId: string) => {
    try {
      await api.boards.update(boardId, { is_default: true });
      toast.success('Default board updated');
      refetch();
    } catch (error) {
      toast.error('Failed to update default board');
    }
  };

  return (
    <div>
      <div>
        <div class="mb-4 flex items-center justify-end">
          <Button onClick={openCreateModal}>+ Create Board</Button>
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
                        <Badge tone="info">Default</Badge>
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
                      <Button size="sm" variant="secondary" onClick={() => handleSetDefault(board.id)}>
                        Set as Default
                      </Button>
                    </Show>
                    <Button size="sm" variant="ghost" onClick={() => openEditModal(board)}>
                      Edit
                    </Button>
                    <Button size="sm" variant="danger" onClick={() => handleDelete(board.id)}>
                      Delete
                    </Button>
                    <Button size="sm" onClick={() => navigate(`/projects/${projectId}/board/${board.id}`)}>
                      View
                    </Button>
                  </div>
                </div>
              </div>
            )}
          </For>

          <Show when={boards() && boards()!.length === 0}>
            <EmptyState
              title="No boards yet"
              action={<Button onClick={openCreateModal}>Create Your First Board</Button>}
            />
          </Show>
        </div>

        {/* Create/Edit Modal */}
        <Modal
          isOpen={showCreateModal()}
          onClose={() => setShowCreateModal(false)}
          title={editingBoard() ? 'Edit Board' : 'Create Board'}
          size="sm"
        >
          <form onSubmit={handleSubmit} class="space-y-4">
            <Field
              label="Board Name"
              required
              value={name()}
              onInput={(e) => setName(e.currentTarget.value)}
              placeholder="Main Board"
            />

            <FieldShell label="Description" for="board-description">
              <textarea
                id="board-description"
                value={description()}
                onInput={(e) => setDescription(e.currentTarget.value)}
                rows={3}
                placeholder="Optional description"
                class="w-full resize-none rounded-lg border px-3 py-2 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1"
                style={{
                  'background-color': 'var(--color-bg-base)',
                  color: 'var(--color-text-primary)',
                  'border-color': 'var(--color-border-medium)',
                  '--tw-ring-color': 'var(--color-focus-ring)',
                }}
              />
            </FieldShell>

            <Select
              label="Group By"
              value={grouping()}
              onChange={(e) => setGrouping(e.currentTarget.value)}
            >
              <option value="status">Status (Kanban)</option>
              <option value="priority">Priority</option>
              <option value="item_type">Item Type</option>
              <option value="sprint">{resolveLabel(project()?.vocabulary, 'sprint')}</option>
            </Select>

            <label class="flex items-center gap-2 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
              <input
                type="checkbox"
                checked={isDefault()}
                onChange={(e) => setIsDefault(e.currentTarget.checked)}
                class="h-4 w-4 rounded"
              />
              Set as default board
            </label>

            <div class="flex justify-end gap-2 pt-2">
              <Button type="button" variant="secondary" onClick={() => setShowCreateModal(false)}>
                Cancel
              </Button>
              <Button type="submit">{editingBoard() ? 'Update' : 'Create'} Board</Button>
            </div>
          </form>
        </Modal>
      </div>
    </div>
  );
}
