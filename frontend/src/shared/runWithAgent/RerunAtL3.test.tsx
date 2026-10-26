import { describe, expect, it, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import RerunAtL3, { needsAPerson, type RerunAtL3Props } from './RerunAtL3';
import { describeAttemptOutcome } from './attemptFormat';
import { buildCreateExecutionInput, type RunWithAgentFormValues } from './shared';
import type { AttemptSummary } from '../execution';

// The dialog is stubbed: what matters here is whether it is opened, and with which cap.
const modal = vi.hoisted(() => ({ last: undefined as undefined | { isOpen: boolean; autonomy?: string } }));
vi.mock('./RunWithAgentModal', () => ({
  default: (props: { isOpen: boolean; autonomy?: string }) => {
    modal.last = props;
    return null;
  },
}));

const SENTENCE = 'docket needs a person to approve the tool call';

const attempt = (over: Partial<AttemptSummary> = {}): AttemptSummary =>
  ({
    attempt_id: 'a1',
    request_id: 'r1',
    attempt_number: 1,
    state: 'failed',
    terminal_reason: { code: 'needs_human', message: SENTENCE },
    rerun_at_l3: { enabled: true, reason: null },
    ...over,
  }) as AttemptSummary;

function mount(props: Partial<RerunAtL3Props> = {}) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(() => <RerunAtL3 attempt={attempt()} itemId="i" itemTitle="Task" projectId="p" {...props} />, container);
  return container;
}

const button = (c: HTMLElement) =>
  [...c.querySelectorAll('button')].find((b) => b.textContent === 'Run again at L3') as HTMLButtonElement;

afterEach(() => {
  document.body.innerHTML = '';
  modal.last = undefined;
});

describe('Run again at L3', () => {
  it('is offered on an attempt that needed a person, and opens the dialog with the L3 cap', () => {
    const c = mount();
    expect(button(c).disabled).toBe(false);
    expect(c.querySelector('[data-testid="rerun-l3-reason"]')).toBeNull();
    expect(modal.last?.isOpen).toBe(false);
    button(c).click();
    expect(modal.last?.isOpen).toBe(true);
    expect(modal.last?.autonomy).toBe('L3');
  });

  it("is disabled with the board's reason when the run cannot be run again at L3, and starts nothing", () => {
    const reason = 'This run already ran at L3';
    const c = mount({ attempt: attempt({ rerun_at_l3: { enabled: false, reason } }) });
    expect(button(c).disabled).toBe(true);
    expect(button(c).title).toBe(reason);
    expect(c.querySelector('[data-testid="rerun-l3-reason"]')!.textContent).toBe(reason);
    button(c).click();
    expect(modal.last).toBeUndefined();
  });

  it('is disabled, with its reason, on a screen that does not know the task', () => {
    const c = mount({ itemTitle: '', projectId: '' });
    expect(button(c).disabled).toBe(true);
    expect(button(c).title).toBe('This screen does not know the task');
  });

  it('is only for a failed attempt whose code is needs_human', () => {
    expect(needsAPerson(attempt())).toBe(true);
    expect(needsAPerson(attempt({ terminal_reason: { code: 'token_limit' } }))).toBe(false);
    expect(needsAPerson(attempt({ state: 'succeeded' }))).toBe(false);
    expect(needsAPerson(undefined)).toBe(false);
  });

  it("is its own outcome: docket's sentence and a badge of its own", () => {
    const outcome = describeAttemptOutcome('failed', { code: 'needs_human', message: SENTENCE });
    expect(outcome.whyStopped).toBe(SENTENCE);
    expect(outcome.badge).toEqual({ label: 'Needs a person', tone: 'warning' });
    expect(describeAttemptOutcome('failed', { code: 'token_limit' }).badge).toBeNull();
  });

  it('sends the cap with the new run, and nothing when there is none', () => {
    const values = {
      itemId: 'i', selectorKind: 'fleet', selectorId: 'f', agentProfileId: 'p', harnessKind: 'docket', model: null,
      timeoutSeconds: 60, allowNetwork: false, tools: [], approvals: 'auto', idempotencyKey: 'k',
    } as RunWithAgentFormValues;
    expect(buildCreateExecutionInput({ ...values, autonomy: 'L3' }).autonomy).toBe('L3');
    expect('autonomy' in buildCreateExecutionInput(values)).toBe(false);
  });
});
