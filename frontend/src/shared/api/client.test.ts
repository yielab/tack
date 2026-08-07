import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import {
  request,
  requestBlob,
  requestForm,
  ApiError,
  tokenStore,
  isOrchestrationDisabledError,
  ORCHESTRATION_DISABLED_CODE,
} from './client';

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

  it('extracts the message from the structured error envelope', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ error: { status: 409, message: 'remote is ahead' } }), {
        status: 409,
        headers: { 'Content-Type': 'application/json' },
      })
    );

    const err = (await request('/backup/remote', { method: 'POST' }).catch(
      (e) => e
    )) as ApiError;
    expect(err).toBeInstanceOf(ApiError);
    expect(err.status).toBe(409);
    // The user sees the human message, not the raw JSON body.
    expect(err.message).toBe('remote is ahead');
  });

  it('extracts an optional error.code from the structured envelope', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(
        JSON.stringify({
          error: { status: 409, message: 'Orchestration is disabled', code: 'orchestration_disabled' },
        }),
        { status: 409, headers: { 'Content-Type': 'application/json' } }
      )
    );

    const err = (await request('/fleet').catch((e) => e)) as ApiError;
    expect(err.status).toBe(409);
    expect(err.code).toBe('orchestration_disabled');
  });

  it('leaves code undefined when the envelope has none', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      new Response(JSON.stringify({ error: { status: 404, message: 'not found' } }), {
        status: 404,
        headers: { 'Content-Type': 'application/json' },
      })
    );

    const err = (await request('/items/1').catch((e) => e)) as ApiError;
    expect(err.code).toBeUndefined();
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
    const blob = await requestBlob('/backup');
    // Fetch and jsdom may construct Blobs in different realms. Assert the
    // complete browser Blob protocol and payload instead of same-realm
    // `instanceof`, which rejects a valid cross-realm Blob.
    expect(Object.prototype.toString.call(blob)).toBe('[object Blob]');
    expect(blob).toMatchObject({ size: 6, type: 'text/plain;charset=utf-8' });
    await expect(blob.text()).resolves.toBe('binary');

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

describe('isOrchestrationDisabledError', () => {
  it('is true for the documented code, regardless of status (409 or 403)', () => {
    expect(isOrchestrationDisabledError(new ApiError(409, 'x', ORCHESTRATION_DISABLED_CODE))).toBe(true);
    expect(isOrchestrationDisabledError(new ApiError(403, 'x', ORCHESTRATION_DISABLED_CODE))).toBe(true);
  });

  it('is true for a legacy bare 404 with no code (pre-migration fallback)', () => {
    expect(isOrchestrationDisabledError(new ApiError(404, 'not found'))).toBe(true);
  });

  it('is false for a 404 that carries a different, unrelated code', () => {
    expect(isOrchestrationDisabledError(new ApiError(404, 'x', 'item_not_found'))).toBe(false);
  });

  it('is false for a 409/403 with no code — those keep their ordinary meaning elsewhere', () => {
    expect(isOrchestrationDisabledError(new ApiError(409, 'already decided'))).toBe(false);
    expect(isOrchestrationDisabledError(new ApiError(403, 'approval token rejected'))).toBe(false);
  });

  it('is false for any other status, a plain Error, or a non-Error value', () => {
    expect(isOrchestrationDisabledError(new ApiError(500, 'boom'))).toBe(false);
    expect(isOrchestrationDisabledError(new Error('network'))).toBe(false);
    expect(isOrchestrationDisabledError(undefined)).toBe(false);
  });
});
