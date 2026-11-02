import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route, createMemoryHistory } from '@solidjs/router';
import ThreadPanel from './ThreadPanel';
import { composerState, entries } from './threadModel';
import { useToasts } from '../../shared/ui/toast';
import { flush } from '../../test/flush';

const disposers: Array<() => void> = [];
const settle = async () => {
  for (let i = 0; i < 8; i += 1) await flush();
};

const T = 'th1';
const thread = (over: Record<string, unknown> = {}) => ({
  id: T,
  title: 'Weekly check',
  profile: 'prof1',
  scope: { kind: 'project', project: 'p1' },
  status: 'open',
  turns: 1,
  created_at: '2026-11-01T10:00:00Z',
  last_message_at: '2026-11-01T10:05:00Z',
  ...over,
});
const msg = (id: string, principal: string, body: string, refs: unknown[] = []) => ({
  id,
  thread: T,
  principal,
  body,
  refs,
  created_at: '2026-11-01T10:00:00Z',
});
const turn = (message: string, state: string, over: Record<string, unknown> = {}) => ({
  thread: T,
  position: 1,
  message,
  request_id: 'req1',
  state,
  started_at: '2026-11-01T10:00:00Z',
  ...over,
});
const changeSet = {
  id: 'cs1',
  project: 'p1',
  created_at: '2026-11-01T10:04:00Z',
  origin: 'thread',
  origin_ref: T,
  principal: `thread:${T}`,
  principal_name: null,
  reason: 'Two items for the payments epic',
  status: 'waiting',
  ops: [{ operation: { op: 'priority', item: 'i1', priority: 'high' }, state: 'waiting' }],
};

interface Detail {
  thread: ReturnType<typeof thread>;
  messages: ReturnType<typeof msg>[];
  turns: ReturnType<typeof turn>[];
}

function mount(detail: Detail, post?: (body: string) => Response, asking = false) {
  const calls: Array<{ url: string; method: string; body?: string }> = [];
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? 'GET';
    calls.push({ url, method, body: init?.body as string | undefined });
    const json = (body: unknown, status = 200) =>
      new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
    if (url.endsWith(`/threads/${T}/messages`)) return post ? post(String(init?.body)) : json({ message: msg('m9', 'operator:local', 'x') });
    if (url.endsWith(`/threads/${T}/close`)) return json({ ...detail.thread, status: 'closed' });
    if (url.endsWith(`/threads/${T}`)) return json(detail);
    if (url.includes('/change-sets/cs1')) return json(changeSet);
    if (url.includes('/executions/req1/attempts/1/decisions'))
      return json({
        protocol_version: 1,
        data: [
          {
            decision_id: 'dec1', attempt_id: 'att1', kind: 'question', prompt: 'Which sprint should I plan?', options: [{ option_id: 'a', label: 'This one' }],
            metadata: {}, expires_at: '2099-01-01T00:00:00Z', state: 'pending', answer: null, resolved_at: null, resolved_by: null,
            created_at: '2026-11-01T10:00:00Z', updated_at: '2026-11-01T10:00:00Z',
          },
        ],
      });
    if (url.includes('/executions/req1/attempts'))
      return json({ protocol_version: 1, data: asking ? [{ attempt_id: 'att1', request_id: 'req1', attempt_number: 1, state: 'running' }] : [] });
    if (url.includes('/executions/req1')) return json({ request_id: 'req1', item_id: 'i0', state: asking ? 'waiting_decision' : 'running', created_at: '2026-11-01T10:00:00Z', cancellation_requested_at: null });
    if (url.includes('/items/')) return json({ item: { id: 'i1', title: 'Payments epic', project_id: 'p1' }, roles: [], dependencies: [] });
    return json({});
  }) as typeof fetch);
  const history = createMemoryHistory();
  history.set({ value: '/t' });
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <MemoryRouter history={history}>
        <Route path="/t" component={() => <ThreadPanel threadId={T} projectId="p1" profileName="Planner" />} />
      </MemoryRouter>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return { c: container, calls };
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  vi.restoreAllMocks();
});

const q = (c: HTMLElement, id: string) => c.querySelector<HTMLElement>(`[data-testid="${id}"]`);
const all = (c: HTMLElement, id: string) => [...c.querySelectorAll<HTMLElement>(`[data-testid="${id}"]`)];

describe('ThreadPanel', () => {
  it('says what to do in an empty thread and lets the person write', async () => {
    const { c } = mount({ thread: thread({ turns: 0 }), messages: [], turns: [] });
    await settle();
    expect(q(c, 'thread-empty')?.textContent).toContain('Nothing has been said yet');
    expect((q(c, 'composer') as HTMLTextAreaElement).disabled).toBe(false);
    expect(q(c, 'composer-reason')).toBeNull();
  });

  it('lists the messages in order with who wrote each', async () => {
    const { c } = mount({
      thread: thread(),
      messages: [msg('m1', 'operator:local', 'What is blocked?'), msg('m2', `thread:${T}`, 'Two items are blocked.')],
      turns: [turn('m1', 'replied', { settled_at: '2026-11-01T10:01:00Z' })],
    });
    await settle();
    const items = all(c, 'thread-message');
    expect(items.map((i) => i.textContent)).toEqual([
      expect.stringContaining('You'),
      expect.stringContaining('Planner'),
    ]);
    expect(items[0].textContent).toContain('What is blocked?');
    expect(items[1].textContent).toContain('Two items are blocked.');
    expect(q(c, 'turn-state')?.getAttribute('data-state')).toBe('replied');
    expect((q(c, 'composer') as HTMLTextAreaElement).disabled).toBe(false);
  });

  it('shows a turn in progress as work in progress and turns the composer off with its reason', async () => {
    const { c } = mount({
      thread: thread(),
      messages: [msg('m1', 'operator:local', 'Plan the next sprint')],
      turns: [turn('m1', 'pending')],
    });
    await settle();
    const state = q(c, 'turn-state')!;
    expect(state.getAttribute('data-state')).toBe('pending');
    expect(state.textContent).toContain('The agent is working on your message');
    expect((q(c, 'composer') as HTMLTextAreaElement).disabled).toBe(true);
    expect((q(c, 'send') as HTMLButtonElement).disabled).toBe(true);
    expect(q(c, 'composer-reason')?.textContent).toContain('still working on your last message');
  });

  it('shows the question a pending turn asks, to be answered in the thread', async () => {
    const { c } = mount(
      { thread: thread(), messages: [msg('m1', 'operator:local', 'Plan')], turns: [turn('m1', 'pending')] },
      undefined,
      true,
    );
    await settle();
    const state = q(c, 'turn-state')!;
    expect(state.textContent).toContain('asking you something');
    expect(state.textContent).toContain('Which sprint should I plan?');
  });

  it('says a failed turn failed and shows its stored reason as it came', async () => {
    const { c } = mount({
      thread: thread(),
      messages: [
        msg('m1', 'operator:local', 'Hello'),
        msg('m2', `thread:${T}`, 'This turn failed: the agent finished without leaving a reply.'),
      ],
      turns: [turn('m1', 'failed')],
    });
    await settle();
    expect(q(c, 'turn-state')?.getAttribute('data-state')).toBe('failed');
    expect(q(c, 'turn-state')?.textContent).toContain('could not answer');
    const reply = all(c, 'thread-message')[1];
    expect(reply.getAttribute('data-failed')).toBe('true');
    expect(reply.textContent).toContain('the agent finished without leaving a reply');
    expect((q(c, 'composer') as HTMLTextAreaElement).disabled).toBe(false);
  });

  it('shows the change sets of a reply inline in the change-set panel', async () => {
    const { c } = mount({
      thread: thread(),
      messages: [msg('m1', 'operator:local', 'Raise the priority'), msg('m2', `thread:${T}`, 'Proposed.', [{ kind: 'change_set', id: 'cs1' }])],
      turns: [turn('m1', 'replied')],
    });
    await settle();
    const inline = q(c, 'message-change-set')!;
    expect(inline.textContent).toContain('Two items for the payments epic');
    expect(inline.textContent).toContain('Apply');
  });

  it('turns the composer off with the thread own reason once it is closed', async () => {
    const { c } = mount({
      thread: thread({ status: 'closed', closed_reason: 'the thread had no message for 7 days' }),
      messages: [msg('m1', 'operator:local', 'Hi')],
      turns: [turn('m1', 'replied')],
    });
    await settle();
    expect((q(c, 'composer') as HTMLTextAreaElement).disabled).toBe(true);
    expect(q(c, 'composer-reason')?.textContent).toContain('This thread is closed. The thread had no message for 7 days.');
    expect((q(c, 'thread-close') as HTMLButtonElement).disabled).toBe(true);
  });

  it('sends a message and reads the thread again', async () => {
    const { c, calls } = mount({ thread: thread({ turns: 0 }), messages: [], turns: [] });
    await settle();
    const box = q(c, 'composer') as HTMLTextAreaElement;
    box.value = 'What waits on me?';
    box.dispatchEvent(new InputEvent('input', { bubbles: true }));
    await settle();
    (q(c, 'send') as HTMLButtonElement).click();
    await settle();
    const post = calls.find((x) => x.method === 'POST' && x.url.endsWith('/messages'))!;
    expect(JSON.parse(post.body!)).toEqual({ body: 'What waits on me?', refs: [] });
    expect(calls.filter((x) => x.method === 'GET' && x.url.endsWith(`/threads/${T}`)).length).toBeGreaterThan(1);
  });

  it('says in plain words that a message was not sent when the server answers 409', async () => {
    const { c } = mount(
      { thread: thread({ turns: 0 }), messages: [], turns: [] },
      () =>
        new Response(JSON.stringify({ error: { status: 409, message: 'the previous turn has not answered yet; write again when it has', code: 'conflict' } }), {
          status: 409,
          headers: { 'Content-Type': 'application/json' },
        }),
    );
    await settle();
    const box = q(c, 'composer') as HTMLTextAreaElement;
    box.value = 'Again';
    box.dispatchEvent(new InputEvent('input', { bubbles: true }));
    await settle();
    (q(c, 'send') as HTMLButtonElement).click();
    await settle();
    const problem = q(c, 'composer-problem')!;
    expect(problem.textContent).toContain('The message was not sent.');
    expect(problem.textContent).toContain('The previous turn has not answered yet; write again when it has.');
    expect(box.value).toBe('Again');
  });

  it('closes the thread', async () => {
    const { c, calls } = mount({ thread: thread(), messages: [msg('m1', 'operator:local', 'Hi')], turns: [turn('m1', 'replied')] });
    await settle();
    (q(c, 'thread-close') as HTMLButtonElement).click();
    await settle();
    expect(calls.some((x) => x.method === 'POST' && x.url.endsWith(`/threads/${T}/close`))).toBe(true);
    expect(q(c, 'thread-status')?.textContent).toBe('Closed');
    expect((q(c, 'composer') as HTMLTextAreaElement).disabled).toBe(true);
  });

  it('closes a thread whose reply carries a change set and keeps showing both', async () => {
    const { c } = mount({
      thread: thread(),
      messages: [msg('m1', 'operator:local', 'Raise the priority'), msg('m2', `thread:${T}`, 'Proposed.', [{ kind: 'change_set', id: 'cs1' }])],
      turns: [turn('m1', 'replied')],
    });
    await settle();
    expect(q(c, 'change-set-panel')).not.toBeNull();
    (q(c, 'thread-close') as HTMLButtonElement).click();
    await settle();
    // A failure while the page redraws is told as a toast; none may appear.
    expect(useToasts()().map((t) => t.message)).toEqual([]);
    expect(q(c, 'thread-status')?.textContent).toBe('Closed');
    expect(q(c, 'change-set-panel')).not.toBeNull();
    const close = q(c, 'thread-close') as HTMLButtonElement;
    expect(close.disabled).toBe(true);
    expect(c.textContent).toContain('This thread is already closed.');
  });
});

describe('threadModel', () => {
  it('pairs a message with its turn and marks the reply after a failed turn', () => {
    const list = entries(
      [msg('m1', 'operator:local', 'a'), msg('m2', `thread:${T}`, 'This turn failed: x.')],
      [turn('m1', 'failed')],
    );
    expect(list[0].turn?.state).toBe('failed');
    expect(list[1].failedTurn?.message).toBe('m1');
  });

  it('allows writing in an open thread with no turn pending', () => {
    expect(composerState(thread() as never, [turn('m1', 'replied')] as never)).toEqual({ disabled: false, reason: null });
  });
});
