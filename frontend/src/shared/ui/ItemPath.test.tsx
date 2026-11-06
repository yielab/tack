import { describe, it, expect, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import ItemPath from './ItemPath';

const items = [
  { id: 'epic', title: 'Checkout', parent_id: null },
  { id: 'story', title: 'Payments', parent_id: 'epic' },
  { id: 'task', title: 'Card form', parent_id: 'story' },
];
const byId = new Map(items.map((i) => [i.id, i]));

const disposers: Array<() => void> = [];
afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
});
function mount(id: string) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  disposers.push(render(() => (
    <MemoryRouter><Route path="/" component={() => <ItemPath item={byId.get(id)!} byId={byId} projectId="p1" />} /></MemoryRouter>
  ), container));
  return container;
}

describe('ItemPath', () => {
  it('shows_the_ancestors_as_links_to_their_pages_outermost_first', () => {
    const links = [...mount('task').querySelectorAll('a')];
    expect(links.map((a) => a.textContent)).toEqual(['Checkout', 'Payments']);
    expect(links.map((a) => a.getAttribute('href'))).toEqual(['/projects/p1/items/epic', '/projects/p1/items/story']);
    expect(mount('task').querySelector('nav')!.getAttribute('aria-label')).toBe('Path');
  });

  it('draws_nothing_for_an_item_at_the_top', () => {
    expect(mount('epic').querySelector('[data-testid="item-path"]')).toBeNull();
  });
});
