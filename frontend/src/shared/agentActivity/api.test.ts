import { describe, it, expect, vi, afterEach } from 'vitest';
import { ApiError } from '../api/client';
import { agentActivityApi, isOrchDisabled } from './api';

afterEach(() => {
  vi.restoreAllMocks();
});

describe('isOrchDisabled', () => {
  it('is true for a 404 ApiError (TACK_ORCH_ENABLE unset — the default install state)', () => {
    expect(isOrchDisabled(new ApiError(404, 'not found'))).toBe(true);
  });

  it('is false for any other ApiError or a plain Error', () => {
    expect(isOrchDisabled(new ApiError(500, 'boom'))).toBe(false);
    expect(isOrchDisabled(new Error('network down'))).toBe(false);
    expect(isOrchDisabled(undefined)).toBe(false);
  });
});

describe('agentActivityApi', () => {
  it('getForItem requests /items/{id}/agent-activity', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response(JSON.stringify({ attempts: [], approvals: [] }), { status: 200 }));

    await agentActivityApi.getForItem('item-1');

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(String(fetchMock.mock.calls[0][0])).toContain('/items/item-1/agent-activity');
  });

  it('listForProject requests /projects/{id}/agent-activity', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(new Response(JSON.stringify({ rows: [] }), { status: 200 }));

    await agentActivityApi.listForProject('proj-1');

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(String(fetchMock.mock.calls[0][0])).toContain('/projects/proj-1/agent-activity');
  });
});
