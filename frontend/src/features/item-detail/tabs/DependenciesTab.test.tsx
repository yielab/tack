import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import DependenciesTab from './DependenciesTab';
import type { Item } from '../../../shared/types';

const ITEM = { id: 'i1', project_id: 'p1', title: 'Item One' } as unknown as Item;

const projectItems = [
  { id: 'i1', title: 'Item One' },
  { id: 'i2', title: 'Item Two' },
  { id: 'i3', title: 'Item Three' },
];

const flush = () => new Promise((r) => setTimeout(r, 0));
let deps: unknown[];
let fetchMock: ReturnType<typeof vi.spyOn>;

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

beforeEach(() => {
  deps = [
    { id: 'd1', source_item_id: 'i1', target_item_id: 'i2', dependency_type: 'blocks', created_at: '' },
    { id: 'd2', source_item_id: 'i3', target_item_id: 'i1', dependency_type: 'blocks', created_at: '' },
  ];
  fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    const method = (init as RequestInit)?.method ?? 'GET';
    if (url.endsWith('/api/items/i1/dependencies') && method === 'GET') {
      return Promise.resolve(new Response(JSON.stringify(deps), { status: 200 }));
    }
    if (url.endsWith('/api/projects/p1/items') && method === 'GET') {
      return Promise.resolve(new Response(JSON.stringify(projectItems), { status: 200 }));
    }
    if (method === 'DELETE') {
      deps = deps.filter((d) => !url.endsWith((d as { id: string }).id));
      return Promise.resolve(new Response(JSON.stringify({ deleted: true }), { status: 200 }));
    }
    return Promise.resolve(new Response('{}', { status: 200 }));
  });
});

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <MemoryRouter>
        <Route path="/" component={() => <DependenciesTab item={ITEM} />} />
      </MemoryRouter>
    ),
    container,
  );
  return { container, dispose };
}

describe('DependenciesTab', () => {
  it('renders blocks and blocked-by directions', async () => {
    const { container } = mount();
    await flush();
    const text = container.textContent ?? '';
    expect(text).toContain('Item Two'); // i1 blocks i2
    expect(text).toContain('Item Three'); // i3 blocks i1 → blocked by
  });

  it('surfaces the server cycle-rejection message and adds nothing', async () => {
    const { container } = mount();
    await flush();

    fetchMock.mockImplementationOnce(() =>
      Promise.resolve(new Response('cycle detected', { status: 400 })),
    );

    // pick a target then Add
    const selects = container.querySelectorAll('select');
    const itemSelect = selects[1]; // [0]=Direction, [1]=Item
    itemSelect.value = 'i2';
    itemSelect.dispatchEvent(new Event('change', { bubbles: true }));
    const addBtn = Array.from(container.querySelectorAll('button')).find((b) => b.textContent === 'Add')!;
    addBtn.click();
    await flush();

    expect(container.textContent).toContain('cycle detected');
    // still only the two original rows
    expect(container.querySelectorAll('li').length).toBe(2);
  });

  it('removes a dependency row on delete', async () => {
    const { container } = mount();
    await flush();
    expect(container.querySelectorAll('li').length).toBe(2);

    const removeBtn = container.querySelector<HTMLButtonElement>('button[aria-label="Remove dependency"]')!;
    removeBtn.click();
    await flush();

    expect(container.querySelectorAll('li').length).toBe(1);
  });
});
