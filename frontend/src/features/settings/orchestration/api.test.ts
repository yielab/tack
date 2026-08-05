import { describe, it, expect, vi, afterEach } from 'vitest';
import { ApiError } from '../../../shared/api/client';
import { isOrchDisabled, orchestrationApi } from './api';

afterEach(() => {
  vi.restoreAllMocks();
});

function mockFetch(status: number, body: unknown) {
  return vi
    .spyOn(globalThis, 'fetch')
    .mockImplementation(() => Promise.resolve(new Response(JSON.stringify(body), { status })));
}

describe('orchestrationApi.getLink', () => {
  it('GETs /projects/{id}/orch-link', async () => {
    const fetchMock = mockFetch(200, { linked: false, link: null });
    const res = await orchestrationApi.getLink('proj-1');
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/projects/proj-1/orch-link');
    expect((init as RequestInit | undefined)?.method ?? 'GET').toBe('GET');
    expect(res.linked).toBe(false);
  });
});

describe('orchestrationApi.putLink', () => {
  it('PUTs the link with status_map always {} and the given budget', async () => {
    const fetchMock = mockFetch(200, {
      project_id: 'proj-1',
      control_plane_id: 'cp-1',
      remote_project: 'remote-1',
      pipeline_file: null,
      blueprint: null,
      auto_dispatch: false,
      budget_usd: 25,
      created_at: '2026-01-01T00:00:00Z',
      updated_at: '2026-01-01T00:00:00Z',
    });

    await orchestrationApi.putLink('proj-1', {
      control_plane_id: 'cp-1',
      remote_project: 'remote-1',
      budget_usd: 25,
    });

    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/projects/proj-1/orch-link');
    expect((init as RequestInit).method).toBe('PUT');
    const body = JSON.parse((init as RequestInit).body as string);
    expect(body).toEqual({
      control_plane_id: 'cp-1',
      remote_project: 'remote-1',
      budget_usd: 25,
      status_map: {},
    });
  });

  it('sends a null budget_usd verbatim (clearing the cap), not omitted', async () => {
    const fetchMock = mockFetch(200, {});
    await orchestrationApi.putLink('proj-1', {
      control_plane_id: 'cp-1',
      remote_project: 'remote-1',
      budget_usd: null,
    });
    const [, init] = fetchMock.mock.calls[0];
    const body = JSON.parse((init as RequestInit).body as string);
    expect(body.budget_usd).toBeNull();
  });
});

describe('orchestrationApi.listControlPlanes', () => {
  it('GETs /control-planes', async () => {
    const fetchMock = mockFetch(200, [{ id: 'cp-1', name: 'docket-1', kind: 'docket', health: 'unknown' }]);
    const res = await orchestrationApi.listControlPlanes();
    expect(String(fetchMock.mock.calls[0][0])).toContain('/control-planes');
    expect(res).toHaveLength(1);
  });
});

describe('orchestrationApi.getBudget', () => {
  it('GETs /projects/{id}/orch-budget', async () => {
    const fetchMock = mockFetch(200, {
      linked: true,
      control_plane_id: 'cp-1',
      control_plane_name: 'docket-1',
      health: 'healthy',
      budget_usd: 50,
      tokens_in: 100,
      tokens_out: 50,
      cost_usd_estimated: 0.02,
      pricing_snapshot_at: null,
    });
    const res = await orchestrationApi.getBudget('proj-1');
    expect(String(fetchMock.mock.calls[0][0])).toContain('/projects/proj-1/orch-budget');
    expect(res.tokens_in).toBe(100);
  });
});

describe('orchestrationApi.getPolicy', () => {
  it('GETs /projects/{id}/orch-policy', async () => {
    const fetchMock = mockFetch(200, {
      linked: true,
      control_plane_id: 'cp-1',
      control_plane_name: 'docket-1',
      health: 'healthy',
      scoped_to_control_plane_only: true,
      scraped_at: null,
      tool_calls: [],
      denial_rate: null,
      policy_hits: [],
      approvals_by_channel: [],
    });
    const res = await orchestrationApi.getPolicy('proj-1');
    expect(String(fetchMock.mock.calls[0][0])).toContain('/projects/proj-1/orch-policy');
    expect(res.scoped_to_control_plane_only).toBe(true);
  });
});

describe('isOrchDisabled', () => {
  it('is true only for a 404 ApiError', () => {
    expect(isOrchDisabled(new ApiError(404, 'not found'))).toBe(true);
    expect(isOrchDisabled(new ApiError(500, 'server error'))).toBe(false);
    expect(isOrchDisabled(new Error('network error'))).toBe(false);
    expect(isOrchDisabled(undefined)).toBe(false);
  });
});
