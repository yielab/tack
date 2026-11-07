import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render } from 'solid-js/web';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import FactoryMetrics from './FactoryMetrics';
import type { components } from '../../shared/api/schema.gen';
import type { Resource } from 'solid-js';
import { flush } from '../../test/flush';

// Mock useParams to provide the project ID
vi.mock('@solidjs/router', () => ({
  useParams: () => ({ id: 'test-project' }),
}));

type FactoryMetrics = components['schemas']['FactoryMetrics'];

const projectValue: ProjectContextValue = {
  projectId: () => 'test-project',
  project: (() => null) as unknown as Resource<null>,
  workflow: () => ({
    workflow_type: 'kanban',
    statuses: [
      { name: 'todo', category: 'todo', order: 0 },
      { name: 'done', category: 'done', order: 1 },
    ],
  }),
  vocabulary: () => ({}),
  refetch: () => {},
};

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ProjectContext.Provider value={projectValue}>
        <FactoryMetrics />
      </ProjectContext.Provider>
    ),
    container,
  );
  return { container, dispose };
}

let fetchMock: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(() => {
    return Promise.resolve(new Response('{}', { status: 200 }));
  });
});

const FACTORY_METRICS_FIXTURE: FactoryMetrics = {
  escalation_rate: {
    numerator: 10,
    denominator: 100,
    value: 0.1,
  },
  attempts: 100,
  decisions: 10,
  decided_alone: 25,
  decided_alone_rate: {
    numerator: 25,
    denominator: 100,
    value: 0.25,
  },
  human_minutes_per_decision: {
    measured: 0,
    null_reason: 'not_measured',
  },
  human_minutes_per_mrp: {
    measured: 5,
    median_minutes: 30.5,
    p90_minutes: 45.2,
    start_viewed_at: 3,
    start_created_at: 2,
  },
  mrp_acceptance_rate: {
    numerator: 8,
    denominator: 10,
    value: 0.8,
  },
  mrp_produced: 2,
  mrp_unreviewed: 2,
  models: 0,
  priced_models: 0,
  cost_by_model: [],
  cost_by_team: [],
  work_quality: {
    success_rate: { numerator: 8, denominator: 10, value: 0.8 },
    first_pass_acceptance: { numerator: 3, denominator: 4, value: 0.75 },
    rework_per_item: { numerator: 1, denominator: 10, value: 0.1 },
    revert_after_merge: { numerator: 1, denominator: 3, value: 1 / 3 },
    verifier_agreement: {
      verifier: 'prill',
      agreed: 2,
      false_approvals: 1,
      false_rejections: 1,
      handed_to_a_person: 0,
      without_a_pack: 0,
      false_approval_rate: { numerator: 1, denominator: 2, value: 0.5 },
      false_rejection_rate: { numerator: 0, denominator: 0, value: null, null_reason: 'denominator_zero' },
    },
    pass_pow_k: { k: 3, tasks_measured: 1, tasks_too_short: 0, value: 0.25 },
    questions: [
      { harness: 'claude-code', class: 'approval', asked: 2, actions: 4, per_100_actions: 50, granted: { numerator: 1, denominator: 2, value: 0.5 } },
    ],
    stops: [{ reason: 'timeout', count: 2, not_started: false }],
    cuts: [
      {
        by: 'harness',
        rows: [
          {
            name: 'claude-code',
            attempts: 9,
            success_rate: { numerator: 8, denominator: 9, value: 8 / 9 },
            first_pass_acceptance: { numerator: 3, denominator: 4, value: 0.75 },
            revert_after_merge: { numerator: 1, denominator: 3, value: 1 / 3 },
          },
        ],
      },
    ],
  },
  outcomes: {
    opened: 15,
    merged: 12,
    closed: 2,
    reverted: 1,
    merged_not_reverted: 11,
  },
  verification_tax: {
    implementation_attempts: 100,
    implementation_tokens: 50000,
    rework_attempts: 20,
    rework_tokens: 15000,
    verification_packs: 10,
    verification_tokens: 20000,
    attempts_unmeasured: 5,
    verification_packs_unmeasured: 1,
    ratio: {
      numerator: 35000,
      denominator: 50000,
      value: 0.7,
    },
  },
};

function setupFetchMock(data: FactoryMetrics) {
  fetchMock.mockImplementation((input) => {
    const url = String(input);
    if (url.includes('/api/projects/') && url.includes('/metrics/factory')) {
      return Promise.resolve(new Response(JSON.stringify(data), { status: 200 }));
    }
    return Promise.resolve(new Response('{}', { status: 200 }));
  });
}

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('FactoryMetrics', () => {
  it('renders the Factory metrics title and description', async () => {
    setupFetchMock(FACTORY_METRICS_FIXTURE);
    const { container } = mount();
    await flush();
    await flush();

    expect(container.textContent).toContain('Factory metrics');
    expect(container.textContent).toContain('Agent work, verification, and outcomes');
  });

  it('renders all metric cards with their titles', async () => {
    setupFetchMock(FACTORY_METRICS_FIXTURE);
    const { container } = mount();
    await flush();
    await flush();

    expect(container.textContent).toContain('Escalation rate');
    expect(container.textContent).toContain('Human minutes per decision');
    expect(container.textContent).toContain('Human minutes per pack review');
    expect(container.textContent).toContain('Pack acceptance rate');
    expect(container.textContent).toContain('Verification tax (tokens)');
    expect(container.textContent).toContain('Outcomes');
  });

  it('a_null_metric_says_not_measured', async () => {
    setupFetchMock(FACTORY_METRICS_FIXTURE);
    const { container } = mount();
    await flush();
    await flush();

    // The human_minutes_per_decision is null in the fixture
    // It should display "Not measured" not "0" or the value
    const text = container.textContent;
    expect(text).toContain('Not measured');
    expect(text).toContain('Nothing in this period was measured.');
    // Ensure it never says "0" for the null metric
    // (just check that it's not showing "0 min" for the null metric)
  });

  it('renders a measured ratio with percentage and details', async () => {
    setupFetchMock(FACTORY_METRICS_FIXTURE);
    const { container } = mount();
    await flush();
    await flush();

    // escalation_rate has value 0.1
    expect(container.textContent).toContain('10.0%');
    // It should also show the inputs
    expect(container.textContent).toContain('decisions');
    expect(container.textContent).toContain('attempts');
  });

  it('shows what the agent decided alone beside the escalation rate, never inside it', async () => {
    setupFetchMock(FACTORY_METRICS_FIXTURE);
    const { container } = mount();
    await flush();
    await flush();

    const card = (title: string) =>
      [...container.querySelectorAll('h2')].find((h) => h.textContent === title)?.parentElement?.textContent ?? '';
    // The person-answered figure keeps its own count: 10 decisions in 100 attempts.
    expect(card('Escalation rate')).toContain('10.0%');
    expect(card('Escalation rate')).toContain('10 decisions / 100 attempts');
    expect(card('Escalation rate')).not.toContain('25');
    // The policy-answered figure is a tile of its own: 25 attempts of 100.
    expect(card('Decided alone')).toContain('25.0%');
    expect(card('Decided alone')).toContain('25 attempts / 100 attempts');
  });

  it('says a decided-alone rate with nothing to count is not measured, never 0', async () => {
    setupFetchMock({
      ...FACTORY_METRICS_FIXTURE,
      attempts: 0,
      decided_alone: 0,
      decided_alone_rate: { numerator: 0, denominator: 0, value: null, null_reason: 'denominator_zero' },
    });
    const { container } = mount();
    await flush();
    await flush();

    const card = [...container.querySelectorAll('h2')].find((h) => h.textContent === 'Decided alone')?.parentElement?.textContent ?? '';
    expect(card).toContain('Not measured');
    expect(card).not.toContain('0.0%');
  });

  it('counts read as words: one decision, one pack', async () => {
    setupFetchMock({
      ...FACTORY_METRICS_FIXTURE,
      decisions: 1,
      attempts: 2,
      verification_tax: { ...FACTORY_METRICS_FIXTURE.verification_tax, verification_packs: 1 },
    });
    const { container } = mount();
    await flush();
    await flush();

    expect(container.textContent).toContain('1 decision / 2 attempts');
    expect(container.textContent).toContain('/ 1 pack');
    expect(container.textContent).not.toContain('1 packs');
  });

  it('renders measured human minutes with both median and p90', async () => {
    setupFetchMock(FACTORY_METRICS_FIXTURE);
    const { container } = mount();
    await flush();
    await flush();

    // human_minutes_per_mrp is measured
    expect(container.textContent).toContain('30.5 min');
    expect(container.textContent).toContain('45.2 min');
    // Should show start indicators
    expect(container.textContent).toContain('viewed');
    expect(container.textContent).toContain('created');
  });

  it('renders outcomes with opened count and sub-metrics', async () => {
    setupFetchMock(FACTORY_METRICS_FIXTURE);
    const { container } = mount();
    await flush();
    await flush();

    // outcomes has opened: 15
    expect(container.textContent).toContain('15');
    // Should show merged, closed
    expect(container.textContent).toContain('merged');
    expect(container.textContent).toContain('closed');
    expect(container.textContent).toContain('reverted');
  });

  it('renders verification tax with the ratio and component inputs', async () => {
    setupFetchMock(FACTORY_METRICS_FIXTURE);
    const { container } = mount();
    await flush();
    await flush();

    // verification_tax ratio is 0.7
    expect(container.textContent).toContain('70.0%');
    // Should show the inputs
    expect(container.textContent).toContain('Implementation:');
    expect(container.textContent).toContain('Rework:');
    expect(container.textContent).toContain('Verification:');
  });
});

describe('FactoryMetrics sources and cost', () => {
  const PRILL_QUALITY = {
    source: 'prill',
    prill_version: '0.4.2',
    repositories: [
      {
        repository: 'acme/app',
        metrics: {
          contract: 'metrics/v1',
          escalationRate: 0.1,
          agreement: { decided: 10, agreed: 9, rate: 0.9 },
          escapedDefects: { merged: 10, escaped: 1, rate: 0.1 },
          qualified: 3,
          qualifiedRate: 0.3,
        },
        cost_usd: 12.5,
        qualified_per_usd: 0.24,
      },
    ],
  } as unknown as FactoryMetrics['quality'];

  it('without quality, one line says why and no verifier block is shown', async () => {
    setupFetchMock({
      ...FACTORY_METRICS_FIXTURE,
      quality: null,
      quality_unavailable: 'The verifier is not configured on this board.',
    });
    const { container } = mount();
    await flush();
    await flush();

    expect(container.textContent).toContain('The verifier is not configured on this board.');
    expect(container.textContent).not.toContain('Quality (from the verifier');
  });

  it('with quality, each repository shows the figures of the verifier under its version', async () => {
    setupFetchMock({ ...FACTORY_METRICS_FIXTURE, quality: PRILL_QUALITY, cost_usd: 12.5, rate_card_as_of: '2026-10-01' });
    const { container } = mount();
    await flush();
    await flush();

    const text = container.textContent ?? '';
    expect(text).toContain('Quality (from the verifier, version 0.4.2)');
    expect(text).toContain('acme/app');
    expect(text).toContain('Escalation');
    expect(text).toContain('10.0%');
    expect(text).toContain('Agreement');
    expect(text).toContain('90.0%');
    expect(text).toContain('Escaped defects');
    expect(text).toContain('3 qualified');
    expect(text).toContain('0.24 qualified per dollar');
  });

  it('with cost_usd, shows the cost and the date the prices were read', async () => {
    setupFetchMock({ ...FACTORY_METRICS_FIXTURE, cost_usd: 12.5, cost_unpriced_attempts: 2, models: 4, priced_models: 3, rate_card_as_of: '2026-10-01' });
    const { container } = mount();
    await flush();
    await flush();

    const text = container.textContent ?? '';
    expect(text).toContain('≈ $12.50 (estimated)');
    expect(text).toContain('3 of 4 models priced');
    expect(text).toContain('Prices as of 2026-10-01');
    expect(text).toContain('2 attempts not priced');
  });

  it('without cost_usd, says the cost is not measured because there is no rate card', async () => {
    setupFetchMock({ ...FACTORY_METRICS_FIXTURE, cost_usd: null });
    const { container } = mount();
    await flush();
    await flush();

    expect(container.textContent).toContain('Enter a rate card below to see cost.');
    expect(container.textContent).toContain('Not measured');
    expect(container.textContent).not.toContain('$12.50');
  });

  it('Cost by model lists each model, and an unpriced one says not priced', async () => {
    setupFetchMock({
      ...FACTORY_METRICS_FIXTURE,
      cost_by_model: [
        { model: 'claude-sonnet-5-5', tokens_in: 1000, cache_read: 7000, tokens_out: 200, attempts: 3, usd: 1.25 },
        { model: 'opaque/unlisted', tokens_in: 40, cache_read: 0, tokens_out: 5, attempts: 1 },
      ],
      cost_by_team: [{ team: 'platform', tokens_in: 1040, cache_read: 7000, tokens_out: 205, attempts: 4 }],
    });
    const { container } = mount();
    await flush();
    await flush();

    const text = container.textContent ?? '';
    expect(text).toContain('Cost by model');
    expect(text).toContain('claude-sonnet-5-5');
    expect(text).toContain('≈ $1.25 (estimated)');
    // Counted tokens in their own column, the cache reads beside them.
    expect(text).toContain('Cache reads');
    const sonnet = [...container.querySelectorAll('tr')].find((r) => r.textContent?.includes('claude-sonnet-5-5'));
    expect([...sonnet!.querySelectorAll('td')].map((td) => td.textContent).slice(1, 4)).toEqual(['1000', '7000', '200']);
    const unlisted = [...container.querySelectorAll('tr')].find((r) => r.textContent?.includes('opaque/unlisted'));
    expect(unlisted?.textContent).toContain('not priced');
    expect(text).toContain('By team');
  });

  it('By team is left out when no run names a team', async () => {
    setupFetchMock({
      ...FACTORY_METRICS_FIXTURE,
      cost_by_model: [{ model: 'm', tokens_in: 1, cache_read: 0, tokens_out: 1, attempts: 1 }],
      cost_by_team: [{ team: 'No team', tokens_in: 1, cache_read: 0, tokens_out: 1, attempts: 1 }],
    });
    const { container } = mount();
    await flush();
    await flush();

    expect(container.textContent).not.toContain('By team');
  });

  it("labels merged-not-reverted as Tack's own figure", async () => {
    setupFetchMock(FACTORY_METRICS_FIXTURE);
    const { container } = mount();
    await flush();
    await flush();

    expect(container.textContent).toContain("Merged and not reverted (Tack's own)");
  });
});

describe('FactoryMetrics quality of the work', () => {
  const openQuality = (container: HTMLElement) => {
    const header = container.querySelector<HTMLButtonElement>('[data-testid="quality-figures"] button');
    header!.click();
  };

  it('is shut at first and says the success rate beside its title', async () => {
    setupFetchMock(FACTORY_METRICS_FIXTURE);
    const { container } = mount();
    await flush();
    await flush();

    const fold = container.querySelector('[data-testid="quality-figures"]')!;
    expect(fold.querySelector('button')!.getAttribute('aria-expanded')).toBe('false');
    expect(fold.textContent).toContain('Quality of the work');
    expect(fold.textContent).toContain('Success 80.0% (8 of 10)');
    // Nothing of its tables is drawn until it is opened.
    expect(fold.querySelector('table')).toBeNull();
  });

  it('opens into the rates with their counts, and a rate with nothing to count says so', async () => {
    setupFetchMock(FACTORY_METRICS_FIXTURE);
    const { container } = mount();
    await flush();
    await flush();
    openQuality(container);
    await flush();

    const text = container.querySelector('[data-testid="quality-figures"]')!.textContent ?? '';
    expect(text).toContain('75.0% (3 of 4)');
    expect(text).toContain('0.10 (1 extra run over 10 tasks)');
    expect(text).toContain('33.3% (1 of 3)');
    expect(text).toContain('50.0% (1 of 2)');
    expect(text).toContain('Not measured');
    expect(text).toContain('25.0% (1 recurring task)');
    expect(text).toContain('The verifier agreed on 2, approved 1 bad and rejected 1 good');
  });

  it('lists the questions, why runs stopped and the cuts', async () => {
    setupFetchMock(FACTORY_METRICS_FIXTURE);
    const { container } = mount();
    await flush();
    await flush();
    openQuality(container);
    await flush();

    const text = container.querySelector('[data-testid="quality-figures"]')!.textContent ?? '';
    expect(text).toContain('claude-code');
    expect(text).toContain('approval');
    expect(text).toContain('50.0');
    expect(text).toContain('timeout');
    expect(text).toContain('88.9% (8 of 9)');
    expect(text).not.toContain('$');
  });

  it('asks for the period it is read over', async () => {
    setupFetchMock(FACTORY_METRICS_FIXTURE);
    const { container } = mount();
    await flush();
    await flush();
    const select = container.querySelector('select')!;
    select.value = '7';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    await flush();
    await flush();

    const urls = fetchMock.mock.calls.map((call) => String(call[0]));
    expect(urls.some((url) => url.includes('/metrics/factory?since='))).toBe(true);
  });
});
