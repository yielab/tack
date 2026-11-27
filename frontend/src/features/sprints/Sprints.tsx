import { createSignal, createResource, createMemo, For, Show } from 'solid-js';
import { ForById } from '../../shared/ui/ForById';
import { useParams, useSearchParams } from '@solidjs/router';
import { api } from '../../shared/api';
import { toast } from '../../shared/ui/toast';
import { Button, Field, FieldShell, Badge, Modal, EmptyState, Card } from '../../shared/ui';
import { IconSprint } from '../../shared/ui/icons';
import { useProject } from '../../shared/state/projectContext';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import { useExecutionStore } from '../../shared/state/executionContext';
import { cardRun, newestRunByItem, type CardRun } from '../../shared/runWithAgent/cardRun';
import { NotReadyLine, RiskChip, notReadyOf } from '../../shared/ui/briefSummary';
import { useVocab } from '../../shared/vocab/useVocab';
import { priorityColor } from '../../shared/ui/PriorityDot';
import RunWithAgentButton from '../../shared/runWithAgent/RunWithAgentButton';
import type { Sprint, Item } from '../../shared/types';
import GroupHeader from '../../shared/ui/GroupHeader';
import { ancestorsOf, treeRows } from '../../shared/tree';

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

/** What the factory figures say about each sprint's decisions; the API serves a row only for a sprint that had some. */
type FactoryByDecision = Awaited<ReturnType<typeof api.metrics.factoryMetrics>> & {
  human_minutes_by_sprint?: { sprint_id: string; minutes: { median_minutes?: number | null } }[];
};

/** The median minutes a decision took, in words: "14 min", or "under 1 min" for a quick one. */
const minutesWords = (minutes: number) => (minutes < 1 ? 'under 1 min' : `${Math.round(minutes)} min`);

function formatDate(d?: string | null) {
  if (!d) return null;
  return new Date(d).toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
}

// ── Component ──────────────────────────────────────────────────────────────

export default function Sprints() {
  const params = useParams();
  const projectId = params.id!;
  const [, setSearchParams] = useSearchParams();

  const { project, vocabulary } = useProject();
  const { t } = useVocab();
  const pointsWord = () => vocabulary()?.story_points?.toLowerCase();

  const [sprints, { refetch: refetchSprints }] = createResource(() =>
    api.sprints.list(projectId),
  );
  const { items, refetch: refetchItems } = useProjectItems();
  const executions = useExecutionStore();
  const [factory] = createResource(() => api.metrics.factoryMetrics(projectId).catch(() => null));

  // Each item's newest run, from the store the cards already watch: a sprint counts the ones that wait for a person.
  const newestRuns = createMemo(() => {
    const summaries = [];
    for (const record of executions.requests().values()) if (record.summary) summaries.push(record.summary);
    return newestRunByItem(summaries);
  });
  const runOf = (item: Item): CardRun | undefined => cardRun(newestRuns().get(item.id), item.needs_review === true);

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

  // A sprint holds work: a group is planned through its items, so it is never a card here.
  const work = createMemo(() => (items() ?? []).filter(i => i.node === 'work'));
  const itemsById = createMemo(() => new Map((items() ?? []).map(i => [i.id, i])));

  const backlogItems = createMemo(() =>
    sortByPriority(work().filter(i => !i.sprint_id)),
  );

  const itemsForSprint = (sprintId: string) =>
    sortByPriority(work().filter(i => i.sprint_id === sprintId));

  const hasGroups = createMemo(() => (items() ?? []).some(i => i.node === 'group'));

  // The backlog under the group each piece of work sits in, in the tree's order, then the work with no group.
  const backlogSections = createMemo(() => {
    const byId = itemsById();
    const groupOf = (item: Item) => ancestorsOf(item, byId).reverse().find(a => a.node === 'group');
    const inGroup = new Map<string, Item[]>();
    const loose: Item[] = [];
    for (const item of backlogItems()) {
      const group = groupOf(item);
      if (!group) { loose.push(item); continue; }
      inGroup.set(group.id, [...(inGroup.get(group.id) ?? []), item]);
    }
    const groups = treeRows((items() ?? []).filter(i => i.node === 'group'))
      .map(({ item }) => item)
      .filter(group => inGroup.has(group.id))
      .map(group => ({ group, items: inGroup.get(group.id)! }));
    return { groups, loose };
  });

  /** What a sprint will ask of a person: its tasks that wait for one, and how long a decision has taken. */
  const sprintDecisions = (sprintId: string) => {
    const waiting = itemsForSprint(sprintId).filter((item) => {
      const state = runOf(item)?.state;
      return state === 'needs' || state === 'review' || state === 'failed';
    }).length;
    const median = (factory() as FactoryByDecision | null | undefined)?.human_minutes_by_sprint
      ?.find((row) => row.sprint_id === sprintId)?.minutes.median_minutes;
    return { waiting, median: median ?? null };
  };

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
        toast.success(`${t('sprint')} updated`);
      } else {
        await api.sprints.create(projectId, body);
        toast.success(`${t('sprint')} created`);
      }
      setShowModal(false);
      await refetchSprints();
    } catch {
      toast.error(`Failed to save ${t('sprint').toLowerCase()}`);
    } finally {
      setSaving(false);
    }
  };

  const updateStatus = async (sprintId: string, status: string) => {
    try {
      await api.sprints.setStatus(sprintId, status);
      toast.success(`${t('sprint')} ${status}`);
      await refetchSprints();
    } catch {
      toast.error(`Failed to update ${t('sprint').toLowerCase()}`);
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
        toast.error(`Cannot add items to a ${sprint.status} ${t('sprint').toLowerCase()}`);
        return;
      }
    }

    try {
      await api.items.update(itemId, { sprint_id: sprintId });
      await refetchItems();
    } catch (error) {
      // The server says why a sprint refuses an item (a group does not go in one), in words for a person.
      toast.error(error instanceof Error && error.message ? error.message : 'Failed to move item');
    }
  };

  const clearDragOver = () => setDragOverZone(null);

  // ── Render ─────────────────────────────────────────────────────────────

  return (
    <div class="flex flex-col gap-3.5 overflow-hidden" style={{ height: 'calc(100vh - 9rem)' }}>
      {/* Header */}
      <div class="shrink-0 flex items-end gap-3">
        <div>
          <h1 class="text-title m-0 text-content">
            {t('sprint')} Planning
          </h1>
          <p class="mt-1 text-small text-content-muted">
            Drag items from the {t('backlog').toLowerCase()} into a {t('sprint').toLowerCase()}, or between {t('sprint').toLowerCase()}s.
          </p>
        </div>
        <Button class="ml-auto" onClick={openCreate}>+ New {t('sprint')}</Button>
      </div>

      {/* Two-pane board */}
      <div class="flex-1 min-h-0 flex gap-3.5 overflow-hidden">
        {/* ── Left: Backlog ─────────────────────────────────────────── */}
        <div
          class={`${hasGroups() ? 'w-96' : 'w-72'} shrink-0 flex flex-col gap-2 p-3.5 rounded-lg overflow-hidden transition-colors`}
          style={{
            'background-color': 'var(--color-accent2-soft)',
            outline: dragOverZone() === 'backlog'
              ? '2px dashed var(--color-primary-600)'
              : 'none',
            'outline-offset': '-2px',
          }}
          onDragOver={(e) => handleDragOver(e, 'backlog')}
          onDragLeave={clearDragOver}
          onDrop={(e) => handleDrop(e, 'backlog')}
        >
          <div class="flex items-center gap-2">
            <h2 class="text-section" style={{ color: 'var(--color-accent2-ink)' }}>
              {t('backlog')}
            </h2>
            <Badge>{backlogItems().length}</Badge>
          </div>

          <div
            class="flex-1 overflow-y-auto flex flex-col gap-2"
            tabindex="0"
            aria-label={`${t('backlog')} items`}
          >
            <Show
              when={backlogItems().length > 0}
              fallback={
                <div class="flex items-center justify-center h-32 text-center">
                  <p class="text-xs text-content-muted">
                    All items are assigned to {t('sprint').toLowerCase()}s.
                  </p>
                </div>
              }
            >
              <Show
                when={hasGroups()}
                fallback={
                  <ForById each={backlogItems()}>
                    {(item) => <ItemCard item={item()} onDragStart={handleDragStart} onOpen={setSearchParams} />}
                  </ForById>
                }
              >
                <For each={backlogSections().groups}>
                  {(section) => (
                    <GroupHeader
                      group={section.group}
                      byId={itemsById()}
                      density="heading"
                      statuses={project()?.workflow?.statuses ?? []}
                      pointsWord={pointsWord()}
                    >
                      <div class="flex flex-col gap-2 pb-2">
                        <ForById each={section.items}>
                          {(item) => <ItemCard item={item()} onDragStart={handleDragStart} onOpen={setSearchParams} />}
                        </ForById>
                      </div>
                    </GroupHeader>
                  )}
                </For>
                <Show when={backlogSections().loose.length > 0}>
                  <section aria-label="No parent" data-testid="no-parent" class="flex flex-col gap-2">
                    <h3 class="font-sans text-small font-bold text-content-muted" style={{ padding: '8px 0' }}>No parent</h3>
                    <ForById each={backlogSections().loose}>
                      {(item) => <ItemCard item={item()} onDragStart={handleDragStart} onOpen={setSearchParams} />}
                    </ForById>
                  </section>
                </Show>
              </Show>
            </Show>
          </div>
        </div>

        {/* ── Right: Sprint lanes ───────────────────────────────────── */}
        <div
          class="flex-1 min-w-0 flex overflow-x-auto overflow-y-hidden"
          tabindex="0"
          aria-label={`${t('sprint')} lanes`}
        >
          <Show
            when={(activeSprints()?.length ?? 0) > 0}
            fallback={
              <div class="flex-1 flex items-center justify-center">
                <Card class="px-8">
                  <EmptyState
                    icon={<IconSprint size={28} />}
                    title={`No active ${t('sprint').toLowerCase()}s yet`}
                    action={<Button variant="secondary" size="sm" onClick={openCreate}>Create your first {t('sprint')}</Button>}
                  />
                </Card>
              </div>
            }
          >
            <div class="flex gap-3.5 h-full">
              <For each={activeSprints()}>
                {(sprint) => {
                  const stats = () => sprintStats(sprint.id);
                  const isOver = () => dragOverZone() === sprint.id;
                  const canAccept = sprint.status === 'planning' || sprint.status === 'active';

                  return (
                    <Card
                      class="w-72 shrink-0 flex flex-col gap-2 p-3.5 overflow-hidden transition-colors"
                      style={{
                        outline: isOver() && canAccept
                          ? '2px dashed var(--color-primary-600)'
                          : 'none',
                        'outline-offset': '-2px',
                      }}
                      onDragOver={canAccept ? (e) => handleDragOver(e, sprint.id) : undefined}
                      onDragLeave={canAccept ? clearDragOver : undefined}
                      onDrop={canAccept ? (e) => handleDrop(e, sprint.id) : undefined}
                    >
                      {/* Sprint header */}
                      <div class="flex flex-col gap-2">
                        <div class="flex items-center gap-2 min-w-0">
                          <h2 class="text-section truncate text-content">
                            {sprint.name}
                          </h2>
                          <Badge tone={STATUS_TONE[sprint.status as keyof typeof STATUS_TONE] ?? 'neutral'}>
                            {sprint.status}
                          </Badge>
                        </div>

                        <Show when={sprint.start_date || sprint.end_date}>
                          <p class="font-mono text-[0.71875rem] text-content-muted">
                            {formatDate(sprint.start_date) ?? '?'} → {formatDate(sprint.end_date) ?? '?'}
                          </p>
                        </Show>

                        {/* Capacity row */}
                        <div class="flex items-center justify-between text-small text-content">
                          <span>
                            {stats().done}/{stats().total} items · {stats().donePts}/{stats().totalPts} pts
                          </span>
                          <span class="font-bold">{stats().pct}%</span>
                        </div>
                        <Show when={sprintDecisions(sprint.id).waiting > 0 || sprintDecisions(sprint.id).median != null}>
                          <p data-testid="sprint-decisions" class="m-0 text-small font-semibold" style={{ color: 'var(--color-text-secondary)' }}>
                            <Show when={sprintDecisions(sprint.id).waiting > 0}>
                              <span style={{ color: 'var(--run-needs-ink)' }}>{sprintDecisions(sprint.id).waiting} waiting for you</span>
                            </Show>
                            <Show when={sprintDecisions(sprint.id).waiting > 0 && sprintDecisions(sprint.id).median != null}>
                              {' · '}
                            </Show>
                            <Show when={sprintDecisions(sprint.id).median}>
                              {(median) => <span>a decision takes {minutesWords(median())}</span>}
                            </Show>
                          </p>
                        </Show>
                        <div class="h-1.5 rounded-full overflow-hidden bg-app">
                          <div
                            class="h-full rounded-full transition-all"
                            style={{
                              width: `${stats().pct}%`,
                              'background-color': stats().pct === 100
                                ? 'var(--color-success-600)'
                                : 'var(--color-primary-600)',
                            }}
                          />
                        </div>

                        {/* Sprint actions */}
                        <div class="flex gap-1.5">
                          <Button variant="ghost" size="sm" onClick={() => openEdit(sprint)}>
                            Edit
                          </Button>
                          <Show when={sprint.status === 'planning'}>
                            <Button variant="primary" size="sm" onClick={() => updateStatus(sprint.id, 'active')}>
                              Start
                            </Button>
                          </Show>
                          <Show when={sprint.status === 'active'}>
                            <Button variant="primary" size="sm" onClick={() => updateStatus(sprint.id, 'review')}>
                              Complete
                            </Button>
                          </Show>
                          <Show when={sprint.status === 'review'}>
                            <Button variant="secondary" size="sm" onClick={() => updateStatus(sprint.id, 'closed')}>
                              Close
                            </Button>
                          </Show>
                        </div>
                      </div>

                      {/* Items */}
                      <div
                        class="flex-1 overflow-y-auto flex flex-col gap-2"
                        tabindex="0"
                        aria-label={`${sprint.name} items`}
                      >
                        <Show
                          when={itemsForSprint(sprint.id).length > 0}
                          fallback={
                            <div class="flex items-center justify-center p-3.5 rounded-lg border-2 border-dashed border-line text-center text-xs text-content-muted">
                              {canAccept ? 'Drop items here' : `${t('sprint')} is closed`}
                            </div>
                          }
                        >
                          <ForById each={itemsForSprint(sprint.id)}>
                            {(item) => <ItemCard item={item()} onDragStart={handleDragStart} onOpen={setSearchParams} />}
                          </ForById>
                        </Show>
                      </div>
                    </Card>
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
            placeholder={`${t('sprint')} 1`}
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
              class="w-full resize-none rounded-md border px-3.5 py-2 text-sm transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-offset-1 disabled:opacity-50"
              style={{
                'background-color': 'var(--color-bg-base)',
                color: 'var(--color-text-primary)',
                'border-color': 'var(--color-border-medium)',
                '--tw-ring-color': 'var(--color-focus-ring)',
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
      class="shrink-0 flex flex-col gap-2 rounded-lg border border-line bg-surface px-3.5 py-3 hover:border-[var(--color-accent-line)] cursor-grab active:cursor-grabbing transition-[box-shadow,border-color] select-none shadow-[var(--shadow-sm)] hover:shadow-[var(--shadow-md)]"
    >
      <div class="flex items-start gap-2">
        <div
          class="mt-1.5 w-2 h-2 rounded-sm shrink-0"
          style={{ 'background-color': priorityColor(props.item.priority) }}
          title={props.item.priority}
        />
        <p class="flex-1 min-w-0 text-lead font-semibold break-words text-content">
          {/* Keyboard entry: its click bubbles to the card's own handler. */}
          <button type="button" class="m-0 block min-h-6 max-w-full cursor-pointer break-words border-0 bg-transparent p-0 text-left" style={{ font: 'inherit', color: 'inherit' }}>
            {props.item.title}
          </button>
        </p>
      </div>
      <Show when={notReadyOf(props.item)}>
        {(reason) => <NotReadyLine reason={reason()} />}
      </Show>
      <div class="flex items-center gap-2 text-small text-content-muted">
        <span>{props.item.status}</span>
        <RiskChip item={props.item} />
        <Show when={props.item.estimate}>
          <span class="font-bold text-accent-ink">{props.item.estimate} pts</span>
        </Show>
        {/* The one solid control of a card: the same run the board's card starts. */}
        <span class="ml-auto" onClick={(e) => e.stopPropagation()}>
          <RunWithAgentButton
            itemId={props.item.id}
            itemTitle={props.item.title}
            projectId={props.item.project_id}
            label="Run"
          />
        </span>
      </div>
    </div>
  );
}
