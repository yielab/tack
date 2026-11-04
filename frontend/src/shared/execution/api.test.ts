import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ApiError, tokenStore } from '../api/client';
import { executionsApi, fleetsApi, agentProfilesApi, runnersApi } from './api';
import { teamsApi } from './teams';

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
    ...init,
  });
}

beforeEach(() => {
  tokenStore.set(null);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('executionsApi', () => {
  it('list() calls GET /api/executions', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await executionsApi.list();
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions');
    expect((fetchMock.mock.calls[0][1] as RequestInit).method ?? 'GET').toBe('GET');
  });

  it('list(itemId) calls GET /api/executions?item_id={id}, URL-encoded', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await executionsApi.list('item/1');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions?item_id=item%2F1');
  });

  it('list(undefined, limit) calls GET /api/executions?limit={n} with no item_id', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await executionsApi.list(undefined, 2000);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions?limit=2000');
  });

  it('list(itemId, limit) combines both query parameters', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await executionsApi.list('item_1', 50);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions?item_id=item_1&limit=50');
  });

  it('list(undefined, undefined, itemIds) calls GET /api/executions?item_ids={comma-joined}, URL-encoded', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await executionsApi.list(undefined, undefined, ['item/1', 'item-2']);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions?item_ids=item%2F1%2Citem-2');
  });

  it('list(undefined, undefined, []) omits item_ids entirely rather than sending an empty parameter', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await executionsApi.list(undefined, undefined, []);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions');
  });

  it('listByProject(projectId) calls GET /api/executions?project_id={id} and returns each run with its latest_hop', async () => {
    const rows = [
      {
        request_id: 'r1',
        item_id: 'i1',
        state: 'running',
        cancellation_requested_at: null,
        created_at: '2026-11-01T10:00:00Z',
        limit: { tokens: null, source: 'none', enforced_by: 'measured', sentence: 'No limit', used: 48210 },
        latest_hop: { role: 'Reviewer', kind: 'finished', verdict: 'REQUEST-CHANGES', cycle: 1 },
      },
      { request_id: 'r2', item_id: 'i2', state: 'queued', cancellation_requested_at: null, created_at: '2026-11-01T11:00:00Z', latest_hop: null },
    ];
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ protocol_version: 1, data: rows }));
    const { data } = await executionsApi.listByProject('proj/1');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions?project_id=proj%2F1');
    expect((fetchMock.mock.calls[0][1] as RequestInit).method ?? 'GET').toBe('GET');
    expect(data.data[0].latest_hop).toEqual({ role: 'Reviewer', kind: 'finished', verdict: 'REQUEST-CHANGES', cycle: 1 });
    expect(data.data[0].limit?.used).toBe(48210);
    expect(data.data[1].latest_hop).toBeNull();
  });

  it('listByProject() surfaces an unknown project as a 404 ApiError', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse(
        { error: { code: 'not_found', message: 'Project does not exist', retryable: false, details: { resource: 'project' } } },
        { status: 404 },
      ),
    );
    await expect(executionsApi.listByProject('nope')).rejects.toBeInstanceOf(ApiError);
  });

  it('list() preserves response headers, not just the parsed body', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse(
        { protocol_version: 1, data: [] },
        { headers: { 'Content-Type': 'application/json', 'X-Tack-Test-Header': 'preserved' } },
      ),
    );
    const { data, headers } = await executionsApi.list();
    expect(data).toEqual({ protocol_version: 1, data: [] });
    expect(headers.get('X-Tack-Test-Header')).toBe('preserved');
  });

  it('get(id) calls GET /api/executions/{id} and returns the thin five-field row', async () => {
    const row = {
      request_id: 'exec_1',
      item_id: 'item_1',
      state: 'running',
      cancellation_requested_at: null,
      created_at: '2026-08-06T12:00:00Z',
    };
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse(row));
    const { data } = await executionsApi.get('exec_1');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions/exec_1');
    expect(data).toEqual(row);
  });

  it('get(id) keeps the two facts the run list carries, never_started and verifier_recommendation', async () => {
    const row = {
      request_id: 'exec_1',
      item_id: 'item_1',
      state: 'succeeded',
      cancellation_requested_at: null,
      created_at: '2026-08-06T12:00:00Z',
      never_started: false,
      verifier_recommendation: 'reject',
    };
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse({ protocol_version: 1, ...row }));
    const { data } = await executionsApi.get('exec_1');
    expect(data.never_started).toBe(false);
    expect(data.verifier_recommendation).toBe('reject');
  });

  it('get(id) URL-encodes the request id', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({ request_id: 'a/b', item_id: 'i', state: 'queued', cancellation_requested_at: null, created_at: '' }),
    );
    await executionsApi.get('a/b');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions/a%2Fb');
  });

  it('create() POSTs the full CreateExecution body and returns the create result', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({ protocol_version: 1, request_id: 'exec_1', state: 'queued', replayed: false }),
    );
    const input = {
      item_id: 'item_1',
      idempotency_key: 'key-1',
      selector_kind: 'exact_runner' as const,
      agent_profile_id: 'ap_1',
      selector_id: 'runr_1',
      requested_harness_kind: 'codex',
      repository_snapshot: {},
      permission_policy: {},
      budgets: {},
      environment: {},
      metadata: {},
      timeout_seconds: 3600,
    };
    const result = await executionsApi.create(input);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions');
    expect((fetchMock.mock.calls[0][1] as RequestInit).method).toBe('POST');
    expect(JSON.parse((fetchMock.mock.calls[0][1] as RequestInit).body as string)).toEqual(input);
    expect(result).toEqual({ protocol_version: 1, request_id: 'exec_1', state: 'queued', replayed: false });
  });

  it('cancel() POSTs to /cancel and surfaces the acknowledgement string as-is', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({ protocol_version: 1, request_id: 'exec_1', state: 'cancellation_requested' }),
    );
    const result = await executionsApi.cancel('exec_1');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions/exec_1/cancel');
    expect((fetchMock.mock.calls[0][1] as RequestInit).method).toBe('POST');
    expect(result.state).toBe('cancellation_requested');
  });

  it('cancel() rejects with ApiError carrying the conflict code on a terminal-state 409', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          error: {
            code: 'conflict',
            message: 'Execution already reached a terminal state before cancellation could apply',
            request_id: 'req_operator',
            retryable: true,
            details: {},
          },
        }),
        { status: 409, headers: { 'Content-Type': 'application/json' } },
      ),
    );
    const err = await executionsApi.cancel('exec_1').catch((e) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect((err as ApiError).status).toBe(409);
    expect((err as ApiError).code).toBe('conflict');
  });

  it('requeue() POSTs the recovery confirmation body', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({
        protocol_version: 1,
        request_id: 'exec_1',
        state: 'queued',
        recovered_from: 'needs_operator',
        replayed: false,
      }),
    );
    const result = await executionsApi.requeue('exec_1', { recovery_key: 'rk-1', reason: 'confirmed dead' });
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions/exec_1/requeue');
    expect(JSON.parse((fetchMock.mock.calls[0][1] as RequestInit).body as string)).toEqual({
      recovery_key: 'rk-1',
      reason: 'confirmed dead',
    });
    expect(result.state).toBe('queued');
  });

  it('continue() POSTs to /continue with no body and returns the new request', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({ protocol_version: 1, request_id: 'exec_2', state: 'queued', replayed: false }),
    );
    const result = await executionsApi.continue('exec_1');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions/exec_1/continue');
    expect((fetchMock.mock.calls[0][1] as RequestInit).method).toBe('POST');
    expect((fetchMock.mock.calls[0][1] as RequestInit).body).toBeUndefined();
    expect(result).toEqual({ protocol_version: 1, request_id: 'exec_2', state: 'queued', replayed: false });
  });

  it('continue() rejects with ApiError carrying the unprocessable code when the run never offered it', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          error: { status: 422, code: 'unprocessable', message: 'This run never offered Continue', request_id: 'r', retryable: false, details: {} },
        }),
        { status: 422, headers: { 'Content-Type': 'application/json' } },
      ),
    );
    const err = await executionsApi.continue('exec_1').catch((e) => e);
    expect(err).toBeInstanceOf(ApiError);
    expect((err as ApiError).status).toBe(422);
  });
});

describe('fleetsApi', () => {
  it('list() calls GET /api/runner-fleets', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await fleetsApi.list();
    expect(fetchMock.mock.calls[0][0]).toBe('/api/runner-fleets');
  });

  it('create() POSTs to /api/runner-fleets', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ protocol_version: 1, fleet_id: 'fleet_1', name: 'primary' }));
    const result = await fleetsApi.create({ name: 'primary' });
    expect(fetchMock.mock.calls[0][0]).toBe('/api/runner-fleets');
    expect(result.fleet_id).toBe('fleet_1');
  });

  it('addMember() POSTs to /api/runner-fleets/{fleet_id}/members with the runner id', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({ protocol_version: 1, fleet_id: 'fleet_1', runner_id: 'runr_1', state: 'added' }),
    );
    const result = await fleetsApi.addMember('fleet_1', 'runr_1');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/runner-fleets/fleet_1/members');
    expect((fetchMock.mock.calls[0][1] as RequestInit).method).toBe('POST');
    expect(JSON.parse((fetchMock.mock.calls[0][1] as RequestInit).body as string)).toEqual({ runner_id: 'runr_1' });
    expect(result.state).toBe('added');
  });

  it('removeMember() DELETEs /api/runner-fleets/{fleet_id}/members/{runner_id}', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({ protocol_version: 1, fleet_id: 'fleet_1', runner_id: 'runr_1', state: 'removed' }),
    );
    const result = await fleetsApi.removeMember('fleet_1', 'runr_1');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/runner-fleets/fleet_1/members/runr_1');
    expect((fetchMock.mock.calls[0][1] as RequestInit).method).toBe('DELETE');
    expect(result.state).toBe('removed');
  });
});

describe('agentProfilesApi', () => {
  it('list() calls GET /api/agent-profiles', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse({ protocol_version: 1, data: [] }));
    await agentProfilesApi.list();
    expect(fetchMock.mock.calls[0][0]).toBe('/api/agent-profiles');
  });
});

describe('runnersApi', () => {
  it('enroll() POSTs to /api/runners/enrollment and returns the one-time secret', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({
        protocol_version: 1,
        runner_id: 'runr_1',
        token_id: 'ent_1',
        enrollment_token: 'enr_secret',
        expires_at: '2026-08-06T13:00:00Z',
      }),
    );
    const result = await runnersApi.enroll({ name: 'box-1', total_capacity: 2, available_capacity: 2 });
    expect(fetchMock.mock.calls[0][0]).toBe('/api/runners/enrollment');
    expect(result.enrollment_token).toBe('enr_secret');
  });

  it('revokeRunner() POSTs to /api/runners/{id}/revoke', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ protocol_version: 1, runner_id: 'runr_1', state: 'revoked' }));
    await runnersApi.revokeRunner('runr_1');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/runners/runr_1/revoke');
  });

  it('revokeEnrollmentToken() POSTs to the nested revoke path', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({ protocol_version: 1, runner_id: 'runr_1', token_id: 'ent_1', state: 'revoked' }),
    );
    await runnersApi.revokeEnrollmentToken('runr_1', 'ent_1');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/runners/runr_1/enrollment-tokens/ent_1/revoke');
  });
});

describe('teamsApi', () => {
  it('resume() posts to the team of that agent, both names URL-encoded, and returns its answer', async () => {
    const answer = { team: 'saas dev', harness: 'docket', runner: 'ru', report: null };
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse(answer));
    await expect(teamsApi.resume('docket', 'saas dev')).resolves.toEqual(answer);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/teams/docket/saas%20dev/resume');
    expect((fetchMock.mock.calls[0][1] as RequestInit).method).toBe('POST');
  });

  it('resume() fails with the sentence the agent refused with', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({ error: { status: 409, code: 'conflict', message: 'This docket cannot resume a team from the board.' } }, { status: 409 }),
    );
    await expect(teamsApi.resume('docket', 'saas-dev')).rejects.toThrow('cannot resume a team');
  });
});
