import { describe, it, expect } from 'vitest';
import { sortItems, filterItems, type SortKey } from './Table';
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
