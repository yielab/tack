import { describe, it, expect, vi, afterEach } from 'vitest';
import { orchestrationSettingsApi } from './api';

afterEach(() => {
  vi.restoreAllMocks();
});

function mockFetch(status: number, body: unknown) {
  return vi
    .spyOn(globalThis, 'fetch')
    .mockImplementation(() => Promise.resolve(new Response(JSON.stringify(body), { status })));
}

const SETTINGS_BODY = {
  enabled: false,
  source: 'env_default',
  reconciler_running: false,
  control_plane_count: 0,
  linked_project_count: 0,
  poll_secs: 10,
  approval_token_set: false,
  env_default: false,
};

describe('orchestrationSettingsApi.get', () => {
  it('GETs /settings/orchestration', async () => {
    const fetchMock = mockFetch(200, SETTINGS_BODY);
    const res = await orchestrationSettingsApi.get();
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/settings/orchestration');
    expect((init as RequestInit | undefined)?.method ?? 'GET').toBe('GET');
    expect(res.enabled).toBe(false);
    expect(res.source).toBe('env_default');
  });
});

describe('orchestrationSettingsApi.update', () => {
  it('PUTs { enabled } and returns the updated settings', async () => {
    const fetchMock = mockFetch(200, { ...SETTINGS_BODY, enabled: true, source: 'database' });
    const res = await orchestrationSettingsApi.update(true);
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/settings/orchestration');
    expect((init as RequestInit).method).toBe('PUT');
    expect(JSON.parse((init as RequestInit).body as string)).toEqual({ enabled: true });
    expect(res.enabled).toBe(true);
    expect(res.source).toBe('database');
  });

  it('sends enabled: false verbatim, not omitted', async () => {
    const fetchMock = mockFetch(200, SETTINGS_BODY);
    await orchestrationSettingsApi.update(false);
    const [, init] = fetchMock.mock.calls[0];
    expect(JSON.parse((init as RequestInit).body as string)).toEqual({ enabled: false });
  });
});

describe('orchestrationSettingsApi control-plane admin', () => {
  it('listControlPlanes GETs /control-planes', async () => {
    const fetchMock = mockFetch(200, [
      {
        id: 'cp-1',
        name: 'docket-prod',
        kind: 'docket',
        base_url: 'https://docket.example.com',
        api_version: null,
        health: 'unknown',
        last_seen_at: null,
        consecutive_failures: 0,
        token_set: false,
        capabilities: null,
        created_at: '2026-01-01T00:00:00Z',
        updated_at: '2026-01-01T00:00:00Z',
      },
    ]);
    const res = await orchestrationSettingsApi.listControlPlanes();
    expect(String(fetchMock.mock.calls[0][0])).toContain('/control-planes');
    expect(res).toHaveLength(1);
    expect(res[0].token_set).toBe(false);
  });

  it('listControlPlanes carries capabilities through for a configured plane (card G1)', async () => {
    const fetchMock = mockFetch(200, [
      {
        id: 'cp-1',
        name: 'docket-prod',
        kind: 'docket',
        base_url: 'https://docket.example.com',
        api_version: null,
        health: 'healthy',
        last_seen_at: '2026-08-05T00:00:00Z',
        consecutive_failures: 0,
        token_set: true,
        capabilities: {
          dispatch: true,
          cancel: false,
          pause: { level: 'unsupported', reason: 'docket profile <pod-id> --resume' },
          resume: { level: 'unsupported', reason: 'r' },
          event_scope: { level: 'project', reason: 'r' },
          artifacts: false,
          decisions: { level: 'poll', reason: 'r' },
          usage: { level: 'from_provider', reason: 'r' },
          model_selection: { level: 'unsupported', reason: 'r' },
          runtimes: true,
          plane_metrics: true,
          provisioning: true,
        },
        created_at: '2026-01-01T00:00:00Z',
        updated_at: '2026-01-01T00:00:00Z',
      },
    ]);
    const res = await orchestrationSettingsApi.listControlPlanes();
    expect(String(fetchMock.mock.calls[0][0])).toContain('/control-planes');
    expect(res[0].capabilities?.pause.level).toBe('unsupported');
    expect(res[0].capabilities?.pause.reason).toContain('docket profile');
  });

  it('getControlPlane GETs /control-planes/{id}', async () => {
    const fetchMock = mockFetch(200, {
      id: 'cp-1',
      name: 'docket-prod',
      kind: 'docket',
      base_url: 'https://docket.example.com',
      api_version: '1',
      health: 'healthy',
      last_seen_at: '2026-08-05T00:00:00Z',
      consecutive_failures: 0,
      token_set: true,
      created_at: '2026-01-01T00:00:00Z',
      updated_at: '2026-01-01T00:00:00Z',
    });
    const res = await orchestrationSettingsApi.getControlPlane('cp-1');
    expect(String(fetchMock.mock.calls[0][0])).toContain('/control-planes/cp-1');
    expect(res.health).toBe('healthy');
  });

  it('createControlPlane POSTs name/base_url/token, kind omittable', async () => {
    const fetchMock = mockFetch(200, {
      id: 'cp-2',
      name: 'new-plane',
      kind: 'docket',
      base_url: 'https://new.example.com',
      api_version: null,
      health: 'unknown',
      last_seen_at: null,
      consecutive_failures: 0,
      token_set: true,
      created_at: '2026-08-05T00:00:00Z',
      updated_at: '2026-08-05T00:00:00Z',
    });
    await orchestrationSettingsApi.createControlPlane({
      name: 'new-plane',
      base_url: 'https://new.example.com',
      token: 'secret-token',
    });
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/control-planes');
    expect((init as RequestInit).method).toBe('POST');
    const body = JSON.parse((init as RequestInit).body as string);
    expect(body).toEqual({
      name: 'new-plane',
      base_url: 'https://new.example.com',
      token: 'secret-token',
    });
    // The token is never echoed back — token_set is the only trace of it.
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('updateControlPlane PATCHes only the given fields, sending null to clear the token', async () => {
    const fetchMock = mockFetch(200, {
      id: 'cp-1',
      name: 'renamed',
      kind: 'docket',
      base_url: 'https://docket.example.com',
      api_version: null,
      health: 'unknown',
      last_seen_at: null,
      consecutive_failures: 0,
      token_set: false,
      created_at: '2026-01-01T00:00:00Z',
      updated_at: '2026-08-05T00:00:00Z',
    });
    await orchestrationSettingsApi.updateControlPlane('cp-1', { name: 'renamed', token: null });
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/control-planes/cp-1');
    expect((init as RequestInit).method).toBe('PATCH');
    const body = JSON.parse((init as RequestInit).body as string);
    expect(body).toEqual({ name: 'renamed', token: null });
  });

  it('deleteControlPlane DELETEs /control-planes/{id}', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response(null, { status: 204 }));
    await orchestrationSettingsApi.deleteControlPlane('cp-1');
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/control-planes/cp-1');
    expect((init as RequestInit).method).toBe('DELETE');
  });
});
