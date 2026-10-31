import { describe, it, expect, afterEach } from 'vitest';
import Calendar from './Calendar';
import { makeItem, mountProjectView, unmountProjectViews } from '../../test/mountProjectView';

afterEach(unmountProjectViews);

/** Noon on the 15th of this month, local time, so no timezone moves it to another day. */
const middleOfThisMonth = new Date(new Date().getFullYear(), new Date().getMonth(), 15, 12).toISOString();

describe('Calendar', () => {
  it('shows_an_item_on_its_due_day_and_the_others_in_the_unscheduled_tray', async () => {
    const container = await mountProjectView(() => <Calendar />, 'calendar', {
      items: [
        makeItem({ id: 'due', title: 'Ship the release', due_date: middleOfThisMonth }),
        makeItem({ id: 'open', title: 'Someday task' }),
      ],
    });

    const monthName = new Date().toLocaleDateString('en-US', { month: 'long', year: 'numeric' });
    expect(container.querySelector('h2')?.textContent).toBe(monthName);

    const scheduled = container.querySelector<HTMLElement>('[title^="Ship the release"]')!;
    expect(scheduled.textContent).toContain('To Do');
    // The day cell that holds it is the 15th.
    expect(scheduled.parentElement?.parentElement?.firstElementChild?.textContent).toBe('15');
    expect(container.querySelector('[title^="Someday task"]')).not.toBeNull();
    expect(container.textContent).toContain('Unscheduled (1)');
    expect(container.textContent).not.toContain('No items to show on the calendar');
  });

  it('says_so_when_the_project_has_no_items', async () => {
    const container = await mountProjectView(() => <Calendar />, 'calendar', { items: [] });
    expect(container.textContent).toContain('No items to show on the calendar');
    expect(container.textContent).toContain('Go to Board');
    expect(container.querySelector('h2')).toBeNull();
  });
});
