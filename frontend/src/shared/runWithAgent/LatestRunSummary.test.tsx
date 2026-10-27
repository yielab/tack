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
  actual_execution: { harness_kind: 'claude-code', model: 'claude-sonnet-5-5', model_source: 'reported' },
  terminal_reason: { result: 'I created test.md with the main idea of the README.', artifacts: [{ kind: 'patch', size_bytes: 120 }] },
  usage: {
    tokens_in: { value: 1_077_873, source: 'measured' }, tokens_out: { value: 3_284, source: 'measured' },
    cache_read_tokens: 791_447, cache_write_tokens: 286_414, model_calls: 6, models: ['claude-sonnet-5-5'],
  },
  counted_tokens: 289_710,
  started_at: '2026-10-08T12:00:05Z', ended_at: '2026-10-08T12:02:00Z',
  created_at: '2026-10-08T12:00:00Z', updated_at: '2026-10-08T12:02:00Z', model_provenance: null,
  usage_economics: {
    model_token_cost_usd_estimated: { value: 1.3368, source: 'measured' },
    runner_time_cost: { wall_clock_ms: 115_000, cost_usd_estimated: { value: null, source: 'not_measured' } },
  },
  pull_request: null, review: null,
};

function mount(executions: unknown[], onOpen = () => {}, attempts: unknown[] = [ATTEMPT], events: unknown[] = [], reviewInTab = false) {
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.includes('/artifacts')) return new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 });
    if (url.includes('/events')) return new Response(JSON.stringify({ protocol_version: 1, data: events }), { status: 200 });
    if (url.includes('/attempts')) return new Response(JSON.stringify({ protocol_version: 1, data: attempts }), { status: 200 });
    if (url.includes('/executions')) return new Response(JSON.stringify({ protocol_version: 1, data: executions }), { status: 200 });
    return new Response('{}', { status: 200 });
  }) as typeof fetch);
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ExecutionStoreProvider>
        <LatestRunSummary itemId="item-1" itemTitle="Fix it" projectId="proj-1" onOpen={onOpen} reviewInTab={reviewInTab} />
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
      '289 710 tokens (+ 791 447 read from cache, not counted) · ≈ $1.34 (estimated) · 6 model calls · claude-sonnet-5-5',
    );
    const open = [...panel.querySelectorAll('button')].find((b) => b.textContent === 'See what it did')!;
    open.click();
    expect(onOpen).toHaveBeenCalledOnce();
  });

  const buttons = (c: Element) => [...c.querySelectorAll('button')].map((b) => b.textContent);

  it('a run that needs a verdict is an amber card with the opening of its report, its duration, Accept and Reject…', async () => {
    const c = mount([RUN]);
    for (let i = 0; i < 4; i++) await flush();
    const card = c.querySelector('[data-testid="latest-run"]')!;
    expect(card.getAttribute('data-state')).toBe('review');
    expect(card.querySelector('[data-testid="latest-run-meta"]')!.textContent).toBe('1m 55s');
    expect(buttons(card)).toEqual(expect.arrayContaining(['Accept', 'Reject…', 'See what it did']));
  });

  it('while the Execution tab is open the card keeps its state and its links but leaves out Accept and Reject…', async () => {
    const c = mount([RUN], () => {}, [ATTEMPT], [], true);
    for (let i = 0; i < 4; i++) await flush();
    const card = c.querySelector('[data-testid="latest-run"]')!;
    expect(card.getAttribute('data-state')).toBe('review');
    expect(card.querySelector('[data-testid="latest-run-meta"]')!.textContent).toBe('1m 55s');
    expect(buttons(card)).toContain('See what it did');
    expect(buttons(card)).not.toContain('Accept');
    expect(buttons(card)).not.toContain('Reject…');
  });

  it('a run that changed no file shows the answer as what it delivered and offers Accept, not Merge', async () => {
    const answer = { ...ATTEMPT, terminal_reason: { result: 'The answer is 42.' }, merge: { enabled: false, reason: 'No branch was pushed' } };
    const c = mount([RUN], () => {}, [answer]);
    for (let i = 0; i < 4; i++) await flush();
    const card = c.querySelector('[data-testid="latest-run"]')!;
    expect(card.textContent).toContain('What it delivered');
    expect(card.querySelector('[data-testid="latest-run-said"]')!.textContent).toBe('“The answer is 42.”');
    expect(card.querySelector('[data-testid="latest-run-meta"]')!.textContent).toContain('no files changed');
    expect(buttons(card)).toContain('Accept');
    expect(buttons(card)).not.toContain('Merge');
  });

  it('merge, when it cannot run, is disabled and says why in plain words', async () => {
    const pushedNothing = { ...ATTEMPT, merge: { enabled: false, reason: 'No branch was pushed' } };
    const c = mount([RUN], () => {}, [pushedNothing]);
    for (let i = 0; i < 4; i++) await flush();
    const merge = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Merge')!;
    expect(merge.disabled).toBe(true);
    expect(c.querySelector('[data-testid="merge-reason"]')!.textContent).toBe('No branch was pushed');
  });

  it('a running card shows the last steps, how far into its limit it is, Follow live and Stop', async () => {
    const onOpen = vi.fn();
    const events = ['one', 'two', 'three', 'four'].map((text, i) => ({ event_id: `e${i}`, sequence: i, source: 'runner', kind: 'agent.tool_call', payload: { text }, occurred_at: '2026-10-08T12:00:00Z', created_at: '2026-10-08T12:00:00Z' }));
    const limit = { tokens: 300_000, source: 'run', enforced_by: 'claude-code', sentence: '300 000 tokens', used: 120_000 };
    const c = mount([{ ...RUN, state: 'running', limit }], onOpen, [{ ...ATTEMPT, state: 'running', terminal_reason: null, ended_at: null }], events);
    for (let i = 0; i < 6; i++) await flush();
    const card = c.querySelector('[data-testid="latest-run"]')!;
    expect(card.getAttribute('data-state')).toBe('running');
    expect([...card.querySelectorAll('[data-testid="latest-run-steps"] li')].map((li) => li.textContent)).toEqual(['two', 'three', 'four']);
    expect(card.querySelector('[data-testid="run-limit-used"]')!.textContent).toBe('Used 120 000 of 300 000 tokens');
    expect(buttons(card)).toEqual(expect.arrayContaining(['Follow live', 'Stop']));
    [...card.querySelectorAll('button')].find((b) => b.textContent === 'Follow live')!.click();
    expect(onOpen).toHaveBeenCalledOnce();
  });

  it('a run that needs a person is one state, in one colour, and the card opens its question', async () => {
    const onOpen = vi.fn();
    const asking = mount([{ ...RUN, state: 'running' }], onOpen, [{ ...ATTEMPT, state: 'waiting_decision', terminal_reason: null, ended_at: null }]);
    for (let i = 0; i < 6; i++) await flush();
    const card = asking.querySelector('[data-testid="latest-run"]')!;
    const status = card.querySelector('[role="status"]')!;
    expect(status.textContent).toBe('Needs a person');
    expect(status.getAttribute('aria-live')).toBe('polite');
    expect(buttons(card)).not.toContain('Follow live');
    [...card.querySelectorAll('button')].find((b) => b.textContent === 'Open the question')!.click();
    expect(onOpen).toHaveBeenCalledOnce();
  });

  it('a queued card offers to run on another computer', async () => {
    const waiting = { ...RUN, state: 'queued', waiting_reason: 'The computer this run asked for has not answered since 2026-10-19T10:00:00Z', waiting_since: null };
    const c = mount([waiting], () => {}, []);
    for (let i = 0; i < 4; i++) await flush();
    const card = c.querySelector('[data-testid="latest-run"]')!;
    expect(card.getAttribute('data-state')).toBe('queued');
    expect(card.textContent).toContain('The computer this run asked for has not answered since 2026-10-19T10:00:00Z');
    expect(buttons(card)).toContain('Run on another computer');
  });

  it('a queued run says why it waits and for how long; a capacity wait has no duration; a request that failed for want of a runner shows the reason as its failure', async () => {
    const waiting = { ...RUN, state: 'queued', waiting_reason: 'No runner has the model it asks for.', waiting_since: new Date(Date.now() - 5 * 60_000).toISOString() };
    let c = mount([waiting], () => {}, []);
    for (let i = 0; i < 4; i++) await flush();
    expect(c.querySelector('[data-testid="latest-run-waiting"]')!.textContent).toContain('Waiting: No runner has the model it asks for.');
    expect(c.querySelector('[data-testid="latest-run-waiting"]')!.textContent).toContain('5m ago');

    while (disposers.length) disposers.pop()!();
    vi.restoreAllMocks();
    c = mount([{ ...waiting, waiting_reason: 'Every runner is busy.', waiting_since: null }], () => {}, []);
    for (let i = 0; i < 4; i++) await flush();
    expect(c.querySelector('[data-testid="latest-run-waiting"]')!.textContent).toBe('Waiting: Every runner is busy.');

    while (disposers.length) disposers.pop()!();
    vi.restoreAllMocks();
    c = mount([{ ...waiting, state: 'failed' }], () => {}, []);
    for (let i = 0; i < 4; i++) await flush();
    expect(c.querySelector('[data-testid="latest-run-waiting"]')).toBeNull();
    expect(c.querySelector('[data-testid="latest-run-failure"]')!.textContent).toBe('No runner has the model it asks for.');
  });

  it.each([
    ['failed', { code: 'timeout', message: "The run's time ran out after 30 minutes while it waited for an answer", result: 'half done' }, "The run's time ran out after 30 minutes while it waited for an answer"],
    ['failed', { code: 'timeout' }, "The run's time ran out"],
    ['cancelled', { code: 'user_cancelled' }, 'user_cancelled'],
    ['lost', null, 'No reason was reported'],
  ])('a %s run prints why it stopped where the agent\'s words go, never the words themselves', async (state, terminal_reason, why) => {
    const c = mount([{ ...RUN, state }], () => {}, [{ ...ATTEMPT, state, terminal_reason }]);
    for (let i = 0; i < 4; i++) await flush();
    expect(c.querySelector('[data-testid="latest-run-why"]')!.textContent).toBe(why);
    expect(c.querySelector('[data-testid="latest-run-said"]')).toBeNull();
  });

  it.each([
    '300 000 tokens · from the profile Implementer · docket stops the task there',
    "300 000 tokens · set for this run · Tack stops Claude Code's run there",
    "300 000 tokens · set for this run · measured, not enforced (Tack reads Codex's tokens but cannot stop it at a limit yet)",
    "No limit set in Tack · docket stops each step at its own limit (docket's setting)",
    'No limit set in Tack',
  ])('prints the limit sentence the server wrote, composing none: %s', async (sentence) => {
    const limit = { tokens: null, source: 'none', enforced_by: 'measured', sentence };
    const c = mount([{ ...RUN, limit }]);
    for (let i = 0; i < 4; i++) await flush();
    expect(c.querySelector('[data-testid="latest-run-limit"]')!.textContent).toBe(sentence);
  });
});
