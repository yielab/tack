import { createSignal, createResource, For, Show } from 'solid-js';
import { useParams, useNavigate } from '@solidjs/router';
import { api } from '../../shared/api';
import { toast } from '../../shared/ui/toast';
import { Button, Field, FieldShell, Badge, Modal } from '../../shared/ui';
import { useProject } from '../../shared/state/projectContext';
import { useVocab } from '../../shared/vocab/useVocab';
import type { Sprint, Item } from '../../types/api';

export default function Sprints() {
  const params = useParams();
  const navigate = useNavigate();
  const projectId = params.id!;

  const { project } = useProject();
  const { t } = useVocab();
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

  const statusTone = (status: string) => {
    switch (status) {
      case 'planning': return 'warning' as const;
      case 'active': return 'success' as const;
      case 'review': return 'primary' as const;
      default: return 'neutral' as const;
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
              {project()?.name || 'Loading...'} - {t('sprint')}s
            </h1>
            <p class="mt-1" style={{ color: "var(--color-text-secondary)" }}>
              Manage {t('sprint').toLowerCase()}s and iterations
            </p>
          </div>
          <div class="flex gap-2">
            <Button onClick={openCreateModal}>Create {t('sprint')}</Button>
            <Button variant="secondary" onClick={() => navigate(`/projects/${projectId}/board`)}>
              Board View
            </Button>
            <Button variant="secondary" onClick={() => navigate('/projects')}>
              Back to Projects
            </Button>
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
                        <Badge tone={statusTone(sprint.status)}>{sprint.status}</Badge>
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
                      <Button size="sm" variant="secondary" onClick={() => openEditModal(sprint)}>
                        Edit
                      </Button>
                      <Show when={sprint.status === 'planning'}>
                        <Button size="sm" variant="success" onClick={() => updateSprintStatus(sprint.id, 'active')}>
                          Start Sprint
                        </Button>
                      </Show>
                      <Show when={sprint.status === 'active'}>
                        <Button size="sm" onClick={() => updateSprintStatus(sprint.id, 'review')}>
                          Complete
                        </Button>
                      </Show>
                      <Show when={sprint.status === 'review'}>
                        <Button size="sm" variant="secondary" onClick={() => updateSprintStatus(sprint.id, 'closed')}>
                          Close Sprint
                        </Button>
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
              <p class="text-lg mb-4">No {t('sprint').toLowerCase()}s yet</p>
              <Button size="lg" onClick={openCreateModal}>Create Your First {t('sprint')}</Button>
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
      <Modal
        isOpen={showCreateModal()}
        onClose={() => setShowCreateModal(false)}
        title={editingSprint() ? `Edit ${t('sprint')}` : `Create ${t('sprint')}`}
        size="sm"
      >
        <form onSubmit={handleSubmit} class="space-y-4">
          <Field
            label={`${t('sprint')} Name`}
            required
            value={name()}
            onInput={(e) => setName(e.currentTarget.value)}
            placeholder="Sprint 1"
            disabled={loading()}
          />

          <FieldShell label={`${t('sprint')} Goal`} for="sprint-goal">
            <textarea
              id="sprint-goal"
              value={goal()}
              onInput={(e) => setGoal(e.currentTarget.value)}
              placeholder="What will be accomplished in this sprint?"
              rows={3}
              disabled={loading()}
              class="w-full resize-none rounded-lg border px-3 py-2 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1 disabled:opacity-50"
              style={{
                'background-color': 'var(--color-bg-base)',
                color: 'var(--color-text-primary)',
                'border-color': 'var(--color-border-medium)',
                '--tw-ring-color': 'var(--color-focus-ring)',
              }}
            />
          </FieldShell>

          <div class="grid grid-cols-2 gap-4">
            <Field
              label="Start Date"
              type="date"
              value={startDate()}
              onInput={(e) => setStartDate(e.currentTarget.value)}
              disabled={loading()}
            />
            <Field
              label="End Date"
              type="date"
              value={endDate()}
              onInput={(e) => setEndDate(e.currentTarget.value)}
              disabled={loading()}
            />
          </div>

          <div class="flex gap-3 pt-4">
            <Button type="submit" class="flex-1" loading={loading()} disabled={loading()}>
              {loading() ? 'Saving...' : editingSprint() ? 'Update Sprint' : 'Create Sprint'}
            </Button>
            <Button
              type="button"
              variant="secondary"
              onClick={() => setShowCreateModal(false)}
              disabled={loading()}
            >
              Cancel
            </Button>
          </div>
        </form>
      </Modal>
    </div>
  );
}
