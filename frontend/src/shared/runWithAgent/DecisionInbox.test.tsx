import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { toast } from '../ui/toast';
import { decisionTokenStore, type DecisionRecord, type ResolveDecisionResult } from '../execution';
import DecisionInbox from './DecisionInbox';

const flush = () => new Promise((r) => setTimeout(r, 0));
const disposers: Array<() => void> = [];

const PENDING_WITH_OPTIONS: DecisionRecord = {
  decision_id: 'dec_pending',
  attempt_id: 'att_1',
  kind: 'permission',
  prompt: 'Allow this agent to run `rm -rf node_modules`?',
  options: [
    { option_id: 'allow_once', label: 'Allow once' },
    { option_id: 'deny', label: 'Deny' },
  ],
  metadata: {},
  expires_at: '2026-08-06T13:00:00Z',
  state: 'pending',
  answer: null,
  resolved_at: null,
  resolved_by: null,
  created_at: '2026-08-06T12:00:00Z',
  updated_at: '2026-08-06T12:00:00Z',
};

const PENDING_FREEFORM: DecisionRecord = {
  ...PENDING_WITH_OPTIONS,
  decision_id: 'dec_pending_freeform',
  options: [],
};

const EXPIRED: DecisionRecord = {
  ...PENDING_WITH_OPTIONS,
  decision_id: 'dec_expired',
  state: 'expired',
};

const RESOLVED: DecisionRecord = {
  ...PENDING_WITH_OPTIONS,
  decision_id: 'dec_resolved',
  state: 'resolved',
  answer: { option_id: 'allow_once', text: 'looked fine' },
  resolved_at: '2026-08-06T12:30:00Z',
  resolved_by: { kind: 'operator', subject_id: 'operator:local' },
};

function mount(props: Parameters<typeof DecisionInbox>[0]) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(() => <DecisionInbox {...props} />, container);
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
  decisionTokenStore.set(null);
});

describe('DecisionInbox — pending / expired / resolved are visually and semantically distinct', () => {
  it('a pending decision shows a "Pending" badge and an enabled resolve form', () => {
    const c = mount({ attemptId: 'att_1', decisions: [PENDING_WITH_OPTIONS] });
    expect(c.textContent).toContain('Pending');
    expect(c.textContent).not.toContain('Expired');
    // The declared options render as real radio inputs, not a fake widget.
    const radios = [...c.querySelectorAll('input[type="radio"]')];
    expect(radios).toHaveLength(2);
    expect(radios.map((r) => (r as HTMLInputElement).value)).toEqual(['allow_once', 'deny']);
  });

  it('an expired decision shows a distinct "Expired" badge, names the reason, and offers no resolve form', () => {
    const c = mount({ attemptId: 'att_1', decisions: [EXPIRED] });
    expect(c.textContent).toContain('Expired');
    expect(c.textContent).toContain('can no longer be resolved');
    // No radios/resolve form for this decision's own row (the manual quick
    // action's own inputs are text fields, not radios, so this stays a
    // clean check that this row specifically renders nothing interactive).
    expect(c.querySelectorAll('input[type="radio"]')).toHaveLength(0);
  });

  it('a resolved decision shows a distinct "Resolved" badge and the recorded answer, read-only', () => {
    const c = mount({ attemptId: 'att_1', decisions: [RESOLVED] });
    expect(c.textContent).toContain('Resolved');
    expect(c.textContent).toContain('allow_once');
    expect(c.textContent).toContain('looked fine');
    expect(c.querySelectorAll('input[type="radio"]')).toHaveLength(0);
  });

  it('all three states are visually distinct from each other in the same list', () => {
    const c = mount({ attemptId: 'att_1', decisions: [PENDING_WITH_OPTIONS, EXPIRED, RESOLVED] });
    const badges = [...c.querySelectorAll('li')].map((li) => {
      if (li.textContent?.includes('Pending')) return 'pending';
      if (li.textContent?.includes('Expired')) return 'expired';
      if (li.textContent?.includes('Resolved')) return 'resolved';
      return 'unknown';
    });
    expect(new Set(badges).size).toBeGreaterThanOrEqual(3);
  });
});

describe('DecisionInbox — disabled controls name their reason', () => {
  it('the Resolve button for a pending decision starts disabled with a visible reason, until an answer is chosen', () => {
    const c = mount({ attemptId: 'att_1', decisions: [PENDING_WITH_OPTIONS] });
    const resolveBtn = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Resolve') as HTMLButtonElement;
    expect(resolveBtn.disabled).toBe(true);
    expect(c.textContent).toContain('Select or enter an answer to enable Resolve.');

    const allowRadio = [...c.querySelectorAll('input[type="radio"]')].find(
      (r) => (r as HTMLInputElement).value === 'allow_once',
    ) as HTMLInputElement;
    allowRadio.checked = true;
    allowRadio.dispatchEvent(new Event('change', { bubbles: true }));

    expect(resolveBtn.disabled).toBe(false);
  });

  it('the manual "resolve by id" action starts disabled with a visible reason until both fields are filled', () => {
    const c = mount({ attemptId: 'att_1' });
    const resolveBtn = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Resolve decision') as HTMLButtonElement;
    expect(resolveBtn.disabled).toBe(true);
    expect(c.textContent).toContain('Enter a decision id and an answer to enable Resolve.');
  });
});

describe('DecisionInbox — keyboard accessibility of the resolve interaction', () => {
  it('every interactive control is a native, positively-focusable element (no divs pretending to be buttons, no negative tabindex anywhere)', () => {
    const c = mount({ attemptId: 'att_1', decisions: [PENDING_WITH_OPTIONS] });
    const interactive = [...c.querySelectorAll('button, input, select, textarea, a[href]')];
    expect(interactive.length).toBeGreaterThan(0);
    for (const el of interactive) {
      const tabindex = el.getAttribute('tabindex');
      // Either no explicit tabindex (native tab order applies) or a
      // non-negative one — never -1, which would silently remove a real
      // control from the keyboard tab sequence.
      if (tabindex !== null) expect(Number(tabindex)).toBeGreaterThanOrEqual(0);
    }
  });

  it('radio options are reachable via .focus() and form a real native radio group (arrow-key navigable by the browser for free)', () => {
    const c = mount({ attemptId: 'att_1', decisions: [PENDING_WITH_OPTIONS] });
    const radios = [...c.querySelectorAll('input[type="radio"]')] as HTMLInputElement[];
    expect(radios).toHaveLength(2);
    // A native radio group needs a shared `name` for the browser's own
    // built-in arrow-key navigation/mutual-exclusion — this is what makes
    // the group keyboard-operable without any custom key handling code.
    expect(radios[0].name).toBe(radios[1].name);
    radios[0].focus();
    expect(document.activeElement).toBe(radios[0]);
  });

  it('selecting an option and activating Resolve — reachable purely via focus + native activation, no mouse coordinates involved — submits the real resolve call', async () => {
    const c = mount({ attemptId: 'att_1', decisions: [PENDING_WITH_OPTIONS] });
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          protocol_version: 1,
          decision_id: 'dec_pending',
          state: 'resolved',
          answer: { option_id: 'deny', text: null },
          resolved_at: '2026-08-06T12:05:00Z',
          resolved_by: { kind: 'operator', subject_id: 'operator:local' },
          replayed: false,
        } satisfies ResolveDecisionResult),
        { status: 200 },
      ),
    );

    const denyRadio = [...c.querySelectorAll('input[type="radio"]')].find(
      (r) => (r as HTMLInputElement).value === 'deny',
    ) as HTMLInputElement;
    // Focus, then select — the same two steps a keyboard-only user performs
    // (Tab to the group, arrow to the option); jsdom does not simulate
    // native arrow-key radio selection, so the change event stands in for
    // "the browser just selected this option", the same technique the rest
    // of this suite (and the rest of this codebase's tests) already use for
    // checkbox/radio interactions.
    denyRadio.focus();
    expect(document.activeElement).toBe(denyRadio);
    denyRadio.checked = true;
    denyRadio.dispatchEvent(new Event('change', { bubbles: true }));

    const resolveBtn = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Resolve') as HTMLButtonElement;
    resolveBtn.focus();
    expect(document.activeElement).toBe(resolveBtn);
    // A native <button type="submit"> inside a <form> responds identically
    // to Enter/Space and a mouse click — `.click()` is the same event this
    // codebase's other tests already use to represent activation
    // (`ExecutionTimeline.test.tsx`'s `cancelBtn.click()`), and is faithful
    // here specifically because this is a real <button>, not a custom
    // click-handler div.
    resolveBtn.click();
    await flush();
    await flush();

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toBe('/api/attempts/att_1/decisions/dec_pending/resolve');
    expect(JSON.parse((init as RequestInit).body as string)).toEqual({ answer: { option_id: 'deny', text: null } });
  });
});

describe('DecisionInbox — resolve outcomes are each named distinctly', () => {
  function jsonError(status: number, code: string, message: string) {
    return new Response(JSON.stringify({ error: { status, message, code } }), { status });
  }

  it('a fail-closed token rejection (403) names the deployment-configuration reason, not a generic error', async () => {
    const c = mount({ attemptId: 'att_1' });
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonError(403, 'forbidden', 'forbidden'));
    const toastError = vi.spyOn(toast, 'error');

    fillManualForm(c, 'dec_1', 'allow_once');
    const resolveBtn = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Resolve decision') as HTMLButtonElement;
    resolveBtn.click();
    await flush();
    await flush();

    expect(toastError).toHaveBeenCalledWith(expect.stringContaining('not configured decision resolution'));
  });

  it('an expired decision (409 decision_expired) is named distinctly from an idempotency conflict (409 idempotency_conflict)', async () => {
    const c = mount({ attemptId: 'att_1' });
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonError(409, 'decision_expired', 'expired'));
    const toastError = vi.spyOn(toast, 'error');

    fillManualForm(c, 'dec_1', 'allow_once');
    const resolveBtn = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Resolve decision') as HTMLButtonElement;
    resolveBtn.click();
    await flush();
    await flush();

    expect(toastError).toHaveBeenCalledWith(expect.stringContaining('already expired'));
    expect(toastError).not.toHaveBeenCalledWith(expect.stringContaining('different answer'));
  });

  it('a successful resolve calls onResolved with the real result', async () => {
    const onResolved = vi.fn();
    const c = mount({ attemptId: 'att_1', onResolved });
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          protocol_version: 1,
          decision_id: 'dec_1',
          state: 'resolved',
          answer: { option_id: 'allow_once', text: null },
          resolved_at: '2026-08-06T12:00:00Z',
          resolved_by: { kind: 'operator', subject_id: 'operator:local' },
          replayed: false,
        }),
        { status: 200 },
      ),
    );

    fillManualForm(c, 'dec_1', 'allow_once');
    const resolveBtn = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Resolve decision') as HTMLButtonElement;
    resolveBtn.click();
    await flush();
    await flush();

    expect(onResolved).toHaveBeenCalledTimes(1);
    expect(onResolved.mock.calls[0][0].decision_id).toBe('dec_1');
  });
});

function fillManualForm(container: HTMLElement, decisionId: string, optionId: string) {
  const inputs = [...container.querySelectorAll('input')].filter(
    (i) => i.type !== 'radio' && i.type !== 'password',
  ) as HTMLInputElement[];
  // The manual quick-action form's own three text fields are the LAST three
  // plain text inputs rendered (after any per-decision-row fields).
  const [decisionIdInput, optionIdInput] = inputs.slice(-3);
  decisionIdInput.value = decisionId;
  decisionIdInput.dispatchEvent(new Event('input', { bubbles: true }));
  optionIdInput.value = optionId;
  optionIdInput.dispatchEvent(new Event('input', { bubbles: true }));
}
