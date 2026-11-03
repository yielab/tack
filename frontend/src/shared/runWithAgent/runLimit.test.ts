import { describe, expect, it } from 'vitest';
import { doubled, hasBudgetStopOffer, isNearLimit, stoppedByLimit } from './runLimit';

describe('a run near its limit', () => {
  it('turns amber from 80% of the limit, and never without one', () => {
    expect(isNearLimit(799, 1000)).toBe(false);
    expect(isNearLimit(800, 1000)).toBe(true);
    expect(isNearLimit(1200, 1000)).toBe(true);
    expect(isNearLimit(800, null)).toBe(false);
    expect(isNearLimit(null, 1000)).toBe(false);
  });

  it('is stopped by its limit when the runner or docket says so', () => {
    expect(stoppedByLimit({ code: 'token_limit' })).toBe(true);
    expect(stoppedByLimit({ code: 'budget' })).toBe(true);
    expect(stoppedByLimit({ code: 'failed' })).toBe(false);
    expect(stoppedByLimit(null)).toBe(false);
  });

  it('a `budget` stop is a token limit only when its hop names no step bound of its own', () => {
    // The shape docket sent before `budgetOutcome`: no hop carries a `budget` at all.
    expect(stoppedByLimit({ code: 'budget', steps: [{ role: 'implementer' }] })).toBe(true);
    // `budgetOutcome`, bound `tokens`: Tack's own limit, read as before.
    expect(stoppedByLimit({ code: 'budget', steps: [{ budget: { bound: 'tokens' } }] })).toBe(true);
    // A step's own round/tool-call/time budget is never "ran out of tokens", whichever field names it.
    expect(stoppedByLimit({ code: 'step_budget', steps: [{ budget: { bound: 'rounds' } }] })).toBe(false);
    expect(stoppedByLimit({ code: 'budget', steps: [{ budget: { bound: 'rounds' } }] })).toBe(false);
    expect(stoppedByLimit({ code: 'budget', steps: [{ budget: { bound: 'toolCalls' } }] })).toBe(false);
    expect(stoppedByLimit({ reason: 'step_budget' })).toBe(false);
  });

  it('runs again with twice the limit', () => {
    expect(doubled(720_000)).toBe(1_440_000);
  });
});

describe('whether a tokens stop also carries docket\'s own continue/start-over offer', () => {
  it('a tokens stop with `budgetOutcome` (`next.continue` present, however `possible` reads) is both a limit stop and a budget-stop offer — decision 5\'s "replaces": RunLimitNote must not also show "Stopped at its limit"', () => {
    const terminalReason = {
      code: 'budget',
      steps: [{ budget: { bound: 'tokens', treeChanged: true } }],
      next: { continue: { possible: false, why: 'docket cannot continue this step.' } },
    };
    expect(stoppedByLimit(terminalReason)).toBe(true);
    expect(hasBudgetStopOffer(terminalReason)).toBe(true);
    // The two screens gate RunLimitNote's "stopped" telling on `stoppedByLimit && !hasBudgetStopOffer`.
    expect(stoppedByLimit(terminalReason) && !hasBudgetStopOffer(terminalReason)).toBe(false);
  });

  it('a `possible: true` continue offer still counts — the key\'s presence is what matters, not its value', () => {
    const terminalReason = { code: 'budget', steps: [{ budget: { bound: 'tokens' } }], next: { continue: { possible: true, why: null } } };
    expect(hasBudgetStopOffer(terminalReason)).toBe(true);
  });

  it('a tokens stop from a docket too old to send `next` at all is still a limit stop, and keeps showing RunLimitNote exactly as before', () => {
    const terminalReason = { code: 'budget', steps: [{ role: 'implementer' }] };
    expect(stoppedByLimit(terminalReason)).toBe(true);
    expect(hasBudgetStopOffer(terminalReason)).toBe(false);
    expect(stoppedByLimit(terminalReason) && !hasBudgetStopOffer(terminalReason)).toBe(true);
  });

  it('a step\'s own round/tool-call/time budget is never a limit stop, whether or not it carries a continue offer', () => {
    expect(hasBudgetStopOffer({ code: 'step_budget', next: { continue: { possible: true, why: null } } })).toBe(true);
    expect(stoppedByLimit({ code: 'step_budget', next: { continue: { possible: true, why: null } } })).toBe(false);
  });

  it('no `terminal_reason` at all offers nothing', () => {
    expect(hasBudgetStopOffer(null)).toBe(false);
    expect(hasBudgetStopOffer({})).toBe(false);
  });
});
