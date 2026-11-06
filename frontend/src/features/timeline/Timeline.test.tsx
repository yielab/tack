import { describe, it, expect, afterEach, vi } from 'vitest';
import Timeline from './Timeline';
import { makeItem, mountProjectView, unmountProjectViews } from '../../test/mountProjectView';

afterEach(unmountProjectViews);

const startOfThisMonth = new Date(new Date().getFullYear(), new Date().getMonth(), 3, 12).toISOString();
const endOfThisMonth = new Date(new Date().getFullYear(), new Date().getMonth(), 20, 12).toISOString();
const dayOfThisMonth = (day: number) => new Date(new Date().getFullYear(), new Date().getMonth(), day, 12).toISOString();

/** An epic whose work runs from the 3rd to the 20th of this month. */
const epic = (over: Partial<Parameters<typeof makeItem>[0]> = {}) =>
  makeItem({
    id: 'epic',
    title: 'Checkout',
    item_type: 'epic',
    node: 'group',
    created_at: '2026-01-01T00:00:00Z',
    progress: { total: 2, done: 1, teams: [], needs_you: 0, open_runs: 0, starts: startOfThisMonth, ends: endOfThisMonth },
    ...over,
  });
const percent = (el: Element | null, prop: 'left' | 'width') => parseFloat((el as HTMLElement).style[prop]);

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

  describe('a group', () => {
    const work = [
      makeItem({ id: 'w1', title: 'Cart page', parent_id: 'epic', started_at: startOfThisMonth, due_date: dayOfThisMonth(12) }),
      makeItem({ id: 'w2', title: 'Payment form', parent_id: 'epic', started_at: dayOfThisMonth(8), due_date: endOfThisMonth }),
    ];

    it('is_a_summary_bar_from_the_start_of_its_work_to_the_end_of_it_with_its_work_under_it', async () => {
      const container = await mountProjectView(() => <Timeline />, 'timeline', {
        // The work is listed before its group, and loose work starts after it.
        items: [
          makeItem({ id: 'loose', title: 'Loose task', started_at: dayOfThisMonth(22), due_date: dayOfThisMonth(24) }),
          ...work,
          epic({ due_date: dayOfThisMonth(25) }),
        ],
      });

      const bar = container.querySelector('[data-testid="summary-bar"]')!;
      const first = container.querySelector('[aria-label="Cart page"]') as HTMLElement;
      const last = container.querySelector('[aria-label="Payment form"]') as HTMLElement;
      expect(percent(bar, 'left')).toBeCloseTo(percent(first, 'left'), 5);
      expect(percent(bar, 'left') + percent(bar, 'width')).toBeCloseTo(percent(last, 'left') + percent(last, 'width'), 5);
      expect(bar.querySelector('[data-testid="type-tile"]')?.textContent).toBe('Epic');
      expect(bar.getAttribute('title')).toContain('1 of 2 done');

      // The tree's order: the group, its work, then work outside the group.
      const rows = [...container.querySelectorAll('[data-depth]')].map((row) => `${row.getAttribute('data-depth')}:${row.querySelector('[aria-label]')?.getAttribute('aria-label')}`);
      expect(rows).toEqual(['0:Fold Checkout', '1:Cart page', '1:Payment form', '0:Loose task']);
    });

    it('turns_the_danger_colour_when_its_work_ends_after_it_is_due_and_shows_its_due_date_as_a_diamond', async () => {
      const late = await mountProjectView(() => <Timeline />, 'timeline', { items: [epic({ due_date: dayOfThisMonth(15) }), ...work] });
      const bar = late.querySelector<HTMLElement>('[data-testid="summary-bar"]')!;
      expect(bar.dataset.late).toBe('true');
      expect(bar.style.backgroundColor).toBe('var(--color-danger-600)');
      expect(late.querySelector('[data-testid="due-milestone"]')).not.toBeNull();
      expect(bar.getAttribute('title')).toContain('ends after the day it is due');
      unmountProjectViews();

      const onTime = await mountProjectView(() => <Timeline />, 'timeline', { items: [epic({ due_date: dayOfThisMonth(28) }), ...work] });
      expect(onTime.querySelector<HTMLElement>('[data-testid="summary-bar"]')!.dataset.late).toBe('false');
      expect(onTime.querySelector<HTMLElement>('[data-testid="summary-bar"]')!.style.backgroundColor).not.toBe('var(--color-danger-600)');
      expect(onTime.querySelector('[data-testid="due-milestone"]')).not.toBeNull();
    });

    it('takes_no_drag_and_its_cursor_says_so_while_its_work_still_drags', async () => {
      const container = await mountProjectView(() => <Timeline />, 'timeline', { items: [epic({ due_date: dayOfThisMonth(25) }), ...work] });
      const bar = container.querySelector<HTMLElement>('[data-testid="summary-bar"]')!;
      expect(bar.style.cursor).toBe('not-allowed');
      expect(container.querySelector<HTMLElement>('[data-testid="due-milestone"]')!.style.cursor).toBe('not-allowed');
      expect(container.querySelectorAll('[data-testid="summary-bar"] .cursor-ew-resize')).toHaveLength(0);
      expect(container.querySelector<HTMLElement>('[aria-label="Cart page"]')!.style.cursor).toBe('grab');

      const updates = vi.mocked(fetch).mock.calls.filter(([, init]) => init?.method === 'PATCH');
      bar.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, clientX: 10 }));
      bar.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, clientX: 200 }));
      bar.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
      expect(vi.mocked(fetch).mock.calls.filter(([, init]) => init?.method === 'PATCH')).toHaveLength(updates.length);
    });

    it('folds_its_work_away_and_back', async () => {
      const container = await mountProjectView(() => <Timeline />, 'timeline', { items: [epic(), ...work] });
      const fold = container.querySelector<HTMLButtonElement>('[aria-label="Fold Checkout"]')!;
      expect(fold.getAttribute('aria-expanded')).toBe('true');
      fold.click();
      await Promise.resolve();
      expect(container.querySelector('[aria-label="Cart page"]')).toBeNull();
      const unfold = container.querySelector<HTMLButtonElement>('[aria-label="Unfold Checkout"]')!;
      expect(unfold.getAttribute('aria-expanded')).toBe('false');
      unfold.click();
      await Promise.resolve();
      expect(container.querySelector('[aria-label="Cart page"]')).not.toBeNull();
    });
  });

  it('draws_no_summary_bar_and_no_group_legend_for_a_project_without_groups', async () => {
    const container = await mountProjectView(() => <Timeline />, 'timeline', {
      items: [makeItem({ id: 'a', title: 'Only task', started_at: startOfThisMonth, due_date: endOfThisMonth })],
    });
    expect(container.querySelector('[data-testid="summary-bar"]')).toBeNull();
    expect(container.textContent).not.toContain('cannot be moved');
  });
});
