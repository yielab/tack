import { createSignal, createResource, For, Show } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { api } from '../lib/api';
import { toast } from '../lib/toast';
import type { Sprint, Item } from '../types/api';

export default function Sprints() {
  const params = useParams();
  const navigate = useNavigate();
  const projectId = params.id!;

  const [project] = createResource(() => api.getProject(projectId));
  const [sprints, { refetch: refetchSprints }] = createResource(
    () => fetch(`http://localhost:3210/api/projects/${projectId}/sprints`).then(r => r.json())
  );
  const [items] = createResource(() => api.listItems(projectId));

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
        // Update sprint
        await fetch(`http://localhost:3210/api/sprints/${editingSprint()!.id}`, {
          method: 'PATCH',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(body),
        });
        toast.success('Sprint updated successfully');
      } else {
        // Create sprint
        await fetch(`http://localhost:3210/api/projects/${projectId}/sprints`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(body),
        });
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
      await fetch(`http://localhost:3210/api/sprints/${sprintId}/status`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ status }),
      });
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

  const getStatusBadgeClass = (status: string) => {
    switch (status) {
      case 'planning': return 'bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200';
      case 'active': return 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200';
      case 'review': return 'bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200';
      case 'closed': return 'bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-200';
      default: return 'bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-200';
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
    <div class="min-h-screen bg-gray-50 dark:bg-gray-900 p-6">
      <div class="max-w-7xl mx-auto">
        {/* Header */}
        <div class="mb-6 flex items-center justify-between">
          <div>
            <h1 class="text-3xl font-bold text-gray-900 dark:text-white">
              {project()?.name || 'Loading...'} - Sprints
            </h1>
            <p class="text-gray-600 dark:text-gray-400 mt-1">
              Manage sprints and iterations
            </p>
          </div>
          <div class="flex gap-2">
            <button
              onClick={openCreateModal}
              class="px-4 py-2 bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition-colors"
            >
              Create Sprint
            </button>
            <button
              onClick={() => navigate(`/projects/${projectId}/board`)}
              class="px-4 py-2 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
            >
              Board View
            </button>
            <button
              onClick={() => navigate('/projects')}
              class="px-4 py-2 bg-white dark:bg-gray-800 border border-gray-300 dark:border-gray-600 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
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
                <div class="bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-6">
                  <div class="flex items-start justify-between mb-4">
                    <div class="flex-1">
                      <div class="flex items-center gap-3 mb-2">
                        <h2 class="text-xl font-bold text-gray-900 dark:text-white">
                          {sprint.name}
                        </h2>
                        <span class={`px-3 py-1 text-sm font-medium rounded-full ${getStatusBadgeClass(sprint.status)}`}>
                          {sprint.status}
                        </span>
                      </div>
                      <Show when={sprint.goal}>
                        <p class="text-gray-600 dark:text-gray-400 mb-2">
                          Goal: {sprint.goal}
                        </p>
                      </Show>
                      <div class="flex items-center gap-4 text-sm text-gray-600 dark:text-gray-400">
                        <span>Start: {formatDate(sprint.start_date)}</span>
                        <span>•</span>
                        <span>End: {formatDate(sprint.end_date)}</span>
                      </div>
                    </div>
                    <div class="flex gap-2">
                      <button
                        onClick={() => openEditModal(sprint)}
                        class="px-3 py-1 text-sm bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded hover:bg-gray-200 dark:hover:bg-gray-600"
                      >
                        Edit
                      </button>
                      <Show when={sprint.status === 'planning'}>
                        <button
                          onClick={() => updateSprintStatus(sprint.id, 'active')}
                          class="px-3 py-1 text-sm bg-green-500 text-white rounded hover:bg-green-600"
                        >
                          Start Sprint
                        </button>
                      </Show>
                      <Show when={sprint.status === 'active'}>
                        <button
                          onClick={() => updateSprintStatus(sprint.id, 'review')}
                          class="px-3 py-1 text-sm bg-blue-500 text-white rounded hover:bg-blue-600"
                        >
                          Complete
                        </button>
                      </Show>
                      <Show when={sprint.status === 'review'}>
                        <button
                          onClick={() => updateSprintStatus(sprint.id, 'closed')}
                          class="px-3 py-1 text-sm bg-gray-500 text-white rounded hover:bg-gray-600"
                        >
                          Close Sprint
                        </button>
                      </Show>
                    </div>
                  </div>

                  {/* Sprint Progress */}
                  <div class="grid grid-cols-3 gap-4 mb-4">
                    <div>
                      <p class="text-sm text-gray-600 dark:text-gray-400">Items</p>
                      <p class="text-2xl font-bold text-gray-900 dark:text-white">
                        {completedItems.length} / {sprintItems.length}
                      </p>
                    </div>
                    <div>
                      <p class="text-sm text-gray-600 dark:text-gray-400">Story Points</p>
                      <p class="text-2xl font-bold text-gray-900 dark:text-white">
                        {completedPoints} / {totalPoints}
                      </p>
                    </div>
                    <div>
                      <p class="text-sm text-gray-600 dark:text-gray-400">Progress</p>
                      <p class="text-2xl font-bold text-gray-900 dark:text-white">
                        {sprintItems.length > 0 ? Math.round((completedItems.length / sprintItems.length) * 100) : 0}%
                      </p>
                    </div>
                  </div>

                  {/* Progress Bar */}
                  <div class="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2 mb-4">
                    <div
                      class="h-2 rounded-full bg-green-500"
                      style={{
                        width: `${sprintItems.length > 0 ? (completedItems.length / sprintItems.length) * 100 : 0}%`,
                      }}
                    />
                  </div>

                  {/* Sprint Items */}
                  <Show when={sprintItems.length > 0}>
                    <div class="border-t border-gray-200 dark:border-gray-700 pt-4">
                      <h3 class="text-sm font-medium text-gray-700 dark:text-gray-300 mb-2">
                        Items ({sprintItems.length})
                      </h3>
                      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-2">
                        <For each={sprintItems.slice(0, 6)}>
                          {(item) => (
                            <div class="text-sm p-2 bg-gray-50 dark:bg-gray-700 rounded border border-gray-200 dark:border-gray-600">
                              <div class="font-medium text-gray-900 dark:text-white truncate">
                                {item.title}
                              </div>
                              <div class="text-xs text-gray-600 dark:text-gray-400">
                                {item.status} • {item.estimate || 0} pts
                              </div>
                            </div>
                          )}
                        </For>
                      </div>
                      <Show when={sprintItems.length > 6}>
                        <p class="text-sm text-gray-500 dark:text-gray-400 mt-2">
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
            <div class="text-center py-12 text-gray-500 dark:text-gray-400">
              <p class="text-lg mb-4">No sprints yet</p>
              <button
                onClick={openCreateModal}
                class="px-6 py-3 bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition-colors"
              >
                Create Your First Sprint
              </button>
            </div>
          </Show>
        </div>

        {/* Backlog */}
        <Show when={getBacklogItems().length > 0}>
          <div class="mt-6 bg-white dark:bg-gray-800 rounded-lg border border-gray-200 dark:border-gray-700 p-6">
            <h2 class="text-xl font-bold text-gray-900 dark:text-white mb-4">
              Backlog ({getBacklogItems().length} items)
            </h2>
            <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">
              Items not assigned to any sprint
            </p>
            <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-2">
              <For each={getBacklogItems().slice(0, 9)}>
                {(item) => (
                  <div class="text-sm p-2 bg-gray-50 dark:bg-gray-700 rounded border border-gray-200 dark:border-gray-600">
                    <div class="font-medium text-gray-900 dark:text-white truncate">
                      {item.title}
                    </div>
                    <div class="text-xs text-gray-600 dark:text-gray-400">
                      {item.status} • {item.estimate || 0} pts
                    </div>
                  </div>
                )}
              </For>
            </div>
            <Show when={getBacklogItems().length > 9}>
              <p class="text-sm text-gray-500 dark:text-gray-400 mt-2">
                +{getBacklogItems().length - 9} more items in backlog
              </p>
            </Show>
          </div>
        </Show>
      </div>

      {/* Create/Edit Sprint Modal */}
      <Show when={showCreateModal()}>
        <div class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
          <div class="bg-white dark:bg-gray-800 rounded-lg p-6 w-full max-w-md">
            <h2 class="text-2xl font-bold text-gray-900 dark:text-white mb-4">
              {editingSprint() ? 'Edit Sprint' : 'Create Sprint'}
            </h2>
            <form onSubmit={handleSubmit} class="space-y-4">
              <div>
                <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                  Sprint Name <span class="text-red-500">*</span>
                </label>
                <input
                  type="text"
                  value={name()}
                  onInput={(e) => setName(e.currentTarget.value)}
                  placeholder="Sprint 1"
                  class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-white"
                  required
                  disabled={loading()}
                />
              </div>

              <div>
                <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                  Sprint Goal
                </label>
                <textarea
                  value={goal()}
                  onInput={(e) => setGoal(e.currentTarget.value)}
                  placeholder="What will be accomplished in this sprint?"
                  rows={3}
                  class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-white resize-none"
                  disabled={loading()}
                />
              </div>

              <div class="grid grid-cols-2 gap-4">
                <div>
                  <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    Start Date
                  </label>
                  <input
                    type="date"
                    value={startDate()}
                    onInput={(e) => setStartDate(e.currentTarget.value)}
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-white"
                    disabled={loading()}
                  />
                </div>

                <div>
                  <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
                    End Date
                  </label>
                  <input
                    type="date"
                    value={endDate()}
                    onInput={(e) => setEndDate(e.currentTarget.value)}
                    class="w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-lg bg-white dark:bg-gray-900 text-gray-900 dark:text-white"
                    disabled={loading()}
                  />
                </div>
              </div>

              <div class="flex gap-3 pt-4">
                <button
                  type="submit"
                  disabled={loading()}
                  class="flex-1 px-4 py-2 bg-blue-500 text-white rounded-lg hover:bg-blue-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                >
                  {loading() ? 'Saving...' : editingSprint() ? 'Update Sprint' : 'Create Sprint'}
                </button>
                <button
                  type="button"
                  onClick={() => setShowCreateModal(false)}
                  disabled={loading()}
                  class="px-4 py-2 bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300 rounded-lg hover:bg-gray-300 dark:hover:bg-gray-600 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
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
