import { describe, it, expect, afterEach } from 'vitest';
import Timeline from './Timeline';
import { makeItem, mountProjectView, unmountProjectViews } from '../../test/mountProjectView';

afterEach(unmountProjectViews);

const startOfThisMonth = new Date(new Date().getFullYear(), new Date().getMonth(), 3, 12).toISOString();
const endOfThisMonth = new Date(new Date().getFullYear(), new Date().getMonth(), 20, 12).toISOString();

describe('Timeline', () => {
  it('draws_a_bar_for_each_item_in_range_and_marks_a_done_one', async () => {
    const container = await mountProjectView(() => <Timeline />, 'timeline', {
      items: [
        makeItem({ id: 'a', title: 'Design the schema', started_at: startOfThisMonth, due_date: endOfThisMonth }),
        makeItem({ id: 'b', title: 'Write the migration', status: 'Done', started_at: startOfThisMonth, due_date: endOfThisMonth }),
        makeItem({ id: 'old', title: 'Long gone', created_at: '2020-01-01T00:00:00Z', due_date: '2020-01-10T00:00:00Z' }),
      ],
    });

    expect(container.querySelector('h1')?.textContent).toBe('Timeline');
    const open = container.querySelector('[aria-label="Design the schema"]')!;
    const done = container.querySelector('[aria-label="Write the migration"]')!;
    expect(open.getAttribute('title')).toContain('To Do · medium');
    expect(open.querySelector('[aria-label="done"]')).toBeNull();
    expect(done.querySelector('[aria-label="done"]')).not.toBeNull();
    // An item whose dates are outside the window gets no bar.
    expect(container.querySelector('[aria-label="Long gone"]')).toBeNull();
    expect(container.textContent).not.toContain('No items to display on the timeline');
  });

  it('marks_an_item_that_another_blocks', async () => {
    const container = await mountProjectView(() => <Timeline />, 'timeline', {
      items: [
        makeItem({ id: 'a', title: 'Waits on the API', started_at: startOfThisMonth, due_date: endOfThisMonth }),
        makeItem({ id: 'b', title: 'Builds the API', started_at: startOfThisMonth, due_date: endOfThisMonth }),
      ],
      dependencies: {
        a: [
          {
            id: 'd1',
            source_item_id: 'b',
            target_item_id: 'a',
            dependency_type: 'blocks',
            created_at: '2026-01-01T00:00:00Z',
          },
        ],
      },
    });

    expect(container.querySelector('[aria-label="Waits on the API"] [aria-label="blocked"]')).not.toBeNull();
    expect(container.querySelector('[aria-label="Builds the API"] [aria-label="blocked"]')).toBeNull();
  });

  it('says_so_when_the_project_has_no_items', async () => {
    const container = await mountProjectView(() => <Timeline />, 'timeline', { items: [] });
    expect(container.textContent).toContain('No items to display on the timeline');
    expect(container.textContent).toContain('Go to Board');
    expect(container.textContent).not.toContain('Today'); // no controls without bars to move
  });
});
