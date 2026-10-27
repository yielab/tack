import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { ExecutionStoreProvider } from '../state/executionContext';
import ExecutionTimeline from './ExecutionTimeline';
import { flush } from '../../test/flush';
const disposers: Array<() => void> = [];

/** The run the tab shows. Only one is on screen: the latest, until a chip picks another. */
const shownRun = (container: HTMLElement) => container.querySelector<HTMLElement>('[data-testid="run"]')!;

/** Picks the run a chip names ("Run 2") and waits for it to show. */
async function pick(container: HTMLElement, label: string) {
  const chip = [...container.querySelectorAll<HTMLButtonElement>('[data-testid="run-chip"]')].find((b) => b.textContent?.startsWith(label))!;
  chip.click();
  for (let i = 0; i < 4; i++) await flush();
  return shownRun(container);
}

const ROWS = [
  { request_id: 'req-running', item_id: 'item-1', state: 'running', cancellation_requested_at: null, created_at: '2025-01-01T00:00:00Z' },
  { request_id: 'req-needs-operator', item_id: 'item-1', state: 'needs_operator', cancellation_requested_at: null, created_at: '2025-01-02T00:00:00Z' },
  { request_id: 'req-mystery', item_id: 'item-1', state: 'some_future_state', cancellation_requested_at: null, created_at: '2025-01-03T00:00:00Z' },
  { request_id: 'req-leased', item_id: 'item-1', state: 'leased', cancellation_requested_at: null, created_at: '2025-01-01T12:00:00Z' },
  { request_id: 'req-solo', item_id: 'item-solo', state: 'succeeded', cancellation_requested_at: null, created_at: '2025-01-01T00:00:00Z' },
  { request_id: 'req-other-item', item_id: 'item-OTHER', state: 'queued', cancellation_requested_at: null, created_at: '2025-01-01T00:00:00Z' },
];

const FIVE_MIN_AGO = new Date(Date.now() - 5 * 60_000).toISOString();
const NO_RUNNER = 'No runner can run this: none has the model it asks for.';
const WAIT_ROWS = [
  { request_id: 'req-wait-since', item_id: 'item-wait', state: 'queued', cancellation_requested_at: null, created_at: '2025-02-01T00:00:00Z', waiting_reason: NO_RUNNER, waiting_since: FIVE_MIN_AGO },
  { request_id: 'req-wait-capacity', item_id: 'item-wait', state: 'queued', cancellation_requested_at: null, created_at: '2025-02-02T00:00:00Z', waiting_reason: 'Every runner is busy.', waiting_since: null },
  { request_id: 'req-wait-failed', item_id: 'item-wait', state: 'failed', cancellation_requested_at: null, created_at: '2025-02-03T00:00:00Z', waiting_reason: NO_RUNNER, waiting_since: FIVE_MIN_AGO },
];

const ATTEMPT_ROW = {
  attempt_id: 'att-1',
  request_id: 'req-running',
  attempt_number: 1,
  runner_id: 'runner-1',
  fencing_token: 1,
  state: 'running',
  lease_issued_at: '2025-01-01T00:00:00Z',
  lease_expires_at: '2025-01-01T00:05:00Z',
  last_heartbeat_at: null,
  event_checkpoint: null,
  completion_id: null,
  workspace_id: null,
  base_revision: null,
  actual_execution: null,
  terminal_reason: null,
  usage: null,
  started_at: null,
  ended_at: null,
  created_at: '2025-01-01T00:00:00Z',
  updated_at: '2025-01-01T00:00:00Z',
  model_provenance: null,
  usage_economics: {
    model_token_cost_usd_estimated: { value: null, source: 'not_measured' },
    runner_time_cost: { wall_clock_ms: null, cost_usd_estimated: { value: null, source: 'not_measured' } },
  },
};

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
    if (url.includes('/attempts') && url.includes('req-leased')) {
      return new Response(JSON.stringify({ protocol_version: 1, data: [{ ...ATTEMPT_ROW, request_id: 'req-leased' }] }), { status: 200 });
    }
    if (url.includes('/attempts') && url.includes('req-running')) {
      return new Response(JSON.stringify({ protocol_version: 1, data: [ATTEMPT_ROW] }), { status: 200 });
    }
    if (url.includes('/attempts')) {
      return new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 });
    }
    if (url.includes('/executions')) {
      return new Response(JSON.stringify({ protocol_version: 1, data: [...ROWS, ...WAIT_ROWS] }), { status: 200 });
    }
    return new Response(JSON.stringify({}), { status: 200 });
  }) as typeof fetch;
}

function mount(itemId = 'item-1', extra: { itemTitle?: string; projectId?: string; layout?: 'drawer' | 'page' } = {}) {
  cancelCalls = [];
  requeueBody = undefined;
  vi.spyOn(globalThis, 'fetch').mockImplementation(mockFetch());
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ExecutionStoreProvider>
        <ExecutionTimeline itemId={itemId} {...extra} />
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

  it('lists only requests for the given item as chips, newest first, and shows the latest run', async () => {
    const c = mount('item-1');
    await flush();
    await flush();
    expect(c.textContent).not.toContain('req-other-item');
    const chips = [...c.querySelectorAll('[data-testid="run-chip"]')].map((b) => b.textContent);
    // Newest (req-mystery, 01-03) is Run 4 and first; oldest (req-running, 01-01) is Run 1 and last.
    expect(chips[0]).toContain('Run 4');
    expect(chips[chips.length - 1]).toContain('Run 1');
    expect(chips).toHaveLength(4);
    // Only the picked run is on screen, and by default it is the latest.
    expect(c.querySelectorAll('[data-testid="run"]')).toHaveLength(1);
    expect(shownRun(c).textContent).toContain('req-mystery');
    expect(c.querySelector('[data-testid="run-chip"][aria-pressed="true"]')?.textContent).toContain('Run 4');
    expect((await pick(c, 'Run 1')).textContent).toContain('Running');
    expect((await pick(c, 'Run 3')).textContent).toContain('Needs operator');
  });

  it('an item with one run shows no chips, and Run again is offered', async () => {
    const c = mount('item-solo', { itemTitle: 'Solo', projectId: 'p1' });
    await flush();
    await flush();
    expect(c.querySelector('[data-testid="run-chips"]')).toBeNull();
    const again = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Run again') as HTMLButtonElement;
    expect(again.disabled).toBe(false);
  });

  it('on the page the run has a side column that holds its ids; in the drawer the ids stay in the one column', async () => {
    const page = mount('item-1', { layout: 'page' });
    for (let i = 0; i < 6; i++) await flush();
    await pick(page, 'Run 1');
    const side = page.querySelector('[data-testid="execution-side"]')!;
    expect(side.querySelector('[data-testid="attempt-ids"]')?.textContent).toContain('req-running');
    expect(shownRun(page).querySelector('[data-testid="attempt-ids"]')).toBeNull();
    const drawer = mount('item-1');
    for (let i = 0; i < 6; i++) await flush();
    await pick(drawer, 'Run 1');
    expect(drawer.querySelector('[data-testid="execution-side"]')).toBeNull();
    expect(shownRun(drawer).querySelector('[data-testid="attempt-ids"]')?.textContent).toContain('req-running');
  });

  it('Run again is disabled, with its reason, on a screen that does not know the task', async () => {
    const c = mount('item-1');
    await flush();
    await flush();
    const again = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Run again') as HTMLButtonElement;
    expect(again.disabled).toBe(true);
    expect(again.title).toBe('This screen does not know the task');
  });

  it('a queued run says why it waits and for how long; a capacity wait has no duration; a request that failed for want of a runner shows the reason as its failure', async () => {
    const c = mount('item-wait');
    await flush();
    await flush();
    // Run 3 is the latest: the failed one.
    const failed = shownRun(c);
    expect(failed.querySelector('[data-testid="run-waiting"]')).toBeNull();
    expect(failed.querySelector('[data-testid="run-failure-reason"]')!.textContent).toBe(NO_RUNNER);
    const capacity = (await pick(c, 'Run 2')).querySelector('[data-testid="run-waiting"]')!;
    expect(capacity.textContent).toBe('Waiting: Every runner is busy.');
    const since = (await pick(c, 'Run 1')).querySelector('[data-testid="run-waiting"]')!;
    expect(since.textContent).toContain(`Waiting: ${NO_RUNNER}`);
    expect(since.textContent).toContain('5m ago');
  });

  it("a run shows one status, its attempt's: a leased request whose attempt runs reads Running", async () => {
    const c = mount('item-1');
    await flush();
    await flush();
    await pick(c, 'Run 2');
    const row = shownRun(c);
    const badges = [...row.querySelectorAll('span')].map((s) => s.textContent);
    expect(badges).toContain('Running');
    expect(row.textContent).not.toContain('Starting');
    expect(row.textContent).not.toContain('Leased');
    expect(row.querySelector('[data-testid="attempt-working"]')).not.toBeNull();
  });

  it('an unrecognised lifecycle state renders its raw value with a visible caveat, never throwing', async () => {
    const c = mount('item-1');
    await flush();
    await flush();
    expect(c.textContent).toContain('some_future_state');
    expect(c.textContent).toContain('unrecognised state');
  });

  it('fetches and renders real attempts for a request that has some, and an honest "no attempts yet" for one that has none', async () => {
    const c = mount('item-1');
    for (let i = 0; i < 6; i++) await flush();

    const runningRow = await pick(c, 'Run 1');
    expect(runningRow.textContent).toMatch(/Run \d/);
    expect(runningRow.textContent).toContain('runner-1');

    const opRow = await pick(c, 'Run 3');
    expect(opRow.textContent).toContain('No attempts yet.');
  });

  it('Cancel is offered for a non-terminal request and calls the store cancel path', async () => {
    const c = mount('item-1');
    await flush();
    await flush();
    const runningRow = await pick(c, 'Run 1');
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

    const runningRow = await pick(c, 'Run 1');
    expect([...runningRow.querySelectorAll('button')].some((b) => b.textContent?.includes('Reconcile'))).toBe(false);

    const opRow = await pick(c, 'Run 3');
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
