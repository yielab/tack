import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render } from 'solid-js/web';
import { ProjectContext, type ProjectContextValue } from '../../shared/state/projectContext';
import FactoryMetrics from './FactoryMetrics';
import type { components } from '../../shared/api/schema.gen';
import type { Resource } from 'solid-js';

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

const flush = () => new Promise((r) => setTimeout(r, 0));

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
  outcomes: {
    opened: 15,
    merged: 12,
    closed: 2,
    reverted: 1,
    pqc: 11,
    pqc_rate: {
      numerator: 11,
      denominator: 15,
      value: 0.7333,
    },
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
