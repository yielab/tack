import { describe, it, expect, vi, afterEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { render } from 'solid-js/web';
import { createSignal } from 'solid-js';
import AttemptList, { attemptRunState } from './AttemptList';
import type { AttemptSummary } from '../execution';
import { flush } from '../../test/flush';
import ready from '../../../../docs/contracts/mrp-v1/fixtures/ready.json';
import { decisionTokenStore } from '../execution';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve()) }));
const disposers: Array<() => void> = [];

function attempt(overrides: Partial<AttemptSummary> = {}): AttemptSummary {
  return {
    attempt_id: 'att_1',
    request_id: 'exec_1',
    attempt_number: 1,
    runner_id: 'runner_1',
    fencing_token: 1,
    state: 'succeeded',
    lease_issued_at: '2026-08-06T12:00:00Z',
    lease_expires_at: '2026-08-06T12:05:00Z',
    last_heartbeat_at: null,
    event_checkpoint: null,
    completion_id: null,
    workspace_id: null,
    base_revision: null,
    actual_execution: null,
    terminal_reason: null,
    usage: null,
    counted_tokens: null,
    started_at: '2026-08-06T12:00:05Z',
    ended_at: '2026-08-06T12:05:00Z',
    created_at: '2026-08-06T12:00:00Z',
    updated_at: '2026-08-06T12:05:00Z',
    model_provenance: null,
    usage_economics: {
      model_token_cost_usd_estimated: { value: null, source: 'not_measured' },
      runner_time_cost: { wall_clock_ms: null, cost_usd_estimated: { value: null, source: 'not_measured' } },
    },
    pull_request: null,
    review: null,
    ...overrides,
  };
}

function mount(attempts: AttemptSummary[]) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <AttemptList requestId="exec_1" attempts={attempts} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

afterEach(() => {
  localStorage.clear();
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('AttemptList', () => {
  it('a refresh that brings the same attempt changed keeps what was opened open', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 })),
    );
    // A finished run keeps its activity folded; opening it is the person's doing.
    const [attempts, setAttempts] = createSignal([attempt({ state: 'succeeded' })]);
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(() => <AttemptList requestId="exec_1" attempts={attempts()} />, container);
    disposers.push(() => { dispose(); container.remove(); });
    const activity = () => container.querySelector('[data-testid="run-activity"] > button') as HTMLButtonElement;
    expect(activity().getAttribute('aria-expanded')).toBe('false');
    activity().click();
    await flush();

    setAttempts([attempt({ state: 'succeeded', event_checkpoint: 'chk:2', updated_at: '2026-08-06T12:06:00Z' })]);
    await flush();
    expect(activity().getAttribute('aria-expanded')).toBe('true');
  });

  it('several attempts are chips, newest first; the newest shows, and a chip picks another', async () => {
    const c = mount([attempt(), attempt({ attempt_id: 'att_2', attempt_number: 2, state: 'failed', runner_id: 'runner_2' })]);
    const chips = Array.from(c.querySelectorAll('[data-testid="attempt-chips"] [data-testid="run-chip"]')) as HTMLButtonElement[];
    expect(chips.map((b) => b.textContent)).toEqual(['Attempt 2, Failed', 'Attempt 1, Finished']);
    expect(chips[0].getAttribute('aria-pressed')).toBe('true');
    // One attempt on screen: the newest.
    expect(c.querySelectorAll('[data-testid="attempt"]')).toHaveLength(1);
    expect(c.textContent).toContain('runner_2');
    expect(c.textContent).not.toContain('runner_1');
    chips[1].click();
    await flush();
    expect(c.querySelectorAll('[data-testid="attempt"]')).toHaveLength(1);
    expect(c.textContent).toContain('runner_1');
    expect(c.textContent).not.toContain('runner_2');
  });

  it('a single attempt carries no number and no chips: the run is numbered instead', () => {
    const c = mount([attempt()]);
    expect(c.textContent).not.toContain('Attempt 1');
    expect(c.querySelector('[data-testid="attempt-chips"]')).toBeNull();
  });

  it('the review banner holds Accept and Reject… once, with an optional note, in the verdict that opens the run', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 })),
    );
    const c = mount([attempt({ terminal_reason: { workspace_kept_at: '/w/1' } })]);
    const banner = c.querySelector('[data-testid="review-banner"]')!;
    expect(banner.textContent).toContain('waiting for your verdict');
    expect(c.querySelectorAll('[data-testid="review-actions"]')).toHaveLength(1);
    expect(banner.contains(c.querySelector('[data-testid="review-actions"]'))).toBe(true);
    // The verdict comes before everything else in the run, and holds the banner.
    const verdict = c.querySelector('[data-testid="attempt"]')!.firstElementChild!;
    expect(verdict.getAttribute('data-testid')).toBe('run-verdict');
    expect(verdict.contains(banner)).toBe(true);
    (Array.from(banner.querySelectorAll('button')).find((b) => b.textContent === 'Reject…') as HTMLButtonElement).click();
    await flush();
    expect(banner.querySelector('input[aria-label="Why it is rejected (optional)"]')).not.toBeNull();
  });

  it('a run that needs no verdict has no banner', () => {
    expect(mount([attempt({ state: 'failed' })]).querySelector('[data-testid="review-banner"]')).toBeNull();
  });

  it('a terminal attempt with a kept workspace and no patch shows Accept and Reject…', () => {
    const c = mount([attempt({ terminal_reason: { workspace_kept_at: '/w/1' } })]);
    // A single attempt's status is the run's heading (`ExecutionTimeline`), not repeated here.
    expect(c.textContent).not.toContain('needs your review');
    expect(Array.from(c.querySelectorAll('button')).map((b) => b.textContent)).toEqual(
      expect.arrayContaining(['Accept', 'Reject…']),
    );
  });

  it("renders the user's rejected attempt leading with why it stopped, not as a matched request", () => {
    const c = mount([
      attempt({
        state: 'failed',
        started_at: null,
        ended_at: null,
        terminal_reason: {
          code: 'harness_rejected',
          message:
            'requested model "claude" is not supported by claude-code',
        },
        model_provenance: { requested: 'claude-sonnet-5-5', served: 'claude-sonnet-5-5', source: 'requested' },
        actual_execution: null,
      }),
    ]);
    expect(c.textContent).toContain('Why it stopped:');
    expect(c.textContent).toContain('Did not start — requested model');
    expect(c.textContent).toContain('Requested claude-sonnet-5-5 (not confirmed by the run)');
    expect(c.textContent).not.toContain('Matched request');
  });

  it('a matched attempt whose actual_execution observed the model says it ran on it', () => {
    const c = mount([
      attempt({
        model_provenance: { requested: 'm1', served: 'm1', source: 'reported' },
        actual_execution: { model_source: 'reported' },
      }),
    ]);
    expect(c.textContent).toContain('Ran on m1, as requested.');
  });

  it('leads the usage with tokens, counts the cache, names the models and shows no dollar figure', () => {
    const c = mount([
      attempt({
        actual_execution: { harness_kind: 'claude-code', model: 'claude-sonnet-5-5', model_source: 'reported' },
        model_provenance: { requested: null, served: 'claude-sonnet-5-5', source: 'reported' },
        usage: {
          tokens_in: { value: 1_077_873, source: 'measured' },
          tokens_out: { value: 3_284, source: 'measured' },
          cache_read_tokens: 791_447,
          cache_write_tokens: 286_414,
          model_calls: 6,
          models: ['claude-sonnet-5-5'],
        },
        counted_tokens: 289_710,
        usage_economics: {
          model_token_cost_usd_estimated: { value: 1.3368, source: 'measured' },
          runner_time_cost: { wall_clock_ms: 113_000, cost_usd_estimated: { value: null, source: 'not_measured' } },
        },
      }),
    ]);
    const usage = c.querySelector('[data-testid="attempt-usage"]')!;
    // The headline is the server's counted figure: the 791 447 cache reads are not in it.
    expect(usage.querySelector('[data-testid="usage-counted"]')!.textContent).toBe('289 710 tokens counted');
    expect(usage.querySelector('[data-testid="usage-counted"]')!.getAttribute('title')).toContain('289 710'.replace(' ', ','));
    expect(usage.querySelector('[data-testid="usage-in-out"]')!.textContent).toBe('1 077 873 in · 3 284 out');
    expect(usage.querySelector('[data-testid="usage-in-out"]')!.getAttribute('title')).toContain('1,077,873 tokens in');
    expect(usage.textContent).toContain('Of the input: 791K read from cache (not counted), 286K written to cache.');
    expect(usage.textContent).not.toContain('new.');
    expect(usage.textContent).toContain('6 model calls');
    expect(usage.textContent).toContain('1m 53s');
    expect(usage.querySelector('[data-testid="usage-models"]')!.textContent).toBe('claude-sonnet-5-5');
    // Limits and reports are in tokens: the harness's dollar estimate stays in the data and is not shown.
    expect(usage.querySelector('[data-testid="usage-cost"]')).toBeNull();
    expect(usage.textContent).not.toMatch(/\$|Cost|estimate/);
    // One time figure: the runner-time dollar tile, never measured, is gone.
    expect(usage.textContent!.match(/Not measured/g)).toBeNull();
  });

  it('reads the cache and calls from the result line of a claude-code run recorded before the runner did', () => {
    const c = mount([
      attempt({
        counted_tokens: 3_296,
        usage: { tokens_in: { value: 12, source: 'measured' }, tokens_out: { value: 3_284, source: 'measured' } },
        terminal_reason: {
          type: 'result',
          num_turns: 6,
          usage: { input_tokens: 12, cache_read_input_tokens: 791_447, cache_creation_input_tokens: 286_414 },
          modelUsage: { 'claude-sonnet-5-5': {} },
        },
      }),
    ]);
    // The server counted the old row's uncached input as it was stored; the cache reads stay out of the headline.
    expect(c.querySelector('[data-testid="usage-counted"]')!.textContent).toBe('3 296 tokens counted');
    expect(c.querySelector('[data-testid="usage-in-out"]')!.textContent).toBe('1 077 873 in · 3 284 out');
    expect(c.textContent).toContain('6 model calls');
    expect(c.querySelector('[data-testid="usage-models"]')!.textContent).toBe('claude-sonnet-5-5');
  });

  it('keeps the in and out figures when the server counted nothing, and never adds a counted headline of its own', () => {
    const c = mount([
      attempt({
        counted_tokens: null,
        usage: {
          tokens_in: { value: 1_077_873, source: 'measured' },
          tokens_out: { value: 3_284, source: 'measured' },
          cache_read_tokens: 791_447,
        },
      }),
    ]);
    expect(c.querySelector('[data-testid="usage-tokens"]')!.textContent).toBe('1.08M tokens in · 3.3K out');
    expect(c.querySelector('[data-testid="usage-counted"]')).toBeNull();
  });

  it('a row whose cache reads dwarf its new input is headlined by the counted figure, not by in plus out', () => {
    const c = mount([
      attempt({
        counted_tokens: 12_000,
        usage: {
          tokens_in: { value: 2_000_000, source: 'measured' },
          tokens_out: { value: 4_000, source: 'measured' },
          cache_read_tokens: 1_992_000,
        },
      }),
    ]);
    expect(c.querySelector('[data-testid="usage-counted"]')!.textContent).toBe('12 000 tokens counted');
    expect(c.querySelector('[data-testid="usage-in-out"]')!.textContent).toBe('2 000 000 in · 4 000 out');
    expect(c.textContent).toContain('Of the input: 1.99M read from cache (not counted).');
  });

  it('a running attempt says the agent is working instead of showing empty figures', () => {
    const c = mount([attempt({ state: 'running', ended_at: null })]);
    expect(c.querySelector('[data-testid="attempt-working"]')!.textContent).toContain('The agent is working');
    expect(c.querySelector('[data-testid="attempt-usage"]')).toBeNull();
    expect(c.textContent).not.toContain('Not yet reported');
  });

  it('renders "Not measured" (exact) for the real-world every-response-today usage_economics shape — never $0.00', () => {
    const c = mount([attempt()]);
    expect(c.textContent).toContain('Not measured');
    expect(c.textContent).not.toContain('$0.00');
  });

  it('shows no dollar figure even when the harness reported one: the cost stays in the data, the screen speaks in tokens', () => {
    const c = mount([
      attempt({
        usage_economics: {
          model_token_cost_usd_estimated: { value: 0.42, source: 'measured' },
          runner_time_cost: { wall_clock_ms: 295_000, cost_usd_estimated: { value: null, source: 'not_measured' } },
        },
      }),
    ]);
    expect(c.textContent).not.toContain('$0.42');
    expect(c.textContent).not.toContain('Cost');
    expect(c.textContent).toContain('4m 55s');
  });

  it('renders model provenance honestly: null is "Model not reported", not a fabricated match', () => {
    const c = mount([attempt({ model_provenance: null })]);
    expect(c.textContent).toContain('Model not reported');
  });

  it('renders a mismatched provenance with both requested and actual values visible', () => {
    const c = mount([
      attempt({
        model_provenance: { requested: 'opaque/model-alpha', served: 'opaque/model-beta', source: 'reported' },
      }),
    ]);
    expect(c.textContent).toContain('Mismatched request');
    expect(c.textContent).toContain('opaque/model-alpha');
    expect(c.textContent).toContain('opaque/model-beta');
  });

  it('the questions are a muted line in the activity fold, and the files a table of their own section', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 })),
    );
    const c = mount([attempt()]);
    await flush();
    await flush();
    const activity = c.querySelector('[data-testid="run-activity"]') as HTMLElement;
    expect(activity.querySelector('button')!.getAttribute('aria-expanded')).toBe('false');
    // An agent that asked nothing gets one grey line, not a heading over nothing.
    expect(activity.textContent).not.toContain('Questions from the agent');
    expect(activity.querySelector('[data-testid="asked-nothing"]')!.textContent).toBe('The agent has asked nothing yet');
    expect(c.querySelector('[data-section="files"]')!.textContent).toContain('No artifacts yet');
  });

  it('an attempt that needs a person says so in words and has a button that opens its question', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 })),
    );
    const asking = mount([attempt({ state: 'waiting_decision', ended_at: null })]);
    // It waits for an answer, so the question has its own section and the activity is open already.
    await flush();
    expect(asking.querySelector('[data-section="needs"]')).not.toBeNull();
    expect(asking.querySelector('[data-testid="run-activity"] > button')!.getAttribute('aria-expanded')).toBe('true');
    expect([...asking.querySelectorAll('button')].some((b) => b.textContent === 'Open the question')).toBe(true);

    const ended = mount([attempt({ state: 'failed', terminal_reason: { code: 'needs_human' } })]);
    expect(ended.textContent).toContain('The agent needed a person to decide');
    expect(ended.textContent).not.toContain('needs_human');
    expect([...ended.querySelectorAll('button')].some((b) => b.textContent === 'Open the question')).toBe(true);

    const plain = mount([attempt()]);
    expect([...plain.querySelectorAll('button')].some((b) => b.textContent === 'Open the question')).toBe(false);
  });

  it('with the Now panel on screen the run holds no second button for the question; without it, it keeps its own', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 })),
    );
    const mountNow = (attempts: AttemptSummary[], nowShown: boolean) => {
      const container = document.createElement('div');
      document.body.appendChild(container);
      const dispose = render(() => <AttemptList requestId="exec_1" attempts={attempts} nowShown={nowShown} />, container);
      disposers.push(() => { dispose(); container.remove(); });
      return container;
    };
    const opens = (c: HTMLElement) => [...c.querySelectorAll('button')].filter((b) => b.textContent === 'Open the question').length;
    const waiting = [attempt({ state: 'waiting_decision', ended_at: null })];
    const ended = [attempt({ state: 'failed', terminal_reason: { code: 'needs_human' } })];
    expect(opens(mountNow(waiting, true))).toBe(0);
    expect(opens(mountNow(ended, true))).toBe(0);
    expect(opens(mountNow(waiting, false))).toBe(1);
    expect(opens(mountNow(ended, false))).toBe(1);
  });

  it('an attempt that starts to wait for an answer while the page is open shows its question by itself', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 })),
    );
    const [attempts, setAttempts] = createSignal([attempt({ state: 'running', ended_at: null })]);
    const container = document.createElement('div');
    document.body.appendChild(container);
    const dispose = render(() => <AttemptList requestId="exec_1" attempts={attempts()} />, container);
    disposers.push(() => { dispose(); container.remove(); });
    await flush();
    expect(container.querySelector('[data-section="needs"]')).toBeNull();
    setAttempts([attempt({ state: 'waiting_decision', ended_at: null })]);
    await flush();
    expect(container.querySelector('[data-section="needs"]')).not.toBeNull();
  });

  it('a failed list of the files is said as one and Try again asks again', async () => {
    let calls = 0;
    vi.spyOn(globalThis, 'fetch').mockImplementation(() => {
      calls += 1;
      return Promise.resolve(calls <= 1
        ? new Response(JSON.stringify({ error: { status: 500, message: 'boom' } }), { status: 500 })
        : new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }));
    });
    const c = mount([attempt()]);
    await flush();
    await flush();
    expect(c.textContent).toContain('Could not load the files from this run');
    ([...c.querySelectorAll('button')].find((b) => b.textContent === 'Try again') as HTMLButtonElement).click();
    await flush();
    await flush();
    expect(c.textContent).not.toContain('Could not load the files from this run');
  });

  describe('pull request rendering', () => {
    it('renders nothing when pull_request is null', () => {
      const c = mount([attempt({ pull_request: null })]);
      expect(c.textContent).not.toContain('PR #');
      expect(c.textContent).not.toContain('Open');
      expect(c.textContent).not.toContain('Merged');
      expect(c.textContent).not.toContain('Closed');
      expect(c.textContent).not.toContain('Reverted');
    });

    it('a_pull_request_renders_its_link_and_state', () => {
      const testCases = [
        { state: 'open', label: 'Open' },
        { state: 'merged', label: 'Merged' },
        { state: 'closed', label: 'Closed' },
        { state: 'reverted', label: 'Reverted' },
      ];

      testCases.forEach(({ state, label }) => {
        const c = mount([
          attempt({
            pull_request: {
              number: 42,
              state,
              url: `https://github.com/owner/repo/pull/42`,
            },
          }),
        ]);

        // Check for the PR number link text
        expect(c.textContent).toContain('PR #42');

        // Check for the state badge
        expect(c.textContent).toContain(label);

        // Check for the link href
        const link = c.querySelector('a[href="https://github.com/owner/repo/pull/42"]');
        expect(link).toBeTruthy();
        expect(link?.getAttribute('target')).toBe('_blank');
        expect(link?.getAttribute('rel')).toBe('noopener noreferrer');

        // Clean up for next iteration
        while (disposers.length) disposers.pop()!();
        document.body.innerHTML = '';
      });
    });
  });

  describe('tools to see what it did', () => {
    const PATCH =
      'diff --git a/src/a.ts b/src/a.ts\n--- a/src/a.ts\n+++ b/src/a.ts\n@@ -1 +1 @@\n-old\n+added a\n' +
      'diff --git a/src/b.ts b/src/b.ts\n--- a/src/b.ts\n+++ b/src/b.ts\n@@ -1 +1 @@\n-x\n+added b\n';
    const art = (artifact_id: string, kind: string, name: string) => ({
      artifact_id, kind, name, media_type: null, size_bytes: 10, content_verified: true, created_at: '2026-08-06T12:05:00Z',
    });
    const mockApi = (artifacts: ReturnType<typeof art>[]) =>
      vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
        const url = String(input);
        if (url.endsWith('/content')) return Promise.resolve(new Response(PATCH));
        return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: artifacts })));
      });
    const labels = (c: HTMLElement) =>
      Array.from(c.querySelectorAll('[data-testid="attempt-tools"] button')).map((b) => b.textContent);

    it('a_terminal_attempt_offers_the_diff_and_the_folder', async () => {
      mockApi([art('a1', 'patch', 'changes.patch'), art('a2', 'log', 'run.log')]);
      const c = mount([attempt({ terminal_reason: { workspace_kept_at: '/w/1', result: 'Done.' } })]);
      await flush();
      await flush();
      expect(labels(c)).toEqual(['Open diff', 'Copy path']);
      // The raw log is with the ids.
      expect((c.querySelector('[data-testid="attempt-ids"] button:not([aria-label])') as HTMLButtonElement).textContent).toBe('Raw log');
      // What the agent said is on the card, not behind a button.
      expect(c.querySelector('[data-testid="agent-result"]')?.textContent?.trim()).toBe('Done.');
      (c.querySelector('[data-testid="attempt-tools"] button') as HTMLButtonElement).click();
      await flush();
      await flush();
      const headings = Array.from(document.querySelectorAll('[data-testid="diff-file"]')).map((h) => h.textContent);
      expect(headings).toEqual(['src/a.ts', 'src/b.ts']);
      const added = Array.from(document.querySelectorAll('[data-diff="add"]')).map((l) => l.textContent);
      expect(added).toEqual(['+added a', '+added b']);
    });

    it('a_run_in_the_users_folder_offers_the_projects_folder', async () => {
      mockApi([art('a1', 'patch', 'changes.patch')]);
      const fetchSpy = vi.mocked(globalThis.fetch);
      const base = fetchSpy.getMockImplementation()!;
      fetchSpy.mockImplementation((input, init) => {
        const url = String(input);
        if (url.endsWith('/items/item-1')) return Promise.resolve(new Response(JSON.stringify({ item: { id: 'item-1', project_id: 'project-1' } }), { headers: { ETag: '"1"' } }));
        if (url.endsWith('/projects/project-1')) return Promise.resolve(new Response(JSON.stringify({ id: 'project-1', workspace_mode: 'local_branch', repository: '/home/alice/app' })));
        return base(input, init);
      });
      const container = document.createElement('div');
      document.body.appendChild(container);
      const dispose = render(() => <AttemptList requestId="exec_1" attempts={[attempt()]} itemId="item-1" />, container);
      disposers.push(() => { dispose(); container.remove(); });
      const writeText = vi.fn();
      Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
      await flush();
      await flush();
      await flush();
      expect(labels(container)).toEqual(['Open diff', 'Copy path']);
      (Array.from(container.querySelectorAll('[data-testid="attempt-tools"] button')).find((b) => b.textContent === 'Copy path') as HTMLButtonElement).click();
      expect(writeText).toHaveBeenCalledWith('/home/alice/app');
    });

    it('without a patch no tool shows, the raw log is enabled, and what the agent said is on the card', async () => {
      mockApi([art('a2', 'log', 'run.log')]);
      const c = mount([attempt({ terminal_reason: { result: 'Done.' } })]);
      await flush();
      await flush();
      expect(labels(c)).toEqual([]);
      const rawLog = Array.from(c.querySelectorAll('button')).find((b) => b.textContent === 'Raw log') as HTMLButtonElement;
      expect(rawLog.disabled).toBe(false);
      expect(c.querySelector('[data-testid="agent-result"]')?.textContent?.trim()).toBe('Done.');
    });

    it('with no log the raw log is disabled and says why, never hidden', async () => {
      mockApi([]);
      const c = mount([attempt({ terminal_reason: { result: 'Done.' } })]);
      await flush();
      await flush();
      const rawLog = Array.from(c.querySelectorAll('button')).find((b) => b.textContent === 'Raw log') as HTMLButtonElement;
      expect(rawLog.disabled).toBe(true);
      expect(rawLog.title).toBe('This run left no log');
    });

    it('an empty patch offers no diff', async () => {
      mockApi([{ ...art('a1', 'patch', 'changes.patch'), size_bytes: 0 }]);
      const c = mount([attempt({ terminal_reason: { workspace_kept_at: '/w/1' } })]);
      await flush();
      await flush();
      expect(labels(c)).toEqual(['Copy path']);
    });

    it('the log names the agent steps, and says when it had no tools', async () => {
      const log = [
        JSON.stringify({ type: 'system', subtype: 'init', tools: [] }),
        JSON.stringify({ type: 'assistant', message: { content: [{ type: 'tool_use', id: 't1', name: 'Write', input: { file_path: '/repo/test.md' } }] } }),
      ].join('\n');
      vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
        const url = String(input);
        if (url.endsWith('/content')) return Promise.resolve(new Response(log));
        return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [art('a2', 'log', 'claude-code-run.log')] })));
      });
      const c = mount([attempt({ terminal_reason: { result: 'Done.' } })]);
      await flush();
      await flush();
      await flush();
      expect(c.querySelector('[data-testid="agent-steps"]')?.textContent).toContain('/repo/test.md');
      expect(c.textContent).toContain('The agent had no tools in this run');
    });

    const toolLog = (path: (name: string) => string) =>
      [
        JSON.stringify({ type: 'system', subtype: 'init', tools: ['Read', 'Bash'] }),
        ...['a.ts', 'b.ts', 'a.ts'].map((f, i) =>
          JSON.stringify({ type: 'assistant', message: { content: [{ type: 'tool_use', id: `r${i}`, name: 'Read', input: { file_path: path(f) } }] } }),
        ),
        JSON.stringify({ type: 'assistant', message: { content: [{ type: 'tool_use', id: 'b1', name: 'Bash', input: { command: 'npm test' } }] } }),
        JSON.stringify({ type: 'user', message: { content: [{ type: 'tool_result', tool_use_id: 'b1', is_error: true }] } }),
      ].join('\n');
    const mockLog = (log: string) =>
      vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
        const url = String(input);
        if (url.endsWith('/content')) return Promise.resolve(new Response(log));
        if (url.endsWith('/events')) return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [{ kind: 'attempt.install_skipped', source: 'runner', occurred_at: '2026-08-06T12:00:01Z', payload: { reason: 'No install: this profile runs no commands' } }] })));
        return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [art('a2', 'log', 'claude-code-run.log')] })));
      });

    it('what the agent did groups the steps by tool with the targets as chips, paths inside the project', async () => {
      mockLog(toolLog((f) => `/var/tmp/ws-1/src/${f}`));
      const c = mount([attempt({ terminal_reason: { workspace_kept_at: '/var/tmp/ws-1', result: 'Done.' } })]);
      for (let i = 0; i < 4; i++) await flush();
      const groups = Array.from(c.querySelectorAll('[data-testid="tool-group"]'));
      expect(groups.map((g) => g.querySelector('span')?.textContent)).toEqual(['Read', 'Bash']);
      const read = groups[0];
      expect(read.querySelector('[data-testid="tool-count"]')?.textContent).toBe('3 calls');
      // Each target once, the workspace prefix left off.
      expect(Array.from(read.querySelectorAll('[data-testid="tool-target"]')).map((t) => t.textContent)).toEqual(['src/a.ts', 'src/b.ts']);
      expect(groups[1].textContent).toContain('1 failed');
      expect(groups[1].querySelector('[data-testid="tool-target"]')?.textContent).toContain('npm test');
      expect(c.textContent).not.toContain('/var/tmp/ws-1/src');
    });

    it('the Timeline view shows the events, with the reason an install was skipped', async () => {
      mockLog(toolLog((f) => `/w/${f}`));
      const c = mount([attempt({ terminal_reason: { result: 'Done.' } })]);
      for (let i = 0; i < 4; i++) await flush();
      expect(c.querySelector('[data-testid="did-tool"]')?.getAttribute('aria-pressed')).toBe('true');
      expect(c.textContent).not.toContain('Install skipped');
      (c.querySelector('[data-testid="did-timeline"]') as HTMLButtonElement).click();
      for (let i = 0; i < 3; i++) await flush();
      expect(c.querySelector('[data-testid="agent-steps"]')).toBeNull();
      expect(c.textContent).toContain('Install skipped');
      expect(c.textContent).toContain('No install: this profile runs no commands');
    });

    it('without steps the timeline shows at once and By tool is disabled with its reason', async () => {
      mockApi([]);
      const c = mount([attempt({ terminal_reason: { result: 'Done.' } })]);
      for (let i = 0; i < 3; i++) await flush();
      const byTool = c.querySelector('[data-testid="did-tool"]') as HTMLButtonElement;
      expect(byTool.disabled).toBe(true);
      expect(byTool.title).toBe("This agent's log does not name its steps");
      expect(c.textContent).toContain('No events reported yet');
    });

    it('a running attempt shows its live timeline and says by-tool waits for the end', async () => {
      mockApi([]);
      const c = mount([attempt({ state: 'running', ended_at: null })]);
      await flush();
      const byTool = c.querySelector('[data-testid="did-tool"]') as HTMLButtonElement;
      expect(byTool.disabled).toBe(true);
      expect(byTool.title).toBe('The steps are listed by tool once the run ends');
    });

    it('what the agent concluded is clipped with Read full report, and shows paths inside the project', async () => {
      mockApi([]);
      const long = `Changed /var/tmp/ws-1/src/app.ts.\n${'More detail about the work. '.repeat(40)}`;
      const c = mount([attempt({ terminal_reason: { workspace_kept_at: '/var/tmp/ws-1', result: long } })]);
      await flush();
      await flush();
      const result = c.querySelector('[data-testid="agent-result"]') as HTMLElement;
      expect(result.textContent).toContain('Changed src/app.ts.');
      expect(result.textContent).not.toContain('/var/tmp');
      expect(result.classList.contains('max-h-60')).toBe(true);
      const toggle = Array.from(c.querySelectorAll('button')).find((b) => b.textContent === 'Read full report') as HTMLButtonElement;
      toggle.click();
      await flush();
      expect(toggle.textContent).toBe('Show less');
      expect(result.classList.contains('max-h-60')).toBe(false);
    });

    it('what the agent concluded is read as a page: headings and links, not their marks, and a link opens in its own tab', async () => {
      mockApi([]);
      const report = '# Findings\n\n- [A source](https://example.com/a)\n- <script>alert(1)</script>plain\n\n| a | b |\n|---|---|\n| 1 | 2 |\n';
      const c = mount([attempt({ terminal_reason: { result: report } })]);
      await flush();
      await flush();
      const result = c.querySelector('[data-testid="agent-result"]') as HTMLElement;
      expect(result.querySelector('h1')?.textContent).toBe('Findings');
      const link = result.querySelector('a') as HTMLAnchorElement;
      expect(link.getAttribute('href')).toBe('https://example.com/a');
      expect(link.getAttribute('target')).toBe('_blank');
      expect(result.querySelector('table td')?.textContent).toBe('1');
      expect(result.querySelector('script')).toBeNull();
      expect(result.textContent).not.toContain('# Findings');
    });

    it('a report the runner kept as a file opens as a page and downloads as that file', async () => {
      const long = `# Findings\n\n${'More detail about the work. '.repeat(40)}`;
      vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
        const url = String(input);
        if (url.endsWith('/content')) return Promise.resolve(new Response(long));
        return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [art('r1', 'report', 'report.md')] })));
      });
      const c = mount([attempt({ terminal_reason: { result: long } })]);
      await flush();
      await flush();
      (c.querySelector('[data-testid="open-report"]') as HTMLButtonElement).click();
      await flush();
      await flush();
      expect(document.querySelector('[data-testid="artifact-markdown"] h1')?.textContent).toBe('Findings');
      expect(document.querySelector('[data-testid="artifact-text"]')).toBeNull();
      expect(document.querySelector('[data-testid="artifact-save"]')?.textContent).toBe('Download report.md');
      expect(c.querySelector('[data-testid="download-report"]')?.textContent).toBe('Download report.md');
    });

    it('a short report is shown whole, with nothing to expand', async () => {
      mockApi([]);
      const c = mount([attempt({ terminal_reason: { result: 'Done.' } })]);
      await flush();
      await flush();
      expect(Array.from(c.querySelectorAll('button')).some((b) => b.textContent === 'Read full report')).toBe(false);
    });
  });
  describe('what the agent got', () => {
    const FENCED = 'work safely\n\n<<<card-text id=abc123>>>\nFix the build\n<<</card-text id=abc123>>>\n';
    const mockExecution = () =>
      vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
        const url = String(input);
        if (url.endsWith('/executions/exec_1')) return Promise.resolve(new Response(JSON.stringify({ request_id: 'exec_1', instructions: FENCED })));
        return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [] })));
      });

    it('shows_the_sandbox_the_variable_names_and_the_workspace', async () => {
      mockExecution();
      const c = mount([attempt({ sandbox: 'none', environment_names: ['PATH', 'HOME'], terminal_reason: { workspace_kept_at: '/w/1' } })]);
      await flush();
      expect(c.querySelector('[data-testid="agent-got-sandbox"]')?.textContent).toBe("Not sandboxed: the agent has your user's access to this machine");
      expect(c.querySelector('[data-testid="agent-got-env"]')?.textContent).toContain('PATH, HOME');
      // The runner's own path is not shown: the agent worked in the project's folder.
      expect(c.querySelector('[data-testid="agent-got-workspace"]')?.textContent).toContain("the project's folder");
      expect(c.querySelector('[data-testid="agent-got-workspace"]')?.textContent).not.toContain('/w/1');
      expect(mount([attempt({ sandbox: 'bwrap' })]).querySelector('[data-testid="agent-got-sandbox"]')?.textContent).toBe('Runs in an isolated sandbox');
    });

    it('says_nothing_for_a_line_that_has_nothing_to_report', async () => {
      mockExecution();
      const c = mount([attempt({ sandbox: 'harness', environment_names: [] })]);
      await flush();
      expect(c.querySelector('[data-testid="agent-got-sandbox"]')?.textContent).toBe('Runs in an isolated sandbox');
      expect(c.querySelector('[data-testid="agent-got-env"]')).toBeNull();
      expect(c.querySelector('[data-testid="agent-got"]')!.textContent).not.toContain('none reported');
      expect(c.textContent).not.toContain('jail');
    });

    it('shows_the_prompt_verbatim_in_a_pre_once_opened', async () => {
      const spy = mockExecution();
      const c = mount([attempt()]);
      expect(c.querySelector('[data-testid="agent-read"]')).toBeNull();
      expect(spy).not.toHaveBeenCalledWith('/api/executions/exec_1', expect.anything());
      (c.querySelector('[data-testid="agent-got"] [data-fold="agent-read"] > button') as HTMLButtonElement).click();
      await flush();
      await flush();
      expect(spy).toHaveBeenCalledWith('/api/executions/exec_1', expect.anything());
      const pre = c.querySelector('pre[data-testid="agent-read"]')!;
      expect(pre.textContent).toBe(FENCED);
      expect(pre.textContent).toContain('<<<card-text id=');
    });

    it('says_what_ended_the_run_in_the_deciding_words', () => {
      const cases: Array<[unknown, string]> = [
        [{ reason: 'exit_status', exit: 'Exited(3)', stream: {} }, 'The agent said it succeeded; its process exited with code 3'],
        [{ reason: 'exit_status', exit: 'Signaled(9)', stream: {} }, 'The agent said it succeeded; its process ended: Signaled(9)'],
        [{ code: 'tree_changed_after_verification' }, 'The tree changed after it was verified; nothing was pushed'],
        [{ code: 'repository_config_changed', path: '.git/config' }, "The agent changed the repository's git configuration; nothing was pushed"],
      ];
      for (const [terminal_reason, sentence] of cases) {
        const c = mount([attempt({ state: 'failed', terminal_reason: terminal_reason as AttemptSummary['terminal_reason'] })]);
        expect(c.textContent).toContain(sentence);
      }
    });

    it('opens_the_folder_through_the_desktop_command_and_copies_the_path_elsewhere', async () => {
      vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
        const url = String(input);
        if (url.endsWith('/items/item-1')) return Promise.resolve(new Response(JSON.stringify({ item: { id: 'item-1', project_id: 'project-1' } }), { headers: { ETag: '"1"' } }));
        if (url.endsWith('/projects/project-1')) return Promise.resolve(new Response(JSON.stringify({ id: 'project-1', workspace_mode: 'local_branch', repository: '/home/alice/app' })));
        return Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [{ artifact_id: 'a1', kind: 'patch', name: 'changes.patch', media_type: null, size_bytes: 10, content_verified: true, created_at: '2026-08-06T12:05:00Z' }] })));
      });
      (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
      try {
        const container = document.createElement('div');
        document.body.appendChild(container);
        const dispose = render(() => <AttemptList requestId="exec_1" attempts={[attempt({ terminal_reason: { workspace_kept_at: '/w/1' } })]} itemId="item-1" />, container);
        disposers.push(() => { dispose(); container.remove(); });
        await flush();
        await flush();
        await flush();
        const open = Array.from(container.querySelectorAll('button')).find((b) => b.textContent === 'Open folder') as HTMLButtonElement;
        open.click();
        await flush();
        expect(invoke).toHaveBeenCalledWith('open_folder', { path: '/w/1', repository: '/home/alice/app' });
      } finally {
        delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
      }
    });
  });

  describe('usage, ids and the page layout', () => {
    const used = { tokens_in: { value: 90, source: 'measured' }, tokens_out: { value: 10, source: 'measured' } };
    const limit = { tokens: 200, source: 'run', enforced_by: 'measured', sentence: 'This run stops at 200 tokens.', used: 100 } as const;

    it('the usage is folded: its summary shows the tokens and the share of the limit', () => {
      const c = mount([attempt({ usage: used as unknown as AttemptSummary['usage'], counted_tokens: 100 })]);
      const fold = c.querySelector('[data-testid="attempt-usage-fold"]') as HTMLElement;
      expect(fold.querySelector('button')!.getAttribute('aria-expanded')).toBe('false');
      expect(fold.querySelector('[data-testid="fold-summary"]')?.textContent).toBe('100 tokens counted');
      expect(fold.querySelector('[data-testid="attempt-usage"]')).not.toBeNull();
    });

    it('the summary adds the share of the limit when the run has one', () => {
      const container = document.createElement('div');
      document.body.appendChild(container);
      const dispose = render(() => <AttemptList requestId="exec_1" attempts={[attempt({ counted_tokens: 100 })]} limit={limit} />, container);
      disposers.push(() => { dispose(); container.remove(); });
      expect(container.querySelector('[data-testid="attempt-usage-fold"] [data-testid="fold-summary"]')?.textContent).toBe('100 tokens counted · 50% of its limit');
    });

    it('the ids are copyable, request and attempt', async () => {
      const writeText = vi.fn(() => Promise.resolve());
      Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
      const c = mount([attempt()]);
      const ids = c.querySelector('[data-testid="attempt-ids"]')!;
      expect(ids.textContent).toContain('exec_1');
      expect(ids.textContent).toContain('att_1');
      (ids.querySelector('button[aria-label="Copy the attempt id"]') as HTMLButtonElement).click();
      expect(writeText).toHaveBeenCalledWith('att_1');
    });

    it('on the page, what the agent got, the usage and the ids go to the side column; in the drawer they stay in the one column', () => {
      const side = document.createElement('div');
      document.body.appendChild(side);
      const container = document.createElement('div');
      document.body.appendChild(container);
      const dispose = render(() => <AttemptList requestId="exec_1" attempts={[attempt()]} layout="page" sideTarget={side} />, container);
      disposers.push(() => { dispose(); container.remove(); side.remove(); });
      expect(side.querySelector('[data-testid="agent-got"]')).not.toBeNull();
      expect(side.querySelector('[data-testid="attempt-usage-fold"]')).not.toBeNull();
      expect(side.querySelector('[data-testid="attempt-ids"]')).not.toBeNull();
      expect(container.querySelector('[data-testid="attempt-ids"]')).toBeNull();
      // The main column keeps the run.
      expect(container.querySelector('[data-testid="agent-did"]')).not.toBeNull();
      const drawer = mount([attempt()]);
      expect(drawer.querySelector('[data-testid="agent-got"]')).not.toBeNull();
      expect(drawer.querySelector('[data-testid="attempt-ids"]')).not.toBeNull();
    });
  });

  describe('a run reads top-down', () => {
    const art = (artifact_id: string, kind: string, name: string, size_bytes = 10) => ({
      artifact_id, kind, name, media_type: null, size_bytes, content_verified: true, created_at: '2026-08-06T12:05:00Z',
    });
    const PENDING = {
      decision_id: 'dec_1', kind: 'tool_approval', prompt: 'Run `rm -rf build`?', options: [{ option_id: 'allow', label: 'Approve' }, { option_id: 'deny', label: 'Deny' }],
      recommendation: null, metadata: {}, viewed_at: null, state: 'pending', answer: null, resolved_at: null, resolved_by: null,
      created_at: '2026-08-06T12:00:00Z', updated_at: '2026-08-06T12:00:00Z',
    };
    const PATCH = 'diff --git a/src/a.ts b/src/a.ts\n--- a/src/a.ts\n+++ b/src/a.ts\n@@ -1 +1,2 @@\n-old\n+added a\n+added b\n';
    function route(opts: { artifacts?: unknown[]; decisions?: unknown[]; brief?: unknown } = {}) {
      const json = (body: unknown, status = 200) => Promise.resolve(new Response(JSON.stringify(body), { status }));
      return vi.spyOn(globalThis, 'fetch').mockImplementation((input) => {
        const url = String(input);
        if (url.endsWith('/artifacts')) return json({ protocol_version: 1, data: opts.artifacts ?? [] });
        if (url.endsWith('/content')) return Promise.resolve(new Response(PATCH));
        if (url.endsWith('/mrp')) return json({ pack: ready, review: null });
        if (url.endsWith('/mrp/viewed')) return json({});
        if (url.endsWith('/decisions')) return json({ protocol_version: 1, data: opts.decisions ?? [] });
        if (url.endsWith('/events')) return json({ protocol_version: 1, data: [] });
        if (url.endsWith('/brief')) return opts.brief ? json(opts.brief) : json({ error: { status: 404, message: 'none' } }, 404);
        return json({});
      });
    }
    const settle = async () => { for (let i = 0; i < 6; i++) await flush(); };
    const sections = (c: HTMLElement) => [...c.querySelectorAll('[data-testid="run-section"]')].map((el) => el.getAttribute('data-section'));
    const FINISHED = () => attempt({
      terminal_reason: { workspace_kept_at: '/w/1', result: 'Done.', steps: [{ role: 'lead', model: 'm-1', class: 'strong', verdict: 'approved', prompts: [], assumptions: [], questions: [] }] },
      counted_tokens: 100,
      usage_economics: {
        model_token_cost_usd_estimated: { value: null, source: 'not_measured' },
        runner_time_cost: { wall_clock_ms: 65_000, cost_usd_estimated: { value: null, source: 'not_measured' } },
      },
      merge: { enabled: true, reason: null },
    });
    const FILES = [art('a1', 'patch', 'changes.patch'), art('a2', 'log', 'run.log'), art('a3', 'mrp', 'pack.json')];
    const limit = { tokens: 200, source: 'run', enforced_by: 'measured', sentence: 'This run stops at 200 tokens.', used: 100 } as const;
    function mountFull(a: AttemptSummary, extra: { itemId?: string } = {}) {
      const container = document.createElement('div');
      document.body.appendChild(container);
      const dispose = render(() => <AttemptList requestId="exec_1" attempts={[a]} limit={limit} {...extra} />, container);
      disposers.push(() => { dispose(); container.remove(); });
      return container;
    }

    it('a finished run is verdict, checks, changes, steps, files, technical, in that order, and none is lost', async () => {
      route({ artifacts: FILES });
      const c = mountFull(FINISHED());
      await settle();
      expect(c.querySelector('[data-testid="attempt"]')!.firstElementChild!.getAttribute('data-testid')).toBe('run-verdict');
      expect(sections(c)).toEqual(['checks', 'changes', 'steps', 'files', 'technical']);
      // What moved behind a fold or into a table is still there.
      for (const id of ['agent-concluded', 'attempt-tools', 'agent-did', 'run-steps', 'agent-got', 'attempt-usage-fold', 'attempt-ids', 'mrp-signature', 'review-banner']) {
        expect(c.querySelector(`[data-testid="${id}"]`), id).not.toBeNull();
      }
      expect(c.querySelector('[data-testid="run-activity"]')!.querySelector('[data-testid="asked-nothing"]')).not.toBeNull();
      // The raw level is the last, folded; so are the files and the steps of a run that finished well, each saying what is in it.
      const folded = (id: string) => c.querySelector(`[data-section="${id}"] > button`)!;
      expect(folded('technical').getAttribute('aria-expanded')).toBe('false');
      expect(folded('files').getAttribute('aria-expanded')).toBe('false');
      expect(folded('files').textContent).toContain('3 files.');
      expect(folded('steps').getAttribute('aria-expanded')).toBe('false');
      expect(folded('steps').textContent).toContain('1 step ran.');
      // The latest result and its checks stay open.
      expect(c.querySelector('[data-section="checks"]')!.tagName).toBe('SECTION');
      expect(c.querySelector('[data-section="checks"] > button')).toBeNull();
    });

    it('a run that is going keeps its steps and files open, and one that failed keeps its steps open', async () => {
      route({ artifacts: [art('a1', 'evidence', 'notes.md')] });
      const live = mountFull(attempt({ state: 'running', ended_at: null }));
      await settle();
      expect(live.querySelector('[data-section="files"] > button')!.getAttribute('aria-expanded')).toBe('true');
      expect(live.querySelector('[data-section="steps"] > button')!.getAttribute('aria-expanded')).toBe('true');
      disposers.pop()!();
      localStorage.clear();
      const failed = mountFull({ ...FINISHED(), state: 'failed' });
      await settle();
      expect(failed.querySelector('[data-section="steps"] > button')!.getAttribute('aria-expanded')).toBe('true');
      expect(failed.querySelector('[data-section="files"] > button')!.getAttribute('aria-expanded')).toBe('false');
    });

    it('a run that waits for an answer says in the Activity header that a question needs you, and the person can shut a fold and have it stay shut', async () => {
      route({ decisions: [PENDING] });
      const c = mountFull(attempt({ state: 'waiting_decision', ended_at: null }));
      await settle();
      const activity = c.querySelector<HTMLButtonElement>('[data-testid="run-activity"] > button')!;
      expect(activity.getAttribute('aria-expanded')).toBe('true');
      activity.click();
      expect(activity.getAttribute('aria-expanded')).toBe('false');
      expect(activity.textContent).toContain('a question needs you');
      expect(localStorage.getItem('tack_fold:run-activity')).toBe('0');
    });

    it('a run that waits for an answer puts the question right after the verdict', async () => {
      route({ decisions: [PENDING] });
      const c = mountFull(attempt({ state: 'waiting_decision', ended_at: null }));
      await settle();
      expect(sections(c)).toEqual(['needs', 'steps', 'files', 'technical']);
      expect(c.querySelector('[data-section="needs"]')!.textContent).toContain('Run `rm -rf build`?');
    });

    it('a run that is still going lists the files it has left so far', async () => {
      route({ artifacts: [art('a1', 'evidence', 'notes.md')] });
      const c = mountFull(attempt({ state: 'running', ended_at: null }));
      await settle();
      const files = c.querySelector('[data-section="files"]')!;
      expect(files.textContent).toContain('notes.md');
    });

    it('the decision token is asked for inside Needs you only, never on a run that asked nothing', async () => {
      decisionTokenStore.set(null);
      route({ decisions: [] });
      const quiet = mountFull(FINISHED());
      await settle();
      expect(quiet.textContent).not.toContain('Your decision token');
      disposers.pop()!();
      document.body.innerHTML = '';

      route({ decisions: [PENDING] });
      const asking = mountFull(attempt({ state: 'waiting_decision', ended_at: null }));
      await settle();
      const needs = asking.querySelector('[data-section="needs"]')!;
      expect(needs.textContent).toContain('Your decision token');
      expect(asking.textContent!.match(/Your decision token/g)).toHaveLength(1);
      expect(needs.querySelectorAll('input[type="password"]')).toHaveLength(1);
    });

    it('a question already answered is history in the activity fold, not asked again', async () => {
      route({ decisions: [{ ...PENDING, state: 'resolved', answer: { option_id: 'allow', text: null } }] });
      const c = mountFull(FINISHED());
      await settle();
      expect(c.querySelector('[data-section="needs"]')).toBeNull();
      const activity = c.querySelector('[data-testid="run-activity"]')!;
      expect(activity.textContent).toContain('Run `rm -rf build`?');
      expect(activity.textContent).not.toContain('Your decision token');
      expect(activity.querySelector('[data-testid="asked-nothing"]')).toBeNull();
    });

    it('the verdict says what the verifier thinks, what it cost and for how long, and that nothing is merged', async () => {
      route({ artifacts: FILES });
      const c = mountFull(FINISHED());
      await settle();
      const verdict = c.querySelector('[data-testid="run-verdict"]')!;
      const chips = [...verdict.querySelectorAll('[data-testid="mrp-verdict"] span')].map((b) => b.textContent);
      expect(chips).toEqual([`Recommendation: ${ready.recommendation.decision}`, `Risk: ${ready.risk.tier}`]);
      expect(verdict.querySelector('[role="meter"]')!.getAttribute('aria-label')).toBe('Used 100 of 200 tokens');
      expect(verdict.querySelector('[data-testid="verdict-duration"]')!.textContent).toBe('1m 5s');
      const merge = verdict.querySelector('[data-testid="verdict-merge"]')!;
      expect(merge.textContent).toContain('Not merged');
      expect([...merge.querySelectorAll('button')].map((b) => b.textContent)).toEqual(['Merge']);
    });

    it('a run the verifier says to reject is not announced as a plain success', async () => {
      const spy = route({ artifacts: FILES });
      const orig = spy.getMockImplementation()!;
      spy.mockImplementation((input, init) =>
        String(input).endsWith('/mrp')
          ? Promise.resolve(new Response(JSON.stringify({ pack: { ...ready, recommendation: { decision: 'reject', rationale: 'prill: reject' } }, review: null }), { status: 200 }))
          : orig(input, init),
      );
      const c = mountFull(FINISHED());
      await settle();
      expect(c.querySelector('[data-testid="run-verdict"]')!.textContent).toContain('The verifier recommends rejecting this run');
    });

    it('a run turned away before it started shows no tokens and no duration', async () => {
      route();
      const container = document.createElement('div');
      document.body.appendChild(container);
      const refused = { ...FINISHED(), state: 'failed', never_started: true, terminal_reason: { code: 'harness_rejected', message: 'ceiling L3' } };
      const dispose = render(() => <AttemptList requestId="exec_1" attempts={[refused as never]} />, container);
      disposers.push(() => { dispose(); container.remove(); });
      await settle();
      expect(container.querySelector('[data-testid="verdict-tokens"]')).toBeNull();
      expect(container.querySelector('[data-testid="verdict-duration"]')).toBeNull();
    });

    it('without a limit the tokens are said in words, not drawn as a meter', async () => {
      route();
      const container = document.createElement('div');
      document.body.appendChild(container);
      const dispose = render(() => <AttemptList requestId="exec_1" attempts={[FINISHED()]} />, container);
      disposers.push(() => { dispose(); container.remove(); });
      await settle();
      expect(container.querySelector('[data-testid="verdict-tokens"]')!.textContent).toBe('100 tokens counted');
      expect(container.querySelector('[data-testid="run-verdict"] [role="meter"]')).toBeNull();
    });

    it('one button is filled: the verdict first, then the merge, then another run', async () => {
      route({ artifacts: FILES });
      const filled = (c: HTMLElement) => [...c.querySelectorAll('[data-testid="run-verdict"] button')]
        .filter((b) => (b as HTMLElement).style.backgroundColor.includes('primary')).map((b) => b.textContent);
      const againButton = (variant: 'primary' | 'secondary') => (
        <button type="button" style={{ 'background-color': variant === 'primary' ? 'var(--color-primary-600)' : 'transparent' }}>Run again</button>
      );
      const mountWith = (a: AttemptSummary) => {
        const container = document.createElement('div');
        document.body.appendChild(container);
        const dispose = render(() => <AttemptList requestId="exec_1" attempts={[a]} runAgain={againButton} />, container);
        disposers.push(() => { dispose(); container.remove(); });
        return container;
      };
      const review = mountWith(FINISHED());
      await settle();
      expect(filled(review)).toEqual(['Accept']);
      const decided = mountWith({ ...FINISHED(), review: { verdict: 'accepted', note: null, reviewed_at: '2026-08-06T12:06:00Z' } });
      await settle();
      expect(filled(decided)).toEqual(['Merge']);
      const failed = mountWith(attempt({ state: 'failed', terminal_reason: { code: 'timeout' } }));
      await settle();
      expect(filled(failed)).toEqual(['Run again']);
    });

    it('a run with a pack has one verdict block, and its reason is required, as the pack demands', async () => {
      const spy = route({ artifacts: FILES });
      const c = mountFull(FINISHED());
      await settle();
      expect(c.querySelectorAll('[data-testid="review-actions"]')).toHaveLength(1);
      expect(c.textContent).not.toContain('Your review');
      expect(c.textContent).not.toContain('The note is optional');
      const banner = c.querySelector('[data-testid="review-banner"]')!;
      expect(banner.textContent).toContain('A reason is required');
      const button = (label: string) => [...banner.querySelectorAll('button')].find((b) => b.textContent === label) as HTMLButtonElement;
      expect(button('Accept').disabled).toBe(true);
      expect(button('Reject').disabled).toBe(true);
      const input = banner.querySelector('input')!;
      input.value = 'looks right';
      input.dispatchEvent(new InputEvent('input', { bubbles: true }));
      expect(button('Accept').disabled).toBe(false);
      spy.mockClear();
      button('Accept').click();
      await settle();
      const calls = spy.mock.calls.map(([u, init]) => [String(u), (init as RequestInit | undefined)?.body] as const);
      expect(calls.find(([u]) => u.endsWith('/mrp/review'))?.[1]).toBe(JSON.stringify({ verdict: 'accept', reason: 'looks right' }));
    });

    /** The pack and the run each keep a verdict: the calls are recorded, and the run's review can be made to fail. */
    function routeReviews(opts: { packReview?: unknown; runReviewFails?: { value: boolean } } = {}) {
      const calls: Array<{ url: string; body: unknown }> = [];
      const json = (body: unknown, status = 200) => Promise.resolve(new Response(JSON.stringify(body), { status }));
      vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
        const url = String(input);
        const body = init?.body ? JSON.parse(String(init.body)) : undefined;
        if (init?.method === 'POST') calls.push({ url, body });
        if (url.endsWith('/artifacts')) return json({ protocol_version: 1, data: FILES });
        if (url.endsWith('/content')) return Promise.resolve(new Response(PATCH));
        if (url.endsWith('/mrp')) return json({ pack: ready, review: opts.packReview ?? null });
        if (url.endsWith('/mrp/viewed')) return json({});
        if (url.endsWith('/mrp/review')) {
          return json({ attempt_id: 'att_1', artifact_id: 'a3', verdict: body.verdict, reason: body.reason, viewed_at: null, reviewed_at: '2026-08-06T12:07:00Z', reviewed_by: 'me' });
        }
        if (url.endsWith('/attempts/1/review')) {
          return opts.runReviewFails?.value ? json({ error: { status: 500, message: 'The run could not be reached' } }, 500) : json({});
        }
        if (url.endsWith('/attempts')) {
          return json({ protocol_version: 1, data: [{ ...FINISHED(), review: { verdict: calls.at(-1)?.body && (calls.at(-1)!.body as { verdict: string }).verdict, note: null, reviewed_at: '2026-08-06T12:07:00Z' } }] });
        }
        if (url.endsWith('/decisions')) return json({ protocol_version: 1, data: [] });
        if (url.endsWith('/events')) return json({ protocol_version: 1, data: [] });
        return json({ error: { status: 404, message: 'none' } }, 404);
      });
      return calls;
    }
    const type = (input: HTMLInputElement, text: string) => {
      input.value = text;
      input.dispatchEvent(new InputEvent('input', { bubbles: true }));
    };
    const runReviewCalls = (calls: Array<{ url: string; body: unknown }>) => calls.filter((c) => c.url.endsWith('/attempts/1/review'));

    it('the reason is saved on the run\'s verdict for Accept and for Reject, and the buttons carry stable ids', async () => {
      for (const [testid, verdict] of [['review-accept', 'accepted'], ['review-reject', 'rejected']] as const) {
        const calls = routeReviews();
        const c = mountFull(FINISHED());
        await settle();
        type(c.querySelector<HTMLInputElement>('[data-testid="review-reason"]')!, 'checked by hand');
        c.querySelector<HTMLButtonElement>(`[data-testid="${testid}"]`)!.click();
        await settle();
        expect(calls.find((x) => x.url.endsWith('/mrp/review'))?.body).toEqual({ verdict: verdict === 'accepted' ? 'accept' : 'reject', reason: 'checked by hand' });
        expect(runReviewCalls(calls).map((x) => x.body)).toEqual([{ verdict, note: 'checked by hand' }]);
        disposers.pop()!();
        document.body.innerHTML = '';
        vi.restoreAllMocks();
      }
    });

    it('when the pack is reviewed and the run is not, one action saves the pack\'s verdict on the run, and says so again if it fails', async () => {
      const packReview = { attempt_id: 'att_1', artifact_id: 'a3', verdict: 'accept', reason: 'checked by hand', viewed_at: null, reviewed_at: '2026-08-06T12:07:00Z', reviewed_by: 'me' };
      const fails = { value: true };
      const calls = routeReviews({ packReview, runReviewFails: fails });
      const c = mountFull(FINISHED());
      await settle();
      const banner = c.querySelector('[data-testid="review-banner"]')!;
      expect(banner.textContent).toContain('You already reviewed the pack');
      // Neither a new Reject nor a new reason is offered: only the pack's verdict is saved on the run.
      expect(banner.querySelector('[data-testid="review-reject"]')).toBeNull();
      expect(banner.querySelector('[data-testid="review-reason"]')).toBeNull();
      expect(banner.textContent).toContain('The pack was accepted');
      const save = () => [...banner.querySelectorAll('button')].filter((b) => b.textContent === 'Save the verdict on the run');
      expect(save()).toHaveLength(1);
      save()[0].click();
      await settle();
      expect(banner.textContent).toContain('The run could not be reached');
      expect(save()).toHaveLength(1);
      fails.value = false;
      save()[0].click();
      await settle();
      expect(runReviewCalls(calls).map((x) => x.body)).toEqual([
        { verdict: 'accepted', note: 'checked by hand' },
        { verdict: 'accepted', note: 'checked by hand' },
      ]);
      expect(calls.some((x) => x.url.endsWith('/mrp/review'))).toBe(false);
    });

    it('a run whose first attempt review fails after the pack was reviewed offers the save, not a new Reject', async () => {
      const fails = { value: true };
      const calls = routeReviews({ runReviewFails: fails });
      const c = mountFull(FINISHED());
      await settle();
      type(c.querySelector<HTMLInputElement>('[data-testid="review-reason"]')!, 'looks right');
      c.querySelector<HTMLButtonElement>('[data-testid="review-accept"]')!.click();
      await settle();
      expect(calls.some((x) => x.url.endsWith('/mrp/review'))).toBe(true);
      const banner = c.querySelector('[data-testid="review-banner"]')!;
      expect([...banner.querySelectorAll('button')].map((b) => b.textContent)).toEqual(['Save the verdict on the run']);
      expect(banner.textContent).toContain('The run could not be reached');
    });

    it('a folder team says it works in a folder, with nothing to merge, in the verdict', async () => {
      route();
      const c = mountFull(attempt({ terminal_reason: { outside_repository: true, summary: 'Wrote three notes.' }, merge: { enabled: false, reason: 'no branch was pushed' } }));
      await settle();
      const line = c.querySelector('[data-testid="run-verdict"] [data-testid="run-folder"]')!;
      expect(line.textContent).toBe('Works in a folder: nothing to merge');
      expect(c.querySelector('[data-testid="merge-button"]')).toBeNull();
      expect(c.querySelector('[data-testid="review-banner"]')).toBeNull();
      // A run in a repository says nothing of the kind.
      disposers.pop()!();
      document.body.innerHTML = '';
      expect(mountFull(FINISHED()).querySelector('[data-testid="run-folder"]')).toBeNull();
    });

    it('a run that left no branch says why it is not merged', async () => {
      route();
      const c = mountFull(attempt({ terminal_reason: { result: 'Answered.' }, merge: { enabled: false, reason: 'no branch was pushed' } }));
      await settle();
      expect(c.querySelector('[data-testid="verdict-merge"]')!.textContent).toBe('Not merged: no branch was pushed');
    });

    it('the changes count their files and their lines', async () => {
      route({ artifacts: FILES });
      const c = mountFull(FINISHED());
      await settle();
      expect(c.querySelector('[data-section="changes"]')!.textContent).toContain('1 file changed · +2 −1');
    });

    it('the files are one table, a row each, with View and Download as icon buttons', async () => {
      route({ artifacts: FILES });
      const c = mountFull(FINISHED());
      await settle();
      const files = c.querySelector('[data-section="files"]')!;
      expect(files.textContent).toContain('3 files.');
      const rows = [...files.querySelectorAll('tr[data-testid="artifact-row"]')];
      expect(rows.map((r) => r.querySelector('th')!.textContent)).toEqual(['changes.patch', 'run.log', 'pack.json']);
      expect(files.querySelectorAll('button[aria-label="View"]')).toHaveLength(3);
      expect(files.querySelectorAll('button[aria-label="Download"]')).toHaveLength(3);
    });

    it('the Done when list and the verifier criteria are one checklist, with the number in a sentence', async () => {
      route({ artifacts: FILES, brief: { item_id: 'item-1', acceptance: [{ kind: 'manual', id: 'extra', title: 'Reads well', text: 'x', checked: true }], constraints: [] } });
      const c = mountFull(FINISHED(), { itemId: 'item-1' });
      await settle();
      const checks = c.querySelector('[data-section="checks"]')!;
      const total = ready.criteria.length + 1;
      expect(checks.textContent).toMatch(new RegExp(`of ${total} checks passed`));
      expect(checks.querySelectorAll('[data-testid="checks-list"] > li')).toHaveLength(total);
      expect(checks.textContent).toContain('Reads well');
      expect(checks.querySelector('[data-testid="done-when-check"]')!.textContent).toContain('Done when');
    });

    it('the technical fold holds the ids, the runner, the raw log and the signature', async () => {
      route({ artifacts: FILES });
      const c = mountFull(FINISHED());
      await settle();
      const technical = c.querySelector('[data-section="technical"]')!;
      for (const text of ['exec_1', 'att_1', 'runner_1', 'Raw log', 'Not signed']) expect(technical.textContent).toContain(text);
    });

    it('names a run in the one run-state language', () => {
      expect(attemptRunState(attempt({ state: 'running', ended_at: null }))).toEqual({ state: 'running' });
      expect(attemptRunState(attempt({ state: 'waiting_decision', ended_at: null }))).toEqual({ state: 'needs', docket: 'Needs a person' });
      expect(attemptRunState(attempt({ state: 'failed', terminal_reason: { code: 'needs_human' } }))).toEqual({ state: 'needs', docket: 'Needs a person' });
      expect(attemptRunState(attempt({ state: 'failed' }))).toEqual({ state: 'failed' });
      expect(attemptRunState(attempt({ state: 'failed', never_started: true, terminal_reason: { code: 'harness_rejected' } }))).toEqual({ state: 'notstarted' });
      expect(attemptRunState(attempt({ state: 'cancelled' }))).toEqual({ state: 'cancelled' });
      expect(attemptRunState(FINISHED())).toEqual({ state: 'review', docket: 'Needs your review' });
      expect(attemptRunState({ ...FINISHED(), review: { verdict: 'accepted', note: null, reviewed_at: 'x' } })).toEqual({ state: 'done' });
      // A run that only answered still wants a verdict; once given, it is done with nothing recorded.
      expect(attemptRunState(attempt({ terminal_reason: { result: 'Answered.' } }))).toEqual({ state: 'review', docket: 'Needs your review' });
      expect(attemptRunState(attempt({ terminal_reason: { result: 'Answered.' }, review: { verdict: 'accepted', note: null, reviewed_at: 'x' } }))).toEqual({ state: 'done', docket: 'no changes recorded' });
      expect(attemptRunState(attempt({ terminal_reason: { outside_repository: true } }))).toEqual({ state: 'done' });
    });
  });
});
