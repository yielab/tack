import type { Item, Priority, UpdateItem } from '../../shared/types';
import { typeKey } from '../../shared/ui/TypeBadge';
import type { CardRun } from '../../shared/runWithAgent/cardRun';

// The List's rows as data: which columns exist, how siblings sort, what a
// filter lets through. Kept apart from the component so each can be tested
// without mounting a view.

export type SortKey = 'title' | 'item_type' | 'status' | 'priority' | 'assignee' | 'due_date' | 'run';
export type SortDir = 'asc' | 'desc';

/** Rank priorities so sorting goes Critical → … → None rather than alphabetic. */
const PRIORITY_RANK: Record<string, number> = {
  critical: 0,
  high: 1,
  medium: 2,
  low: 3,
  none: 4,
};

/** Every priority an item can take, most urgent first. */
export const PRIORITIES: Priority[] = ['critical', 'high', 'medium', 'low', 'none'];

/** What needs a person comes first, then what is going, then what is over; no run last. */
const RUN_RANK: Record<string, number> = {
  needs: 0,
  review: 1,
  failed: 2,
  running: 3,
  queued: 4,
  done: 5,
  cancelled: 6,
};

/** Where a run sorts: the item's newest run, or after every run when it has none. */
export function runRank(run: CardRun | undefined): number {
  return run ? (RUN_RANK[run.state] ?? 99) : 100;
}

function cellValue(item: Item, key: SortKey, runOf?: (item: Item) => CardRun | undefined): string | number {
  if (key === 'run') return runRank(runOf?.(item));
  if (key === 'priority') return PRIORITY_RANK[item.priority] ?? 99;
  if (key === 'due_date') return item.due_date ?? '￿'; // undated sorts last
  if (key === 'item_type') return typeKey(item.item_type).toLowerCase();
  return (item[key] ?? '').toString().toLowerCase();
}

/** Stable sort by a column; returns a new array (does not mutate input). */
export function sortItems(
  items: Item[],
  key: SortKey | null,
  dir: SortDir,
  runOf?: (item: Item) => CardRun | undefined,
): Item[] {
  if (!key) return items.slice();
  const sorted = items.slice().sort((a, b) => {
    const av = cellValue(a, key, runOf);
    const bv = cellValue(b, key, runOf);
    if (av < bv) return -1;
    if (av > bv) return 1;
    // Tie-break on created_at for a stable, predictable order.
    return a.created_at.localeCompare(b.created_at);
  });
  return dir === 'desc' ? sorted.reverse() : sorted;
}

/** Whether an item's title, assignee or status contains the query, ignoring case. */
export function matchesQuery(item: Item, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return [item.title, item.assignee ?? '', item.status].some((f) => f.toLowerCase().includes(q));
}

/** Case-insensitive filter across title, assignee, and status. */
export function filterItems(items: Item[], query: string): Item[] {
  if (!query.trim()) return items;
  return items.filter((it) => matchesQuery(it, query));
}

// ── Hierarchy ─────────────────────────────────────────────────────────────────

export type ItemNode = Item & { children: ItemNode[]; level: number };

/**
 * Items as a tree: a child under its parent, an item whose parent is not in the
 * list at the top. Each set of siblings sorts on its own, so a sorted column
 * reorders rows without pulling a child out from under its parent; with no
 * key they keep the order they were created in.
 */
export function buildTree(
  items: Item[],
  key: SortKey | null,
  dir: SortDir,
  runOf?: (item: Item) => CardRun | undefined,
): ItemNode[] {
  const byId = new Map<string, ItemNode>();
  for (const item of items) byId.set(item.id, { ...item, children: [], level: 0 });

  const roots: ItemNode[] = [];
  for (const item of items) {
    const node = byId.get(item.id)!;
    const parent = item.parent_id ? byId.get(item.parent_id) : undefined;
    (parent ? parent.children : roots).push(node);
  }

  const order = (nodes: ItemNode[], level: number): ItemNode[] => {
    const sorted = key
      ? (sortItems(nodes, key, dir, runOf) as ItemNode[])
      : nodes.slice().sort((a, b) => a.created_at.localeCompare(b.created_at));
    for (const node of sorted) {
      node.level = level;
      node.children = order(node.children, level + 1);
    }
    return sorted;
  };
  return order(roots, 0);
}

/**
 * Keeps the items `keep` accepts and the parents above them, so a match is
 * never shown without the item it belongs under.
 */
export function pruneTree(nodes: ItemNode[], keep: (item: Item) => boolean): ItemNode[] {
  const out: ItemNode[] = [];
  for (const node of nodes) {
    const children = pruneTree(node.children, keep);
    if (children.length > 0 || keep(node)) out.push({ ...node, children });
  }
  return out;
}

/** The rows in view, parents first; a parent's children follow it only while it is open. */
export function visibleRows(nodes: ItemNode[], isOpen: (id: string) => boolean): ItemNode[] {
  const rows: ItemNode[] = [];
  const walk = (list: ItemNode[]) => {
    for (const node of list) {
      rows.push(node);
      if (node.children.length > 0 && isOpen(node.id)) walk(node.children);
    }
  };
  walk(nodes);
  return rows;
}

// ── Columns ───────────────────────────────────────────────────────────────────

export type ColumnKey = SortKey | 'details';

export interface ColDef {
  key: ColumnKey;
  /** Vocabulary key for the header label, or none when the label is literal. */
  vocabKey?: string;
  label: string;
  /** Cell width in rem; the title takes what is left. */
  rem: number;
  sortable: boolean;
  editable: boolean;
  /** Cannot be hidden, with the reason shown beside it. */
  pinned?: string;
}

export const COLUMNS: ColDef[] = [
  { key: 'title', label: 'Title', rem: 12, sortable: true, editable: true, pinned: 'Rows are named and nested by their title' },
  { key: 'item_type', label: 'Type', rem: 5.5, sortable: true, editable: false },
  { key: 'status', label: 'Status', rem: 7.5, sortable: true, editable: true },
  // Work is what the newest run reports; Status is the person's. The key stays `run` so a remembered choice of columns holds.
  { key: 'run', label: 'Work', rem: 9, sortable: true, editable: false },
  { key: 'priority', label: 'Priority', rem: 6, sortable: true, editable: true },
  { key: 'assignee', vocabKey: 'assignee', label: 'Assignee', rem: 7, sortable: true, editable: true },
  { key: 'due_date', label: 'Due', rem: 6.5, sortable: true, editable: true },
  { key: 'details', label: 'Details', rem: 8, sortable: false, editable: false },
];

/** Columns hidden until someone shows them: the title tile already says the type. */
export const DEFAULT_HIDDEN: ColumnKey[] = ['item_type'];

/**
 * The columns no item has a value in: shown only once someone fills one in, or
 * asks for them in the Columns menu. Nothing is empty before there are items.
 */
export function emptyColumns(items: Item[], runOf?: (item: Item) => CardRun | undefined): ColumnKey[] {
  if (items.length === 0) return [];
  const some = (has: (item: Item) => boolean) => items.some(has);
  const empty: ColumnKey[] = [];
  if (!some((i) => !!i.assignee)) empty.push('assignee');
  if (!some((i) => !!i.due_date)) empty.push('due_date');
  if (!some((i) => runOf?.(i) !== undefined)) empty.push('run');
  if (!some((i) => !!i.merge_note || i.started_by === 'auto_start' || !!i.team || i.merged_by === 'auto' || (i.tags?.length ?? 0) > 0)) {
    empty.push('details');
  }
  return empty;
}

/** The columns that can be edited in place. */
export type EditableKey = 'title' | 'status' | 'priority' | 'assignee' | 'due_date';

/** The values an item has for the edit it is offered: used to tell a change from none. */
export function currentRaw(item: Item, key: EditableKey): string {
  switch (key) {
    case 'title': return item.title;
    case 'status': return item.status;
    case 'priority': return item.priority;
    case 'assignee': return item.assignee ?? '';
    case 'due_date': return item.due_date ? item.due_date.slice(0, 10) : '';
  }
}

/** The patch an edit makes, or `null` when the typed value changes nothing or is not allowed (an empty title). */
export function editPatch(item: Item, key: EditableKey, raw: string): UpdateItem | null {
  if (key === 'title') {
    const v = raw.trim();
    return !v || v === item.title ? null : { title: v };
  }
  if (key === 'status') return !raw || raw === item.status ? null : { status: raw };
  if (key === 'priority') return raw === item.priority ? null : { priority: raw as Priority };
  if (key === 'assignee') {
    const v = raw.trim();
    return v === (item.assignee ?? '') ? null : { assignee: v === '' ? null : v };
  }
  return raw === currentRaw(item, 'due_date') ? null : { due_date: raw === '' ? null : `${raw}T00:00:00Z` };
}
