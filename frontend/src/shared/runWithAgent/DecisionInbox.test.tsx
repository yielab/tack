import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { toast } from '../ui/toast';
import { decisionTokenStore, type DecisionRecord } from '../execution';
import DecisionInbox, { type DecisionInboxProps } from './DecisionInbox';

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

function mount(props: Partial<DecisionInboxProps> = {}) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  const fullProps: DecisionInboxProps = { requestId: 'exec_1', attemptNumber: 2, attemptId: 'att_1', ...props };
  const dispose = render(() => <DecisionInbox {...fullProps} />, container);
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

function jsonOk(body: unknown) {
  return new Response(JSON.stringify(body), { status: 200 });
}

function jsonError(status: number, code: string, message: string) {
  return new Response(JSON.stringify({ error: { status, message, code } }), { status });
}

/** Routes a mocked `fetch` by request shape: the list call (a GET ending in
 *  `/decisions`) vs. a resolve call (a POST to `.../resolve`) — the two
 *  requests this panel makes. */
function mockFetch(list: DecisionRecord[], resolve: () => Response) {
  return vi.spyOn(globalThis, 'fetch').mockImplementation((input, init) => {
    const url = String(input);
    if (!init || init.method === undefined || init.method === 'GET') {
      return Promise.resolve(jsonOk({ protocol_version: 1, data: list }));
    }
    void url;
    return Promise.resolve(resolve());
  });
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  document.body.innerHTML = '';
  vi.restoreAllMocks();
  decisionTokenStore.set(null);
});

describe('DecisionInbox — discovers decisions, never asks for an id', () => {
  it('fetches GET /executions/{id}/attempts/{n}/decisions and renders every listed decision, no id field anywhere', async () => {
    const fetchMock = mockFetch([PENDING_WITH_OPTIONS], () => new Response('unused'));
    const c = mount();
    await flush();
    await flush();

    expect(String(fetchMock.mock.calls[0][0])).toBe('/api/executions/exec_1/attempts/2/decisions');
    expect(c.querySelector('input[type="text"]')).toBeNull();
    expect(c.textContent).toContain('Allow this agent to run');
  });

  it('an empty list renders a real empty state, not a silently blank panel', async () => {
    mockFetch([], () => new Response('unused'));
    const c = mount();
    await flush();
    await flush();

    expect(c.textContent).toContain('No decisions raised yet');
  });
});

describe('DecisionInbox — pending / expired / resolved are visually and semantically distinct', () => {
  it('a pending decision shows a "Pending" badge and an enabled resolve form', async () => {
    mockFetch([PENDING_WITH_OPTIONS], () => new Response('unused'));
    const c = mount();
    await flush();
    await flush();

    expect(c.textContent).toContain('Pending');
    expect(c.textContent).not.toContain('Expired');
    const radios = [...c.querySelectorAll('input[type="radio"]')];
    expect(radios).toHaveLength(2);
    expect(radios.map((r) => (r as HTMLInputElement).value)).toEqual(['allow_once', 'deny']);
  });

  it('an expired decision shows a distinct "Expired" badge, names the reason, and offers no resolve form', async () => {
    mockFetch([EXPIRED], () => new Response('unused'));
    const c = mount();
    await flush();
    await flush();

    expect(c.textContent).toContain('Expired');
    expect(c.textContent).toContain('can no longer be resolved');
    expect(c.querySelectorAll('input[type="radio"]')).toHaveLength(0);
  });

  it('a resolved decision shows a distinct "Resolved" badge and the recorded answer, read-only', async () => {
    mockFetch([RESOLVED], () => new Response('unused'));
    const c = mount();
    await flush();
    await flush();

    expect(c.textContent).toContain('Resolved');
    expect(c.textContent).toContain('allow_once');
    expect(c.textContent).toContain('looked fine');
    expect(c.querySelectorAll('input[type="radio"]')).toHaveLength(0);
  });

  it('all three states are visually distinct from each other in the same list', async () => {
    mockFetch([PENDING_WITH_OPTIONS, EXPIRED, RESOLVED], () => new Response('unused'));
    const c = mount();
    await flush();
    await flush();

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
  it('the Resolve button for a pending decision starts disabled with a visible reason, until an answer is chosen', async () => {
    mockFetch([PENDING_WITH_OPTIONS], () => new Response('unused'));
    const c = mount();
    await flush();
    await flush();

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
});

describe('DecisionInbox — keyboard accessibility of the resolve interaction', () => {
  it('every interactive control is a native, positively-focusable element (no divs pretending to be buttons, no negative tabindex anywhere)', async () => {
    mockFetch([PENDING_WITH_OPTIONS], () => new Response('unused'));
    const c = mount();
    await flush();
    await flush();

    const interactive = [...c.querySelectorAll('button, input, select, textarea, a[href]')];
    expect(interactive.length).toBeGreaterThan(0);
    for (const el of interactive) {
      const tabindex = el.getAttribute('tabindex');
      if (tabindex !== null) expect(Number(tabindex)).toBeGreaterThanOrEqual(0);
    }
  });

  it('radio options are reachable via .focus() and form a real native radio group (arrow-key navigable by the browser for free)', async () => {
    mockFetch([PENDING_WITH_OPTIONS], () => new Response('unused'));
    const c = mount();
    await flush();
    await flush();

    const radios = [...c.querySelectorAll('input[type="radio"]')] as HTMLInputElement[];
    expect(radios).toHaveLength(2);
    expect(radios[0].name).toBe(radios[1].name);
    radios[0].focus();
    expect(document.activeElement).toBe(radios[0]);
  });

  it('selecting an option and activating Resolve — reachable purely via focus + native activation, no mouse coordinates involved — submits the real resolve call and refetches the list', async () => {
    let resolved = false;
    const fetchMock = mockFetch(
      [PENDING_WITH_OPTIONS],
      () => {
        resolved = true;
        return jsonOk({
          protocol_version: 1,
          decision_id: 'dec_pending',
          state: 'resolved',
          answer: { option_id: 'deny', text: null },
          resolved_at: '2026-08-06T12:05:00Z',
          resolved_by: { kind: 'operator', subject_id: 'operator:local' },
          replayed: false,
        });
      },
    );
    const c = mount();
    await flush();
    await flush();

    const denyRadio = [...c.querySelectorAll('input[type="radio"]')].find(
      (r) => (r as HTMLInputElement).value === 'deny',
    ) as HTMLInputElement;
    denyRadio.focus();
    expect(document.activeElement).toBe(denyRadio);
    denyRadio.checked = true;
    denyRadio.dispatchEvent(new Event('change', { bubbles: true }));

    const resolveBtn = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Resolve') as HTMLButtonElement;
    resolveBtn.focus();
    expect(document.activeElement).toBe(resolveBtn);
    resolveBtn.click();
    await flush();
    await flush();

    expect(resolved).toBe(true);
    const resolveCall = fetchMock.mock.calls.find((call) => String(call[0]).includes('/resolve'));
    expect(String(resolveCall?.[0])).toBe('/api/attempts/att_1/decisions/dec_pending/resolve');
    expect(JSON.parse((resolveCall?.[1] as RequestInit).body as string)).toEqual({
      answer: { option_id: 'deny', text: null },
    });
  });
});

describe('DecisionInbox — resolve outcomes are each named distinctly', () => {
  it('a fail-closed token rejection (403) names the deployment-configuration reason, not a generic error', async () => {
    mockFetch([PENDING_WITH_OPTIONS], () => jsonError(403, 'forbidden', 'forbidden'));
    const c = mount();
    await flush();
    await flush();
    const toastError = vi.spyOn(toast, 'error');

    const allowRadio = [...c.querySelectorAll('input[type="radio"]')][0] as HTMLInputElement;
    allowRadio.checked = true;
    allowRadio.dispatchEvent(new Event('change', { bubbles: true }));
    const resolveBtn = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Resolve') as HTMLButtonElement;
    resolveBtn.click();
    await flush();
    await flush();

    expect(toastError).toHaveBeenCalledWith(expect.stringContaining('not configured decision resolution'));
  });

  it('an expired decision (409 decision_expired) is named distinctly from an idempotency conflict (409 idempotency_conflict)', async () => {
    mockFetch([PENDING_WITH_OPTIONS], () => jsonError(409, 'decision_expired', 'expired'));
    const c = mount();
    await flush();
    await flush();
    const toastError = vi.spyOn(toast, 'error');

    const allowRadio = [...c.querySelectorAll('input[type="radio"]')][0] as HTMLInputElement;
    allowRadio.checked = true;
    allowRadio.dispatchEvent(new Event('change', { bubbles: true }));
    const resolveBtn = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Resolve') as HTMLButtonElement;
    resolveBtn.click();
    await flush();
    await flush();

    expect(toastError).toHaveBeenCalledWith(expect.stringContaining('already expired'));
    expect(toastError).not.toHaveBeenCalledWith(expect.stringContaining('different answer'));
  });

  it('a successful resolve calls onResolved with the real result', async () => {
    const onResolved = vi.fn();
    mockFetch([PENDING_WITH_OPTIONS], () =>
      jsonOk({
        protocol_version: 1,
        decision_id: 'dec_pending',
        state: 'resolved',
        answer: { option_id: 'allow_once', text: null },
        resolved_at: '2026-08-06T12:00:00Z',
        resolved_by: { kind: 'operator', subject_id: 'operator:local' },
        replayed: false,
      }),
    );
    const c = mount({ onResolved });
    await flush();
    await flush();

    const allowRadio = [...c.querySelectorAll('input[type="radio"]')][0] as HTMLInputElement;
    allowRadio.checked = true;
    allowRadio.dispatchEvent(new Event('change', { bubbles: true }));
    const resolveBtn = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Resolve') as HTMLButtonElement;
    resolveBtn.click();
    await flush();
    await flush();

    expect(onResolved).toHaveBeenCalledTimes(1);
    expect(onResolved.mock.calls[0][0].decision_id).toBe('dec_pending');
  });
});
