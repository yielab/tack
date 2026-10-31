import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { onMount } from 'solid-js';
import { ExecutionStoreProvider, useExecutionStore } from '../../shared/state/executionContext';
import { useItemEditor } from './useItemEditor';

const ID = 'item-1';

let itemStatus = 'todo';
let runState = 'running';
let itemReads = 0;

function respond(url: string): unknown {
  if (url.endsWith(`/api/items/${ID}`)) {
    itemReads += 1;
    return { item: { id: ID, project_id: 'p1', title: 'T', item_type: 'task', status: itemStatus, priority: 'medium', tags: [] }, roles: [], dependencies: [] };
  }
  const run = { request_id: 'req-1', item_id: ID, state: runState, cancellation_requested_at: null, created_at: '2026-10-31T10:00:00Z' };
  if (url.includes('/executions/req-1')) return { protocol_version: 1, ...run };
  if (url.includes('/executions')) return { protocol_version: 1, data: [run] };
  return [];
}

const Probe = () => {
  const store = useExecutionStore();
  const editor = useItemEditor(() => ID);
  onMount(() => void store.loadList(ID));
  return <p data-testid="status">{editor.item()?.status ?? 'none'}</p>;
};

beforeEach(() => {
  itemStatus = 'todo';
  runState = 'running';
  itemReads = 0;
  vi.useFakeTimers();
  vi.spyOn(globalThis, 'fetch').mockImplementation((input: RequestInfo | URL) =>
    Promise.resolve(new Response(JSON.stringify(respond(String(input))), { status: 200, headers: { 'Content-Type': 'application/json', ETag: '"1"' } })),
  );
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('useItemEditor follows the run', () => {
  it('re-reads the item when its run finishes, with no reload, and stops once it is finished', async () => {
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(() => (
      <ExecutionStoreProvider>
        <Probe />
      </ExecutionStoreProvider>
    ), container);
    await vi.advanceTimersByTimeAsync(10);
    expect(container.querySelector('[data-testid="status"]')?.textContent).toBe('todo');

    // The run finishes and the item moves on the server; the page is not touched.
    runState = 'succeeded';
    itemStatus = 'done';
    await vi.advanceTimersByTimeAsync(4100);
    expect(container.querySelector('[data-testid="status"]')?.textContent).toBe('done');

    const reads = itemReads;
    await vi.advanceTimersByTimeAsync(12000);
    expect(itemReads).toBe(reads);
    dispose();
  });

  it('does not re-read the item when it opens on a finished history', async () => {
    runState = 'succeeded';
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(() => (
      <ExecutionStoreProvider>
        <Probe />
      </ExecutionStoreProvider>
    ), container);
    await vi.advanceTimersByTimeAsync(10);
    expect(itemReads).toBe(1);
    await vi.advanceTimersByTimeAsync(9000);
    expect(itemReads).toBe(1);
    dispose();
  });
});
