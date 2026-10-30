import { describe, it, expect, vi, afterEach } from 'vitest';
import { tokenStore } from '../api/client';
import { attemptsApi, folderRunSummary, isFolderRun, readRunRecords, readRunSteps } from './attempts';

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
      model_provenance: { requested: 'opaque/model-alpha', served: 'opaque/model-alpha', source: 'reported' },
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

describe('what the run was made of', () => {
  it('readRunSteps is undefined when the run has no steps key, and reads each hop in order', () => {
    expect(readRunSteps({ result: 'Done.' })).toBeUndefined();
    expect(readRunSteps(null)).toBeUndefined();
    const steps = readRunSteps({
      steps: [
        {
          role: 'implementer', model: 'qwen2.5-coder:7b', class: 'cheap', family: 'qwen', verdict: null, ok: true,
          summary: 'Wrote the module.',
          prompts: [{ layer: 'identity', name: 'implementer', version: '1.0.0', sha256: 'abc', scope: 'shipped', tokens: 410 }],
          assumptions: ['The file exists.'], questions: ['Which folder?'],
        },
        { role: null, model: null, class: null, family: null, verdict: 'approve', ok: null, summary: null, prompts: [], assumptions: [], questions: [] },
      ],
    });
    expect(steps).toHaveLength(2);
    expect(steps![0]).toMatchObject({ role: 'implementer', class: 'cheap', ok: true, assumptions: ['The file exists.'], questions: ['Which folder?'] });
    expect(steps![0].prompts[0]).toEqual({ layer: 'identity', name: 'implementer', version: '1.0.0', sha256: 'abc', scope: 'shipped', tokens: 410 });
    expect(steps![1]).toMatchObject({ role: null, verdict: 'approve', prompts: [] });
  });

  it('readRunSteps reads a missing prompt list as none, and drops a step that is not an object', () => {
    const steps = readRunSteps({ steps: [null, { role: 'lead' }] });
    expect(steps).toEqual([
      expect.objectContaining({ role: 'lead', prompts: [], assumptions: [], questions: [], model: null }),
    ]);
  });

  it('readRunRecords is undefined without records, and keeps the cut flag and the rows', () => {
    expect(readRunRecords({ result: 'Done.' })).toBeUndefined();
    const sets = readRunRecords({
      records: [{ set: 'leads', columns: ['_id', 'email'], rows: [{ _id: 1, email: 'a@example.test' }], cut: true }],
    });
    expect(sets).toEqual([{ set: 'leads', columns: ['_id', 'email'], rows: [{ _id: 1, email: 'a@example.test' }], cut: true }]);
  });

  it('a folder team run is one that says outside_repository, and its summary is the last step', () => {
    const folder = { code: 'done', outside_repository: true, team: 'brief', task: 'k', summary: 'Wrote the brief.' };
    expect(isFolderRun(folder)).toBe(true);
    expect(folderRunSummary(folder)).toBe('Wrote the brief.');
    expect(isFolderRun({ result: 'x' })).toBe(false);
    expect(folderRunSummary({ summary: null })).toBeNull();
  });
});
