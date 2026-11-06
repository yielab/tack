import { describe, it, expect } from 'vitest';
import { ancestorsOf, itemsUnder, treeRows, type TreeItem } from './tree';

const item = (id: string, parent_id?: string | null): TreeItem => ({ id, parent_id });
const index = (items: TreeItem[]) => new Map(items.map((i) => [i.id, i]));

describe('ancestorsOf', () => {
  const items = [item('epic'), item('story', 'epic'), item('task', 'story'), item('loose')];
  const byId = index(items);

  it('lists_the_items_an_item_sits_under_outermost_first', () => {
    expect(ancestorsOf(items[2], byId).map((i) => i.id)).toEqual(['epic', 'story']);
    expect(ancestorsOf(items[1], byId).map((i) => i.id)).toEqual(['epic']);
  });

  it('is_empty_for_an_item_at_the_top', () => {
    expect(ancestorsOf(items[0], byId)).toEqual([]);
    expect(ancestorsOf(items[3], byId)).toEqual([]);
    expect(ancestorsOf(item('x', null), byId)).toEqual([]);
  });

  it('stops_at_a_parent_the_project_does_not_hold', () => {
    expect(ancestorsOf(item('orphan', 'gone'), byId)).toEqual([]);
    const partial = index([item('mid', 'gone'), item('leaf', 'mid')]);
    expect(ancestorsOf(item('leaf', 'mid'), partial).map((i) => i.id)).toEqual(['mid']);
  });

  it('does_not_run_on_in_a_loop', () => {
    const loop = [item('a', 'b'), item('b', 'a')];
    expect(ancestorsOf(loop[0], index(loop)).map((i) => i.id)).toEqual(['b']);
  });
});

describe('itemsUnder', () => {
  const items = [
    item('epic'),
    item('other'),
    item('story', 'epic'),
    item('task-a', 'story'),
    item('task-b', 'epic'),
    item('task-c', 'other'),
  ];

  it('lists_everything_below_a_group_at_any_depth_in_the_order_given', () => {
    expect(itemsUnder(items[0], items).map((i) => i.id)).toEqual(['story', 'task-a', 'task-b']);
    expect(itemsUnder(items[2], items).map((i) => i.id)).toEqual(['task-a']);
  });

  it('is_empty_for_an_item_with_nothing_under_it', () => {
    expect(itemsUnder(items[5], items)).toEqual([]);
  });

  it('does_not_include_the_group_even_when_the_data_loops', () => {
    const loop = [item('a', 'b'), item('b', 'a')];
    expect(itemsUnder(loop[0], loop).map((i) => i.id)).toEqual(['b']);
  });
});

describe('treeRows', () => {
  it('reads_each_top_item_then_everything_under_it_in_the_order_given', () => {
    const items = [item('b'), item('a'), item('a2', 'a'), item('b1', 'b'), item('a1', 'a'), item('a1x', 'a1')];
    expect(treeRows(items).map((r) => [r.item.id, r.depth])).toEqual([
      ['b', 0], ['b1', 1], ['a', 0], ['a2', 1], ['a1', 1], ['a1x', 2],
    ]);
  });

  it('counts_an_item_whose_parent_is_not_held_as_a_top_item', () => {
    expect(treeRows([item('x', 'gone'), item('y', 'x')]).map((r) => [r.item.id, r.depth])).toEqual([['x', 0], ['y', 1]]);
  });

  it('lists_an_item_caught_in_a_loop_once', () => {
    expect(treeRows([item('a', 'b'), item('b', 'a')]).map((r) => r.item.id)).toEqual(['a', 'b']);
  });
});
