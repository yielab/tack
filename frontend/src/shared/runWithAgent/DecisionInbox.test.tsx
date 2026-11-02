import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { toast } from '../ui/toast';
import { decisionTokenStore, type DecisionRecord } from '../execution';
import DecisionInbox, { type DecisionInboxProps } from './DecisionInbox';
import { flush } from '../../test/flush';
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

const ANSWERED_ELSEWHERE: DecisionRecord = {
  ...PENDING_WITH_OPTIONS,
  decision_id: 'dec_elsewhere',
  state: 'answered_elsewhere',
  resolved_at: '2026-08-06T12:30:00Z',
  resolved_by: { kind: 'harness', subject_id: 'docket' },
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
    if (url.endsWith('/viewed')) return Promise.resolve(jsonOk({ viewed_at: '2026-08-06T12:01:00Z' }));
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

  it('the token prompt names no variable and no file, and links to the Administrator Guide', async () => {
    mockFetch([PENDING_WITH_OPTIONS], () => new Response('unused'));
    const c = mount();
    await flush();
    await flush();

    expect(c.textContent).toContain('Ask your administrator for it');
    expect(c.textContent).not.toMatch(/TACK_EXECUTION|decision-token file/);
    (c.querySelector('button[aria-label="Help: Your decision token"]') as HTMLButtonElement).click();
    expect(c.innerHTML).toContain('admin-guide/administration.html#authentication');
    expect((c.querySelector('input[type="password"]') as HTMLInputElement).placeholder).toBe('');
  });

  it('a token already given this session is not asked for again on a run; Change brings the field back', async () => {
    mockFetch([PENDING_WITH_OPTIONS], () => new Response('unused'));
    decisionTokenStore.set('given-elsewhere');
    const c = mount();
    await flush();
    await flush();

    expect(c.querySelector('input[type="password"]')).toBeNull();
    expect(c.querySelector('[data-testid="decision-token-given"]')!.textContent).toContain('Decision token given for this browser session');
    expect(c.textContent).not.toContain('given-elsewhere');
    [...c.querySelectorAll('button')].find((b) => b.textContent === 'Change')!.click();
    await flush();
    expect(c.querySelector('input[type="password"]')).not.toBeNull();
  });

  it('a failed load says what could not load and Try again loads it', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValueOnce(new Response(JSON.stringify({ error: { status: 500, message: 'boom' } }), { status: 500 }))
      .mockResolvedValue(new Response(JSON.stringify({ protocol_version: 1, data: [PENDING_WITH_OPTIONS] }), { status: 200 }));
    const c = mount();
    await flush();
    await flush();
    expect(c.textContent).toContain('Could not load the questions');
    expect(c.textContent).not.toContain('The agent has asked nothing yet');
    ([...c.querySelectorAll('button')].find((b) => b.textContent === 'Try again') as HTMLButtonElement).click();
    await flush();
    await flush();
    expect(fetchMock.mock.calls.length).toBeGreaterThan(1);
    expect(c.textContent).toContain('Allow this agent to run');
    expect(c.querySelector('[data-testid="load-error"]')).toBeNull();
  });

  it('an empty list renders a real empty state, not a silently blank panel', async () => {
    mockFetch([], () => new Response('unused'));
    const c = mount();
    await flush();
    await flush();

    expect(c.textContent).toContain('The agent has asked nothing yet');
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

  it('a decision answered in the agent\'s own tool says where, is not "Expired" and offers no resolve form', async () => {
    mockFetch([ANSWERED_ELSEWHERE], () => new Response('unused'));
    const c = mount();
    await flush();
    await flush();

    expect(c.textContent).toContain('Answered in docket');
    expect(c.textContent).not.toContain('Expired');
    expect(c.textContent).not.toContain('unrecognised');
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

    expect(toastError).toHaveBeenCalledWith(expect.stringContaining('no decision token set up'));
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

describe('DecisionInbox — the consultation pack', () => {
  const WITH_PACK: DecisionRecord = {
    ...PENDING_WITH_OPTIONS,
    decision_id: 'dec_pack',
    options: [
      {
        option_id: 'allow_once',
        label: 'Allow once',
        description: 'Runs the test command one time.',
        risks: ['Executes a command on the runner host'],
        estimated_tokens: 1200,
      },
      { option_id: 'deny', label: 'Deny' },
    ],
    recommendation: {
      option_id: 'allow_once',
      rationale: 'The command is scoped to one crate.',
      evidence_refs: ['art_1'],
    },
  };

  it('shows the kind, each option\'s description, risks and tokens, and preselects the recommended option with its rationale', async () => {
    mockFetch([WITH_PACK], () => new Response('unused'));
    const c = mount();
    await flush();
    await flush();

    expect(c.textContent).toContain('permission');
    expect(c.textContent).toContain('Runs the test command one time.');
    expect(c.textContent).toContain('Executes a command on the runner host');
    expect(c.textContent).toContain('1,200');
    expect(c.textContent).toContain('The command is scoped to one crate.');
    expect(c.textContent).toContain('art_1');
    const radios = [...c.querySelectorAll('input[type="radio"]')] as HTMLInputElement[];
    expect(radios.find((r) => r.checked)?.value).toBe('allow_once');
  });

  it('calls viewed exactly once per decision id, however often the list is refetched', async () => {
    const fetchMock = mockFetch([WITH_PACK], () => jsonOk({}));
    const c = mount();
    await flush();
    await flush();
    // A resolve refetches the list and hands the row a new object.
    const resolveBtn = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Resolve') as HTMLButtonElement;
    resolveBtn.click();
    await flush();
    await flush();

    const viewedCalls = fetchMock.mock.calls.filter((call) => String(call[0]).endsWith('/viewed'));
    expect(viewedCalls.map((call) => String(call[0]))).toEqual(['/api/attempts/att_1/decisions/dec_pack/viewed']);
  });
  describe('an action that leaves the team', () => {
    const CALL = Array.from({ length: 30 }, (_, i) => `line ${i + 1} to ana@example.com`).join('\n');
    const LEAVING: DecisionRecord = {
      ...PENDING_WITH_OPTIONS,
      decision_id: 'dec_leaves',
      kind: 'approval',
      prompt: 'tool call send_email: ask before every send (cut)',
      metadata: { outward: true, call: CALL },
    };

    it('is titled as one, and its whole call sits above the options in a block that scrolls', async () => {
      mockFetch([LEAVING], () => new Response('unused'));
      const c = mount();
      await flush();
      await flush();

      expect(c.textContent).toContain('This action leaves the team');
      const block = c.querySelector('[data-testid="decision-call"]') as HTMLElement;
      expect(block.textContent).toBe(CALL);
      expect(block.style.fontFamily).toContain('mono');
      expect(block.style.maxHeight).toBe('30em'); // twenty lines of 1.5
      expect(block.style.overflow || block.className).toMatch(/overflow/);
      const options = c.querySelector('fieldset') as HTMLElement;
      expect(block.compareDocumentPosition(options) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    });

    it('once answered is still titled as one but holds no call', async () => {
      mockFetch([{ ...LEAVING, state: 'resolved', answer: { option_id: 'allow_once' }, metadata: { outward: true } }], () => new Response('unused'));
      const c = mount();
      await flush();
      await flush();

      expect(c.textContent).toContain('This action leaves the team');
      expect(c.querySelector('[data-testid="decision-call"]')).toBeNull();
    });

    it('an ordinary question has neither the title nor the block', async () => {
      mockFetch([PENDING_WITH_OPTIONS], () => new Response('unused'));
      const c = mount();
      await flush();
      await flush();

      expect(c.textContent).not.toContain('leaves the team');
      expect(c.querySelector('[data-testid="decision-call"]')).toBeNull();
    });
  });

  describe('what is being approved comes first', () => {
    const DISPATCH = '<<<card-text id=abc123>>>\nFix the importer\nLong description of the work\n<<</card-text>>>\nWhen done, write the summary.';

    it('an approval puts the call first and the dispatch prompt in a closed fold below it', async () => {
      mockFetch(
        [{ ...PENDING_WITH_OPTIONS, kind: 'approval', prompt: DISPATCH, metadata: { outward: true, call: 'send_email --to ana@example.com' } }],
        () => new Response('unused'),
      );
      const c = mount();
      await flush();
      await flush();
      const call = c.querySelector('[data-testid="decision-call"]') as HTMLElement;
      const fold = c.querySelector('[data-testid="decision-given"]') as HTMLDetailsElement;
      expect(call.textContent).toBe('send_email --to ana@example.com');
      expect(fold.open).toBe(false);
      expect(fold.textContent).toContain('What the agent was given');
      expect(fold.textContent).toContain('<<<card-text id=abc123>>>');
      expect(call.compareDocumentPosition(fold) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    });

    it('a plain question stays the headline and has no fold', async () => {
      mockFetch([PENDING_WITH_OPTIONS], () => new Response('unused'));
      const c = mount();
      await flush();
      await flush();
      expect(c.querySelector('[data-testid="decision-headline"]')!.textContent).toContain('rm -rf node_modules');
      expect(c.querySelector('[data-testid="decision-given"]')).toBeNull();
    });

    it('a long id is cut in the middle, whole in its title and in what Copy copies', async () => {
      const id = 'dec_6174f3a1b2c4d5e6f7a8b9c0d1e23d6f';
      const write = vi.fn().mockResolvedValue(undefined);
      Object.defineProperty(navigator, 'clipboard', { value: { writeText: write }, configurable: true });
      mockFetch([{ ...PENDING_WITH_OPTIONS, decision_id: id }], () => new Response('unused'));
      const c = mount();
      await flush();
      await flush();
      const shown = c.querySelector('[data-testid="decision-id"]') as HTMLElement;
      expect(shown.textContent).toBe('dec_6174…3d6f');
      expect(shown.title).toBe(id);
      (c.querySelector('button[aria-label="Copy the decision id"]') as HTMLButtonElement).click();
      await flush();
      expect(write).toHaveBeenCalledWith(id);
    });
  });

  describe('while the attempt is not over', () => {
    afterEach(() => vi.useRealTimers());

    it('a question asked after the page opened appears with no reload', async () => {
      vi.useFakeTimers();
      let rows: DecisionRecord[] = [];
      vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
        Promise.resolve(jsonOk({ protocol_version: 1, data: rows })),
      );
      const running = mount({ attemptState: 'running', pendingOnly: true });
      await vi.advanceTimersByTimeAsync(10);
      expect(running.textContent).not.toContain('Allow this agent');

      rows = [PENDING_WITH_OPTIONS];
      await vi.advanceTimersByTimeAsync(3100);
      expect(running.textContent).toContain('Allow this agent');
      expect(running.querySelector('button[type="submit"]')?.textContent).toBe('Resolve');
    });

    it('a finished attempt does not read again', async () => {
      vi.useFakeTimers();
      const spy = vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
        Promise.resolve(jsonOk({ protocol_version: 1, data: [RESOLVED] })),
      );
      mount({ attemptState: 'succeeded' });
      await vi.advanceTimersByTimeAsync(10);
      const first = spy.mock.calls.length;
      await vi.advanceTimersByTimeAsync(10000);
      expect(spy.mock.calls.length).toBe(first);
    });

    it('only the questions that wait are shown, and nothing when none does', async () => {
      vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
        Promise.resolve(jsonOk({ protocol_version: 1, data: [RESOLVED, PENDING_WITH_OPTIONS] })),
      );
      const some = mount({ pendingOnly: true });
      await flush();
      await flush();
      expect(some.querySelectorAll('li').length).toBe(1);
      expect(some.textContent).not.toContain('Answered');
      disposers.pop()!();
      vi.restoreAllMocks();
      vi.spyOn(globalThis, 'fetch').mockImplementation(() =>
        Promise.resolve(jsonOk({ protocol_version: 1, data: [RESOLVED] })),
      );
      const none = mount({ pendingOnly: true });
      await flush();
      await flush();
      expect(none.textContent).toBe('');
    });
  });

  describe('as a history', () => {
    it('shows only the questions already settled, without the token field or a form to answer', async () => {
      mockFetch([RESOLVED, EXPIRED, PENDING_WITH_OPTIONS], () => jsonOk({}));
      const c = mount({ settledOnly: true });
      await flush();
      await flush();
      expect(c.querySelectorAll('li').length).toBe(2);
      expect(c.textContent).toContain('Resolved');
      expect(c.textContent).toContain('Expired');
      expect(c.textContent).not.toContain('Pending');
      expect(c.textContent).not.toContain('Your decision token');
      expect(c.querySelector('form')).toBeNull();
    });

    it('says in one muted line that the agent asked nothing, and does not say it when a question waits', async () => {
      mockFetch([], () => jsonOk({}));
      const none = mount({ settledOnly: true });
      await flush();
      await flush();
      expect(none.querySelector('[data-testid="asked-nothing"]')!.textContent).toBe('The agent has asked nothing yet');
      expect(none.textContent).not.toContain('Your decision token');
      disposers.pop()!();
      vi.restoreAllMocks();
      mockFetch([PENDING_WITH_OPTIONS], () => jsonOk({}));
      const waiting = mount({ settledOnly: true });
      await flush();
      await flush();
      expect(waiting.querySelector('[data-testid="asked-nothing"]')).toBeNull();
      expect(waiting.textContent).toBe('');
    });
  });
});
