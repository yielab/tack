import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { Resource } from 'solid-js';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import type { Item } from '../../shared/types';
import ItemProperties from './ItemProperties';

const projectValue: ProjectContextValue = {
  projectId: () => 'p1',
  project: (() => null) as unknown as Resource<null>,
  workflow: () => ({
    workflow_type: 'kanban',
    statuses: [{ name: 'todo', category: 'todo', order: 0 }],
  }),
  vocabulary: () => ({}),
  refetch: () => {},
};

const item = {
  id: 'item-1',
  project_id: 'p1',
  title: 'Pay rent',
  item_type: 'task',
  status: 'todo',
  priority: 'medium',
  estimate_unit: 'story_points',
  tags: [],
  sort_order: 0,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
} as Item;

beforeEach(() => {
  vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
    Promise.resolve(new Response('[]', { status: 200, headers: { 'Content-Type': 'application/json' } })),
  );
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

function mount(onPatch: () => void, overrides: Partial<Item> = {}, layout: 'grid' | 'list' = 'grid') {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ProjectContext.Provider value={projectValue}>
        <ItemProperties item={{ ...item, ...overrides }} onPatch={onPatch} layout={layout} blocks={2} />
      </ProjectContext.Provider>
    ),
    container,
  );
  return { container, dispose };
}

const labelled = (container: HTMLElement, tag: string, label: string) =>
  [...container.querySelectorAll<HTMLSelectElement | HTMLInputElement>(tag)].find(
    (c) => c.labels?.[0]?.textContent?.includes(label),
  )!;

describe('ItemProperties', () => {
  it('lists every property, with what the item blocks', () => {
    const { container, dispose } = mount(vi.fn());
    const text = container.textContent ?? '';
    for (const label of ['Priority', 'Due date', 'Assignee', 'Estimate', 'Repeats', 'Last update', 'Tags', 'Blocks']) {
      expect(text).toContain(label);
    }
    expect(text).toContain('2 items');
    dispose();
  });

  it('the_assignee_field_patches_assignee_and_clears_it_with_null', () => {
    const onPatch = vi.fn();
    const { container, dispose } = mount(onPatch, { assignee: 'Ana' });
    const input = labelled(container, 'input', 'Assignee') as HTMLInputElement;
    expect(input.value).toBe('Ana');
    input.value = 'Luis';
    input.dispatchEvent(new Event('change', { bubbles: true }));
    expect(onPatch).toHaveBeenLastCalledWith({ assignee: 'Luis' });
    input.value = '';
    input.dispatchEvent(new Event('change', { bubbles: true }));
    expect(onPatch).toHaveBeenLastCalledWith({ assignee: null });
    dispose();
  });

  it('the_repeats_select_patches_recurrence', () => {
    const onPatch = vi.fn();
    const { container, dispose } = mount(onPatch, { recurrence: 'weekly' });
    const select = labelled(container, 'select', 'Repeats') as HTMLSelectElement;
    expect(select.value).toBe('weekly');
    select.value = 'monthly';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    expect(onPatch).toHaveBeenLastCalledWith({ recurrence: 'monthly' });
    select.value = '';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    expect(onPatch).toHaveBeenLastCalledWith({ recurrence: null });
    dispose();
  });
});
