import { describe, it, expect, afterEach } from 'vitest';
import List from './List';
import { flush } from '../../test/flush';
import { makeItem, mountProjectView, unmountProjectViews } from '../../test/mountProjectView';

afterEach(unmountProjectViews);

describe('List', () => {
  it('shows_each_root_item_with_the_count_and_keeps_a_child_under_its_collapsed_parent', async () => {
    const container = await mountProjectView(() => <List />, 'list', {
      items: [
        makeItem({ id: 'epic', title: 'Billing epic', item_type: 'epic', created_at: '2026-01-01T00:00:00Z' }),
        makeItem({ id: 'child', title: 'Invoice PDF', parent_id: 'epic', created_at: '2026-01-02T00:00:00Z' }),
        makeItem({ id: 'solo', title: 'Fix typo', created_at: '2026-01-03T00:00:00Z' }),
      ],
    });

    expect(container.querySelector('h1')?.textContent).toBe('Proj');
    expect(container.textContent).toContain('2 items'); // the child is hidden until its parent opens
    expect(container.textContent).toContain('Billing epic');
    expect(container.textContent).toContain('Fix typo');
    expect(container.textContent).not.toContain('Invoice PDF');
    expect(container.textContent).not.toContain('No items yet');

    [...container.querySelectorAll<HTMLButtonElement>('button')]
      .find((b) => b.title === 'Expand all')!
      .click();
    await flush();
    expect(container.textContent).toContain('3 items');
    expect(container.textContent).toContain('Invoice PDF');
  });

  it('says_so_when_the_project_has_no_items', async () => {
    const container = await mountProjectView(() => <List />, 'list', { items: [] });
    expect(container.textContent).toContain('0 items');
    expect(container.textContent).toContain('No items yet');
    expect(container.textContent).toContain('Create your first item');
  });
});
