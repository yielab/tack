import { describe, expect, it, vi, afterEach } from 'vitest';
import { render } from 'solid-js/web';
import ContinueOrStartOver, { type ContinueOrStartOverProps } from './ContinueOrStartOver';
import { executionsApi, type AttemptSummary } from '../execution';
import { flush } from '../../test/flush';

// The dialog is stubbed: what matters here is whether it is opened, and with what it is asked to run.
const modal = vi.hoisted(() => ({ last: undefined as undefined | { isOpen: boolean; fromRun?: string; initialTokenLimit?: number } }));
vi.mock('./RunWithAgentModal', () => ({
  default: (props: { isOpen: boolean; fromRun?: string; initialTokenLimit?: number }) => {
    modal.last = props;
    return null;
  },
}));

const attempt = (over: Partial<AttemptSummary> = {}): AttemptSummary =>
  ({
    attempt_id: 'a1',
    request_id: 'r1',
    attempt_number: 1,
    state: 'failed',
    terminal_reason: null,
    ...over,
  }) as AttemptSummary;

function mount(props: Partial<ContinueOrStartOverProps> = {}) {
  const container = document.createElement('div');
  document.body.appendChild(container);
  render(() => <ContinueOrStartOver attempt={attempt()} itemId="i" itemTitle="Task" projectId="p" {...props} />, container);
  return container;
}

const continueButton = (c: HTMLElement) => [...c.querySelectorAll('button')].find((b) => b.textContent === 'Continue') as HTMLButtonElement;
const startOverButton = (c: HTMLElement) =>
  [...c.querySelectorAll('button')].find((b) => b.textContent === "Start over from this run's work") as HTMLButtonElement;
const reasonFor = (b: HTMLButtonElement) => document.getElementById(b.getAttribute('aria-describedby')!)?.textContent;

afterEach(() => {
  document.body.innerHTML = '';
  modal.last = undefined;
  vi.restoreAllMocks();
});

describe('ContinueOrStartOver', () => {
  it('renders nothing for an ending that never offered Continue at all', () => {
    const c = mount({ attempt: attempt({ terminal_reason: { code: 'hop_failed' } }) });
    expect(c.querySelector('[data-testid="continue-or-start-over"]')).toBeNull();
  });

  // Decision 5's table, row 1: rounds/toolCalls/time, treeChanged: true — Continue is primed.
  it('rounds/toolCalls/time with kept work: Continue is primed, and Start over from this run\'s work is also offered', async () => {
    const onCreated = vi.fn();
    const continueSpy = vi.spyOn(executionsApi, 'continue').mockResolvedValue({
      protocol_version: 1, request_id: 'r2', state: 'queued', replayed: false,
    });
    const c = mount({
      attempt: attempt({
        terminal_reason: {
          code: 'step_budget',
          message: 'The step ran out of its round limit: 20 of 20. Raise maxIterations to allow more.',
          branch: 'tack/attempt-1', head_commit: 'abc123',
          next: { continue: { possible: true } },
        },
      }),
      onCreated,
    });
    expect(c.querySelector('[data-testid="budget-stop-message"]')!.textContent).toBe(
      'The step ran out of its round limit: 20 of 20. Raise maxIterations to allow more.',
    );
    expect(c.querySelector('[data-testid="budget-stop-kept-work"]')!.textContent).toContain('tack/attempt-1');
    expect(continueButton(c).disabled).toBe(false);
    expect(startOverButton(c).disabled).toBe(false);
    continueButton(c).click();
    await flush();
    expect(continueSpy).toHaveBeenCalledWith('r1');
    expect(onCreated).toHaveBeenCalledWith('r2');
  });

  // Decision 5's table, row 2: bound tokens, treeChanged: true — Continue is disabled (tokens are
  // task-wide and cannot be raised by continuing); Start over from this run's work is primed and
  // doubles the token limit, since what stopped the step was the token limit.
  it('a tokens-bound stop with kept work: Continue is disabled with the backend\'s own why, Start over from this run\'s work doubles the limit', () => {
    const c = mount({
      attempt: attempt({
        terminal_reason: {
          code: 'budget',
          branch: 'tack/attempt-2', head_commit: 'def456',
          next: { continue: { possible: false, why: "The task's token limit is spent, and continuing cannot raise it." } },
        },
      }),
      tokenLimit: 100_000,
    });
    expect(continueButton(c).disabled).toBe(true);
    expect(reasonFor(continueButton(c))).toBe("The task's token limit is spent, and continuing cannot raise it.");
    expect(startOverButton(c).disabled).toBe(false);
    startOverButton(c).click();
    expect(modal.last?.isOpen).toBe(true);
    expect(modal.last?.fromRun).toBe('r1');
    expect(modal.last?.initialTokenLimit).toBe(200_000);
  });

  // Decision 5's table, row 3: treeChanged false (the step changed nothing) — neither action applies.
  it('a stop that changed nothing: both actions are disabled, each with its own reason', () => {
    const c = mount({
      attempt: attempt({
        terminal_reason: {
          code: 'step_budget',
          next: { continue: { possible: false, why: 'The step changed nothing; the task may be unclear. Reword it before running again.' } },
        },
      }),
    });
    expect(continueButton(c).disabled).toBe(true);
    expect(reasonFor(continueButton(c))).toBe('The step changed nothing; the task may be unclear. Reword it before running again.');
    expect(startOverButton(c).disabled).toBe(true);
    expect(reasonFor(startOverButton(c))).toBe("This run's step changed nothing, so there is no work to start from.");
    startOverButton(c).click();
    expect(modal.last).toBeUndefined();
  });

  // Decision 5's table, row 4: a docket missing retryContinues/budgetOutcome.
  it('a docket too old to say: Continue is disabled naming the fix, and there is no kept work either', () => {
    const c = mount({
      attempt: attempt({
        terminal_reason: { code: 'budget', next: { continue: { possible: false, why: 'This team\'s docket cannot continue a step. Update docket.' } } },
      }),
    });
    expect(continueButton(c).disabled).toBe(true);
    expect(reasonFor(continueButton(c))).toBe("This team's docket cannot continue a step. Update docket.");
    expect(startOverButton(c).disabled).toBe(true);
  });

  it('does not double the token limit when what stopped the step was not the token limit', () => {
    const c = mount({
      attempt: attempt({
        terminal_reason: {
          code: 'step_budget',
          branch: 'tack/attempt-3', head_commit: 'ghi789',
          next: { continue: { possible: true } },
        },
      }),
      tokenLimit: 100_000,
    });
    startOverButton(c).click();
    expect(modal.last?.initialTokenLimit).toBeUndefined();
  });

  it('is disabled with "This screen does not know the task" when it has none, even with kept work', () => {
    const c = mount({
      attempt: attempt({
        terminal_reason: { code: 'step_budget', branch: 'b', head_commit: 'h', next: { continue: { possible: false, why: 'x' } } },
      }),
      itemTitle: '',
      projectId: '',
    });
    expect(startOverButton(c).disabled).toBe(true);
    expect(reasonFor(startOverButton(c))).toBe('This screen does not know the task');
  });
});
