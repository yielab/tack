import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import AttemptList from './AttemptList';
import type { AttemptSummary } from '../execution';

const flush = () => new Promise((r) => setTimeout(r, 0));
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
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
});

describe('AttemptList', () => {
  it('renders every attempt with its number, state badge and runner id', () => {
    const c = mount([attempt(), attempt({ attempt_id: 'att_2', attempt_number: 2, state: 'failed', runner_id: 'runner_2' })]);
    expect(c.textContent).toContain('Attempt 1');
    expect(c.textContent).toContain('Attempt 2');
    expect(c.textContent).toContain('runner_1');
    expect(c.textContent).toContain('runner_2');
    expect(c.textContent).toContain('Finished');
    expect(c.textContent).toContain('Failed');
  });

  it('a single attempt carries no number: the run is numbered instead', () => {
    const c = mount([attempt()]);
    expect(c.textContent).not.toContain('Attempt');
  });

  it('a terminal attempt with a kept workspace and no patch shows Accept and Reject', () => {
    const c = mount([attempt({ terminal_reason: { workspace_kept_at: '/w/1' } })]);
    // A single attempt's status is the run's heading (`ExecutionTimeline`), not repeated here.
    expect(c.textContent).not.toContain('needs your review');
    expect(Array.from(c.querySelectorAll('button')).map((b) => b.textContent)).toEqual(
      expect.arrayContaining(['Accept', 'Reject']),
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
            'requested model provider "claude" is not supported by claude-code; supported: anthropic, bedrock, vertex, foundry, vercel-ai-gateway, anthropic-direct',
        },
        model_provenance: { kind: 'matched', provider: 'claude', model_id: 'claude-sonnet-5-5' },
        actual_execution: null,
      }),
    ]);
    expect(c.textContent).toContain('Why it stopped:');
    expect(c.textContent).toContain('Did not start — requested model provider');
    expect(c.textContent).toContain('Requested claude / claude-sonnet-5-5 (not confirmed by the run)');
    expect(c.textContent).not.toContain('Matched request');
  });

  it('a matched attempt whose actual_execution observed the model says it ran on it', () => {
    const c = mount([
      attempt({
        model_provenance: { kind: 'matched', provider: 'anthropic', model_id: 'm1' },
        actual_execution: { model_observation_source: 'observed' },
      }),
    ]);
    expect(c.textContent).toContain('Ran on anthropic / m1, as requested.');
  });

  it('leads the usage with tokens, counts the cache, names the models and calls the cost approximate', () => {
    const c = mount([
      attempt({
        actual_execution: { harness_kind: 'claude-code', model_id: 'claude-sonnet-5-5', model_observation_source: 'observed' },
        model_provenance: { kind: 'auto_select_observed', actual_provider: 'anthropic', actual_model_id: 'claude-sonnet-5-5' },
        usage: {
          tokens_in: { value: 1_077_873, source: 'measured' },
          tokens_out: { value: 3_284, source: 'measured' },
          cache_read_tokens: 791_447,
          cache_write_tokens: 286_414,
          model_calls: 6,
          models: ['claude-sonnet-5-5'],
        },
        usage_economics: {
          model_token_cost_usd_estimated: { value: 1.3368, source: 'measured' },
          runner_time_cost: { wall_clock_ms: 113_000, cost_usd_estimated: { value: null, source: 'not_measured' } },
        },
      }),
    ]);
    const usage = c.querySelector('[data-testid="attempt-usage"]')!;
    expect(usage.querySelector('[data-testid="usage-tokens"]')!.textContent).toBe('1.08M tokens in · 3.3K out');
    expect(usage.textContent).toContain('Of the input: 791K read from cache, 286K written to cache, 12 new.');
    expect(usage.textContent).toContain('6 model calls');
    expect(usage.textContent).toContain('1m 53s');
    expect(usage.querySelector('[data-testid="usage-models"]')!.textContent).toBe('claude-sonnet-5-5');
    expect(usage.querySelector('[data-testid="usage-cost"]')!.textContent).toContain('≈ $1.34 (approx.)');
    expect(usage.textContent).toContain("Claude Code's own estimate at list price, not your bill");
    // One time figure: the runner-time dollar tile, never measured, is gone.
    expect(usage.textContent!.match(/Not measured/g)).toBeNull();
  });

  it('reads the cache and calls from the result line of a claude-code run recorded before the runner did', () => {
    const c = mount([
      attempt({
        usage: { tokens_in: { value: 12, source: 'measured' }, tokens_out: { value: 3_284, source: 'measured' } },
        terminal_reason: {
          type: 'result',
          num_turns: 6,
          usage: { input_tokens: 12, cache_read_input_tokens: 791_447, cache_creation_input_tokens: 286_414 },
          modelUsage: { 'claude-sonnet-5-5': {} },
        },
      }),
    ]);
    expect(c.querySelector('[data-testid="usage-tokens"]')!.textContent).toBe('1.08M tokens in · 3.3K out');
    expect(c.textContent).toContain('6 model calls');
    expect(c.querySelector('[data-testid="usage-models"]')!.textContent).toBe('claude-sonnet-5-5');
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

  it('renders a real measured/estimated dollar figure honestly when present, distinct from "Not measured"', () => {
    const c = mount([
      attempt({
        usage_economics: {
          model_token_cost_usd_estimated: { value: 0.42, source: 'measured' },
          runner_time_cost: { wall_clock_ms: 295_000, cost_usd_estimated: { value: null, source: 'not_measured' } },
        },
      }),
    ]);
    expect(c.textContent).toContain('≈ $0.42 (approx.)');
    expect(c.textContent).not.toContain('$0.42 (measured)');
    expect(c.textContent).toContain('4m 55s');
  });

  it('renders model provenance honestly: null is "Model not reported", not a fabricated match', () => {
    const c = mount([attempt({ model_provenance: null })]);
    expect(c.textContent).toContain('Model not reported');
  });

  it('renders a mismatched provenance with both requested and actual values visible', () => {
    const c = mount([
      attempt({
        model_provenance: {
          kind: 'mismatched',
          requested_provider: 'openai',
          requested_model_id: 'opaque/model-alpha',
          actual_provider: 'anthropic',
          actual_model_id: 'opaque/model-beta',
        },
      }),
    ]);
    expect(c.textContent).toContain('Mismatched request');
    expect(c.textContent).toContain('opaque/model-alpha');
    expect(c.textContent).toContain('opaque/model-beta');
  });

  it('events/decisions/artifacts are collapsed by default and expand on demand', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
      Promise.resolve(new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 })),
    );
    const c = mount([attempt()]);
    expect(c.textContent).not.toContain('Loading events');
    expect(c.textContent).not.toContain('No events reported yet');

    const toggle = [...c.querySelectorAll('button')].find((b) => b.textContent?.includes('Show timeline'))!;
    toggle.click();
    await flush();
    await flush();

    expect(c.textContent).toContain('No events reported yet');
    expect(c.textContent).toContain('Questions from the agent');
    expect(c.textContent).toContain('Files from this run');
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
      expect(labels(c)).toEqual(['Open diff', 'Copy path', 'Log']);
      // What the agent said is on the card, not behind a button.
      expect(c.querySelector('[data-testid="agent-result"]')?.textContent).toBe('Done.');
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
        if (url.endsWith('/projects/project-1')) return Promise.resolve(new Response(JSON.stringify({ id: 'project-1', workspace_mode: 'local_branch', repository: '/home/ox/app' })));
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
      expect(writeText).toHaveBeenCalledWith('/home/ox/app');
    });

    it('without a patch only Log shows, and what the agent said is on the card', async () => {
      mockApi([art('a2', 'log', 'run.log')]);
      const c = mount([attempt({ terminal_reason: { result: 'Done.' } })]);
      await flush();
      await flush();
      expect(labels(c)).toEqual(['Log']);
      expect(c.querySelector('[data-testid="agent-result"]')?.textContent).toBe('Done.');
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
      expect(c.querySelector('[data-testid="agent-steps"]')?.textContent).toContain('Wrote /repo/test.md');
      expect(c.textContent).toContain('The agent had no tools in this run');
    });
  });
});
