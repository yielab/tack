import { describe, it, expect, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import { MemoryRouter, Route } from '@solidjs/router';
import { ExecutionStoreProvider } from '../state/executionContext';
import LatestRunSummary, { primaryNowAction, type LatestRunSummaryProps } from './LatestRunSummary';
import { flush } from '../../test/flush';
const disposers: Array<() => void> = [];

const ATTEMPT = {
  attempt_id: 'att-1', request_id: 'req-1', attempt_number: 1, runner_id: 'runner-1', fencing_token: 1,
  state: 'succeeded', lease_issued_at: '2026-10-08T12:00:00Z', lease_expires_at: '2026-10-08T12:05:00Z',
  last_heartbeat_at: null, event_checkpoint: null, completion_id: null, workspace_id: null, base_revision: null,
  actual_execution: { harness_kind: 'claude-code', model: 'claude-sonnet-5-5', model_source: 'reported' },
  terminal_reason: { result: 'I created test.md with the main idea of the README.', artifacts: [{ kind: 'patch', size_bytes: 120 }] },
  usage: {
    tokens_in: { value: 1_077_873, source: 'measured' }, tokens_out: { value: 3_284, source: 'measured' },
    cache_read_tokens: 791_447, cache_write_tokens: 286_414, model_calls: 6, models: ['claude-sonnet-5-5'],
  },
  counted_tokens: 289_710,
  started_at: '2026-10-08T12:00:05Z', ended_at: '2026-10-08T12:02:00Z',
  created_at: '2026-10-08T12:00:00Z', updated_at: '2026-10-08T12:02:00Z', model_provenance: null,
  usage_economics: {
    model_token_cost_usd_estimated: { value: 1.3368, source: 'measured' },
    runner_time_cost: { wall_clock_ms: 115_000, cost_usd_estimated: { value: null, source: 'not_measured' } },
  },
  pull_request: null, review: null,
};

function mount(executions: unknown[], onOpen = () => {}, attempts: unknown[] = [ATTEMPT], events: unknown[] = [], reviewInTab = false, decisions: unknown[] = [], extra: Partial<LatestRunSummaryProps> = {}) {
  vi.spyOn(globalThis, 'fetch').mockImplementation((async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.endsWith('/decisions')) return new Response(JSON.stringify({ protocol_version: 1, data: decisions }), { status: 200 });
    if (url.includes('/artifacts')) return new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 });
    if (url.includes('/events')) return new Response(JSON.stringify({ protocol_version: 1, data: events }), { status: 200 });
    if (url.includes('/attempts')) return new Response(JSON.stringify({ protocol_version: 1, data: attempts }), { status: 200 });
    if (url.includes('/executions')) return new Response(JSON.stringify({ protocol_version: 1, data: executions }), { status: 200 });
    return new Response('{}', { status: 200 });
  }) as typeof fetch);
  const container = document.createElement('div');
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <ExecutionStoreProvider>
        <MemoryRouter>
          <Route
            path="/"
            component={() => (
              <LatestRunSummary itemId="item-1" itemTitle="Fix it" projectId="proj-1" onOpen={onOpen} reviewInTab={reviewInTab} {...extra} />
            )}
          />
        </MemoryRouter>
      </ExecutionStoreProvider>
    ),
    container,
  );
  disposers.push(() => {
    dispose();
    container.remove();
  });
  return container;
}

afterEach(() => {
  while (disposers.length) disposers.pop()!();
  vi.restoreAllMocks();
});

const RUN = { request_id: 'req-1', item_id: 'item-1', state: 'succeeded', cancellation_requested_at: null, created_at: '2026-10-08T12:00:00Z' };

describe('LatestRunSummary', () => {
  it('renders nothing for an item that has never run', async () => {
    const c = mount([]);
    await flush();
    await flush();
    expect(c.querySelector('[data-testid="latest-run"]')).toBeNull();
  });

  it('coming back to a task shows its last run: status, what the agent said, what it used, and a way to the report', async () => {
    const onOpen = vi.fn();
    const c = mount([RUN], onOpen);
    for (let i = 0; i < 4; i++) await flush();
    const panel = c.querySelector('[data-testid="latest-run"]')!;
    expect(panel.textContent).toContain('Finished — needs your review');
    expect(panel.querySelector('[data-testid="latest-run-said"]')!.textContent).toContain('I created test.md');
    expect(panel.querySelector('[data-testid="latest-run-usage"]')!.textContent).toBe(
      '289 710 tokens (+ 791 447 read from cache, not counted) · 6 model calls · claude-sonnet-5-5',
    );
    const open = [...panel.querySelectorAll('button')].find((b) => b.textContent === 'Review and merge')!;
    open.click();
    expect(onOpen).toHaveBeenCalledOnce();
  });

  const buttons = (c: Element) => [...c.querySelectorAll('button')].map((b) => b.textContent);

  it('a run that needs a verdict is a Ready for review card with the opening of its report, its duration, Accept and Reject…', async () => {
    const c = mount([RUN]);
    for (let i = 0; i < 4; i++) await flush();
    const card = c.querySelector('[data-testid="latest-run"]')!;
    expect(card.getAttribute('data-state')).toBe('review');
    expect(card.querySelector('[data-testid="latest-run-meta"]')!.textContent).toBe('1m 55s');
    expect(buttons(card)).toEqual(expect.arrayContaining(['Review and merge', 'Accept', 'Reject…']));
  });

  it('while the Execution tab is open the card keeps its state and its links but leaves out Accept and Reject…', async () => {
    const c = mount([RUN], () => {}, [ATTEMPT], [], true);
    for (let i = 0; i < 4; i++) await flush();
    const card = c.querySelector('[data-testid="latest-run"]')!;
    expect(card.getAttribute('data-state')).toBe('review');
    expect(card.querySelector('[data-testid="latest-run-meta"]')!.textContent).toBe('1m 55s');
    expect(buttons(card)).toContain('Review and merge');
    expect(buttons(card)).not.toContain('Accept');
    expect(buttons(card)).not.toContain('Reject…');
  });

  it('a run that changed no file shows the answer as what it delivered and offers Accept, not Merge', async () => {
    const answer = { ...ATTEMPT, terminal_reason: { result: 'The answer is 42.' }, merge: { enabled: false, reason: 'No branch was pushed' } };
    const c = mount([RUN], () => {}, [answer]);
    for (let i = 0; i < 4; i++) await flush();
    const card = c.querySelector('[data-testid="latest-run"]')!;
    expect(card.textContent).toContain('What it delivered');
    expect(card.querySelector('[data-testid="latest-run-said"]')!.textContent).toBe('“The answer is 42.”');
    expect(card.querySelector('[data-testid="latest-run-meta"]')!.textContent).toContain('no files changed');
    expect(buttons(card)).toContain('Accept');
    expect(buttons(card)).not.toContain('Merge');
  });

  it('merge, when it cannot run, is disabled and says why in plain words', async () => {
    const pushedNothing = { ...ATTEMPT, merge: { enabled: false, reason: 'No branch was pushed' } };
    const c = mount([RUN], () => {}, [pushedNothing]);
    for (let i = 0; i < 4; i++) await flush();
    const merge = [...c.querySelectorAll('button')].find((b) => b.textContent === 'Merge')!;
    expect(merge.disabled).toBe(true);
    expect(c.querySelector('[data-testid="merge-reason"]')!.textContent).toBe('No branch was pushed');
  });

  it('a running card shows the last steps, how far into its limit it is, Follow live and Stop', async () => {
    const onOpen = vi.fn();
    const events = ['one', 'two', 'three', 'four'].map((text, i) => ({ event_id: `e${i}`, sequence: i, source: 'runner', kind: 'agent.tool_call', payload: { text }, occurred_at: '2026-10-08T12:00:00Z', created_at: '2026-10-08T12:00:00Z' }));
    const limit = { tokens: 300_000, source: 'run', enforced_by: 'claude-code', sentence: '300 000 tokens', used: 120_000 };
    const c = mount([{ ...RUN, state: 'running', limit }], onOpen, [{ ...ATTEMPT, state: 'running', terminal_reason: null, ended_at: null }], events);
    for (let i = 0; i < 6; i++) await flush();
    const card = c.querySelector('[data-testid="latest-run"]')!;
    expect(card.getAttribute('data-state')).toBe('running');
    expect([...card.querySelectorAll('[data-testid="latest-run-steps"] li')].map((li) => li.textContent)).toEqual(['two', 'three', 'four']);
    // The count is a bar; its sentence is the bar's accessible name.
    const meter = card.querySelector('[data-testid="run-limit-used"]')!;
    expect(meter.getAttribute('role')).toBe('meter');
    expect(meter.getAttribute('aria-label')).toBe('Used 120 000 of 300 000 tokens');
    expect(meter.getAttribute('aria-valuenow')).toBe('120000');
    expect(meter.getAttribute('aria-valuemax')).toBe('300000');
    expect(buttons(card)).toEqual(expect.arrayContaining(['Follow live', 'Stop']));
    [...card.querySelectorAll('button')].find((b) => b.textContent === 'Follow live')!.click();
    expect(onOpen).toHaveBeenCalledOnce();
  });

  it('a run that needs a person is one state, in one colour, and the card opens its question', async () => {
    const onOpen = vi.fn();
    const asking = mount([{ ...RUN, state: 'running' }], onOpen, [{ ...ATTEMPT, state: 'waiting_decision', terminal_reason: null, ended_at: null }]);
    for (let i = 0; i < 6; i++) await flush();
    const card = asking.querySelector('[data-testid="latest-run"]')!;
    const status = card.querySelector('[role="status"]')!;
    expect(status.textContent).toBe('Needs youNeeds a person');
    expect(status.getAttribute('aria-live')).toBe('polite');
    expect(buttons(card)).not.toContain('Follow live');
    [...card.querySelectorAll('button')].find((b) => b.textContent === 'Answer the question')!.click();
    expect(onOpen).toHaveBeenCalledOnce();
  });

  const QUESTION = {
    decision_id: 'dec-1', attempt_id: 'att-1', kind: 'choice', prompt: 'Which chart library should it add?',
    options: [{ option_id: 'uplot', label: 'uPlot' }, { option_id: 'svg', label: 'Hand-written SVG' }], metadata: {},
    expires_at: '2026-10-08T13:00:00Z', state: 'pending', answer: null, resolved_at: null, resolved_by: null,
    created_at: '2026-10-08T12:00:00Z', updated_at: '2026-10-08T12:00:00Z',
  };
  const waitingAttempt = { ...ATTEMPT, state: 'waiting_decision', terminal_reason: null, ended_at: null };

  it('a question waiting for the person is the first thing in the card, with its answer controls open', async () => {
    const c = mount([{ ...RUN, state: 'running' }], () => {}, [waitingAttempt], [], false, [QUESTION]);
    for (let i = 0; i < 6; i++) await flush();
    const card = c.querySelector('[data-testid="latest-run"]')!;
    expect(card.textContent).toContain('Which chart library should it add?');
    expect(card.querySelector('button[type="submit"]')?.textContent).toBe('Resolve');
    expect(card.querySelectorAll('input[type="radio"]').length).toBe(2);
    // Before the card's own lines, and not behind a toggle.
    const question = card.querySelector('form')!;
    const working = card.querySelector('[role="status"]')!;
    expect(working.compareDocumentPosition(question) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect([...card.querySelectorAll('button')].some((b) => b.textContent === 'Show questions & files')).toBe(false);
  });

  it('a question that was answered, or a run that does not wait, shows no question in the card', async () => {
    const answered = mount([{ ...RUN, state: 'running' }], () => {}, [waitingAttempt], [], false, [{ ...QUESTION, state: 'resolved', answer: { option_id: 'svg', text: null } }]);
    for (let i = 0; i < 6; i++) await flush();
    expect(answered.querySelector('[data-testid="latest-run"] form')).toBeNull();
    const plain = mount([RUN], () => {}, [ATTEMPT], [], false, [QUESTION]);
    for (let i = 0; i < 6; i++) await flush();
    expect(plain.textContent).not.toContain('Which chart library');
  });

  it('on the Execution tab the card leaves the question to the attempt', async () => {
    const c = mount([{ ...RUN, state: 'running' }], () => {}, [waitingAttempt], [], true, [QUESTION]);
    for (let i = 0; i < 6; i++) await flush();
    expect(c.querySelector('[data-testid="latest-run"] form')).toBeNull();
  });

  it('a queued card offers to run on another computer', async () => {
    const waiting = { ...RUN, state: 'queued', waiting_reason: 'The computer this run asked for has not answered since 2026-10-19T10:00:00Z', waiting_since: null };
    const c = mount([waiting], () => {}, []);
    for (let i = 0; i < 4; i++) await flush();
    const card = c.querySelector('[data-testid="latest-run"]')!;
    expect(card.getAttribute('data-state')).toBe('queued');
    expect(card.textContent).toContain('The computer this run asked for has not answered since 2026-10-19T10:00:00Z');
    expect(buttons(card)).toContain('Run on another computer');
  });

  it('a queued run behind a full computer says what the computer is busy with, links the task and the limit, and offers to answer the question it waits on', async () => {
    const behind = {
      runner_id: 'runner-1', runner_name: 'Main', runs_at_a_time: 1, request_id: 'req-busy', attempt_number: 1,
      item_id: 'item-busy', project_id: 'proj-1', item_title: 'Reference notes', started_at: '2026-10-31T15:30:00Z',
      question: 'Allow the focused test suite?', question_since: '2026-10-31T15:33:00Z',
    };
    const sentence = "This computer runs 1 at a time and is busy with 'Reference notes' since 15:30, which waits for your answer since 15:33";
    const waiting = { ...RUN, state: 'queued', waiting_reason: sentence, waiting_since: null, waiting_behind: [behind] };
    let c = mount([waiting], () => {}, []);
    for (let i = 0; i < 4; i++) await flush();
    const line = c.querySelector('[data-testid="latest-run-waiting"]')!;
    expect(line.textContent).toBe(`Waiting: ${sentence}`);
    const links = [...line.querySelectorAll('a')].map((a) => [a.textContent, a.getAttribute('href')]);
    expect(links).toEqual([
      ['runs 1 at a time', '/agents'],
      ["'Reference notes'", '/projects/proj-1/items/item-busy'],
    ]);
    const card = c.querySelector('[data-testid="latest-run"]')!;
    expect(buttons(card)).toEqual(expect.arrayContaining(['Run on another computer', 'Answer it']));

    // A busy run that waits for no one: the sentence and its links, and no Answer it.
    while (disposers.length) disposers.pop()!();
    vi.restoreAllMocks();
    const running = { ...behind, question: null, question_since: null };
    c = mount([{ ...waiting, waiting_reason: "This computer runs 1 at a time and is busy with 'Reference notes' since 15:30", waiting_behind: [running] }], () => {}, []);
    for (let i = 0; i < 4; i++) await flush();
    expect(buttons(c.querySelector('[data-testid="latest-run"]')!)).not.toContain('Answer it');
    expect(c.querySelectorAll('[data-testid="latest-run-waiting"] a')).toHaveLength(2);
  });

  it('a queued run says why it waits and for how long; a capacity wait has no duration; a request that failed for want of a runner shows the reason as its failure', async () => {
    const waiting = { ...RUN, state: 'queued', waiting_reason: 'No runner has the model it asks for.', waiting_since: new Date(Date.now() - 5 * 60_000).toISOString() };
    let c = mount([waiting], () => {}, []);
    for (let i = 0; i < 4; i++) await flush();
    expect(c.querySelector('[data-testid="latest-run-waiting"]')!.textContent).toContain('Waiting: No runner has the model it asks for.');
    expect(c.querySelector('[data-testid="latest-run-waiting"]')!.textContent).toContain('5m ago');

    while (disposers.length) disposers.pop()!();
    vi.restoreAllMocks();
    c = mount([{ ...waiting, waiting_reason: 'Every runner is busy.', waiting_since: null }], () => {}, []);
    for (let i = 0; i < 4; i++) await flush();
    expect(c.querySelector('[data-testid="latest-run-waiting"]')!.textContent).toBe('Waiting: Every runner is busy.');

    while (disposers.length) disposers.pop()!();
    vi.restoreAllMocks();
    c = mount([{ ...waiting, state: 'failed' }], () => {}, []);
    for (let i = 0; i < 4; i++) await flush();
    expect(c.querySelector('[data-testid="latest-run-waiting"]')).toBeNull();
    expect(c.querySelector('[data-testid="latest-run-failure"]')!.textContent).toBe('No runner has the model it asks for.');
  });

  it.each([
    ['failed', { code: 'timeout', message: "The run's time ran out after 30 minutes while it waited for an answer", result: 'half done' }, "The run's time ran out after 30 minutes while it waited for an answer"],
    ['failed', { code: 'timeout' }, "The run's time ran out"],
    ['cancelled', { code: 'user_cancelled' }, 'user_cancelled'],
    ['lost', null, 'No reason was reported'],
  ])('a %s run prints why it stopped where the agent\'s words go, never the words themselves', async (state, terminal_reason, why) => {
    const c = mount([{ ...RUN, state }], () => {}, [{ ...ATTEMPT, state, terminal_reason }]);
    for (let i = 0; i < 4; i++) await flush();
    expect(c.querySelector('[data-testid="latest-run-why"]')!.textContent).toBe(why);
    expect(c.querySelector('[data-testid="latest-run-said"]')).toBeNull();
  });

  it.each([
    '300 000 tokens · from the profile Implementer · docket stops the task there',
    "300 000 tokens · set for this run · Tack stops Claude Code's run there",
    "300 000 tokens · set for this run · measured, not enforced (Tack reads Codex's tokens but cannot stop it at a limit yet)",
    "No limit set in Tack · docket stops each step at its own limit (docket's setting)",
    'No limit set in Tack',
  ])('prints the limit sentence the server wrote, composing none: %s', async (sentence) => {
    const limit = { tokens: null, source: 'none', enforced_by: 'measured', sentence };
    const c = mount([{ ...RUN, limit }]);
    for (let i = 0; i < 4; i++) await flush();
    expect(c.querySelector('[data-testid="latest-run-limit"]')!.textContent).toBe(sentence);
  });
});

describe('LatestRunSummary: questions the agent asked and a folder team', () => {
  const question = (questions: string[]) => ({
    role: 'lead', model: null, class: null, family: null, verdict: null, ok: true, summary: null,
    prompts: [], assumptions: [], questions,
  });

  it('a run whose steps asked questions says how many, and the line opens the tab', async () => {
    const onOpen = vi.fn();
    const steps = [question(['Which tone?']), question(['Which date?', 'Which city?'])];
    const c = mount([RUN], onOpen, [{ ...ATTEMPT, terminal_reason: { ...ATTEMPT.terminal_reason, steps } }]);
    for (let i = 0; i < 4; i++) await flush();
    const line = c.querySelector('[data-testid="latest-run-questions"]')!;
    expect(line.textContent).toBe('The agent asked 3 questions');
    line.querySelector('button')!.click();
    expect(onOpen).toHaveBeenCalledOnce();
  });

  it('a run whose steps asked nothing shows no questions line', async () => {
    const c = mount([RUN], () => {}, [{ ...ATTEMPT, terminal_reason: { ...ATTEMPT.terminal_reason, steps: [question([])] } }]);
    for (let i = 0; i < 4; i++) await flush();
    expect(c.querySelector('[data-testid="latest-run"]')).not.toBeNull();
    expect(c.querySelector('[data-testid="latest-run-questions"]')).toBeNull();
  });

  it('a folder team run is done: nothing to merge, what its last step did, and no Accept, Reject or Merge', async () => {
    const folder = {
      ...ATTEMPT,
      terminal_reason: { code: 'done', outside_repository: true, team: 'brief', task: 'k', summary: 'Wrote the brief to the folder.' },
      merge: { enabled: false, reason: 'No branch was pushed' },
    };
    const c = mount([RUN], () => {}, [folder]);
    for (let i = 0; i < 4; i++) await flush();
    const card = c.querySelector('[data-testid="latest-run"]')!;
    expect(card.getAttribute('data-state')).toBe('done');
    expect(card.querySelector('[data-testid="latest-run-folder"]')!.textContent).toBe('This team works in a folder: nothing to merge');
    expect(card.textContent).toContain('What it did');
    expect(card.querySelector('[data-testid="latest-run-said"]')!.textContent).toContain('Wrote the brief to the folder.');
    const labels = [...card.querySelectorAll('button')].map((b) => b.textContent);
    expect(labels).not.toContain('Accept');
    expect(labels).not.toContain('Reject…');
    expect(labels).not.toContain('Merge');
  });
});

describe('LatestRunSummary: the Now panel in each state', () => {
  const buttonsOf = (root: Element) => [...root.querySelectorAll<HTMLButtonElement>('button')];
  /** The filled buttons: the panel has at most one. */
  const filled = (root: Element) =>
    buttonsOf(root).filter((b) => (b.getAttribute('style') ?? '').includes('--color-primary-600')).map((b) => b.textContent);
  const banner = (c: Element) => c.querySelector('[data-testid="now-panel"] [role="status"]')!.textContent;
  const panel = (c: Element) => c.querySelector('[data-testid="now-panel"]')!;
  const settle = async () => {
    for (let i = 0; i < 6; i++) await flush();
  };

  it('an item with no run shows Run with agent as its one filled button, and who would do it', async () => {
    const c = mount([], () => {}, [], [], false, [], { idle: <p data-testid="now-who">Would go to Code agent · team saas-code</p> });
    await settle();
    expect(c.querySelector('[data-testid="latest-run"]')).toBeNull();
    expect(c.querySelector('[data-testid="now-idle"]')).not.toBeNull();
    expect(filled(panel(c))).toEqual(['Run with agent']);
    expect(c.querySelector('[data-testid="now-who"]')!.textContent).toBe('Would go to Code agent · team saas-code');
  });

  it('a queued run says Queued and has no filled button, only another computer and no Run again', async () => {
    const c = mount([{ ...RUN, state: 'queued' }], () => {}, []);
    await settle();
    expect(banner(c)).toBe('Queued');
    expect(filled(panel(c))).toEqual([]);
    expect(buttonsOf(panel(c)).map((b) => b.textContent)).toEqual(['Run on another computer']);
  });

  it('a running run says Running; Follow live and Stop are secondary and nothing is filled', async () => {
    const c = mount([{ ...RUN, state: 'running' }], () => {}, [{ ...ATTEMPT, state: 'running', terminal_reason: null, ended_at: null }]);
    await settle();
    expect(banner(c)).toBe('Running');
    expect(filled(panel(c))).toEqual([]);
    expect(buttonsOf(panel(c)).map((b) => b.textContent)).toEqual(['Follow live', 'Stop']);
  });

  it('a run that needs you says Needs you with the docket words, and Answer the question is the filled button', async () => {
    const c = mount([{ ...RUN, state: 'running' }], () => {}, [{ ...ATTEMPT, state: 'waiting_decision', terminal_reason: null, ended_at: null }]);
    await settle();
    expect(banner(c)).toBe('Needs youNeeds a person');
    expect(filled(panel(c))).toEqual(['Answer the question']);
  });

  it('a run ready for review says so, and Review and merge is the only filled button; Accept is secondary', async () => {
    const c = mount([RUN]);
    await settle();
    expect(banner(c)).toBe('Ready for reviewNeeds your review');
    expect(filled(panel(c))).toEqual(['Review and merge']);
    expect(buttonsOf(panel(c)).map((b) => b.textContent)).toEqual(expect.arrayContaining(['Accept', 'Reject…']));
  });

  it('a failed run says Failed with why, and See why it failed is the filled button', async () => {
    const c = mount([{ ...RUN, state: 'failed' }], () => {}, [{ ...ATTEMPT, state: 'failed', terminal_reason: { code: 'timeout' } }]);
    await settle();
    expect(banner(c)).toBe('Failed');
    expect(c.querySelector('[data-testid="latest-run-why"]')!.textContent).toBe("The run's time ran out");
    expect(filled(panel(c))).toEqual(['See why it failed']);
    expect(buttonsOf(panel(c)).map((b) => b.textContent)).toContain('Run again');
  });

  it('a merged run says Done, and Run again is the filled button, once', async () => {
    const merged = { ...ATTEMPT, merge: { enabled: false, reason: null, merged: { merged_at: '2026-10-08T12:30:00Z' } } };
    const c = mount([RUN], () => {}, [merged]);
    await settle();
    expect(banner(c)).toBe('Done');
    expect(c.querySelector('[data-testid="latest-run-merged"]')).not.toBeNull();
    expect(filled(panel(c))).toEqual(['Run again']);
    expect(buttonsOf(panel(c)).filter((b) => b.textContent === 'Run again')).toHaveLength(1);
  });

  it('the panel tells its screen the run state, and undefined while there is no run', async () => {
    const onState = vi.fn();
    mount([RUN], () => {}, [ATTEMPT], [], false, [], { onState });
    await settle();
    expect(onState).toHaveBeenLastCalledWith({ state: 'review', docket: 'Needs your review' });
    const none = vi.fn();
    mount([], () => {}, [], [], false, [], { onState: none });
    await settle();
    expect(none).toHaveBeenLastCalledWith(undefined);
  });

  it('the footer slot is the last thing in the panel', async () => {
    const c = mount([RUN], () => {}, [ATTEMPT], [], false, [], { footer: <p data-testid="foot">By hand</p> });
    await settle();
    expect(panel(c).lastElementChild!.getAttribute('data-testid')).toBe('foot');
  });
});

describe('primaryNowAction', () => {
  const base = { hasRun: true, asking: false, askingBehind: false };
  it.each([
    [{ hasRun: false, kind: 'done' }, 'run'],
    [{ kind: 'queued' }, null],
    [{ kind: 'queued', askingBehind: true }, 'answer'],
    [{ kind: 'running' }, null],
    [{ kind: 'running', asking: true }, 'answer'],
    [{ kind: 'review' }, 'review'],
    [{ kind: 'failed' }, 'why'],
    [{ kind: 'done' }, 'again'],
    [{ kind: 'merged' }, 'again'],
  ] as const)('%j chooses %s', (input, expected) => {
    expect(primaryNowAction({ ...base, ...input } as Parameters<typeof primaryNowAction>[0])).toBe(expected);
  });
});
