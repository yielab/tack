import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ApiError } from '../api/client';
import {
  decisionTokenStore,
  decisionsApi,
  isDecisionExpired,
  isDecisionIdempotencyConflict,
  isDecisionInvalidOption,
  isDecisionNotFound,
  isDecisionTokenRejected,
} from './decisions';

beforeEach(() => {
  decisionTokenStore.set(null);
});

afterEach(() => {
  vi.restoreAllMocks();
  decisionTokenStore.set(null);
});

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

describe('decisionsApi.resolve', () => {
  it('POSTs {answer} to /attempts/{attempt_id}/decisions/{decision_id}/resolve and never sends the decision-token header when none is stored', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({
        protocol_version: 1,
        decision_id: 'dec_1',
        state: 'resolved',
        answer: { option_id: 'allow_once', text: null },
        resolved_at: '2026-08-06T12:00:00Z',
        resolved_by: { kind: 'operator', subject_id: 'operator:local' },
        replayed: false,
      }),
    );

    const result = await decisionsApi.resolve('att_1', 'dec_1', { option_id: 'allow_once' });

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toBe('/api/attempts/att_1/decisions/dec_1/resolve');
    expect((init as RequestInit).method).toBe('POST');
    expect(JSON.parse((init as RequestInit).body as string)).toEqual({ answer: { option_id: 'allow_once' } });
    const headers = new Headers((init as RequestInit).headers);
    expect(headers.has('x-tack-decision-token')).toBe(false);
    expect(result.state).toBe('resolved');
    expect(result.replayed).toBe(false);
  });

  it('sends the stored decision token as x-tack-decision-token, never inside the JSON body', async () => {
    decisionTokenStore.set('super-secret-decision-token');
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({
        protocol_version: 1,
        decision_id: 'dec_1',
        state: 'resolved',
        answer: { option_id: 'allow_once', text: null },
        resolved_at: '2026-08-06T12:00:00Z',
        resolved_by: { kind: 'operator', subject_id: 'operator:local' },
        replayed: false,
      }),
    );

    await decisionsApi.resolve('att_1', 'dec_1', { option_id: 'allow_once' });

    const [, init] = fetchMock.mock.calls[0];
    const headers = new Headers((init as RequestInit).headers);
    expect(headers.get('x-tack-decision-token')).toBe('super-secret-decision-token');
    const body = JSON.parse((init as RequestInit).body as string);
    expect(JSON.stringify(body)).not.toContain('super-secret-decision-token');
  });

  it('URL-encodes both the attempt id and the decision id', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({
        protocol_version: 1,
        decision_id: 'dec/1',
        state: 'resolved',
        answer: { option_id: 'x', text: null },
        resolved_at: '',
        resolved_by: { kind: 'operator', subject_id: '' },
        replayed: false,
      }),
    );
    await decisionsApi.resolve('att/1', 'dec/1', { option_id: 'x' });
    expect(String(fetchMock.mock.calls[0][0])).toBe('/api/attempts/att%2F1/decisions/dec%2F1/resolve');
  });
});

describe('decisionTokenStore', () => {
  it('round-trips through sessionStorage and clears on null', () => {
    expect(decisionTokenStore.get()).toBeNull();
    decisionTokenStore.set('token-abc');
    expect(decisionTokenStore.get()).toBe('token-abc');
    decisionTokenStore.set(null);
    expect(decisionTokenStore.get()).toBeNull();
  });
});

describe('error classifiers', () => {
  it('isDecisionTokenRejected is true only for a 403 ApiError — the fail-closed "not configured on this deployment" case', () => {
    expect(isDecisionTokenRejected(new ApiError(403, 'forbidden'))).toBe(true);
    expect(isDecisionTokenRejected(new ApiError(404, 'not found'))).toBe(false);
  });

  it('isDecisionExpired requires both 409 AND code === decision_expired — a bare 409 is not enough', () => {
    expect(isDecisionExpired(new ApiError(409, 'expired', 'decision_expired'))).toBe(true);
    expect(isDecisionExpired(new ApiError(409, 'conflict', 'idempotency_conflict'))).toBe(false);
    expect(isDecisionExpired(new ApiError(409, 'conflict'))).toBe(false);
  });

  it('isDecisionIdempotencyConflict requires both 409 AND code === idempotency_conflict — distinct from expiry despite sharing a status', () => {
    expect(isDecisionIdempotencyConflict(new ApiError(409, 'conflict', 'idempotency_conflict'))).toBe(true);
    expect(isDecisionIdempotencyConflict(new ApiError(409, 'expired', 'decision_expired'))).toBe(false);
  });

  it('isDecisionNotFound is true only for a 404 ApiError', () => {
    expect(isDecisionNotFound(new ApiError(404, 'not found'))).toBe(true);
    expect(isDecisionNotFound(new ApiError(400, 'bad request'))).toBe(false);
  });

  it('isDecisionInvalidOption is true only for a 400 ApiError', () => {
    expect(isDecisionInvalidOption(new ApiError(400, 'invalid option'))).toBe(true);
    expect(isDecisionInvalidOption(new ApiError(404, 'not found'))).toBe(false);
  });

  it('the two 409 classifiers never both fire for the same error', () => {
    const expired = new ApiError(409, 'x', 'decision_expired');
    const conflict = new ApiError(409, 'x', 'idempotency_conflict');
    expect(isDecisionExpired(expired) && isDecisionIdempotencyConflict(expired)).toBe(false);
    expect(isDecisionExpired(conflict) && isDecisionIdempotencyConflict(conflict)).toBe(false);
  });
});
