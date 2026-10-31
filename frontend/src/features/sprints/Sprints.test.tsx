import { describe, it, expect, afterEach } from 'vitest';
import Sprints from './Sprints';
import { makeItem, mountProjectView, unmountProjectViews } from '../../test/mountProjectView';
import type { Sprint } from '../../shared/types';

afterEach(unmountProjectViews);

const SPRINT: Sprint = {
  id: 's1',
  project_id: 'p1',
  name: 'Sprint 7',
  status: 'active',
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
};

describe('Sprints', () => {
  it('puts_an_item_in_its_sprint_lane_and_the_rest_in_the_backlog_with_the_lane_progress', async () => {
    const container = await mountProjectView(() => <Sprints />, 'sprints', {
      sprints: [SPRINT],
      items: [
        makeItem({ id: 'a', title: 'Planned and done', sprint_id: 's1', status: 'Done', estimate: 3 }),
        makeItem({ id: 'b', title: 'Planned and open', sprint_id: 's1', estimate: 5 }),
        makeItem({ id: 'c', title: 'Still in the backlog' }),
      ],
    });

    const backlog = container.querySelector('[aria-label="Backlog items"]')!;
    expect(backlog.textContent).toContain('Still in the backlog');
    expect(backlog.textContent).not.toContain('Planned and');

    const lanes = container.querySelector('[aria-label="Sprint lanes"]')!;
    expect(lanes.textContent).toContain('Sprint 7');
    expect(lanes.textContent).toContain('active');
    expect(lanes.textContent).toContain('Planned and done');
    expect(lanes.textContent).toContain('Planned and open');
    expect(lanes.textContent).not.toContain('Still in the backlog');
    // One of the two lane items is in a done status: 3 of 8 points, half the items.
    expect(lanes.textContent).toContain('1/2 items · 3/8 pts');
    expect(lanes.textContent).toContain('50%');
  });

  it('says_so_when_there_is_no_sprint_and_no_item', async () => {
    const container = await mountProjectView(() => <Sprints />, 'sprints', { sprints: [], items: [] });
    expect(container.textContent).toContain('No active sprints yet');
    expect(container.textContent).toContain('Create your first Sprint');
    expect(container.textContent).toContain('All items are assigned to sprints.');
  });
});
