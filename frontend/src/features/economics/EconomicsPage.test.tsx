import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import EconomicsPage from './EconomicsPage';
import type { EconomicsSlice, EconomicsSummaryResponse } from './api';

const flush = () => new Promise((r) => setTimeout(r, 0));

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <MemoryRouter>
        <Route path="/" component={EconomicsPage} />
      </MemoryRouter>
    ),
    container,
  );
  return { container, dispose };
}

function mockFetch(status: number, body: unknown) {
  return vi
    .spyOn(globalThis, 'fetch')
    .mockImplementation(() => Promise.resolve(new Response(JSON.stringify(body), { status })));
}

function emptySlice(key: string): EconomicsSlice {
  return {
    key,
    completed_item_count: 0,
    agent_completed_count: 0,
    human_completed_count: 0,
    tokens_in: 0,
    tokens_out: 0,
    cost_usd_estimated: null,
    pricing_snapshot_at: null,
    cost_usd_estimated_per_item: null,
    agent_lead_time: { sample_count: 0, below_min_sample: true, avg_hours: null, raw_hours: null },
    human_lead_time: { sample_count: 0, below_min_sample: true, avg_hours: null, raw_hours: null },
    lead_time_selection_bias_note:
      'Items dispatched to agents are not a random sample of all work — auto-dispatch fires only on specific statuses.',
    rework: {
      attempts_total: 0,
      attempts_excluded_stale: 0,
      attempts_with_rework_signal: 0,
      below_min_sample: true,
      rate: null,
      definition: 'Share of dispatched items with a rework_started, verification_failed, or tester_verdict_failed event.',
      truncation_note: 'Rework signals age out after the configured retention window.',
    },
  };
}

function summary(overrides: Partial<EconomicsSummaryResponse> = {}): EconomicsSummaryResponse {
  return {
    generated_at: new Date().toISOString(),
    min_sample_size: 5,
    events_retention_days: 90,
    overall: emptySlice('overall'),
    by_project_type: [],
    by_item_type: [],
    ...overrides,
  };
}

describe('EconomicsPage — orchestration disabled (404, the default for every existing install)', () => {
  it('shows the disabled explanation', async () => {
    mockFetch(404, { error: { status: 404, message: 'not found' } });
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('Agent-fleet orchestration is disabled');
    expect(container.textContent).toContain('TACK_ORCH_ENABLE');
  });
});

describe('EconomicsPage — enabled, no completed items yet', () => {
  it('shows the empty state, not a misleading zero dashboard', async () => {
    mockFetch(200, summary());
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('No completed items yet');
  });
});

describe('EconomicsPage — request failure (not a 404)', () => {
  it('shows a retry-able error state', async () => {
    mockFetch(500, { error: { status: 500, message: 'boom' } });
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain("Couldn't load unit economics");
  });
});

describe('EconomicsPage — populated', () => {
  const populated = summary({
    overall: {
      ...emptySlice('overall'),
      completed_item_count: 8,
      agent_completed_count: 5,
      human_completed_count: 3,
      tokens_in: 12_000,
      tokens_out: 6_000,
      cost_usd_estimated: 3.5,
      cost_usd_estimated_per_item: 0.7,
    },
    by_project_type: [{ ...emptySlice('software'), completed_item_count: 8 }],
    by_item_type: [{ ...emptySlice('task'), completed_item_count: 8 }],
  });

  it('renders tokens and estimated cost with the estimated wording', async () => {
    mockFetch(200, populated);
    const { container } = mount();
    await flush();
    expect(container.textContent).toMatch(/estimated/);
    expect(container.textContent).toContain('12.0k');
  });

  it('never shows a bare "agents are Nx faster" figure and always names the selection-bias caveat', async () => {
    mockFetch(200, populated);
    const { container } = mount();
    await flush();
    const text = container.textContent ?? '';
    expect(text).toMatch(/not a random sample/);
    expect(text).not.toMatch(/\d+x faster/i);
  });

  it('renders the rework definition next to the rate, never the rate alone', async () => {
    mockFetch(200, populated);
    const { container } = mount();
    await flush();
    const text = container.textContent ?? '';
    expect(text).toContain('Definition:');
    expect(text).toMatch(/rework_started/);
  });

  it('renders both breakdown tables', async () => {
    mockFetch(200, populated);
    const { container } = mount();
    await flush();
    expect(container.textContent).toContain('By project type');
    expect(container.textContent).toContain('By item type');
  });

  it('shows the stale-attempt truncation note only when attempts were actually excluded', async () => {
    const withStale = summary({
      overall: {
        ...emptySlice('overall'),
        completed_item_count: 5,
        agent_completed_count: 5,
        rework: {
          ...emptySlice('overall').rework,
          attempts_total: 5,
          attempts_excluded_stale: 2,
        },
      },
    });
    mockFetch(200, withStale);
    const { container } = mount();
    await flush();
    expect(container.textContent).toMatch(/excluded/);
    expect(container.textContent).toMatch(/retention window/);
  });
});
