import { createSignal, createResource, For, Show } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { api } from '../../shared/api';
import { toast } from '../../shared/ui/toast';
import type { Sprint, Item } from '../../types/api';

export default function Sprints() {
  const params = useParams();
  const navigate = useNavigate();
  const projectId = params.id!;

  const [project] = createResource(() => api.projects.get(projectId));
  const [sprints, { refetch: refetchSprints }] = createResource(
    () => api.sprints.list(projectId)
  );
  const [items] = createResource(() => api.items.list(projectId));

  const [showCreateModal, setShowCreateModal] = createSignal(false);
  const [editingSprint, setEditingSprint] = createSignal<Sprint | null>(null);
  const [loading, setLoading] = createSignal(false);

  // Form state
  const [name, setName] = createSignal('');
  const [goal, setGoal] = createSignal('');
  const [startDate, setStartDate] = createSignal('');
  const [endDate, setEndDate] = createSignal('');

  const openCreateModal = () => {
    setName('');
    setGoal('');
    setStartDate('');
    setEndDate('');
    setEditingSprint(null);
    setShowCreateModal(true);
  };

  const openEditModal = (sprint: Sprint) => {
    setName(sprint.name);
    setGoal(sprint.goal || '');
    setStartDate(sprint.start_date ? sprint.start_date.split('T')[0] : '');
    setEndDate(sprint.end_date ? sprint.end_date.split('T')[0] : '');
    setEditingSprint(sprint);
    setShowCreateModal(true);
  };

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    if (!name().trim()) {
      toast.error('Sprint name is required');
      return;
    }

    setLoading(true);
    try {
      const body = {
        name: name().trim(),
        goal: goal().trim() || undefined,
        start_date: startDate() || undefined,
        end_date: endDate() || undefined,
      };

      if (editingSprint()) {
        await api.sprints.update(editingSprint()!.id, body);
        toast.success('Sprint updated successfully');
      } else {
        await api.sprints.create(projectId, body);
        toast.success('Sprint created successfully');
      }

      setShowCreateModal(false);
      await refetchSprints();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'Failed to save sprint');
    } finally {
      setLoading(false);
    }
  };

  const updateSprintStatus = async (sprintId: string, status: string) => {
    try {
      await api.sprints.setStatus(sprintId, status);
      toast.success(`Sprint ${status}`);
      await refetchSprints();
    } catch (error) {
      toast.error('Failed to update sprint status');
    }
  };

  const getSprintItems = (sprintId: string): Item[] => {
    return (items() || []).filter(item => item.sprint_id === sprintId);
  };

  const getBacklogItems = (): Item[] => {
    return (items() || []).filter(item => !item.sprint_id);
  };

  const getStatusBadgeStyle = (status: string) => {
    switch (status) {
      case 'planning': return { 'background-color': 'var(--color-warning-100)', color: 'var(--color-warning-700)' };
      case 'active': return { 'background-color': 'var(--color-success-100)', color: 'var(--color-success-700)' };
      case 'review': return { 'background-color': 'var(--color-primary-100)', color: 'var(--color-primary-700)' };
      case 'closed': return { 'background-color': 'var(--color-bg-subtle)', color: 'var(--color-text-secondary)' };
      default: return { 'background-color': 'var(--color-bg-subtle)', color: 'var(--color-text-secondary)' };
    }
  };

  const formatDate = (dateStr?: string) => {
    if (!dateStr) return 'Not set';
    return new Date(dateStr).toLocaleDateString('en-US', {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
    });
  };

  return (
    <div class="min-h-screen p-6" style={{ "background-color": "var(--color-bg-subtle)" }}>
      <div class="max-w-7xl mx-auto">
        {/* Header */}
        <div class="mb-6 flex items-center justify-between">
          <div>
            <h1 class="text-3xl font-bold" style={{ color: "var(--color-text-primary)" }}>
              {project()?.name || 'Loading...'} - Sprints
            </h1>
            <p class="mt-1" style={{ color: "var(--color-text-secondary)" }}>
              Manage sprints and iterations
            </p>
          </div>
          <div class="flex gap-2">
            <button
              onClick={openCreateModal}
              class="px-4 py-2 rounded-lg transition-colors"
              style={{ "background-color": "var(--color-primary-600)", color: "var(--color-text-inverse)" }}
            >
              Create Sprint
            </button>
            <button
              onClick={() => navigate(`/projects/${projectId}/board`)}
              class="px-4 py-2 border rounded-lg transition-colors"
              style={{ "background-color": "var(--color-bg-base)", "border-color": "var(--color-border-light)" }}
            >
              Board View
            </button>
            <button
              onClick={() => navigate('/projects')}
              class="px-4 py-2 border rounded-lg transition-colors"
              style={{ "background-color": "var(--color-bg-base)", "border-color": "var(--color-border-light)" }}
            >
              Back to Projects
            </button>
          </div>
        </div>

        {/* Sprints List */}
        <div class="space-y-4">
          <For each={sprints()}>
            {(sprint: Sprint) => {
              const sprintItems = getSprintItems(sprint.id);
              const completedItems = sprintItems.filter(item => {
                const status = project()?.workflow?.statuses.find(s => s.name === item.status);
                return status?.category === 'done';
              });
              const totalPoints = sprintItems.reduce((sum, item) => sum + (item.estimate || 0), 0);
              const completedPoints = completedItems.reduce((sum, item) => sum + (item.estimate || 0), 0);

              return (
                <div class="rounded-lg border p-6" style={{ "background-color": "var(--color-bg-base)", "border-color": "var(--color-border-light)" }}>
                  <div class="flex items-start justify-between mb-4">
                    <div class="flex-1">
                      <div class="flex items-center gap-3 mb-2">
                        <h2 class="text-xl font-bold" style={{ color: "var(--color-text-primary)" }}>
                          {sprint.name}
                        </h2>
                        <span class="px-3 py-1 text-sm font-medium rounded-full" style={getStatusBadgeStyle(sprint.status)}>
                          {sprint.status}
                        </span>
                      </div>
                      <Show when={sprint.goal}>
                        <p class="mb-2" style={{ color: "var(--color-text-secondary)" }}>
                          Goal: {sprint.goal}
                        </p>
                      </Show>
                      <div class="flex items-center gap-4 text-sm" style={{ color: "var(--color-text-secondary)" }}>
                        <span>Start: {formatDate(sprint.start_date)}</span>
                        <span>•</span>
                        <span>End: {formatDate(sprint.end_date)}</span>
                      </div>
                    </div>
                    <div class="flex gap-2">
                      <button
                        onClick={() => openEditModal(sprint)}
                        class="px-3 py-1 text-sm rounded"
                        style={{ "background-color": "var(--color-bg-subtle)", color: "var(--color-text-secondary)" }}
                      >
                        Edit
                      </button>
                      <Show when={sprint.status === 'planning'}>
                        <button
                          onClick={() => updateSprintStatus(sprint.id, 'active')}
                          class="px-3 py-1 text-sm rounded"
                          style={{ "background-color": "var(--color-success-600)", color: "var(--color-text-inverse)" }}
                        >
                          Start Sprint
                        </button>
                      </Show>
                      <Show when={sprint.status === 'active'}>
                        <button
                          onClick={() => updateSprintStatus(sprint.id, 'review')}
                          class="px-3 py-1 text-sm rounded"
                          style={{ "background-color": "var(--color-primary-600)", color: "var(--color-text-inverse)" }}
                        >
                          Complete
                        </button>
                      </Show>
                      <Show when={sprint.status === 'review'}>
                        <button
                          onClick={() => updateSprintStatus(sprint.id, 'closed')}
                          class="px-3 py-1 text-sm rounded"
                          style={{ "background-color": "var(--color-bg-subtle)", color: "var(--color-text-inverse)" }}
                        >
                          Close Sprint
                        </button>
                      </Show>
                    </div>
                  </div>

                  {/* Sprint Progress */}
                  <div class="grid grid-cols-3 gap-4 mb-4">
                    <div>
                      <p class="text-sm" style={{ color: "var(--color-text-secondary)" }}>Items</p>
                      <p class="text-2xl font-bold" style={{ color: "var(--color-text-primary)" }}>
                        {completedItems.length} / {sprintItems.length}
                      </p>
                    </div>
                    <div>
                      <p class="text-sm" style={{ color: "var(--color-text-secondary)" }}>Story Points</p>
                      <p class="text-2xl font-bold" style={{ color: "var(--color-text-primary)" }}>
                        {completedPoints} / {totalPoints}
                      </p>
                    </div>
                    <div>
                      <p class="text-sm" style={{ color: "var(--color-text-secondary)" }}>Progress</p>
                      <p class="text-2xl font-bold" style={{ color: "var(--color-text-primary)" }}>
                        {sprintItems.length > 0 ? Math.round((completedItems.length / sprintItems.length) * 100) : 0}%
                      </p>
                    </div>
                  </div>

                  {/* Progress Bar */}
                  <div class="w-full rounded-full h-2 mb-4" style={{ "background-color": "var(--color-bg-subtle)" }}>
                    <div
                      class="h-2 rounded-full"
                      style={{
                        "background-color": "var(--color-success-500)",
                        width: `${sprintItems.length > 0 ? (completedItems.length / sprintItems.length) * 100 : 0}%`,
                      }}
                    />
                  </div>

                  {/* Sprint Items */}
                  <Show when={sprintItems.length > 0}>
                    <div class="border-t pt-4" style={{ "border-color": "var(--color-border-light)" }}>
                      <h3 class="text-sm font-medium mb-2" style={{ color: "var(--color-text-primary)" }}>
                        Items ({sprintItems.length})
                      </h3>
                      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-2">
                        <For each={sprintItems.slice(0, 6)}>
                          {(item) => (
                            <div class="text-sm p-2 rounded border" style={{ "background-color": "var(--color-bg-subtle)", "border-color": "var(--color-border-light)" }}>
                              <div class="font-medium truncate" style={{ color: "var(--color-text-primary)" }}>
                                {item.title}
                              </div>
                              <div class="text-xs" style={{ color: "var(--color-text-secondary)" }}>
                                {item.status} • {item.estimate || 0} pts
                              </div>
                            </div>
                          )}
                        </For>
                      </div>
                      <Show when={sprintItems.length > 6}>
                        <p class="text-sm mt-2" style={{ color: "var(--color-text-tertiary)" }}>
                          +{sprintItems.length - 6} more items
                        </p>
                      </Show>
                    </div>
                  </Show>
                </div>
              );
            }}
          </For>

          <Show when={!sprints() || sprints()!.length === 0}>
            <div class="text-center py-12" style={{ color: "var(--color-text-tertiary)" }}>
              <p class="text-lg mb-4">No sprints yet</p>
              <button
                onClick={openCreateModal}
                class="px-6 py-3 rounded-lg transition-colors"
                style={{ "background-color": "var(--color-primary-600)", color: "var(--color-text-inverse)" }}
              >
                Create Your First Sprint
              </button>
            </div>
          </Show>
        </div>

        {/* Backlog */}
        <Show when={getBacklogItems().length > 0}>
          <div class="mt-6 rounded-lg border p-6" style={{ "background-color": "var(--color-bg-base)", "border-color": "var(--color-border-light)" }}>
            <h2 class="text-xl font-bold mb-4" style={{ color: "var(--color-text-primary)" }}>
              Backlog ({getBacklogItems().length} items)
            </h2>
            <p class="text-sm mb-4" style={{ color: "var(--color-text-secondary)" }}>
              Items not assigned to any sprint
            </p>
            <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-2">
              <For each={getBacklogItems().slice(0, 9)}>
                {(item) => (
                  <div class="text-sm p-2 rounded border" style={{ "background-color": "var(--color-bg-subtle)", "border-color": "var(--color-border-light)" }}>
                    <div class="font-medium truncate" style={{ color: "var(--color-text-primary)" }}>
                      {item.title}
                    </div>
                    <div class="text-xs" style={{ color: "var(--color-text-secondary)" }}>
                      {item.status} • {item.estimate || 0} pts
                    </div>
                  </div>
                )}
              </For>
            </div>
            <Show when={getBacklogItems().length > 9}>
              <p class="text-sm mt-2" style={{ color: "var(--color-text-tertiary)" }}>
                +{getBacklogItems().length - 9} more items in backlog
              </p>
            </Show>
          </div>
        </Show>
      </div>

      {/* Create/Edit Sprint Modal */}
      <Show when={showCreateModal()}>
        <div class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
          <div class="rounded-lg p-6 w-full max-w-md" style={{ "background-color": "var(--color-bg-base)" }}>
            <h2 class="text-2xl font-bold mb-4" style={{ color: "var(--color-text-primary)" }}>
              {editingSprint() ? 'Edit Sprint' : 'Create Sprint'}
            </h2>
            <form onSubmit={handleSubmit} class="space-y-4">
              <div>
                <label class="block text-sm font-medium mb-1" style={{ color: "var(--color-text-primary)" }}>
                  Sprint Name <span class="text-red-500">*</span>
                </label>
                <input
                  type="text"
                  value={name()}
                  onInput={(e) => setName(e.currentTarget.value)}
                  placeholder="Sprint 1"
                  class="w-full px-3 py-2 border rounded-lg"
                  style={{ "background-color": "var(--color-bg-base)", "border-color": "var(--color-border-light)", color: "var(--color-text-primary)" }}
                  required
                  disabled={loading()}
                />
              </div>

              <div>
                <label class="block text-sm font-medium mb-1" style={{ color: "var(--color-text-primary)" }}>
                  Sprint Goal
                </label>
                <textarea
                  value={goal()}
                  onInput={(e) => setGoal(e.currentTarget.value)}
                  placeholder="What will be accomplished in this sprint?"
                  rows={3}
                  class="w-full px-3 py-2 border rounded-lg resize-none"
                  style={{ "background-color": "var(--color-bg-base)", "border-color": "var(--color-border-light)", color: "var(--color-text-primary)" }}
                  disabled={loading()}
                />
              </div>

              <div class="grid grid-cols-2 gap-4">
                <div>
                  <label class="block text-sm font-medium mb-1" style={{ color: "var(--color-text-primary)" }}>
                    Start Date
                  </label>
                  <input
                    type="date"
                    value={startDate()}
                    onInput={(e) => setStartDate(e.currentTarget.value)}
                    class="w-full px-3 py-2 border rounded-lg"
                    style={{ "background-color": "var(--color-bg-base)", "border-color": "var(--color-border-light)", color: "var(--color-text-primary)" }}
                    disabled={loading()}
                  />
                </div>

                <div>
                  <label class="block text-sm font-medium mb-1" style={{ color: "var(--color-text-primary)" }}>
                    End Date
                  </label>
                  <input
                    type="date"
                    value={endDate()}
                    onInput={(e) => setEndDate(e.currentTarget.value)}
                    class="w-full px-3 py-2 border rounded-lg"
                    style={{ "background-color": "var(--color-bg-base)", "border-color": "var(--color-border-light)", color: "var(--color-text-primary)" }}
                    disabled={loading()}
                  />
                </div>
              </div>

              <div class="flex gap-3 pt-4">
                <button
                  type="submit"
                  disabled={loading()}
                  class="flex-1 px-4 py-2 rounded-lg disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                  style={{ "background-color": "var(--color-primary-600)", color: "var(--color-text-inverse)" }}
                >
                  {loading() ? 'Saving...' : editingSprint() ? 'Update Sprint' : 'Create Sprint'}
                </button>
                <button
                  type="button"
                  onClick={() => setShowCreateModal(false)}
                  disabled={loading()}
                  class="px-4 py-2 rounded-lg disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                  style={{ "background-color": "var(--color-bg-subtle)", color: "var(--color-text-secondary)" }}
                >
                  Cancel
                </button>
              </div>
            </form>
          </div>
        </div>
      </Show>
    </div>
  );
}
