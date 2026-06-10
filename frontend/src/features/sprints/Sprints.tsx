import { createSignal, createResource, createMemo, For, Show } from 'solid-js';
import { useParams, useSearchParams } from '@solidjs/router';
import { api } from '../../shared/api';
import { toast } from '../../shared/ui/toast';
import { Button, Field, FieldShell, Badge, Modal } from '../../shared/ui';
import { useProject } from '../../shared/state/projectContext';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import { useVocab } from '../../shared/vocab/useVocab';
import type { Sprint, Item } from '../../types/api';

// ── Types ──────────────────────────────────────────────────────────────────

type DropZone = 'backlog' | string; // 'backlog' or sprintId

// ── Helpers ────────────────────────────────────────────────────────────────

const PRIORITY_ORDER = ['critical', 'high', 'medium', 'low', 'none'];

function sortByPriority(items: Item[]) {
  return [...items].sort(
    (a, b) =>
      PRIORITY_ORDER.indexOf(a.priority) - PRIORITY_ORDER.indexOf(b.priority),
  );
}

const STATUS_TONE = {
  planning: 'warning',
  active: 'success',
  review: 'primary',
  closed: 'neutral',
} as const;

const PRIORITY_DOT: Record<string, string> = {
  critical: '#ef4444',
  high: '#f97316',
  medium: '#eab308',
  low: '#22c55e',
};

function formatDate(d?: string) {
  if (!d) return null;
  return new Date(d).toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
}

// ── Component ──────────────────────────────────────────────────────────────

export default function Sprints() {
  const params = useParams();
  const projectId = params.id!;
  const [, setSearchParams] = useSearchParams();

  const { project } = useProject();
  const { t } = useVocab();

  const [sprints, { refetch: refetchSprints }] = createResource(() =>
    api.sprints.list(projectId),
  );
  const { items, refetch: refetchItems } = useProjectItems();

  // Sprint form
  const [showModal, setShowModal] = createSignal(false);
  const [editingSprint, setEditingSprint] = createSignal<Sprint | null>(null);
  const [formName, setFormName] = createSignal('');
  const [formGoal, setFormGoal] = createSignal('');
  const [formStart, setFormStart] = createSignal('');
  const [formEnd, setFormEnd] = createSignal('');
  const [saving, setSaving] = createSignal(false);

  // Drag state
  const [dragOverZone, setDragOverZone] = createSignal<DropZone | null>(null);

  // ── Sprint helpers ─────────────────────────────────────────────────────

  const activeSprints = createMemo(() =>
    (sprints() ?? []).filter(s => s.status !== 'closed'),
  );

  const backlogItems = createMemo(() =>
    sortByPriority((items() ?? []).filter(i => !i.sprint_id)),
  );

  const itemsForSprint = (sprintId: string) =>
    sortByPriority((items() ?? []).filter(i => i.sprint_id === sprintId));

  const sprintStats = (sprintId: string) => {
    const its = itemsForSprint(sprintId);
    const total = its.length;
    const done = its.filter(i => {
      const s = project()?.workflow?.statuses?.find(ws => ws.name === i.status);
      return s?.category === 'done';
    }).length;
    const totalPts = its.reduce((n, i) => n + (i.estimate ?? 0), 0);
    const donePts  = its
      .filter(i => {
        const s = project()?.workflow?.statuses?.find(ws => ws.name === i.status);
        return s?.category === 'done';
      })
      .reduce((n, i) => n + (i.estimate ?? 0), 0);
    const pct = total > 0 ? Math.round((done / total) * 100) : 0;
    return { total, done, totalPts, donePts, pct };
  };

  // ── Sprint CRUD ────────────────────────────────────────────────────────

  const openCreate = () => {
    setEditingSprint(null);
    setFormName('');
    setFormGoal('');
    setFormStart('');
    setFormEnd('');
    setShowModal(true);
  };

  const openEdit = (s: Sprint) => {
    setEditingSprint(s);
    setFormName(s.name);
    setFormGoal(s.goal ?? '');
    setFormStart(s.start_date ? s.start_date.split('T')[0] : '');
    setFormEnd(s.end_date ? s.end_date.split('T')[0] : '');
    setShowModal(true);
  };

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    if (!formName().trim()) { toast.error('Name required'); return; }
    setSaving(true);
    try {
      const body = {
        name: formName().trim(),
        goal: formGoal().trim() || undefined,
        start_date: formStart() || undefined,
        end_date: formEnd() || undefined,
      };
      if (editingSprint()) {
        await api.sprints.update(editingSprint()!.id, body);
        toast.success('Sprint updated');
      } else {
        await api.sprints.create(projectId, body);
        toast.success('Sprint created');
      }
      setShowModal(false);
      await refetchSprints();
    } catch {
      toast.error('Failed to save sprint');
    } finally {
      setSaving(false);
    }
  };

  const updateStatus = async (sprintId: string, status: string) => {
    try {
      await api.sprints.setStatus(sprintId, status);
      toast.success(`Sprint ${status}`);
      await refetchSprints();
    } catch {
      toast.error('Failed to update sprint');
    }
  };

  // ── Drag-and-drop ──────────────────────────────────────────────────────

  const handleDragStart = (e: DragEvent, item: Item) => {
    e.dataTransfer!.effectAllowed = 'move';
    e.dataTransfer!.setData('text/plain', item.id);
  };

  const handleDragOver = (e: DragEvent, zone: DropZone) => {
    e.preventDefault();
    e.dataTransfer!.dropEffect = 'move';
    setDragOverZone(zone);
  };

  const handleDrop = async (e: DragEvent, zone: DropZone) => {
    e.preventDefault();
    setDragOverZone(null);
    const itemId = e.dataTransfer!.getData('text/plain');
    if (!itemId) return;

    const sprintId = zone === 'backlog' ? null : zone;

    // Validate: only planning/active sprints accept items
    if (sprintId !== null) {
      const sprint = (sprints() ?? []).find(s => s.id === sprintId);
      if (sprint && sprint.status !== 'planning' && sprint.status !== 'active') {
        toast.error(`Cannot add items to a ${sprint.status} sprint`);
        return;
      }
    }

    try {
      await api.items.update(itemId, { sprint_id: sprintId });
      await refetchItems();
    } catch {
      toast.error('Failed to move item');
    }
  };

  const clearDragOver = () => setDragOverZone(null);

  // ── Render ─────────────────────────────────────────────────────────────

  return (
    <div class="flex flex-col overflow-hidden" style={{ height: 'calc(100vh - 9rem)' }}>
      {/* Header */}
      <div
        class="shrink-0 px-6 py-4 border-b flex items-center justify-between"
        style={{ 'border-color': 'var(--color-border-light)', 'background-color': 'var(--color-bg-base)' }}
      >
        <div>
          <h1 class="text-xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
            Sprint Planning
          </h1>
          <p class="text-xs mt-0.5" style={{ color: 'var(--color-text-secondary)' }}>
            Drag items from the backlog into a sprint, or between sprints.
          </p>
        </div>
        <Button size="sm" onClick={openCreate}>+ New {t('sprint')}</Button>
      </div>

      {/* Two-pane board */}
      <div class="flex-1 flex overflow-hidden">
        {/* ── Left: Backlog ─────────────────────────────────────────── */}
        <div
          class="w-72 shrink-0 flex flex-col border-r overflow-hidden transition-colors"
          style={{
            'border-color': 'var(--color-border-light)',
            'background-color': dragOverZone() === 'backlog'
              ? 'var(--color-primary-50)'
              : 'var(--color-bg-subtle)',
            outline: dragOverZone() === 'backlog'
              ? '2px solid var(--color-primary-400)'
              : 'none',
            'outline-offset': '-2px',
          }}
          onDragOver={(e) => handleDragOver(e, 'backlog')}
          onDragLeave={clearDragOver}
          onDrop={(e) => handleDrop(e, 'backlog')}
        >
          <div class="px-4 py-3 border-b" style={{ 'border-color': 'var(--color-border-light)' }}>
            <span class="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
              Backlog
            </span>
            <span
              class="ml-2 text-xs px-1.5 py-0.5 rounded-full"
              style={{ 'background-color': 'var(--color-bg-base)', color: 'var(--color-text-secondary)' }}
            >
              {backlogItems().length}
            </span>
          </div>

          <div class="flex-1 overflow-y-auto p-3 space-y-2">
            <Show
              when={backlogItems().length > 0}
              fallback={
                <div class="flex items-center justify-center h-32 text-center">
                  <p class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
                    All items are assigned to sprints.
                  </p>
                </div>
              }
            >
              <For each={backlogItems()}>
                {(item) => <ItemCard item={item} onDragStart={handleDragStart} onOpen={setSearchParams} />}
              </For>
            </Show>
          </div>
        </div>

        {/* ── Right: Sprint lanes ───────────────────────────────────── */}
        <div class="flex-1 flex overflow-x-auto overflow-y-hidden">
          <Show
            when={(activeSprints()?.length ?? 0) > 0}
            fallback={
              <div class="flex-1 flex items-center justify-center">
                <div class="text-center">
                  <p class="text-3xl mb-4">🏃</p>
                  <p class="text-sm mb-4" style={{ color: 'var(--color-text-secondary)' }}>
                    No active sprints yet
                  </p>
                  <Button onClick={openCreate}>Create your first {t('sprint')}</Button>
                </div>
              </div>
            }
          >
            <div class="flex gap-4 p-4 h-full">
              <For each={activeSprints()}>
                {(sprint) => {
                  const stats = sprintStats(sprint.id);
                  const isOver = () => dragOverZone() === sprint.id;
                  const canAccept = sprint.status === 'planning' || sprint.status === 'active';

                  return (
                    <div
                      class="w-72 shrink-0 flex flex-col rounded-xl transition-colors overflow-hidden"
                      style={{
                        'background-color': isOver() && canAccept
                          ? 'var(--color-primary-50)'
                          : 'var(--color-bg-elevated)',
                        border: isOver() && canAccept
                          ? '2px solid var(--color-primary-400)'
                          : '1px solid var(--color-border-light)',
                      }}
                      onDragOver={canAccept ? (e) => handleDragOver(e, sprint.id) : undefined}
                      onDragLeave={canAccept ? clearDragOver : undefined}
                      onDrop={canAccept ? (e) => handleDrop(e, sprint.id) : undefined}
                    >
                      {/* Sprint header */}
                      <div class="px-4 pt-4 pb-3 border-b" style={{ 'border-color': 'var(--color-border-light)' }}>
                        <div class="flex items-start justify-between mb-1">
                          <span class="font-semibold text-sm truncate" style={{ color: 'var(--color-text-primary)' }}>
                            {sprint.name}
                          </span>
                          <Badge tone={STATUS_TONE[sprint.status as keyof typeof STATUS_TONE] ?? 'neutral'}>
                            {sprint.status}
                          </Badge>
                        </div>

                        <Show when={sprint.start_date || sprint.end_date}>
                          <p class="text-xs mb-2" style={{ color: 'var(--color-text-tertiary)' }}>
                            {formatDate(sprint.start_date) ?? '?'} → {formatDate(sprint.end_date) ?? '?'}
                          </p>
                        </Show>

                        {/* Capacity row */}
                        <div class="flex items-center justify-between text-xs mb-1.5">
                          <span style={{ color: 'var(--color-text-secondary)' }}>
                            {stats.done}/{stats.total} items · {stats.donePts}/{stats.totalPts} pts
                          </span>
                          <span style={{ color: 'var(--color-text-secondary)' }}>{stats.pct}%</span>
                        </div>
                        <div class="h-1.5 rounded-full overflow-hidden" style={{ 'background-color': 'var(--color-bg-subtle)' }}>
                          <div
                            class="h-full rounded-full transition-all"
                            style={{
                              width: `${stats.pct}%`,
                              'background-color': stats.pct === 100
                                ? 'var(--color-success-500)'
                                : 'var(--color-primary-500)',
                            }}
                          />
                        </div>

                        {/* Sprint actions */}
                        <div class="flex gap-1 mt-2">
                          <button
                            class="text-xs px-2 py-1 rounded"
                            style={{ 'background-color': 'var(--color-bg-subtle)', color: 'var(--color-text-secondary)' }}
                            onClick={() => openEdit(sprint)}
                          >
                            Edit
                          </button>
                          <Show when={sprint.status === 'planning'}>
                            <button
                              class="text-xs px-2 py-1 rounded"
                              style={{ 'background-color': 'var(--color-success-light)', color: 'var(--color-success)' }}
                              onClick={() => updateStatus(sprint.id, 'active')}
                            >
                              Start
                            </button>
                          </Show>
                          <Show when={sprint.status === 'active'}>
                            <button
                              class="text-xs px-2 py-1 rounded"
                              style={{ 'background-color': 'var(--color-primary-100)', color: 'var(--color-primary-700)' }}
                              onClick={() => updateStatus(sprint.id, 'review')}
                            >
                              Complete
                            </button>
                          </Show>
                          <Show when={sprint.status === 'review'}>
                            <button
                              class="text-xs px-2 py-1 rounded"
                              style={{ 'background-color': 'var(--color-bg-subtle)', color: 'var(--color-text-secondary)' }}
                              onClick={() => updateStatus(sprint.id, 'closed')}
                            >
                              Close
                            </button>
                          </Show>
                        </div>
                      </div>

                      {/* Items */}
                      <div class="flex-1 overflow-y-auto p-3 space-y-2">
                        <Show
                          when={itemsForSprint(sprint.id).length > 0}
                          fallback={
                            <div
                              class="flex items-center justify-center h-20 rounded-lg border-2 border-dashed text-xs"
                              style={{
                                'border-color': 'var(--color-border-medium)',
                                color: 'var(--color-text-tertiary)',
                              }}
                            >
                              {canAccept ? 'Drop items here' : 'Sprint is closed'}
                            </div>
                          }
                        >
                          <For each={itemsForSprint(sprint.id)}>
                            {(item) => <ItemCard item={item} onDragStart={handleDragStart} onOpen={setSearchParams} />}
                          </For>
                        </Show>
                      </div>
                    </div>
                  );
                }}
              </For>
            </div>
          </Show>
        </div>
      </div>

      {/* Sprint modal */}
      <Modal
        isOpen={showModal()}
        onClose={() => setShowModal(false)}
        title={editingSprint() ? `Edit ${t('sprint')}` : `New ${t('sprint')}`}
        size="sm"
      >
        <form onSubmit={handleSubmit} class="space-y-4">
          <Field
            label="Name"
            required
            value={formName()}
            onInput={(e) => setFormName(e.currentTarget.value)}
            placeholder="Sprint 1"
            disabled={saving()}
          />
          <FieldShell label="Goal" for="sprint-goal">
            <textarea
              id="sprint-goal"
              value={formGoal()}
              onInput={(e) => setFormGoal(e.currentTarget.value)}
              placeholder="What will be accomplished?"
              rows={2}
              disabled={saving()}
              class="w-full resize-none rounded-lg border px-3 py-2 text-sm transition-colors focus:outline-none focus-visible:ring-2 disabled:opacity-50"
              style={{
                'background-color': 'var(--color-bg-base)',
                color: 'var(--color-text-primary)',
                'border-color': 'var(--color-border-medium)',
              }}
            />
          </FieldShell>
          <div class="grid grid-cols-2 gap-3">
            <Field label="Start" type="date" value={formStart()} onInput={(e) => setFormStart(e.currentTarget.value)} disabled={saving()} />
            <Field label="End"   type="date" value={formEnd()}   onInput={(e) => setFormEnd(e.currentTarget.value)}   disabled={saving()} />
          </div>
          <div class="flex gap-2 pt-2">
            <Button type="submit" class="flex-1" loading={saving()} disabled={saving()}>
              {editingSprint() ? 'Update' : 'Create'}
            </Button>
            <Button type="button" variant="secondary" onClick={() => setShowModal(false)} disabled={saving()}>
              Cancel
            </Button>
          </div>
        </form>
      </Modal>
    </div>
  );
}

// ── Item card (shared by backlog + sprint lanes) ───────────────────────────

function ItemCard(props: {
  item: Item;
  onDragStart: (e: DragEvent, item: Item) => void;
  onOpen: (params: Record<string, string>) => void;
}) {
  return (
    <div
      draggable={true}
      onDragStart={(e) => props.onDragStart(e, props.item)}
      onClick={() => props.onOpen({ item: props.item.id })}
      class="rounded-lg px-3 py-2 cursor-grab active:cursor-grabbing hover:opacity-90 transition-opacity select-none"
      style={{
        'background-color': 'var(--color-bg-base)',
        border: '1px solid var(--color-border-light)',
      }}
    >
      <div class="flex items-start gap-2">
        <div
          class="mt-1 w-2 h-2 rounded-full shrink-0"
          style={{ 'background-color': PRIORITY_DOT[props.item.priority] ?? '#9ca3af' }}
          title={props.item.priority}
        />
        <div class="flex-1 min-w-0">
          <p class="text-xs font-medium truncate" style={{ color: 'var(--color-text-primary)' }}>
            {props.item.title}
          </p>
          <div class="flex items-center gap-2 mt-0.5">
            <span class="text-xs" style={{ color: 'var(--color-text-tertiary)' }}>
              {props.item.status}
            </span>
            <Show when={props.item.estimate}>
              <span class="text-xs" style={{ color: 'var(--color-primary-600)' }}>
                {props.item.estimate} pts
              </span>
            </Show>
          </div>
        </div>
      </div>
    </div>
  );
}
