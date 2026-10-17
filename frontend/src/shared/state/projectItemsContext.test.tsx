import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { ProjectItemsProvider, useProjectItems, shareUnchanged } from './projectItemsContext';
import type { Item } from '../types';

// Mock useParams so we don't need a live router in these unit tests.
vi.mock('@solidjs/router', () => ({
  useParams: () => ({ id: 'test-proj' }),
}));

const ITEMS = [
  { id: 'i1', project_id: 'test-proj', title: 'Alpha', item_type: 'task', status: 'todo', priority: 'medium', tags: [], estimate_unit: 'story_points', sort_order: 0, created_at: '', updated_at: '' },
  { id: 'i2', project_id: 'test-proj', title: 'Beta',  item_type: 'bug',  status: 'done', priority: 'high',   tags: [], estimate_unit: 'story_points', sort_order: 1, created_at: '', updated_at: '' },
];

const flush = () => new Promise<void>((r) => setTimeout(r, 0));
let fetchMock: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
    // The list endpoint returns the paginated envelope, not a bare array.
    new Response(JSON.stringify({ data: ITEMS, total: ITEMS.length, page: 1, per_page: 200 }), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    }),
  );
});

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

function Consumer() {
  const { items } = useProjectItems();
  return <span data-testid="count">{String(items()?.length ?? -1)}</span>;
}

function RefetchConsumer(props: { onRef: (fn: () => void) => void }) {
  const { refetch } = useProjectItems();
  props.onRef(refetch);
  return null;
}

function mount(Child: () => unknown) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ProjectItemsProvider>
        {Child() as never}
      </ProjectItemsProvider>
    ),
    container,
  );
  return { container, dispose };
}

describe('ProjectItemsProvider', () => {
  it('fetches items from GET /api/projects/{id}/items on mount', async () => {
    const { dispose } = mount(() => <Consumer />);
    await flush();
    expect(
      fetchMock.mock.calls.some((c) => String(c[0]).includes('/api/projects/test-proj/items')),
    ).toBe(true);
    dispose();
  });

  it('exposes the fetched items to consumers via items()', async () => {
    const { container, dispose } = mount(() => <Consumer />);
    await flush();
    expect(container.querySelector('[data-testid="count"]')?.textContent).toBe('2');
    dispose();
  });

  it('refetch() triggers a second GET /items call', async () => {
    let refetchFn: (() => void) | undefined;
    mount(() => <RefetchConsumer onRef={(fn) => { refetchFn = fn; }} />);
    await flush();
    const callsBefore = fetchMock.mock.calls.length;
    refetchFn?.();
    await flush();
    expect(fetchMock.mock.calls.length).toBeGreaterThan(callsBefore);
  });
});

describe('a refetch keeps what is shown', () => {
  it('keeps loading() false and items() set while it refetches', async () => {
    let api!: ReturnType<typeof useProjectItems>;
    mount(() => { api = useProjectItems(); return null; });
    expect(api.loading()).toBe(true); // the first load has nothing to show yet
    await flush();
    expect(api.loading()).toBe(false);

    let release!: (r: Response) => void;
    fetchMock.mockImplementationOnce(() => new Promise<Response>((r) => { release = r; }));
    api.refetch();
    expect(api.loading()).toBe(false);
    expect(api.items()?.length).toBe(2);
    release(new Response(JSON.stringify({ data: ITEMS, total: 2, page: 1, per_page: 200 }), { status: 200 }));
    await flush();
  });

  it('hands back the same object for an item that did not change', async () => {
    let api!: ReturnType<typeof useProjectItems>;
    mount(() => { api = useProjectItems(); return null; });
    await flush();
    const [alpha, beta] = api.items()!;

    const changed = [ITEMS[0], { ...ITEMS[1], title: 'Beta, renamed' }];
    fetchMock.mockResolvedValueOnce(
      new Response(JSON.stringify({ data: changed, total: 2, page: 1, per_page: 200 }), { status: 200 }),
    );
    api.refetch();
    await flush();
    const [alpha2, beta2] = api.items()!;
    expect(alpha2).toBe(alpha);
    expect(beta2).not.toBe(beta);
    expect(beta2.title).toBe('Beta, renamed');
  });
});

describe('shareUnchanged', () => {
  const a = { id: 'a', title: 'A' } as Item;
  const b = { id: 'b', title: 'B' } as Item;

  it('returns the new list as is when there was none before', () => {
    const next = [a];
    expect(shareUnchanged(undefined, next)).toBe(next);
  });

  it('reuses unchanged objects, takes changed and new ones, drops removed ones', () => {
    const out = shareUnchanged([a, b], [{ ...a }, { ...b, title: 'B2' }, { id: 'c', title: 'C' } as Item]);
    expect(out[0]).toBe(a);
    expect(out[1]).not.toBe(b);
    expect(out.map((i) => i.id)).toEqual(['a', 'b', 'c']);
  });
});

describe('useProjectItems outside provider', () => {
  it('throws when called outside ProjectItemsProvider', () => {
    expect(() => useProjectItems()).toThrow('useProjectItems must be used within ProjectItemsProvider');
  });
});
