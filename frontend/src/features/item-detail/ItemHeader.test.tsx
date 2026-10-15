import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import type { Resource } from 'solid-js';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import type { Item } from '../../shared/types';
import ItemHeader from './ItemHeader';

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

afterEach(() => {
  document.body.innerHTML = '';
});

describe('ItemHeader', () => {
  it('the_repeats_select_patches_recurrence', () => {
    const onPatch = vi.fn();
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(
      () => (
        <ProjectContext.Provider value={projectValue}>
          <ItemHeader item={{ ...item, recurrence: 'weekly' }} onPatch={onPatch} />
        </ProjectContext.Provider>
      ),
      container,
    );
    const select = [...container.querySelectorAll<HTMLSelectElement>('select')].find(
      (s) => s.labels?.[0]?.textContent?.includes('Repeats'),
    )!;
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
