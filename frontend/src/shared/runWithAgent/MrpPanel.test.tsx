import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import ready from '../../../../docs/contracts/mrp-v1/fixtures/ready.json';
import notReady from '../../../../docs/contracts/mrp-v1/fixtures/not-ready.json';
import manualOnly from '../../../../docs/contracts/mrp-v1/fixtures/manual-only.json';
import verifierFailed from '../../../../docs/contracts/mrp-v1/fixtures/verifier-failed.json';
import MrpPanel from './MrpPanel';

const flush = () => new Promise((r) => setTimeout(r, 0));
const disposers: Array<() => void> = [];

function mount() {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <MrpPanel requestId="exec_1" attemptNumber={2} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

/** Routes the three MRP calls; returns the spy for call counting. */
function mockFetch(pack: unknown, review: unknown = null) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    if (url.endsWith('/mrp/viewed')) return Promise.resolve(json({ viewed_at: 'now' }));
    if (url.endsWith('/mrp/review')) {
      const body = JSON.parse(String(init?.body));
      return Promise.resolve(json({ verdict: body.verdict, reason: body.reason }));
    }
    return Promise.resolve(json({ pack, review }));
  });
}

const calls = (spy: ReturnType<typeof mockFetch>, suffix: string) =>
  spy.mock.calls.filter(([u]) => String(u).endsWith(suffix));

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  vi.restoreAllMocks();
});

describe('MrpPanel', () => {
  it('a_null_section_says_not_run', async () => {
    mockFetch({ ...ready, verify: null, mutation: null, static_analysis: null, judge: null });
    const el = mount();
    await flush();
    expect(el.textContent!.match(/Not run/g)).toHaveLength(4);
    expect(el.textContent).not.toContain('Exit code');
  });

  it.each([
    ['ready', ready],
    ['not-ready', notReady],
    ['manual-only', manualOnly],
    ['verifier-failed', verifierFailed],
  ])('renders the %s fixture, one row per criterion, a null section as not run', async (_n, pack) => {
    mockFetch(pack);
    const el = mount();
    await flush();
    const p = pack as typeof ready;
    expect(el.querySelectorAll('[data-testid="mrp-criterion"]')).toHaveLength(p.criteria.length);
    expect(el.textContent).toContain(p.recommendation.decision);
    const nullSections = [p.verify, p.mutation, p.static_analysis, p.judge].filter((s) => s == null).length;
    expect(el.textContent!.match(/Not run/g)?.length ?? 0).toBe(nullSections);
  });

  it('accept is disabled until the reason is non-blank, then records the verdict', async () => {
    const spy = mockFetch(ready);
    const el = mount();
    await flush();
    const accept = [...el.querySelectorAll('button')].find((b) => b.textContent === 'Accept')!;
    expect(accept.disabled).toBe(true);
    const input = el.querySelector('input')!;
    input.value = '   ';
    input.dispatchEvent(new InputEvent('input', { bubbles: true }));
    expect(accept.disabled).toBe(true);
    input.value = 'looks right';
    input.dispatchEvent(new InputEvent('input', { bubbles: true }));
    expect(accept.disabled).toBe(false);
    accept.click();
    await flush();
    expect(JSON.parse(String(calls(spy, '/mrp/review')[0][1]!.body))).toEqual({ verdict: 'accept', reason: 'looks right' });
    expect(el.querySelector('[data-testid="mrp-review"]')!.textContent).toContain('looks right');
  });

  it('calls viewed once for an unreviewed pack, and not for a viewed one', async () => {
    const spy = mockFetch(ready);
    mount();
    await flush();
    expect(calls(spy, '/mrp/viewed')).toHaveLength(1);
    disposers.pop()!();
    spy.mockRestore();
    const spy2 = mockFetch(ready, { verdict: null, viewed_at: 'earlier' });
    mount();
    await flush();
    expect(calls(spy2, '/mrp/viewed')).toHaveLength(0);
  });
});
