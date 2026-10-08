import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { ExecutionStoreProvider } from '../state/executionContext';
import LatestRunSummary from './LatestRunSummary';

const flush = () => new Promise((r) => setTimeout(r, 0));
const disposers: Array<() => void> = [];

const ATTEMPT = {
  attempt_id: 'att-1', request_id: 'req-1', attempt_number: 1, runner_id: 'runner-1', fencing_token: 1,
  state: 'succeeded', lease_issued_at: '2026-10-08T12:00:00Z', lease_expires_at: '2026-10-08T12:05:00Z',
  last_heartbeat_at: null, event_checkpoint: null, completion_id: null, workspace_id: null, base_revision: null,
  actual_execution: { harness_kind: 'claude-code', model_id: 'claude-sonnet-5-5' },
  terminal_reason: { result: 'I created test.md with the main idea of the README.', artifacts: [{ kind: 'patch', size_bytes: 120 }] },
  usage: {
    tokens_in: { value: 1_077_873, source: 'measured' }, tokens_out: { value: 3_284, source: 'measured' },
    cache_read_tokens: 791_447, cache_write_tokens: 286_414, model_calls: 6, models: ['claude-sonnet-5-5'],
  },
  started_at: '2026-10-08T12:00:05Z', ended_at: '2026-10-08T12:02:00Z',
  created_at: '2026-10-08T12:00:00Z', updated_at: '2026-10-08T12:02:00Z', model_provenance: null,
  usage_economics: {
    model_token_cost_usd_estimated: { value: 1.3368, source: 'measured' },
    runner_time_cost: { wall_clock_ms: 115_000, cost_usd_estimated: { value: null, source: 'not_measured' } },
  },
  pull_request: null, review: null,
};

function mount(executions: unknown[], onOpen = () => {}) {
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.includes('/attempts')) return new Response(JSON.stringify({ protocol_version: 1, data: [ATTEMPT] }), { status: 200 });
    if (url.includes('/executions')) return new Response(JSON.stringify({ protocol_version: 1, data: executions }), { status: 200 });
    return new Response('{}', { status: 200 });
  }) as typeof fetch);
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ExecutionStoreProvider>
        <LatestRunSummary itemId="item-1" onOpen={onOpen} />
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
  vi.restoreAllMocks();
});

const RUN = { request_id: 'req-1', item_id: 'item-1', state: 'succeeded', cancellation_requested_at: null, created_at: '2026-10-08T12:00:00Z' };

describe('LatestRunSummary', () => {
  it('renders nothing for an item that has never run', async () => {
    const c = mount([]);
    await flush();
    await flush();
    expect(c.querySelector('[data-testid="latest-run"]')).toBeNull();
  });

  it('coming back to a task shows its last run: status, what the agent said, what it used, and a way to the report', async () => {
    const onOpen = vi.fn();
    const c = mount([RUN], onOpen);
    for (let i = 0; i < 4; i++) await flush();
    const panel = c.querySelector('[data-testid="latest-run"]')!;
    expect(panel.textContent).toContain('Finished — needs your review');
    expect(panel.querySelector('[data-testid="latest-run-said"]')!.textContent).toContain('I created test.md');
    expect(panel.querySelector('[data-testid="latest-run-usage"]')!.textContent).toBe(
      '1.08M tokens · ≈ $1.34 · 6 model calls · claude-sonnet-5-5',
    );
    const open = [...panel.querySelectorAll('button')].find((b) => b.textContent === 'See what the agent did')!;
    open.click();
    expect(onOpen).toHaveBeenCalledOnce();
  });
});
