import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { ProjectItemsProvider, useProjectItems } from './projectItemsContext';

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

describe('useProjectItems outside provider', () => {
  it('throws when called outside ProjectItemsProvider', () => {
    expect(() => useProjectItems()).toThrow('useProjectItems must be used within ProjectItemsProvider');
  });
});
