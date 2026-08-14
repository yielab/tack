import { describe, it, expect, vi, afterEach } from 'vitest';
import { tokenStore } from '../api/client';
import { attemptsApi } from './attempts';

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
    ...init,
  });
}

afterEach(() => {
  vi.restoreAllMocks();
  tokenStore.set(null);
});

describe('attemptsApi', () => {
  it('list(requestId) calls GET /api/executions/{id}/attempts', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await attemptsApi.list('exec_1');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions/exec_1/attempts');
  });

  it('list(requestId) URL-encodes the request id', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await attemptsApi.list('a/b');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions/a%2Fb/attempts');
  });

  it('list(requestId) returns real AttemptSummary rows, including model_provenance and usage_economics', async () => {
    const row = {
      attempt_id: 'att_1',
      request_id: 'exec_1',
      attempt_number: 1,
      runner_id: 'runner_1',
      fencing_token: 1,
      state: 'succeeded',
      lease_issued_at: '2026-08-06T12:00:00Z',
      lease_expires_at: '2026-08-06T12:05:00Z',
      last_heartbeat_at: null,
      event_checkpoint: 'checkpoint-0012',
      completion_id: 'complete_1',
      workspace_id: 'ws_1',
      base_revision: '0123',
      actual_execution: { harness_kind: 'codex' },
      terminal_reason: { code: 'completed' },
      usage: { tokens_in: { value: 10, source: 'measured' } },
      started_at: '2026-08-06T12:00:05Z',
      ended_at: '2026-08-06T12:05:00Z',
      created_at: '2026-08-06T12:00:00Z',
      updated_at: '2026-08-06T12:05:00Z',
      model_provenance: { kind: 'matched', provider: 'openai', model_id: 'opaque/model-alpha' },
      usage_economics: {
        model_token_cost_usd_estimated: { value: null, source: 'not_measured' },
        runner_time_cost: { wall_clock_ms: 295000, cost_usd_estimated: { value: null, source: 'not_measured' } },
      },
    };
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse({ protocol_version: 1, data: [row] }));
    const { data } = await attemptsApi.list('exec_1');
    expect(data.data).toEqual([row]);
  });

  it('events(requestId, attemptNumber) calls GET .../attempts/{n}/events', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await attemptsApi.events('exec_1', 2);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions/exec_1/attempts/2/events');
  });

  it('events(...) returns EventSummary rows oldest-first, verbatim', async () => {
    const events = [
      { event_id: 'evt_1', sequence: 1, source: 'harness', kind: 'message', payload: { text: 'started' }, occurred_at: '2026-08-06T12:00:00Z', created_at: '2026-08-06T12:00:00Z' },
      { event_id: 'evt_2', sequence: 2, source: 'runner', kind: 'progress', payload: { phase: 'testing' }, occurred_at: '2026-08-06T12:01:00Z', created_at: '2026-08-06T12:01:00Z' },
    ];
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse({ protocol_version: 1, data: events }));
    const { data } = await attemptsApi.events('exec_1', 1);
    expect(data.data).toEqual(events);
  });
});
