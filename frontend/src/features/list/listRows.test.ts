import { describe, it, expect } from 'vitest';
import { sortItems, filterItems, matchesQuery, buildTree, pruneTree, visibleRows, editPatch, currentRaw, runRank, type ItemNode } from './listRows';
import type { CardRun } from '../../shared/runWithAgent/cardRun';
import type { Item, ItemType, Priority } from '../../shared/types';

function makeItem(p: Partial<Item> & { id: string }): Item {
  return {
    project_id: 'proj',
    title: 'Untitled',
    item_type: 'task' as ItemType,
    status: 'To Do',
    priority: 'medium' as Priority,
    estimate_unit: 'story_points',
    tags: [],
    sort_order: 0,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...p,
  };
}

const items: Item[] = [
  makeItem({ id: 'a', title: 'Banana', priority: 'low', status: 'Done', assignee: 'Sam', due_date: '2026-03-01T00:00:00Z', created_at: '2026-01-01T00:00:00Z' }),
  makeItem({ id: 'b', title: 'apple', priority: 'critical', status: 'To Do', assignee: 'Kim', due_date: undefined, created_at: '2026-01-02T00:00:00Z' }),
  makeItem({ id: 'c', title: 'Cherry', priority: 'high', status: 'In Progress', assignee: undefined, due_date: '2026-02-01T00:00:00Z', created_at: '2026-01-03T00:00:00Z' }),
];

const ids = (xs: Item[]) => xs.map((x) => x.id);

describe('sortItems', () => {
  it('returns a copy and does not mutate the input', () => {
    const before = ids(items);
    const out = sortItems(items, 'title', 'asc');
    expect(out).not.toBe(items);
    expect(ids(items)).toEqual(before);
  });

  it('sorts by title case-insensitively (apple < Banana < Cherry)', () => {
    expect(ids(sortItems(items, 'title', 'asc'))).toEqual(['b', 'a', 'c']);
    expect(ids(sortItems(items, 'title', 'desc'))).toEqual(['c', 'a', 'b']);
  });

  it('sorts by priority rank, not alphabetically (critical → high → low)', () => {
    expect(ids(sortItems(items, 'priority', 'asc'))).toEqual(['b', 'c', 'a']);
  });

  it('sorts undated items last when ascending by due_date', () => {
    // c (Feb) < a (Mar) < b (none)
    expect(ids(sortItems(items, 'due_date', 'asc'))).toEqual(['c', 'a', 'b']);
  });

  it('is a no-op when no sort key is given', () => {
    expect(ids(sortItems(items, null, 'asc'))).toEqual(['a', 'b', 'c']);
  });

  it('orders priority Critical, High, Medium, Low, None and reverses it for descending', () => {
    const all = (['none', 'low', 'critical', 'medium', 'high'] as const).map((priority) => makeItem({ id: priority, priority }));
    expect(ids(sortItems(all, 'priority', 'asc'))).toEqual(['critical', 'high', 'medium', 'low', 'none']);
    expect(ids(sortItems(all, 'priority', 'desc'))).toEqual(['none', 'low', 'medium', 'high', 'critical']);
  });

  it('sorts by status, assignee and type by their text', () => {
    expect(ids(sortItems(items, 'status', 'asc'))).toEqual(['a', 'c', 'b']); // Done, In Progress, To Do
    expect(ids(sortItems(items, 'assignee', 'asc'))).toEqual(['c', 'b', 'a']); // none, Kim, Sam
    const typed = [
      makeItem({ id: 'x', item_type: 'task' }),
      makeItem({ id: 'y', item_type: 'bug' }),
      makeItem({ id: 'z', item_type: { custom: 'Alpha' } }),
    ];
    expect(ids(sortItems(typed, 'item_type', 'asc'))).toEqual(['z', 'y', 'x']);
  });

  it('sorts by run: what needs a person first, then what is going, and an item with no run last', () => {
    const runs: Record<string, CardRun> = {
      failed: run('failed'), needs: run('needs'), running: run('running'), review: run('review'), done: run('done'),
    };
    const all = ['none', 'done', 'running', 'failed', 'review', 'needs'].map((id) => makeItem({ id }));
    expect(ids(sortItems(all, 'run', 'asc', (it) => runs[it.id]))).toEqual(['needs', 'review', 'failed', 'running', 'done', 'none']);
    expect(runRank(undefined)).toBeGreaterThan(runRank(run('cancelled')));
  });
});

function run(state: CardRun['state']): CardRun {
  return { state, live: false, asking: false };
}

describe('matchesQuery', () => {
  it('is true for every item when the query is empty', () => {
    expect(items.every((it) => matchesQuery(it, ''))).toBe(true);
  });

  it('looks at the title, the assignee and the status', () => {
    expect(items.filter((it) => matchesQuery(it, 'an')).map((it) => it.id)).toEqual(['a']); // Banana
    expect(items.filter((it) => matchesQuery(it, 'kim')).map((it) => it.id)).toEqual(['b']);
    expect(items.filter((it) => matchesQuery(it, 'done')).map((it) => it.id)).toEqual(['a']);
  });
});

describe('filterItems', () => {
  it('returns all items for an empty query', () => {
    expect(filterItems(items, '   ')).toHaveLength(3);
  });

  it('matches title case-insensitively', () => {
    expect(ids(filterItems(items, 'CHERRY'))).toEqual(['c']);
  });

  it('matches assignee', () => {
    expect(ids(filterItems(items, 'kim'))).toEqual(['b']);
  });

  it('matches status', () => {
    expect(ids(filterItems(items, 'in progress'))).toEqual(['c']);
  });

  it('returns nothing when no field matches', () => {
    expect(filterItems(items, 'zzz')).toHaveLength(0);
  });

  it('tolerates items with no assignee', () => {
    expect(() => filterItems(items, 'sam')).not.toThrow();
    expect(ids(filterItems(items, 'sam'))).toEqual(['a']);
  });
});

// A tree: epic > (design, build > (api, ui)), plus a loose task.
const family: Item[] = [
  makeItem({ id: 'epic', title: 'Epic', priority: 'low', created_at: '2026-01-01T00:00:00Z' }),
  makeItem({ id: 'design', title: 'Zeta design', parent_id: 'epic', priority: 'critical', created_at: '2026-01-02T00:00:00Z' }),
  makeItem({ id: 'build', title: 'Alpha build', parent_id: 'epic', priority: 'high', created_at: '2026-01-03T00:00:00Z' }),
  makeItem({ id: 'api', title: 'Yankee api', parent_id: 'build', priority: 'none', created_at: '2026-01-04T00:00:00Z' }),
  makeItem({ id: 'ui', title: 'Bravo ui', parent_id: 'build', priority: 'medium', created_at: '2026-01-05T00:00:00Z' }),
  makeItem({ id: 'loose', title: 'Loose', priority: 'critical', created_at: '2026-01-06T00:00:00Z' }),
];

const outline = (nodes: ItemNode[]): string[] => nodes.flatMap((n) => [n.id, ...outline(n.children)]);

describe('buildTree', () => {
  it('keeps creation order when nothing is sorted, with each level set', () => {
    const tree = buildTree(family, null, 'asc');
    expect(outline(tree)).toEqual(['epic', 'design', 'build', 'api', 'ui', 'loose']);
    expect(Object.fromEntries(visibleRows(tree, () => true).map((n) => [n.id, n.level]))).toEqual({
      epic: 0, design: 1, build: 1, api: 2, ui: 2, loose: 0,
    });
  });

  it('sorts the siblings of every parent and never moves a child out of its parent', () => {
    const byTitle = buildTree(family, 'title', 'asc');
    expect(outline(byTitle)).toEqual(['epic', 'build', 'ui', 'api', 'design', 'loose']);
    expect(outline(buildTree(family, 'title', 'desc'))).toEqual(['loose', 'epic', 'design', 'build', 'api', 'ui']);
  });

  it('sorts by priority inside each parent, Critical first', () => {
    // The roots: Critical (loose) before Low (epic); under build: Medium (ui) before None (api).
    expect(outline(buildTree(family, 'priority', 'asc'))).toEqual(['loose', 'epic', 'design', 'build', 'ui', 'api']);
  });

  it('puts an item whose parent is missing at the top', () => {
    const orphan = makeItem({ id: 'orphan', parent_id: 'gone', created_at: '2026-01-07T00:00:00Z' });
    expect(buildTree([...family, orphan], null, 'asc').map((n) => n.id)).toEqual(['epic', 'loose', 'orphan']);
  });
});

describe('pruneTree', () => {
  it('keeps what matches and the parents above it, and drops the rest', () => {
    const tree = buildTree(family, null, 'asc');
    const pruned = pruneTree(tree, (it) => it.id === 'ui');
    expect(outline(pruned)).toEqual(['epic', 'build', 'ui']);
  });

  it('keeps a matching parent without its non-matching children', () => {
    const pruned = pruneTree(buildTree(family, null, 'asc'), (it) => it.id === 'epic');
    expect(outline(pruned)).toEqual(['epic']);
  });
});

describe('visibleRows', () => {
  it('lists a parent’s children only while it is open', () => {
    const tree = buildTree(family, null, 'asc');
    expect(visibleRows(tree, () => false).map((n) => n.id)).toEqual(['epic', 'loose']);
    expect(visibleRows(tree, (id) => id === 'epic').map((n) => n.id)).toEqual(['epic', 'design', 'build', 'loose']);
  });
});

describe('editPatch', () => {
  const item = makeItem({ id: 'e', title: 'Fix login', status: 'To Do', priority: 'medium', assignee: 'Kim', due_date: '2026-03-01T00:00:00Z' });

  it('makes a patch for each of the five fields', () => {
    expect(editPatch(item, 'title', '  Fix logout ')).toEqual({ title: 'Fix logout' });
    expect(editPatch(item, 'status', 'Done')).toEqual({ status: 'Done' });
    expect(editPatch(item, 'priority', 'high')).toEqual({ priority: 'high' });
    expect(editPatch(item, 'assignee', 'Sam')).toEqual({ assignee: 'Sam' });
    expect(editPatch(item, 'due_date', '2026-04-02')).toEqual({ due_date: '2026-04-02T00:00:00Z' });
  });

  it('clears an assignee or a due date when the field is emptied', () => {
    expect(editPatch(item, 'assignee', ' ')).toEqual({ assignee: null });
    expect(editPatch(item, 'due_date', '')).toEqual({ due_date: null });
  });

  it('makes no patch when nothing changed or the title would be empty', () => {
    expect(editPatch(item, 'title', 'Fix login')).toBeNull();
    expect(editPatch(item, 'title', '   ')).toBeNull();
    expect(editPatch(item, 'status', 'To Do')).toBeNull();
    expect(editPatch(item, 'priority', 'medium')).toBeNull();
    expect(editPatch(item, 'assignee', 'Kim')).toBeNull();
    expect(editPatch(item, 'due_date', '2026-03-01')).toBeNull();
  });

  it('reads a due date as the day it falls on', () => {
    expect(currentRaw(item, 'due_date')).toBe('2026-03-01');
    expect(currentRaw(makeItem({ id: 'n' }), 'due_date')).toBe('');
  });
});
