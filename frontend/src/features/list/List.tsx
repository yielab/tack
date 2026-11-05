import { createSignal, createEffect, For, Show, Switch, Match, createMemo, onMount, onCleanup, type JSX } from 'solid-js';
import MergeNoteBadge from '../../shared/runWithAgent/MergeNoteBadge';
import AutomationBadge from '../../shared/ui/AutomationBadge';
import { useParams, useSearchParams } from '@solidjs/router';
import {
  DragDropProvider,
  DragDropSensors,
  SortableProvider,
  createSortable,
  closestCenter
} from '@thisbeyond/solid-dnd';
import { api } from '../../shared/api';
import { toast } from '../../shared/ui/toast';
import { useItemTypes } from '../../shared/vocab/useItemTypes';
import { useProject } from '../../shared/state/projectContext';
import { useProjectItems } from '../../shared/state/projectItemsContext';
import { useExecutionStore } from '../../shared/state/executionContext';
import { useVocab } from '../../shared/vocab/useVocab';
import type { Item, ItemTypeDef } from '../../shared/types';
import { Button, Badge, EmptyState, RunState, createConfirm } from '../../shared/ui';
import { RUN_STATE_LABEL } from '../../shared/ui/RunState';
import { IconList } from '../../shared/ui/icons';
import { priorityColor } from '../../shared/ui/PriorityDot';
import { typeTone as toneOf } from '../../shared/ui/TypeBadge';
import type { Priority } from '../../shared/types';
import { ITEM_UPDATED_EVENT } from '../../shared/state/itemEvents';
import {
  cardRun, newestRunByItem, passesFilters, type CardRun, type RunFilter,
} from '../../shared/runWithAgent/cardRun';
import { FiPlus, FiMenu, FiCheck, FiX, FiChevronRight, FiChevronDown, FiTrash2, FiMaximize2, FiMinimize2, FiEdit2 } from 'solid-icons/fi';
import Card from '../../shared/ui/Card';
import {
  COLUMNS, PRIORITIES as EDIT_PRIORITIES, buildTree, currentRaw, editPatch, emptyColumns, matchesQuery, pruneTree, visibleRows,
  type ColDef, type EditableKey, type ItemNode, type SortDir, type SortKey,
} from './listRows';
import { hasColumnChoice, loadDensity, loadHiddenColumns, saveDensity, saveHiddenColumns, type Density } from './listPrefs';


// What a new item can be given; "none" is only for editing an item that has one.
const PRIORITIES = [
  { value: 'critical', emoji: '🔥', label: 'Critical' },
  { value: 'high', emoji: '⬆️', label: 'High' },
  { value: 'medium', emoji: '➡️', label: 'Medium' },
  { value: 'low', emoji: '⬇️', label: 'Low' },
];

const priorityLabel = (p: string) => p.charAt(0).toUpperCase() + p.slice(1);

/** The work a filter can ask for, in the words the board uses. */
const RUN_FILTERS: { value: RunFilter; label: string }[] = [
  { value: 'waiting', label: 'Your turn' },
  { value: 'queued', label: RUN_STATE_LABEL.queued },
  { value: 'running', label: RUN_STATE_LABEL.running },
  { value: 'needs', label: RUN_STATE_LABEL.needs },
  { value: 'review', label: RUN_STATE_LABEL.review },
  { value: 'failed', label: RUN_STATE_LABEL.failed },
  { value: 'notstarted', label: RUN_STATE_LABEL.notstarted },
];

/** Space between rows and inside them, by density. */
const DENSITY_STYLE: Record<Density, { gap: string; pad: string }> = {
  comfortable: { gap: '0.625rem', pad: '0.625rem' },
  compact: { gap: '0.25rem', pad: '0.25rem' },
};

/** What the cells need to start, finish or abandon an in-place edit. */
interface EditApi {
  is: (id: string, key: EditableKey) => boolean;
  start: (id: string, key: EditableKey) => void;
  commit: (item: Item, key: EditableKey, raw: string) => void;
  cancel: (id: string, key: EditableKey) => void;
  saving: (id: string) => boolean;
  statuses: () => string[];
}

export default function List() {
  const params = useParams();
  const projectId = params.id;

  const { items, refetch } = useProjectItems();
  const { project } = useProject();
  const { t } = useVocab();
  const itemTypes = useItemTypes();
  const types = createMemo(() => itemTypes.list());
  const executions = useExecutionStore();

  const [, setSearchParams] = useSearchParams();
  const [expandedItems, setExpandedItems] = createSignal<Set<string>>(new Set());
  const [creatingAt, setCreatingAt] = createSignal<{ parentId?: string } | null>(null);
  const [newItemTitle, setNewItemTitle] = createSignal('');
  const [newItemType, setNewItemType] = createSignal('');
  const [newItemPriority, setNewItemPriority] = createSignal('medium');

  const [sort, setSort] = createSignal<{ key: SortKey; dir: SortDir } | null>(null);
  const [query, setQuery] = createSignal('');
  const [runFilter, setRunFilter] = createSignal<RunFilter | ''>('');
  // Until the person picks columns, the ones no item has a value in stay hidden.
  const [chosen, setChosen] = createSignal(hasColumnChoice());
  const [hidden, setHidden] = createSignal(loadHiddenColumns());
  const [density, setDensity] = createSignal<Density>(loadDensity());
  const [columnsOpen, setColumnsOpen] = createSignal(false);
  const [editing, setEditing] = createSignal<{ id: string; key: EditableKey } | null>(null);
  const [saving, setSaving] = createSignal<string | null>(null);

  // Every item's newest run, from the store the board reads. The List asks about
  // all of its items, not just the open rows, so the run filter can see them.
  const newestRuns = createMemo(() => {
    const summaries = [];
    for (const record of executions.requests().values()) if (record.summary) summaries.push(record.summary);
    return newestRunByItem(summaries);
  });
  const runOf = (item: Item): CardRun | undefined => cardRun(newestRuns().get(item.id), item.needs_review === true);

  const watched = new Map<string, () => void>();
  createEffect(() => {
    const ids = new Set((items() ?? []).map((i) => i.id));
    for (const [id, unwatch] of watched) {
      if (!ids.has(id)) {
        unwatch();
        watched.delete(id);
      }
    }
    for (const id of ids) if (!watched.has(id)) watched.set(id, executions.watchItem(id));
  });
  onCleanup(() => {
    for (const unwatch of watched.values()) unwatch();
    watched.clear();
  });

  const filtering = () => query().trim() !== '' || runFilter() !== '';
  const keep = (item: Item) => {
    const run = runFilter();
    return matchesQuery(item, query()) && (run === '' || passesFilters(runOf(item), new Set([run])));
  };

  const shownHidden = createMemo(() =>
    chosen() ? hidden() : new Set([...hidden(), ...emptyColumns(items() ?? [], runOf)]),
  );

  // Sorting reorders each set of siblings; a filter keeps the parents of what it lets through.
  const organizedItems = createMemo(() => {
    const current = sort();
    const tree = buildTree(items() ?? [], current?.key ?? null, current?.dir ?? 'asc', runOf);
    return filtering() ? pruneTree(tree, keep) : tree;
  });

  // A filter opens every parent that holds a match, so none is hidden behind a closed row.
  const isOpen = (id: string) => filtering() || expandedItems().has(id);

  const flattenedItems = createMemo(() => visibleRows(organizedItems(), isOpen));

  const allExpandableIds = createMemo(() => {
    const ids: string[] = [];
    const collect = (nodes: ItemNode[]) => {
      for (const node of nodes) {
        if (node.children.length > 0) {
          ids.push(node.id);
          collect(node.children);
        }
      }
    };
    collect(organizedItems());
    return ids;
  });

  const matchCount = createMemo(() => (items() ?? []).filter(keep).length);

  const expandAll = () => setExpandedItems(new Set(allExpandableIds()));
  const collapseAll = () => setExpandedItems(new Set<string>());

  const toggleExpand = (itemId: string) => {
    setExpandedItems(prev => {
      const newSet = new Set(prev);
      if (newSet.has(itemId)) {
        newSet.delete(itemId);
      } else {
        newSet.add(itemId);
      }
      return newSet;
    });
  };

  // A column sorts ascending, then descending, then goes back to creation order.
  const toggleSort = (key: SortKey) => {
    setSort((prev) => {
      if (prev?.key !== key) return { key, dir: 'asc' };
      return prev.dir === 'asc' ? { key, dir: 'desc' } : null;
    });
  };

  const visibleColumns = createMemo(() => COLUMNS.filter((c) => !shownHidden().has(c.key)));
  const headerLabel = (c: ColDef) => (c.vocabKey ? t(c.vocabKey) : c.label);

  // The title takes what the other columns leave; the list scrolls inside itself when they do not fit.
  const gridColumns = createMemo(
    () => [...visibleColumns().map((c) => (c.key === 'title' ? `minmax(${c.rem}rem, 1fr)` : `${c.rem}rem`)), '4.5rem'].join(' '),
  );
  const minWidth = createMemo(() => `${visibleColumns().reduce((n, c) => n + c.rem, 4.5) + visibleColumns().length * 0.5 + 1.75}rem`);

  const toggleColumn = (key: ColDef['key']) => {
    setHidden(() => {
      const next = new Set(shownHidden());
      if (!next.delete(key)) next.add(key);
      setChosen(true);
      saveHiddenColumns(next);
      // A hidden column cannot say it is the one sorting.
      if (next.has(key)) setSort((prev) => (prev?.key === key ? null : prev));
      return next;
    });
  };

  const toggleDensity = () => {
    setDensity((d) => {
      const next: Density = d === 'compact' ? 'comfortable' : 'compact';
      saveDensity(next);
      return next;
    });
  };

  const statuses = createMemo(() => project()?.workflow?.statuses?.map((s) => s.name) ?? []);

  // Keyboard users land back on the cell they edited, unless they have already moved on.
  const refocusCell = (id: string, key: EditableKey) =>
    queueMicrotask(() => {
      const now = document.activeElement;
      if (now && now !== document.body && now.isConnected) return;
      document.querySelector<HTMLElement>(`[data-cell="${id}:${key}"]`)?.focus();
    });

  const edit: EditApi = {
    is: (id, key) => editing()?.id === id && editing()?.key === key,
    start: (id, key) => setEditing({ id, key }),
    saving: (id) => saving() === id,
    statuses,
    cancel: (id, key) => {
      setEditing(null);
      refocusCell(id, key);
    },
    commit: async (item, key, raw) => {
      // Enter and the blur it causes both arrive here; only the first one counts.
      if (!edit.is(item.id, key)) return;
      setEditing(null);
      refocusCell(item.id, key);
      const patch = editPatch(item, key, raw);
      if (!patch) return;
      setSaving(item.id);
      try {
        await api.items.update(item.id, patch);
        await refetch();
        // The refetch draws the rows again, and the cell with them.
        refocusCell(item.id, key);
      } catch (e) {
        toast.error(e instanceof Error ? e.message : 'Failed to update item');
      } finally {
        setSaving(null);
      }
    },
  };

  const startCreating = (parentId?: string) => {
    setCreatingAt({ parentId });
    setNewItemTitle('');
    setNewItemType(itemTypes.defaultKey());
    setNewItemPriority('medium');
  };
  const cancelCreating = () => {
    setCreatingAt(null);
    setNewItemTitle('');
  };

  const handleCreate = async () => {
    const title = newItemTitle().trim();
    if (!title) return;

    const creating = creatingAt();
    if (!creating || !projectId) return;

    try {
      await api.items.create(projectId, {
        title,
        item_type: newItemType(),
        priority: newItemPriority() as any,
        parent_id: creating.parentId,
      });
      toast.success('Item created');
      await refetch();
      setCreatingAt(null);
      setNewItemTitle('');

      if (creating.parentId) {
        setExpandedItems(prev => new Set([...prev, creating.parentId!]));
      }
    } catch (err) {
      toast.error('Failed to create item');
    }
  };

  const [ask, ConfirmHost] = createConfirm();

  const handleDelete = async (item: Item) => {
    if (!(await ask({ title: 'Delete item?', message: `"${item.title}" will be deleted.`, confirmLabel: 'Delete', danger: true }))) return;
    try {
      await api.items.remove(item.id);
      toast.success('Deleted');
      await refetch();
    } catch {
      toast.error('Failed to delete');
    }
  };

  const handleMove = (_itemId: string, _newParentId: string | null) => {
    // Re-parenting is not supported by the API: `PATCH /items/:id` (UpdateItem in
    // the OpenAPI contract) has no `parent_id` field, so the backend would
    // silently ignore the change. Surface that honestly rather than showing a
    // misleading "moved" toast. (Re-parenting needs a backend UpdateItem change.)
    toast.info("Re-parenting items isn't supported yet");
  };

  const onDragEnd = ({ draggable, droppable }: any) => {
    if (draggable && droppable && draggable.id !== droppable.id) {
      const draggedItem = flattenedItems().find(i => i.id === draggable.id);
      const targetItem = flattenedItems().find(i => i.id === droppable.id);

      if (draggedItem && targetItem) {
        handleMove(draggedItem.id, targetItem.parent_id || null);
      }
    }
  };

  // Open the item detail drawer (deep-linkable via ?item=).
  const handleViewItem = (item: Item) => {
    setSearchParams({ item: item.id });
  };

  // Refresh the list when the drawer edits an item.
  onMount(() => {
    const onItemUpdated = () => void refetch();
    window.addEventListener(ITEM_UPDATED_EVENT, onItemUpdated);
    onCleanup(() => window.removeEventListener(ITEM_UPDATED_EVENT, onItemUpdated));
  });


  const spacing = () => DENSITY_STYLE[density()];

  return (
    <div class="h-full flex flex-col">
      <ConfirmHost />
      {/* Header */}
      <div class="sticky top-0 z-10 bg-app pb-4">
        <div class="flex items-end gap-2.5">
          <div class="min-w-0">
            <h1 class="text-content m-0 leading-tight truncate" style={{ 'font-size': '2.125rem' }}>
              {project()?.name || 'List View'}
            </h1>
            <p class="text-small text-content-subtle">
              <Show
                when={filtering()}
                fallback={<>{flattenedItems().length} {flattenedItems().length === 1 ? 'item' : 'items'}</>}
              >
                {matchCount()} of {(items() ?? []).length} items match
              </Show>
            </p>
          </div>

          <div class="ml-auto flex items-center gap-2">
            <Show when={allExpandableIds().length > 0 && !filtering()}>
              <Button variant="ghost" onClick={expandAll} title="Expand all">
                <FiMaximize2 size={14} />
                Expand all
              </Button>
              <Button variant="ghost" onClick={collapseAll} title="Collapse all">
                <FiMinimize2 size={14} />
                Collapse all
              </Button>
            </Show>
            <Button onClick={() => startCreating()}>
              <FiPlus size={16} />
              New Item
            </Button>
          </div>
        </div>

        {/* Filter, run state, row height and columns */}
        <div class="flex flex-wrap items-center gap-2.5 pt-3">
          <input
            type="search"
            placeholder="Filter…"
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
            aria-label="Filter items"
            class="min-h-9 px-3.5 py-1.5 rounded-full text-sm w-56 max-w-full border border-line bg-surface text-content focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-focus-ring)]"
          />
          <select
            aria-label="Filter by work"
            value={runFilter()}
            onChange={(e) => setRunFilter(e.currentTarget.value as RunFilter | '')}
            class="min-h-9 px-3.5 rounded-full text-sm border border-line bg-surface text-content focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-focus-ring)]"
          >
            <option value="">Any work</option>
            <For each={RUN_FILTERS}>
              {(f) => <option value={f.value} selected={runFilter() === f.value}>{f.label}</option>}
            </For>
          </select>
          <Show when={filtering()}>
            <Button variant="ghost" size="sm" onClick={() => { setQuery(''); setRunFilter(''); }}>
              Clear filter
            </Button>
          </Show>
          <Button
            type="button"
            variant="secondary"
            onClick={toggleDensity}
            aria-pressed={density() === 'compact'}
            title="Toggle row density"
            class="ml-auto"
          >
            {density() === 'compact' ? 'Comfortable' : 'Compact'}
          </Button>
          <div class="relative" onKeyDown={(e) => { if (e.key === 'Escape') setColumnsOpen(false); }}>
            <Button
              type="button"
              variant="secondary"
              onClick={() => setColumnsOpen((o) => !o)}
              aria-expanded={columnsOpen()}
              aria-haspopup="true"
            >
              Columns ▾
            </Button>
            <Show when={columnsOpen()}>
              <div
                role="group"
                aria-label="Columns"
                class="absolute right-0 mt-2 z-20 w-64 flex flex-col gap-0.5 p-2.5 rounded-xl bg-surface"
                style={{ 'box-shadow': 'var(--shadow-lg)' }}
              >
                <For each={COLUMNS}>
                  {(c) => (
                    <label class="flex items-center gap-2.5 px-2.5 py-1.5 rounded-item text-small text-content cursor-pointer hover:bg-sunken">
                      <input
                        type="checkbox"
                        class="w-4 h-4 flex-none"
                        style={{ 'accent-color': 'var(--color-primary-600)' }}
                        checked={!shownHidden().has(c.key)}
                        disabled={!!c.pinned}
                        onChange={() => toggleColumn(c.key)}
                      />
                      <span class="min-w-0">
                        {headerLabel(c)}
                        <Show when={c.pinned}>
                          <span class="block text-xs text-content-subtle">{c.pinned}</span>
                        </Show>
                      </span>
                    </label>
                  )}
                </For>
              </div>
            </Show>
          </div>
        </div>
      </div>

      <div class="flex-1 overflow-auto">
        <Show when={projectId} fallback={
          <Card>
            <EmptyState icon={<IconList size={28} />} title="Select a project from the sidebar" />
          </Card>
        }>
          {/* Items */}
          <DragDropProvider onDragEnd={onDragEnd} collisionDetector={closestCenter}>
            <DragDropSensors />
            <SortableProvider ids={flattenedItems().map(i => i.id)}>
              <div>
                <Show when={flattenedItems().length > 0 || (creatingAt() && !filtering())} fallback={
                  <Card>
                    <Show
                      when={filtering()}
                      fallback={
                        <EmptyState
                          icon={<IconList size={28} />}
                          title="No items yet"
                          action={
                            <Button variant="secondary" size="sm" onClick={() => startCreating()}>
                              Create your first item
                            </Button>
                          }
                        />
                      }
                    >
                      <EmptyState
                        icon={<IconList size={28} />}
                        title="No items match your filter"
                        action={
                          <Button variant="secondary" size="sm" onClick={() => { setQuery(''); setRunFilter(''); }}>
                            Clear filter
                          </Button>
                        }
                      />
                    </Show>
                  </Card>
                }>
                  <div role="table" aria-label="Items" class="flex flex-col" style={{ 'min-width': minWidth(), gap: spacing().gap }}>
                    {/* Column headers: a button sorts, and says which way */}
                    <div
                      role="row"
                      class="sticky top-0 grid items-center gap-x-2 px-3.5 py-1.5 bg-app border-b border-line"
                      style={{ 'grid-template-columns': gridColumns(), 'z-index': 1 }}
                    >
                      <For each={visibleColumns()}>
                        {(c) => (
                          <div
                            role="columnheader"
                            class="min-w-0"
                            style={c.key === 'title' ? { 'padding-left': '3.25rem' } : undefined}
                            aria-sort={
                              c.sortable
                                ? sort()?.key === c.key ? (sort()!.dir === 'asc' ? 'ascending' : 'descending') : 'none'
                                : undefined
                            }
                          >
                            <Show
                              when={c.sortable}
                              fallback={<span class="text-micro font-bold uppercase tracking-wider text-content-subtle">{headerLabel(c)}</span>}
                            >
                              <button
                                type="button"
                                onClick={() => toggleSort(c.key as SortKey)}
                                class="max-w-full inline-flex items-center gap-1 rounded-full text-micro font-bold uppercase tracking-wider text-content-subtle hover:text-content focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-focus-ring)]"
                              >
                                <span class="truncate">{headerLabel(c)}</span>
                                <Show when={sort()?.key === c.key}>
                                  <span aria-hidden="true">{sort()!.dir === 'asc' ? '▲' : '▼'}</span>
                                </Show>
                              </button>
                            </Show>
                          </div>
                        )}
                      </For>
                      <div role="columnheader"><span class="sr-only">Actions</span></div>
                    </div>

                    <For each={flattenedItems()}>
                      {(item) => (
                        <>
                          <ItemRow
                            item={item}
                            types={types()}
                            run={runOf(item)}
                            columns={visibleColumns()}
                            gridColumns={gridColumns()}
                            padding={spacing().pad}
                            edit={edit}
                            onToggleExpand={() => toggleExpand(item.id)}
                            onAddChild={() => startCreating(item.id)}
                            onDelete={() => handleDelete(item)}
                            onView={() => handleViewItem(item)}
                            isExpanded={isOpen(item.id)}
                            canCollapse={!filtering()}
                          />

                          {/* Inline creation form */}
                          <Show when={creatingAt()?.parentId === item.id && expandedItems().has(item.id)}>
                            <CreateForm
                              level={item.level + 1}
                              types={types()}
                              title={newItemTitle()}
                              type={newItemType()}
                              priority={newItemPriority()}
                              onTitleChange={setNewItemTitle}
                              onTypeChange={setNewItemType}
                              onPriorityChange={setNewItemPriority}
                              onSave={handleCreate}
                              onCancel={cancelCreating}
                            />
                          </Show>
                        </>
                      )}
                    </For>

                    {/* Top-level create form */}
                    <Show when={creatingAt() && !creatingAt()?.parentId}>
                      <CreateForm
                        level={0}
                        types={types()}
                        title={newItemTitle()}
                        type={newItemType()}
                        priority={newItemPriority()}
                        onTitleChange={setNewItemTitle}
                        onTypeChange={setNewItemType}
                        onPriorityChange={setNewItemPriority}
                        onSave={handleCreate}
                        onCancel={cancelCreating}
                      />
                    </Show>
                  </div>
                </Show>
              </div>
            </SortableProvider>
          </DragDropProvider>
        </Show>
      </div>

    </div>
  );
}

// Sortable item row — a pill on the page ground, one cell per shown column.
function ItemRow(props: {
  item: ItemNode;
  types: ItemTypeDef[];
  run?: CardRun;
  columns: ColDef[];
  gridColumns: string;
  /** Space above and below the cells: the row height. */
  padding: string;
  edit: EditApi;
  onToggleExpand: () => void;
  onAddChild: () => void;
  onDelete: () => void;
  onView: () => void;
  isExpanded: boolean;
  /** A filter holds every parent open, so there is nothing to collapse. */
  canCollapse: boolean;
}) {
  const sortable = createSortable(props.item.id);
  const [showActions, setShowActions] = createSignal(false);
  const typeConfig = () => props.types.find((t) => t.key === props.item.item_type);
  const typeLabel = () => typeConfig()?.name ?? props.item.item_type;
  const typeTone = () => toneOf(typeConfig()?.color ?? 'neutral');
  const hasChildren = props.item.children.length > 0;

  return (
    <div
      ref={sortable.ref}
      role="row"
      data-testid="list-row"
      data-item-id={props.item.id}
      class={`group relative rounded-full transition-[box-shadow,border-color,opacity] duration-200 ${sortable.isActiveDraggable ? 'opacity-50' : ''}`}
      style={{
        'background-color': 'var(--color-bg-panel)',
        border: showActions() ? '2px solid var(--color-accent-line)' : '2px solid transparent',
        'box-shadow': showActions() || sortable.isActiveDraggable ? 'var(--shadow-md)' : 'none',
        opacity: props.edit.saving(props.item.id) ? 0.5 : undefined,
      }}
      onMouseEnter={() => setShowActions(true)}
      onMouseLeave={() => setShowActions(false)}
    >
      <div
        class="grid items-center gap-x-2 px-3.5"
        style={{ 'grid-template-columns': props.gridColumns, 'padding-block': props.padding }}
      >
        <For each={props.columns}>
          {(col) => (
            <div role="cell" class="min-w-0 overflow-hidden">
              <Switch>
                <Match when={col.key === 'title'}>
                  <div class="flex items-center gap-2 min-w-0" style={{ 'padding-left': `${props.item.level * 24}px` }}>
                    {/* Drag Handle */}
                    <div {...sortable.dragActivators} class="cursor-grab active:cursor-grabbing flex-shrink-0">
                      <FiMenu size={15} class="text-content-subtle hover:text-brand transition-colors" />
                    </div>

                    {/* Expand/Collapse */}
                    <button
                      type="button"
                      onClick={props.onToggleExpand}
                      aria-label={props.isExpanded ? 'Collapse children' : 'Expand children'}
                      aria-expanded={hasChildren ? (props.isExpanded ? 'true' : 'false') : undefined}
                      disabled={!props.canCollapse && hasChildren}
                      title={!props.canCollapse && hasChildren ? 'A filter is holding every parent open' : undefined}
                      tabindex={hasChildren ? undefined : -1}
                      class={`flex-shrink-0 grid place-items-center w-5 h-5 rounded-full text-content-muted hover:bg-sunken transition-colors ${hasChildren ? '' : 'invisible'}`}
                    >
                      {props.isExpanded ? <FiChevronDown size={14} /> : <FiChevronRight size={14} />}
                    </button>

                    {/* Type tile */}
                    <div
                      class="flex-shrink-0 w-8 h-8 rounded-full grid place-items-center font-heading text-sm"
                      style={{ 'background-color': typeTone().bg, color: typeTone().fg }}
                      title={typeLabel()}
                    >
                      {typeLabel().charAt(0).toUpperCase()}
                    </div>

                    <Show
                      when={props.edit.is(props.item.id, 'title')}
                      fallback={
                        <>
                          {/* Title - Clickable */}
                          <button
                            onClick={props.onView}
                            class="flex-1 min-w-0 text-left group/title"
                          >
                            <div class="flex items-center gap-2">
                              <span class="min-w-0 line-clamp-2 break-words text-sm font-bold text-content group-hover/title:text-brand transition-colors">
                                {props.item.title}
                              </span>
                              <Show when={hasChildren}>
                                <Badge class="flex-shrink-0">{props.item.children.length}</Badge>
                              </Show>
                              <Show when={props.item.recurrence}>
                                <Badge class="flex-shrink-0">Repeats</Badge>
                              </Show>
                            </div>
                            <Show when={props.item.description}>
                              <p class="text-xs text-content-subtle truncate">
                                {props.item.description?.replace(/<[^>]*>/g, '').substring(0, 80)}
                              </p>
                            </Show>
                          </button>
                          {/* The title opens the item, so changing it in place has a button of its own. */}
                          <button
                            type="button"
                            data-cell={`${props.item.id}:title`}
                            onClick={() => props.edit.start(props.item.id, 'title')}
                            aria-label={`Edit title of ${props.item.title}`}
                            title="Edit title"
                            class={`flex-shrink-0 w-7 h-7 rounded-full grid place-items-center text-content-muted hover:text-brand transition-opacity focus:opacity-100 ${showActions() ? 'opacity-100' : 'opacity-0'}`}
                          >
                            <FiEdit2 size={13} />
                          </button>
                        </>
                      }
                    >
                      <CellEditor item={props.item} col={col} edit={props.edit} />
                    </Show>
                  </div>
                </Match>

                <Match when={col.key === 'item_type'}>
                  <span class="block truncate text-small text-content">{typeLabel()}</span>
                </Match>

                <Match when={col.key === 'status'}>
                  <EditableCell item={props.item} col={col} edit={props.edit} shown={props.item.status}>
                    <Badge tone="info" class="max-w-full truncate">{props.item.status}</Badge>
                  </EditableCell>
                </Match>

                <Match when={col.key === 'priority'}>
                  {/* Priority pill — coloured edge instead of an emoji */}
                  <EditableCell item={props.item} col={col} edit={props.edit} shown={priorityLabel(props.item.priority)}>
                    <span
                      class="inline-flex items-center rounded-full px-2.5 py-[3px] text-xs font-semibold whitespace-nowrap text-content-muted"
                      style={{
                        'background-color': 'var(--color-bg-app)',
                        'box-shadow': `inset 3px 0 0 ${priorityColor(props.item.priority as Priority)}`,
                      }}
                    >
                      {priorityLabel(props.item.priority)}
                    </span>
                  </EditableCell>
                </Match>

                <Match when={col.key === 'assignee'}>
                  <EditableCell item={props.item} col={col} edit={props.edit} shown={props.item.assignee || 'none'}>
                    <span class="text-small text-content">{props.item.assignee || '—'}</span>
                  </EditableCell>
                </Match>

                <Match when={col.key === 'due_date'}>
                  <EditableCell item={props.item} col={col} edit={props.edit} shown={currentRaw(props.item, 'due_date') || 'none'}>
                    <span class="font-mono text-xs text-content">{currentRaw(props.item, 'due_date') || '—'}</span>
                  </EditableCell>
                </Match>

                <Match when={col.key === 'run'}>
                  <Show when={props.run} fallback={<span class="text-small text-content-subtle" aria-label="No run">—</span>}>
                    {(run) => <RunState state={run().state} docket={run().docket} note={run().note} />}
                  </Show>
                </Match>

                <Match when={col.key === 'details'}>
                  <div class="flex items-center gap-1 min-w-0">
                    <MergeNoteBadge note={props.item.merge_note} />
                    <AutomationBadge startedBy={props.item.started_by} team={props.item.team} mergedBy={props.item.merged_by} />
                    {/* Tags */}
                    <Show when={props.item.tags && props.item.tags.length > 0}>
                      <div class="flex-shrink-0 flex gap-1">
                        <For each={props.item.tags?.slice(0, 2)}>
                          {(tag) => (
                            <span class="text-micro font-semibold px-2.5 py-[2px] rounded-full border border-line text-content-muted">
                              {tag}
                            </span>
                          )}
                        </For>
                      </div>
                    </Show>
                  </div>
                </Match>
              </Switch>
            </div>
          )}
        </For>

        {/* Actions */}
        {/* Shown on hover, and whenever the keyboard is on one of them. */}
        <div role="cell" class={`flex items-center justify-end gap-1 transition-opacity focus-within:opacity-100 ${showActions() ? 'opacity-100' : 'opacity-0'}`}>
          <button
            type="button"
            onClick={props.onAddChild}
            aria-label={`Add child item to ${props.item.title}`}
            class="w-7 h-7 rounded-full grid place-items-center bg-app text-content-muted hover:text-brand transition-colors"
            title="Add child item"
          >
            <FiPlus size={14} />
          </button>
          <button
            type="button"
            onClick={props.onDelete}
            aria-label={`Delete ${props.item.title}`}
            class="w-7 h-7 rounded-full grid place-items-center transition-[filter] hover:brightness-95"
            style={{ 'background-color': 'var(--color-danger-100)', color: 'var(--color-danger-600)' }}
            title="Delete"
          >
            <FiTrash2 size={13} />
          </button>
        </div>
      </div>
    </div>
  );
}

/** A cell that turns into its editor when pressed; the button names the cell and its value. */
function EditableCell(props: { item: Item; col: ColDef; edit: EditApi; shown: string; children: JSX.Element }) {
  const key = () => props.col.key as EditableKey;
  return (
    <Show
      when={props.edit.is(props.item.id, key())}
      fallback={
        <button
          type="button"
          data-cell={`${props.item.id}:${key()}`}
          onClick={() => props.edit.start(props.item.id, key())}
          aria-label={`${props.col.label} of ${props.item.title}: ${props.shown}. Edit`}
          title={`Edit ${props.col.label.toLowerCase()}`}
          class="block max-w-full min-w-0 text-left truncate rounded-full decoration-dotted underline-offset-4 hover:underline focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-focus-ring)]"
        >
          {props.children}
        </button>
      }
    >
      <CellEditor item={props.item} col={props.col} edit={props.edit} />
    </Show>
  );
}

/** The control an edited cell shows. Enter or leaving it saves; Escape puts the cell back. */
function CellEditor(props: { item: Item; col: ColDef; edit: EditApi }) {
  const key = () => props.col.key as EditableKey;
  // The editor named by its column and row: "Status of Fix login".
  const name = () => `${props.col.label} of ${props.item.title}`;
  const finish = (raw: string) => props.edit.commit(props.item, key(), raw);
  const onKey = (e: KeyboardEvent & { currentTarget: HTMLInputElement | HTMLSelectElement }) => {
    if (e.key === 'Enter') finish(e.currentTarget.value);
    if (e.key === 'Escape') props.edit.cancel(props.item.id, key());
  };
  const focus = (el: HTMLElement) => queueMicrotask(() => el.focus());
  const field =
    'min-w-0 w-full min-h-8 px-3 rounded-full border-0 bg-app text-small font-semibold text-content ' +
    'focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-focus-ring)]';

  return (
    <Switch>
      <Match when={key() === 'status'}>
        <select ref={focus} aria-label={name()} class={field} onChange={(e) => finish(e.currentTarget.value)} onBlur={(e) => finish(e.currentTarget.value)} onKeyDown={onKey}>
          <For each={props.edit.statuses()}>
            {(s) => <option value={s} selected={s === props.item.status}>{s}</option>}
          </For>
        </select>
      </Match>
      <Match when={key() === 'priority'}>
        <select ref={focus} aria-label={name()} class={field} onChange={(e) => finish(e.currentTarget.value)} onBlur={(e) => finish(e.currentTarget.value)} onKeyDown={onKey}>
          <For each={EDIT_PRIORITIES}>
            {(p) => <option value={p} selected={p === props.item.priority}>{priorityLabel(p)}</option>}
          </For>
        </select>
      </Match>
      <Match when={key() === 'due_date'}>
        <input
          ref={focus}
          type="date"
          aria-label={name()}
          value={currentRaw(props.item, 'due_date')}
          class={field}
          onBlur={(e) => finish(e.currentTarget.value)}
          onKeyDown={onKey}
        />
      </Match>
      <Match when={true}>
        <input
          ref={focus}
          type="text"
          aria-label={name()}
          value={currentRaw(props.item, key())}
          class={field}
          onBlur={(e) => finish(e.currentTarget.value)}
          onKeyDown={onKey}
        />
      </Match>
    </Switch>
  );
}

const pillControl =
  'flex-shrink-0 min-h-8 text-small px-3 rounded-full border-0 bg-app text-content ' +
  'focus:outline-none focus-visible:ring-2 focus-visible:ring-[var(--color-focus-ring)]';

// Inline create form — a soft accent pill under its parent.
function CreateForm(props: {
  level: number;
  types: ItemTypeDef[];
  title: string;
  type: string;
  priority: string;
  onTitleChange: (v: string) => void;
  onTypeChange: (v: string) => void;
  onPriorityChange: (v: string) => void;
  onSave: () => void;
  onCancel: () => void;
}) {
  return (
    <div role="row">
      <div
        role="cell"
        class="flex items-center gap-2 p-2 rounded-full bg-accent-soft"
        style={{ 'margin-left': `${props.level * 24}px` }}
      >
        {/* Type Selector */}
        <select
          aria-label="Type"
          value={props.type}
          onChange={(e) => props.onTypeChange(e.currentTarget.value)}
          class={pillControl}
        >
          <For each={props.types}>
            {(t) => <option value={t.key}>{t.name}</option>}
          </For>
        </select>

        {/* Priority Selector */}
        <select
          aria-label="Priority"
          value={props.priority}
          onChange={(e) => props.onPriorityChange(e.currentTarget.value)}
          class={pillControl}
        >
          <For each={PRIORITIES}>
            {(p) => <option value={p.value}>{p.emoji} {p.label}</option>}
          </For>
        </select>

        {/* Title Input */}
        <input
          type="text"
          value={props.title}
          onInput={(e) => props.onTitleChange(e.currentTarget.value)}
          onKeyPress={(e) => {
            if (e.key === 'Enter') props.onSave();
            if (e.key === 'Escape') props.onCancel();
          }}
          aria-label="Item title"
          placeholder="Item title... (Enter to save, Esc to cancel)"
          class={`${pillControl} flex-1 min-w-0 px-4 placeholder-[var(--color-text-tertiary)]`}
          autofocus
        />

        {/* Actions */}
        <button
          type="button"
          aria-label="Save"
          onClick={props.onSave}
          class="flex-shrink-0 w-8 h-8 rounded-full grid place-items-center transition-[filter] hover:brightness-95"
          style={{ color: 'var(--color-on-accent)', 'background-color': 'var(--color-primary-600)' }}
          title="Save (Enter)"
        >
          <FiCheck size={16} />
        </button>
        <button
          type="button"
          aria-label="Cancel"
          onClick={props.onCancel}
          class="flex-shrink-0 w-8 h-8 rounded-full grid place-items-center bg-app text-content-muted hover:text-content transition-colors"
          title="Cancel (Esc)"
        >
          <FiX size={16} />
        </button>
      </div>
    </div>
  );
}
