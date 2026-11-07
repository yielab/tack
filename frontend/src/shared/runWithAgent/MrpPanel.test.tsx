import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import ready from '../../../../docs/contracts/mrp-v1/fixtures/ready.json';
import notReady from '../../../../docs/contracts/mrp-v1/fixtures/not-ready.json';
import manualOnly from '../../../../docs/contracts/mrp-v1/fixtures/manual-only.json';
import verifierFailed from '../../../../docs/contracts/mrp-v1/fixtures/verifier-failed.json';
import signed from '../../../../docs/contracts/mrp-v1/fixtures/signed-with-next-steps.json';
import { Show } from 'solid-js';
import MrpPanel, { MergeState, MrpVerdict, SignatureState, buildChecks, checksSummary, createMrp } from './MrpPanel';
import type { AcceptanceCriterion } from '../api/briefs';
import type { AttemptMerge } from '../execution/attempts';
import { flush } from '../../test/flush';
const disposers: Array<() => void> = [];

/** The pieces of a run's screen that read one pack, side by side: the verdict's chips, the Checks body, and the Technical signature. */
const Pack = (props: { merge?: AttemptMerge; doneWhen?: AcceptanceCriterion[] }) => {
  const mrp = createMrp(() => ({ requestId: 'exec_1', attemptNumber: 2 }));
  return (
    <>
      <Show when={mrp.pack()}>{(pack) => <MrpVerdict pack={pack()} />}</Show>
      <Show when={props.merge}>{(m) => <MergeState merge={m()} />}</Show>
      <MrpPanel mrp={mrp} doneWhen={props.doneWhen} />
      <Show when={mrp.pack()}>{(pack) => <SignatureState signature={pack().signature} />}</Show>
    </>
  );
};

function mount(merge?: AttemptMerge, doneWhen?: AcceptanceCriterion[]) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <Pack merge={merge} doneWhen={doneWhen} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status });

/** Routes the pack's reads and the run's one review; returns the spy for call counting. */
function mockFetch(pack: unknown, review: unknown = null, viewedAt: string | null = null) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    if (url.endsWith('/mrp/viewed')) return Promise.resolve(json({ viewed_at: 'now' }));
    if (url.endsWith('/attempts/2/review')) {
      const body = JSON.parse(String(init?.body));
      // The review is the attempt's now: the next read of the pack carries it.
      review = { verdict: body.verdict, reason: body.reason ?? null, reviewed_by: 'operator:local', reviewed_at: '2026-11-07T10:00:00Z' };
      return Promise.resolve(json(review));
    }
    return Promise.resolve(json({ pack, viewed_at: viewedAt, review }));
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

  it('Reject needs a reason, Accept does not, and the review goes to the run\'s one endpoint', async () => {
    const spy = mockFetch(ready);
    const el = mount();
    await flush();
    const button = (label: string) => [...el.querySelectorAll('button')].find((b) => b.textContent === label)!;
    expect(button('Accept').disabled).toBe(false);
    expect(button('Reject').disabled).toBe(true);
    const input = el.querySelector('input')!;
    input.value = '   ';
    input.dispatchEvent(new InputEvent('input', { bubbles: true }));
    expect(button('Reject').disabled).toBe(true);
    input.value = 'looks right';
    input.dispatchEvent(new InputEvent('input', { bubbles: true }));
    expect(button('Reject').disabled).toBe(false);
    button('Accept').click();
    await flush();
    expect(JSON.parse(String(calls(spy, '/attempts/2/review')[0][1]!.body))).toEqual({ verdict: 'accepted', reason: 'looks right' });
    expect(calls(spy, '/mrp/review')).toHaveLength(0);
    expect(el.querySelector('[data-testid="mrp-review"]')!.textContent).toContain('looks right');
  });

  it('calls viewed once for an unreviewed pack, and not for a viewed one', async () => {
    const spy = mockFetch(ready);
    mount();
    await flush();
    expect(calls(spy, '/mrp/viewed')).toHaveLength(1);
    disposers.pop()!();
    spy.mockRestore();
    const spy2 = mockFetch(ready, null, 'earlier');
    mount();
    await flush();
    expect(calls(spy2, '/mrp/viewed')).toHaveLength(0);
  });

  it('the_pack_panel_says_the_pack_is_signed_and_what_was_signed', async () => {
    mockFetch(signed);
    const el = mount();
    await flush();
    const text = el.querySelector('[data-testid="mrp-signature"]')!.textContent!;
    expect(text).toContain('Signed by the verifier');
    expect(text).toContain(signed.signature.statement_sha256.slice(0, 12));
    expect(text).not.toContain('Not signed');
  });

  it('the_pack_panel_says_not_signed_for_a_pack_with_no_signature', async () => {
    mockFetch(ready);
    const el = mount();
    await flush();
    const text = el.querySelector('[data-testid="mrp-signature"]')!.textContent!;
    expect(text).toContain('Not signed');
    expect(text).not.toContain('Signed by the verifier');
  });

  it('the_pack_panel_shows_the_held_reason', async () => {
    mockFetch(ready);
    const el = mount({ enabled: true, reason: 'CI is still running on the head commit' });
    await flush();
    expect(el.querySelector('[data-testid="mrp-merge-state"]')!.textContent).toContain('CI is still running on the head commit');
  });

  it('the_pack_panel_says_who_merged_with_the_policy_behind_a_disclosure', async () => {
    mockFetch(ready);
    const el = mount({
      enabled: false,
      reason: 'Already merged',
      merged: { trigger: 'auto', level: 'L4', domain: 'docs', merged_at: '2026-10-13T10:00:00Z', merge_sha: 'abc1234', policy_snapshot: { level: 'L4' } },
    });
    await flush();
    const state = el.querySelector('[data-testid="mrp-merge-state"]')!;
    expect(state.textContent).toContain('Merged by Tack (L4, domain docs)');
    expect(state.querySelector('details pre')!.textContent).toContain('"level": "L4"');
    disposers.pop()!();
    const byYou = mount({ enabled: false, merged: { trigger: 'button', merged_at: '2026-10-13T10:00:00Z' } });
    await flush();
    expect(byYou.querySelector('[data-testid="mrp-merge-state"]')!.textContent).toContain('Merged by you');
  });

  it('the_pack_panel_lists_the_next_steps', async () => {
    mockFetch({
      ...ready,
      next_steps: [
        { code: 'suite_failed', severity: 'error', stage: 'suite', command: 'cargo test', message: 'The suite failed' },
        { code: 'criterion_open', severity: 'warning', criterion_id: 'c1', path: 'src/a.rs', line: 4, message: 'Criterion c1 has no evidence' },
      ],
    });
    const el = mount();
    await flush();
    const steps = el.querySelectorAll('[data-testid="mrp-next-step"]');
    expect(steps).toHaveLength(2);
    expect(steps[0].textContent).toContain('The suite failed');
    expect(steps[0].textContent).toContain('cargo test');
    expect(steps[1].textContent).toContain('src/a.rs:4');
  });

  describe('the checklist', () => {
    const pack = (criteria: unknown[]) => ({ ...ready, criteria });
    const row = (id: string, status: string, extra: object = {}) => ({ id, kind: 'command', status, evidence_ref: null, note: null, ...extra });

    it('marks each state with its own glyph and its words, never by colour alone', async () => {
      mockFetch(pack([row('a', 'passed'), row('b', 'failed'), row('c', 'manual'), row('d', 'skipped')]));
      const el = mount();
      await flush();
      const marks = [...el.querySelectorAll('[data-testid="mrp-criterion"]')].map((li) => {
        const mark = li.querySelector('[role="img"]')!;
        return [li.getAttribute('data-status'), mark.textContent, mark.getAttribute('aria-label')];
      });
      expect(marks).toEqual([
        ['passed', '✓', 'Passed'],
        ['failed', '✗', 'Failed'],
        ['manual', '', 'Needs you — Needs a person'],
        ['skipped', '–', 'Skipped'],
      ]);
    });

    it('names the kind and the source as secondary text, and the evidence when there is some', async () => {
      mockFetch(pack([row('a', 'passed', { evidence_ref: 'ev/a.txt' }), row('b', 'failed')]));
      const el = mount();
      await flush();
      const [first, second] = [...el.querySelectorAll('[data-testid="mrp-criterion"]')];
      expect(first.textContent).toContain('command · Agent');
      expect(first.textContent).toContain('Evidence: ev/a.txt');
      expect(second.textContent).not.toContain('Evidence');
    });

    it('puts the Done when ticks and the verifier criteria in one list, each naming its source', async () => {
      mockFetch(pack([row('c1', 'manual'), row('c2', 'passed')]));
      const doneWhen: AcceptanceCriterion[] = [
        { kind: 'manual', id: 'c1', title: 'Reads well', text: 'x', checked: true },
        { kind: 'manual', id: 'c3', title: 'Looks right', text: 'y' },
        { kind: 'command', id: 'c4', title: 'Builds', run: 'make', expect_exit: 0 },
      ];
      const el = mount(undefined, doneWhen);
      await flush();
      const items = [...el.querySelectorAll('[data-testid="checks-list"] > li')];
      expect(items.map((li) => [li.querySelector('p')!.textContent, li.getAttribute('data-status')])).toEqual([
        // A criterion the verifier reports under an id the list has takes the list's title, and a ticked one passes.
        ['Reads well', 'passed'],
        ['c2', 'passed'],
        ['Looks right', 'manual'],
        ['Builds', 'skipped'],
      ]);
      expect(items[0].textContent).toContain('Done when');
      expect(items[0].textContent).toContain('Ticked by a person');
      expect(items[1].textContent).toContain("from the agent's checks");
      expect(items[0].textContent).toContain("from the item's Done when");
      // A check the agent brings (a command) is the agent's, even when the item's brief lists it.
      expect(items[3].textContent).toContain("from the agent's checks");
      expect(items[3].textContent).toContain('The verifier did not run it');
    });

    it('says a check and a next step in plain words and keeps the raw values under How we know', async () => {
      mockFetch({
        ...pack([row('m1', 'manual', { kind: 'manual', note: 'needs_human' })]),
        next_steps: [
          { code: 'criterion_manual', severity: 'review', path: 'm1' },
          { code: 'stage_degraded', severity: 'warning', path: 'anti_tautology' },
        ],
      });
      const el = mount();
      await flush();
      const first = el.querySelector('[data-testid="mrp-criterion"]')!;
      expect(first.querySelector('p:nth-of-type(2)')!.textContent).toBe("A person checks it · from the agent's checks · waits for a person to decide");
      expect(first.querySelector('details')!.textContent).toContain('manual · Agent · needs_human');
      const steps = [...el.querySelectorAll('[data-testid="mrp-next-step"]')].map((li) => li.textContent);
      expect(steps).toEqual(['Needs a look A criterion needs a person to check it.', 'Warning A verifier stage ran in a reduced way. (anti tautology)']);
      expect(steps.join()).not.toMatch(/criterion_manual|stage_degraded|anti_tautology/);
    });

    it('says the number in one sentence', () => {
      expect(checksSummary([])).toBe('No checks were recorded for this run.');
      const items = buildChecks(pack([row('a', 'passed'), row('b', 'failed'), row('c', 'manual')]) as never, null);
      expect(checksSummary(items)).toBe('1 of 3 checks passed · 1 failed · 1 need a person');
    });
  });

  it('a reject leads with the verifier\'s reason in plain words', async () => {
    mockFetch({ ...ready, recommendation: { decision: 'reject', rationale: 'prill: reject — blocking: criterion_failed — impossible' }, risk: { tier: 'low', reasons: [] } });
    const el = mount();
    await flush();
    const text = el.querySelector('[data-testid="mrp-verdict"]')!.textContent!;
    expect(text).toContain('The verifier recommends rejecting this run: A check failed (impossible)');
  });

  it('shows the recommendation and the risk as two chips, with the verifier\'s sentence', async () => {
    mockFetch({ ...ready, recommendation: { decision: 'review', rationale: 'One criterion needs a person.' }, risk: { tier: 'high', reasons: ['touches auth'] } });
    const el = mount();
    await flush();
    const chips = [...el.querySelectorAll('[data-testid="mrp-verdict"] span')].map((b) => b.textContent);
    expect(chips).toEqual(['Recommendation: review', 'Risk: high']);
    expect(el.querySelector('[data-testid="mrp-verdict"]')!.textContent).toContain('One criterion needs a person.');
    // The reasons for the risk belong to the checks, not to the chips.
    expect(el.textContent).toContain('touches auth');
  });

  it('says the verifier\'s sentence, its risk reasons and its next step in plain words, the raw line on hover', async () => {
    const raw = 'prill: reject — blocking: criterion_failed — impossible; review: criterion_manual — m1';
    mockFetch({
      ...ready,
      recommendation: { decision: 'reject', rationale: raw },
      risk: { tier: 'low', reasons: ['blocking: criterion_failed — impossible'] },
      next_steps: [{ code: 'criterion_failed', severity: 'error', stage: 'verify', message: 'criterion impossible failed: file_present', path: 'DONE.txt' }],
    });
    const el = mount();
    await flush();
    const verdict = el.querySelector('[data-testid="mrp-verdict"]')!;
    expect(verdict.textContent).not.toMatch(/prill|criterion_failed|criterion_manual/);
    expect(verdict.textContent).toContain('A check failed (impossible); a check only a person can make (m1).');
    expect(verdict.querySelector('p')!.getAttribute('title')).toBe(raw);
    const step = el.querySelector('[data-testid="mrp-next-step"]')!;
    expect(step.textContent).toContain('The check “impossible” failed');
    expect(step.textContent).not.toContain('file_present');
  });

  it('folds the verify command and its output', async () => {
    mockFetch({ ...ready, verify: { command: 'cargo test', exit_code: 0, output_tail: 'ok', duration_s: 3 } });
    const el = mount();
    await flush();
    const fold = el.querySelector('[data-testid="mrp-verify"]') as HTMLDetailsElement;
    expect(fold.open).toBe(false);
    expect(fold.querySelector('summary')!.textContent).toContain('cargo test');
    expect(fold.querySelector('pre')!.textContent).toBe('ok');
  });
});
