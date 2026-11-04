import { describe, it, expect, vi, afterEach } from 'vitest';
import { applyTeams } from './teams';

afterEach(() => vi.unstubAllGlobals());

describe('applyTeams', () => {
  it('posts the teams to make, with the code of a review the person read', async () => {
    const fetchMock = vi.fn(async () => new Response(JSON.stringify([]), { status: 200 }));
    vi.stubGlobal('fetch', fetchMock);
    await applyTeams('p1', [{ name: 'fab-support', shape: 'respond', accept: '3f9a' }]);
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toContain('/projects/p1/teams/apply');
    expect(init.method).toBe('POST');
    expect(JSON.parse(String(init.body))).toEqual({
      teams: [{ name: 'fab-support', shape: 'respond', accept: '3f9a' }],
    });
  });
});
