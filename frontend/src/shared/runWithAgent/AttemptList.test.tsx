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
    expect(c.textContent).toContain('Attempt #1');
    expect(c.textContent).toContain('Attempt #2');
    expect(c.textContent).toContain('runner_1');
    expect(c.textContent).toContain('runner_2');
    expect(c.textContent).toContain('Succeeded');
    expect(c.textContent).toContain('Failed');
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
    expect(c.textContent).toContain('$0.42 (measured)');
    // The runner-time dollar figure is STILL "Not measured" in this fixture
    // — the two dimensions never share one figure.
    expect(c.textContent).toContain('Not measured');
    expect(c.textContent).toContain('4m 55s');
  });

  it('renders model provenance honestly: null is "Not yet reported", not a fabricated match', () => {
    const c = mount([attempt({ model_provenance: null })]);
    expect(c.textContent).toContain('Not yet reported');
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
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }),
    );
    const c = mount([attempt()]);
    expect(c.textContent).not.toContain('Loading events');
    expect(c.textContent).not.toContain('No events reported yet');

    const toggle = [...c.querySelectorAll('button')].find((b) => b.textContent?.includes('Show events'))!;
    toggle.click();
    await flush();
    await flush();

    expect(c.textContent).toContain('No events reported yet');
    expect(c.textContent).toContain('Decisions');
    expect(c.textContent).toContain('Artifacts');
  });
});
