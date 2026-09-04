import { describe, it, expect, vi, afterEach } from 'vitest';
import { ApiError, tokenStore } from '../api/client';
import { artifactsApi, isArtifactContentNotVerified, isArtifactNotFound } from './artifacts';

afterEach(() => {
  vi.restoreAllMocks();
  tokenStore.set(null);
});

function errorResponse(status: number, code: string, message: string): Response {
  return new Response(JSON.stringify({ error: { status, message, code } }), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

describe('artifactsApi.list', () => {
  it('GETs the discovery route and returns the manifest rows', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          protocol_version: 1,
          data: [
            {
              artifact_id: 'art_1',
              kind: 'diff',
              name: 'patch.diff',
              media_type: 'text/plain',
              size_bytes: 42,
              content_verified: true,
              created_at: '2026-08-06T12:00:00Z',
            },
          ],
        }),
        { status: 200 },
      ),
    );
    const rows = await artifactsApi.list('exec_1', 2);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions/exec_1/attempts/2/artifacts');
    expect(rows).toHaveLength(1);
    expect(rows[0].artifact_id).toBe('art_1');
    expect(rows[0].content_verified).toBe(true);
  });

  it('URL-encodes every segment', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ protocol_version: 1, data: [] }), { status: 200 }),
    );
    await artifactsApi.list('a/b', 1);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions/a%2Fb/attempts/1/artifacts');
  });

  it('throws ApiError(404) when the request or attempt does not exist', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(errorResponse(404, 'not_found', 'Attempt does not exist'));
    await expect(artifactsApi.list('exec_1', 99)).rejects.toBeInstanceOf(ApiError);
  });
});

describe('artifactsApi.contentUrl', () => {
  it('builds the exact route the operator artifact-download handler mounts', () => {
    expect(artifactsApi.contentUrl('exec_1', 2, 'art_1')).toBe(
      '/api/executions/exec_1/attempts/2/artifacts/art_1/content',
    );
  });

  it('URL-encodes every segment', () => {
    expect(artifactsApi.contentUrl('a/b', 1, 'art/1')).toBe(
      '/api/executions/a%2Fb/attempts/1/artifacts/art%2F1/content',
    );
  });
});

describe('artifactsApi.download', () => {
  it('GETs the content route and resolves to a Blob on 200', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response('hello', { status: 200, headers: { 'Content-Type': 'text/plain' } }),
    );
    const blob = await artifactsApi.download('exec_1', 1, 'art_1');
    expect(fetchMock.mock.calls[0][0]).toBe('/api/executions/exec_1/attempts/1/artifacts/art_1/content');
    expect(await blob.text()).toBe('hello');
  });

  it('throws ApiError(404) when no manifest exists — distinguishable via isArtifactNotFound', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(errorResponse(404, 'not_found', 'Artifact not found'));
    await expect(artifactsApi.download('exec_1', 1, 'missing')).rejects.toBeInstanceOf(ApiError);
    try {
      await artifactsApi.download('exec_1', 1, 'missing');
    } catch (err) {
      expect(isArtifactNotFound(err)).toBe(true);
      expect(isArtifactContentNotVerified(err)).toBe(false);
    }
  });

  it('throws ApiError(409) when content is not verified yet — distinguishable via isArtifactContentNotVerified, never conflated with 404', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      errorResponse(409, 'conflict', 'Artifact content has not been verified yet'),
    );
    try {
      await artifactsApi.download('exec_1', 1, 'art_1');
      expect.unreachable('expected a rejection');
    } catch (err) {
      expect(isArtifactContentNotVerified(err)).toBe(true);
      expect(isArtifactNotFound(err)).toBe(false);
    }
  });

  it('a generic 500 is neither isArtifactNotFound nor isArtifactContentNotVerified', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(errorResponse(500, 'internal_error', 'boom'));
    try {
      await artifactsApi.download('exec_1', 1, 'art_1');
      expect.unreachable('expected a rejection');
    } catch (err) {
      expect(isArtifactNotFound(err)).toBe(false);
      expect(isArtifactContentNotVerified(err)).toBe(false);
    }
  });
});
