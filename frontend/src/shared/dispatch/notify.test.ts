import { describe, it, expect, vi, afterEach } from 'vitest';
import { notifyDispatchOutcome } from './notify';
import { toast } from '../ui/toast';
import type { DispatchItemResponse } from './api';

const BASE: DispatchItemResponse = {
  outcome: 'dispatched',
  task: null,
  approval_token: null,
  current_status: null,
  dispatch_from: null,
  message: null,
  policy_id: null,
  status_applied: null,
  status_map_rejected: null,
};

afterEach(() => {
  vi.restoreAllMocks();
});

describe('notifyDispatchOutcome', () => {
  it('toasts success for "dispatched"', () => {
    const spy = vi.spyOn(toast, 'success');
    notifyDispatchOutcome({ ...BASE, outcome: 'dispatched' });
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it('toasts warning (never success) for "waiting_approval" — the headline correctness rule', () => {
    const success = vi.spyOn(toast, 'success');
    const warning = vi.spyOn(toast, 'warning');
    notifyDispatchOutcome({ ...BASE, outcome: 'waiting_approval' });
    expect(warning).toHaveBeenCalledTimes(1);
    expect(success).not.toHaveBeenCalled();
  });

  it('toasts error for "blocked" and includes the policy id in the message', () => {
    const spy = vi.spyOn(toast, 'error');
    notifyDispatchOutcome({ ...BASE, outcome: 'blocked', policy_id: 'prompt-injection', message: 'nope' });
    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy.mock.calls[0][0]).toContain('prompt-injection');
  });

  it('toasts info for the remaining outcomes (already_in_flight, no_dispatch_policy, not_eligible)', () => {
    const spy = vi.spyOn(toast, 'info');
    notifyDispatchOutcome({ ...BASE, outcome: 'already_in_flight' });
    notifyDispatchOutcome({ ...BASE, outcome: 'no_dispatch_policy' });
    notifyDispatchOutcome({ ...BASE, outcome: 'not_eligible' });
    expect(spy).toHaveBeenCalledTimes(3);
  });

  it('prefixes the message with the given label when provided', () => {
    const spy = vi.spyOn(toast, 'success');
    notifyDispatchOutcome({ ...BASE, outcome: 'dispatched' }, 'Fix the login bug');
    expect(spy.mock.calls[0][0]).toContain('Fix the login bug');
  });
});
