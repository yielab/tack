import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import { ExecutionStoreProvider } from '../../shared/state/executionContext';
import { ItemCard } from './Board';
import type { Item } from '../../shared/types';

const flush = () => new Promise((r) => setTimeout(r, 0));
const disposers: Array<() => void> = [];

function makeItem(p: Partial<Item> = {}): Item {
  return {
    id: 'item-1',
    project_id: 'project-1',
    title: 'Fix login bug',
    item_type: 'task',
    status: 'To Do',
    priority: 'medium',
    estimate_unit: 'story_points',
    tags: [],
    sort_order: 0,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
    ...p,
  };
}

async function mount(item: Item) {
  vi.spyOn(globalThis, 'fetch').mockImplementation(
    (async () => new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 })) as typeof fetch,
  );
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ExecutionStoreProvider>
        <MemoryRouter>
          <Route path="/" component={() => <ItemCard item={item} typeLabel="Task" onEdit={() => {}} />} />
        </MemoryRouter>
      </ExecutionStoreProvider>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  await flush();
  return container;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('Board ItemCard', () => {
  it('a_card_shows_needs_your_review', async () => {
    const flagged = await mount(makeItem({ needs_review: true }));
    expect(flagged.textContent).toContain('Needs your review');
    const plain = await mount(makeItem({ needs_review: false }));
    expect(plain.textContent).not.toContain('Needs your review');
  });
});
