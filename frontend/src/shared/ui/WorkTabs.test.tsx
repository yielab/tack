import { describe, it, expect, afterEach } from 'vitest';
import WorkTabs from './WorkTabs';
import { flush } from '../../test/flush';
import { mountProjectView, unmountProjectViews } from '../../test/mountProjectView';

afterEach(unmountProjectViews);

describe('WorkTabs', () => {
  it('offers_Board_List_Calendar_Timeline_and_Sprint_and_no_Table', async () => {
    const c = await mountProjectView(() => <WorkTabs />, 'list', { items: [] });
    const tabs = [...c.querySelectorAll<HTMLAnchorElement>('a')];
    expect(tabs.map((a) => a.textContent)).toEqual(['Board', 'List', 'Calendar', 'Timeline', 'Sprint']);
    expect(tabs.map((a) => a.getAttribute('href'))).not.toContain('/projects/p1/table');
  });

  it('marks_the_List_as_the_current_view_on_the_list_path', async () => {
    const c = await mountProjectView(() => <WorkTabs />, 'list', { items: [] });
    await flush();
    const weight = (name: string) => [...c.querySelectorAll<HTMLAnchorElement>('a')].find((a) => a.textContent === name)!.style.fontWeight;
    expect(weight('List')).toBe('700');
    expect(weight('Board')).toBe('500');
  });
});
