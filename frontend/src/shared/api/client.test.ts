import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { request, requestBlob, requestForm, ApiError, tokenStore } from './client';

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
    ...init,
  });
}

describe('shared/api/client', () => {
  beforeEach(() => {
    tokenStore.set(null);
    vi.restoreAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('joins the default base (/api) with the path', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ ok: true }));

    await request('/projects');

    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][0]).toBe('/api/projects');
  });

  it('sends Content-Type: application/json by default', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({}));

    await request('/projects', { method: 'POST', body: '{}' });

    const headers = fetchMock.mock.calls[0][1]!.headers as Headers;
    expect(headers.get('Content-Type')).toBe('application/json');
  });

  it('returns the parsed JSON body on 2xx', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      jsonResponse({ id: '42' })
    );
    await expect(request<{ id: string }>('/projects/42')).resolves.toEqual({
      id: '42',
    });
  });

  it('returns undefined for 204 No Content', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(null, { status: 204 })
    );
    await expect(request('/projects/1', { method: 'DELETE' })).resolves.toBeUndefined();
  });

  it('throws ApiError carrying status + message on non-2xx', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response('cycle detected', { status: 400 })
    );

    const err = (await request('/items/1/dependencies', { method: 'POST' }).catch(
      (e) => e
    )) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(400);
    expect(err.message).toBe('cycle detected');
  });

  it('attaches a bearer token from the token store when present', async () => {
    vi.spyOn(tokenStore, 'get').mockReturnValue('secret');
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({}));

    await request('/projects');

    const headers = fetchMock.mock.calls[0][1]!.headers as Headers;
    expect(headers.get('Authorization')).toBe('Bearer secret');
  });

  it('omits Authorization when no token is set', async () => {
    vi.spyOn(tokenStore, 'get').mockReturnValue(null);
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({}));

    await request('/projects');

    const headers = fetchMock.mock.calls[0][1]!.headers as Headers;
    expect(headers.has('Authorization')).toBe(false);
  });

  it('requestBlob returns a Blob and throws ApiError on failure', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response('binary', { status: 200 })
    );
    await expect(requestBlob('/backup')).resolves.toBeInstanceOf(Blob);

    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response('nope', { status: 500 })
    );
    await expect(requestBlob('/backup')).rejects.toBeInstanceOf(ApiError);
  });

  it('requestForm does NOT set a Content-Type header (browser adds boundary)', async () => {
    const fetchMock = vi
      .spyOn(globalThis, 'fetch')
      .mockResolvedValue(jsonResponse({ id: 'a' }));

    const form = new FormData();
    form.append('file', new Blob(['x']), 'x.txt');
    await requestForm('/items/1/attachments', form);

    const headers = fetchMock.mock.calls[0][1]!.headers as Headers;
    expect(headers.has('Content-Type')).toBe(false);
  });
});
