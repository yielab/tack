import { describe, it, expect } from 'vitest';
import { buildTree } from './buildTree';
import type { Item } from '../../shared/types';

const item = (id: string, parent_id?: string): Item =>
  ({ id, parent_id, title: id, status: 'todo' } as unknown as Item);

describe('buildTree', () => {
  it('builds a nested hierarchy from a flat list and stamps depth', () => {
    const flat = [
      item('epic'),
      item('feat', 'epic'),
      item('task', 'feat'),
      item('other'),
    ];
    const roots = buildTree(flat);

    expect(roots.map((r) => r.id)).toEqual(['epic', 'other']);
    const epic = roots[0];
    expect(epic.depth).toBe(0);
    expect(epic.children.map((c) => c.id)).toEqual(['feat']);
    expect(epic.children[0].depth).toBe(1);
    expect(epic.children[0].children[0].id).toBe('task');
    expect(epic.children[0].children[0].depth).toBe(2);
  });

  it('treats items with a missing parent as roots', () => {
    const roots = buildTree([item('orphan', 'ghost')]);
    expect(roots.map((r) => r.id)).toEqual(['orphan']);
  });

  it('preserves input order among siblings', () => {
    const roots = buildTree([item('p'), item('b', 'p'), item('a', 'p')]);
    expect(roots[0].children.map((c) => c.id)).toEqual(['b', 'a']);
  });
});
