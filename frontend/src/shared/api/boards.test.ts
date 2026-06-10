import { describe, it, expect } from 'vitest';
import { deriveBoard } from './boards';
import type { Project, Item } from '../types';

function makeProject(
  statuses: Array<{ name: string; order: number; wip_limit?: number }>,
): Project {
  return {
    id: 'p1',
    workspace_id: 'ws1',
    name: 'Test',
    project_type: 'software',
    vocabulary: {},
    workflow: {
      workflow_type: 'kanban',
      statuses: statuses.map((s) => ({ ...s, category: 'todo' as const })),
    },
    created_at: '',
    updated_at: '',
    archived: false,
  };
}

function item(id: string, status: string): Item {
  return {
    id,
    project_id: 'p1',
    title: id,
    item_type: 'task',
    status,
    priority: 'medium',
    tags: [],
    estimate_unit: 'story_points',
    sort_order: 0,
    created_at: '',
    updated_at: '',
  };
}

describe('deriveBoard', () => {
  it('produces columns in workflow status order, not insertion order', () => {
    const p = makeProject([
      { name: 'Done', order: 2 },
      { name: 'Todo', order: 0 },
      { name: 'In Progress', order: 1 },
    ]);
    const { columns } = deriveBoard(p, []);
    expect(columns.map((c) => c.status)).toEqual(['Todo', 'In Progress', 'Done']);
  });

  it('places each item into its matching status column', () => {
    const p = makeProject([{ name: 'Todo', order: 0 }, { name: 'Done', order: 1 }]);
    const { columns } = deriveBoard(p, [
      item('a', 'Todo'),
      item('b', 'Done'),
      item('c', 'Todo'),
    ]);
    expect(columns[0].items.map((i) => i.id)).toEqual(['a', 'c']);
    expect(columns[1].items.map((i) => i.id)).toEqual(['b']);
  });

  it('puts items with an unknown status into the first column', () => {
    const p = makeProject([{ name: 'Todo', order: 0 }, { name: 'Done', order: 1 }]);
    const { columns } = deriveBoard(p, [item('x', 'not-a-real-status')]);
    expect(columns[0].items[0].id).toBe('x');
    expect(columns[1].items).toHaveLength(0);
  });

  it('marks wip_exceeded when column item count exceeds the limit', () => {
    const p = makeProject([{ name: 'WIP', order: 0, wip_limit: 1 }]);
    const { columns } = deriveBoard(p, [item('a', 'WIP'), item('b', 'WIP')]);
    expect(columns[0].wip_exceeded).toBe(true);
  });

  it('does not mark wip_exceeded when count exactly equals the limit', () => {
    const p = makeProject([{ name: 'WIP', order: 0, wip_limit: 2 }]);
    const { columns } = deriveBoard(p, [item('a', 'WIP'), item('b', 'WIP')]);
    expect(columns[0].wip_exceeded).toBe(false);
  });

  it('does not mark wip_exceeded when no wip_limit is set', () => {
    const p = makeProject([{ name: 'Col', order: 0 }]);
    const manyItems = Array.from({ length: 50 }, (_, i) => item(`i${i}`, 'Col'));
    const { columns } = deriveBoard(p, manyItems);
    expect(columns[0].wip_exceeded).toBe(false);
  });

  it('returns columns with empty item arrays for a project with no items', () => {
    const p = makeProject([{ name: 'Todo', order: 0 }, { name: 'Done', order: 1 }]);
    const { columns } = deriveBoard(p, []);
    expect(columns).toHaveLength(2);
    expect(columns.every((c) => c.items.length === 0)).toBe(true);
  });
});
