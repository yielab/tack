import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { ExecutionStoreProvider } from '../state/executionContext';
import ExecutionTimeline from './ExecutionTimeline';

const flush = () => new Promise((r) => setTimeout(r, 0));
const disposers: Array<() => void> = [];

const ROWS = [
  { request_id: 'req-running', item_id: 'item-1', state: 'running', cancellation_requested_at: null, created_at: '2025-01-01T00:00:00Z' },
  { request_id: 'req-needs-operator', item_id: 'item-1', state: 'needs_operator', cancellation_requested_at: null, created_at: '2025-01-02T00:00:00Z' },
  { request_id: 'req-mystery', item_id: 'item-1', state: 'some_future_state', cancellation_requested_at: null, created_at: '2025-01-03T00:00:00Z' },
  { request_id: 'req-other-item', item_id: 'item-OTHER', state: 'queued', cancellation_requested_at: null, created_at: '2025-01-01T00:00:00Z' },
];

let cancelCalls: string[];
let requeueBody: unknown;

function mockFetch(): typeof fetch {
  return (async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    if (url.endsWith('/cancel') && init?.method === 'POST') {
      cancelCalls.push(url);
      return new Response(JSON.stringify({ protocol_version: 1, request_id: 'req-running', state: 'cancellation_requested' }), { status: 200 });
    }
    if (url.endsWith('/requeue') && init?.method === 'POST') {
      requeueBody = JSON.parse(String(init.body));
      return new Response(
        JSON.stringify({ protocol_version: 1, request_id: 'req-needs-operator', state: 'queued', recovered_from: 'needs_operator', replayed: false }),
        { status: 200 },
      );
    }
    if (url.includes('/executions')) {
      return new Response(JSON.stringify({ protocol_version: 1, data: ROWS }), { status: 200 });
    }
    return new Response(JSON.stringify({}), { status: 200 });
  }) as typeof fetch;
}

function mount(itemId = 'item-1') {
  cancelCalls = [];
  requeueBody = undefined;
  vi.spyOn(globalThis, 'fetch').mockImplementation(mockFetch());
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ExecutionStoreProvider>
        <ExecutionTimeline itemId={itemId} />
      </ExecutionStoreProvider>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('ExecutionTimeline', () => {
  it('shows the empty state for an item with zero requests', async () => {
    const c = mount('item-with-nothing');
    await flush();
    await flush();
    expect(c.textContent).toContain('No execution requests yet');
  });

  it('lists only requests for the given item, newest first, with a state badge', async () => {
    const c = mount('item-1');
    await flush();
    await flush();
    expect(c.textContent).not.toContain('req-other-item');
    const ids = [...c.querySelectorAll('li')].map((li) => li.textContent);
    // newest (req-mystery, 01-03) first, oldest (req-running, 01-01) last.
    expect(ids[0]).toContain('req-mystery');
    expect(ids[ids.length - 1]).toContain('req-running');
    expect(c.textContent).toContain('Running');
    expect(c.textContent).toContain('Needs operator');
  });

  it('an unrecognised lifecycle state renders its raw value with a visible caveat, never throwing', async () => {
    const c = mount('item-1');
    await flush();
    await flush();
    expect(c.textContent).toContain('some_future_state');
    expect(c.textContent).toContain('unrecognised state');
  });

  it('every request shows the typed "attempt history not available" reason (Gap 2) instead of a fake empty list', async () => {
    const c = mount('item-1');
    await flush();
    await flush();
    expect(c.textContent).toContain("Attempt history isn't available yet");
    expect(c.textContent).toContain('III-E2.md');
  });

  it('Cancel is offered for a non-terminal request and calls the store cancel path', async () => {
    const c = mount('item-1');
    await flush();
    await flush();
    const runningRow = [...c.querySelectorAll('li')].find((li) => li.textContent?.includes('req-running'))!;
    const cancelBtn = [...runningRow.querySelectorAll('button')].find((b) => b.textContent === 'Cancel')!;
    cancelBtn.click();
    await flush();
    await flush();
    expect(cancelCalls.some((u) => u.includes('req-running'))).toBe(true);
  });

  it('Reconcile is offered ONLY for a needs_operator request, and requires an explicit recovery key + reason', async () => {
    const c = mount('item-1');
    await flush();
    await flush();

    const runningRow = [...c.querySelectorAll('li')].find((li) => li.textContent?.includes('req-running'))!;
    expect([...runningRow.querySelectorAll('button')].some((b) => b.textContent?.includes('Reconcile'))).toBe(false);

    const opRow = [...c.querySelectorAll('li')].find((li) => li.textContent?.includes('req-needs-operator'))!;
    const reconcileBtn = [...opRow.querySelectorAll('button')].find((b) => b.textContent?.includes('Reconcile'))!;
    reconcileBtn.click();
    await flush();

    const confirmBtn = [...opRow.querySelectorAll('button')].find((b) => b.textContent === 'Confirm requeue') as HTMLButtonElement;
    // No silent auto-retry: the confirm control starts disabled until both
    // fields are filled by hand.
    expect(confirmBtn.disabled).toBe(true);

    const inputs = [...opRow.querySelectorAll('input[type="text"], input:not([type])')] as HTMLInputElement[];
    expect(inputs.length).toBeGreaterThanOrEqual(2);
    inputs[0].value = 'recovery-key-123';
    inputs[0].dispatchEvent(new Event('input', { bubbles: true }));
    inputs[1].value = 'confirmed safe to requeue';
    inputs[1].dispatchEvent(new Event('input', { bubbles: true }));
    await flush();

    expect(confirmBtn.disabled).toBe(false);
    confirmBtn.click();
    await flush();
    await flush();

    expect(requeueBody).toEqual({ recovery_key: 'recovery-key-123', reason: 'confirmed safe to requeue' });
  });
});
