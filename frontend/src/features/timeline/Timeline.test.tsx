import { describe, it, expect, afterEach } from 'vitest';
import Timeline from './Timeline';
import { makeItem, mountProjectView, unmountProjectViews } from '../../test/mountProjectView';
import { flush } from '../../test/flush';

afterEach(unmountProjectViews);

const middleOfThisMonth = new Date(new Date().getFullYear(), new Date().getMonth(), 15, 12).toISOString();
const items = [
  makeItem({ id: 'due', title: 'Ship the release', started_at: middleOfThisMonth, due_date: middleOfThisMonth }),
  makeItem({ id: 'open', title: 'Someday task' }),
];
const pressed = (container: HTMLElement, label: string) =>
  container.querySelector(`[aria-label="Layout"] button[aria-pressed="true"]`)?.textContent === label;

describe('Timeline', () => {
  it('opens_on_bars_with_a_switch_beside_the_title', async () => {
    const container = await mountProjectView(() => <Timeline />, 'timeline', { items });
    expect(container.querySelector('h1')?.textContent).toBe('Timeline');
    expect(pressed(container, 'Bars')).toBe(true);
    expect(container.querySelector('[aria-label="Ship the release"]')).not.toBeNull();
    expect(container.textContent).not.toContain('Unscheduled');
  });

  it('shows_the_month_when_the_switch_says_so_and_goes_back_to_bars', async () => {
    const container = await mountProjectView(() => <Timeline />, 'timeline', { items });
    const buttons = () => [...container.querySelectorAll<HTMLButtonElement>('[aria-label="Layout"] button')];

    buttons().find((b) => b.textContent === 'Month')!.click();
    await flush();
    expect(pressed(container, 'Month')).toBe(true);
    expect(container.textContent).toContain('Unscheduled (1)');
    expect(container.querySelector('[aria-label="Ship the release"]')).toBeNull();
    expect(container.textContent).toContain('Previous');

    buttons().find((b) => b.textContent === 'Bars')!.click();
    await flush();
    expect(pressed(container, 'Bars')).toBe(true);
    expect(container.querySelector('[aria-label="Ship the release"]')).not.toBeNull();
    expect(container.textContent).not.toContain('Unscheduled');
  });

  it('says_so_when_the_project_has_no_items_in_either_layout', async () => {
    const container = await mountProjectView(() => <Timeline />, 'timeline', { items: [] });
    expect(container.textContent).toContain('No items to show on the timeline');
    expect(container.textContent).toContain('Go to Board');
    expect(container.textContent).not.toContain('Today'); // no controls without items to move
    expect(container.querySelector('[aria-label="Layout"]')).not.toBeNull();
  });
});
