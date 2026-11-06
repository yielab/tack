import { describe, it, expect, afterEach, vi } from 'vitest';
import Sprints from './Sprints';
import { makeItem, mountProjectView, unmountProjectViews } from '../../test/mountProjectView';
import type { Sprint } from '../../shared/types';
import { useToasts } from '../../shared/ui/toast';
import { flush } from '../../test/flush';

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

  describe('with groups', () => {
    const epic = (id: string, title: string, over = {}) =>
      makeItem({
        id, title, item_type: 'epic', node: 'group',
        progress: { total: 2, done: 0, teams: [], needs_you: 0, open_runs: 0 },
        ...over,
      });
    const items = [
      epic('e1', 'Checkout'),
      epic('e2', 'Search'),
      epic('e3', 'Empty epic'),
      makeItem({ id: 'a', title: 'Cart page', parent_id: 'e1' }),
      makeItem({ id: 'b', title: 'Payment form', parent_id: 'e1' }),
      makeItem({ id: 'c', title: 'Query box', parent_id: 'e2' }),
      makeItem({ id: 'd', title: 'Loose chore' }),
      makeItem({ id: 'e', title: 'Planned page', parent_id: 'e1', sprint_id: 's1' }),
    ];

    it('sets_the_backlog_out_under_each_group_then_no_parent_and_lists_only_work', async () => {
      const container = await mountProjectView(() => <Sprints />, 'sprints', { sprints: [SPRINT], items });

      const backlog = container.querySelector('[aria-label="Backlog items"]')!;
      const headings = [...backlog.querySelectorAll('[data-testid="group-header"]')];
      // A group with nothing in the backlog has no heading, and a heading is the large one.
      expect(headings.map((h) => h.getAttribute('aria-label'))).toEqual(['Checkout', 'Search']);
      expect(headings.every((h) => h.getAttribute('data-density') === 'heading')).toBe(true);
      expect(headings[0].textContent).toContain('Cart page');
      expect(headings[0].textContent).toContain('Payment form');
      expect(headings[0].textContent).not.toContain('Planned page');
      expect(headings[1].textContent).toContain('Query box');
      expect(backlog.querySelector('[data-testid="no-parent"]')?.textContent).toContain('Loose chore');
      expect(backlog.textContent).toContain('No parent');

      // No card is a group: not in the backlog, not in a lane, and the count is work.
      for (const card of container.querySelectorAll('[draggable="true"]')) {
        expect(card.textContent).not.toMatch(/Checkout|Search|Empty epic/);
      }
      expect(container.querySelector('[aria-label="Sprint lanes"]')!.textContent).toContain('0/1 items');
      expect(backlog.parentElement!.textContent).toContain('4');
    });

    it('shows_no_heading_and_no_no_parent_group_for_a_project_without_groups', async () => {
      const container = await mountProjectView(() => <Sprints />, 'sprints', {
        sprints: [SPRINT],
        items: [makeItem({ id: 'c', title: 'Still in the backlog' })],
      });
      expect(container.querySelector('[data-testid="group-header"]')).toBeNull();
      expect(container.textContent).not.toContain('No parent');
    });

    it('shows_the_servers_sentence_when_a_sprint_refuses_an_item', async () => {
      const container = await mountProjectView(() => <Sprints />, 'sprints', { sprints: [SPRINT], items });
      const normal = vi.mocked(fetch).getMockImplementation()!;
      vi.mocked(fetch).mockImplementation((async (input: RequestInfo | URL, init?: RequestInit) =>
        init?.method === 'PATCH'
          ? new Response(
              JSON.stringify({ error: { status: 422, code: 'group_in_sprint', message: 'An epic does not go in a sprint: its items do.' } }),
              { status: 422, headers: { 'Content-Type': 'application/json' } },
            )
          : normal(input, init)) as typeof fetch);

      const card = [...container.querySelectorAll<HTMLElement>('[draggable="true"]')].find((c) => c.textContent?.includes('Cart page'))!;
      const lane = container.querySelector('[aria-label="Sprint lanes"] .rounded-card, [aria-label="Sprint lanes"] > div > div')!;
      const dataTransfer = { getData: () => 'a', setData: () => {}, effectAllowed: '', dropEffect: '' };
      const drop = new Event('drop', { bubbles: true, cancelable: true });
      Object.defineProperty(drop, 'dataTransfer', { value: dataTransfer });
      expect(card).toBeTruthy();
      lane.dispatchEvent(drop);
      await flush();
      await flush();

      expect(useToasts()().map((toast) => toast.message)).toContain('An epic does not go in a sprint: its items do.');
    });
  });
});
