import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import AutonomyByDomain from './AutonomyByDomain';
import { flush } from '../../test/flush';

vi.mock('@solidjs/router', () => ({
  useParams: () => ({ id: 'p1' }),
}));

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

const ratio = (numerator: number, denominator: number) => ({
  numerator,
  denominator,
  value: denominator === 0 ? null : numerator / denominator,
});
const row = (name: string, attempts: number, ok: number) => ({
  name,
  attempts,
  success_rate: ratio(ok, attempts),
  first_pass_acceptance: ratio(ok - 1, attempts),
  revert_after_merge: ratio(1, ok),
});

const SETTINGS = {
  level: 'L4',
  merge_on_done: false,
  verify_loop_max_attempts: 2,
  verify_require: [],
  budgets: {},
  domains: [
    { name: 'Docs', level: 'L5', path_globs: ['docs/**'], merged: 12, undone: 0 },
    { name: 'Payments', level: 'L3', path_globs: ['src/pay/**'], merged: 5, undone: 2 },
  ],
};

const cuts = (...by: Array<[string, ReturnType<typeof row>[]]>) => ({
  work_quality: { cuts: by.map(([b, rows]) => ({ by: b, rows })) },
});

/** Mounts the part with the autonomy read answered by `settings` and the metrics read by `metrics`. */
function mount(settings: unknown, metrics: unknown, since?: string) {
  const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
    const url = String(input);
    const body = url.includes('/autonomy') ? settings : url.includes('/metrics/factory') ? metrics : {};
    return new Response(JSON.stringify(body), { status: 200 });
  }) as typeof fetch);
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <AutonomyByDomain since={since} />, container);
  return { container, dispose, urls: () => fetchMock.mock.calls.map((call) => String(call[0])) };
}

describe('AutonomyByDomain', () => {
  it('leads with the project level, in words, and what merged by itself and what was undone', async () => {
    const { container, dispose } = mount(SETTINGS, cuts());
    await flush();
    await flush();
    const level = container.querySelector('[data-testid="autonomy-level"]')!;
    expect(level.textContent).toContain('L4');
    expect(level.textContent).toContain('Tack merges changes that stay inside the domains.');
    expect(container.querySelector('[data-testid="autonomy-totals"]')!.textContent).toBe('17 merged by itself, 2 undone');
    // The level comes before the domains.
    const headings = [...container.querySelectorAll('h2')].map((h) => h.textContent);
    expect(headings).toEqual(["The project's level", 'Domains', 'How the work went']);
    dispose();
  });

  it('lists each domain with its level, what merged by itself and what was undone', async () => {
    const { container, dispose } = mount(SETTINGS, cuts());
    await flush();
    await flush();
    expect(container.querySelector('[data-testid="domain-Docs"]')!.textContent).toBe('DocsL5120');
    expect(container.querySelector('[data-testid="domain-Payments"]')!.textContent).toBe('PaymentsL352');
    dispose();
  });

  it('says so when the project has no domains', async () => {
    const { container, dispose } = mount({ ...SETTINGS, domains: [] }, cuts());
    await flush();
    await flush();
    expect(container.textContent).toContain('No domains are set in this project.');
    expect(container.querySelector('[data-testid="autonomy-totals"]')!.textContent).toBe('0 merged by itself, 0 undone');
    dispose();
  });

  it('shows the rates by domain and by level when the board cuts the work that way, the work that did not merge in its own row', async () => {
    const { container, dispose } = mount(
      SETTINGS,
      cuts(['domain', [row('Docs', 10, 9)]], ['level', [row('L4', 8, 6), row('not merged', 4, 1)]]),
    );
    await flush();
    await flush();
    const domain = container.querySelector('[data-testid="autonomy-cut-domain"]')!.textContent ?? '';
    expect(domain).toContain('Docs');
    expect(domain).toContain('90.0% (9 of 10)');
    const level = container.querySelector('[data-testid="autonomy-cut-level"]')!.textContent ?? '';
    expect(level).toContain('L4');
    expect(level).toContain('Not merged');
    expect(level).toContain('25.0% (1 of 4)');
    expect(container.querySelector('[aria-disabled="true"]')).toBeNull();
    dispose();
  });

  it('shows the table disabled, with its reason, when the board does not cut the work by level', async () => {
    const { container, dispose } = mount(SETTINGS, cuts(['domain', [row('Docs', 10, 9)]], ['harness', [row('claude-code', 3, 3)]]));
    await flush();
    await flush();
    const level = container.querySelector('[data-testid="autonomy-cut-level"]')!;
    expect(level.querySelector('[aria-disabled="true"]')).not.toBeNull();
    expect(level.textContent).toContain('Not measured yet: the board does not cut the work by level');
    // What it does cut stays readable, and the cuts by who did the work are left to Agents' work.
    expect(container.querySelector('[data-testid="autonomy-cut-domain"] [aria-disabled="true"]')).toBeNull();
    expect(container.textContent).not.toContain('claude-code');
    dispose();
  });

  it('reads the rates over the period it is given', async () => {
    const { urls, dispose } = mount(SETTINGS, cuts(), '2026-11-19T00:00:00.000Z');
    await flush();
    await flush();
    expect(urls().some((url) => url.includes('/metrics/factory?since=2026-11-19'))).toBe(true);
    dispose();
  });
});
